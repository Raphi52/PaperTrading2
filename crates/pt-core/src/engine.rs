//! Le moteur unique, partagé par le backtest et le paper trading en direct.
//!
//! Corrections par rapport à l'ancien bot :
//! - UN seul chemin de code pour le backtest et le direct (l'ancien
//!   `backtest.py` et `bot.py` avaient chacun leur logique) ;
//! - décision à la clôture d'une bougie, exécution à l'ouverture de la suivante :
//!   aucune exécution à un prix que le bot ne pouvait pas connaître ;
//! - les stops et objectifs agissent DANS la bougie (plus bas / plus haut), le
//!   stop étant supposé touché en premier si les deux le sont ;
//! - on n'achète jamais une deuxième fois un symbole déjà détenu, sauf
//!   renforcement explicitement borné par la stratégie ;
//! - la taille d'une position dépend du capital total et de la distance au stop,
//!   pas de la trésorerie restante.

use crate::candle::Candle;
use crate::indicators::atr;
use crate::portfolio::{CostModel, Portfolio};
use crate::strategy::{compute_signals, entry_strength, External, Preset, Signal, Sizing, TakeProfit};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Ce que le moteur sait d'un symbole à un instant donné : les bougies
/// clôturées disponibles, et éventuellement la bougie en cours.
#[derive(Debug, Clone, Copy)]
pub struct Feed<'a> {
    pub closed: &'a [Candle],
    pub forming: Option<&'a Candle>,
}

/// Fin de traitement d'un instant : ouverture et clôture de la bougie.
#[derive(Debug, Clone, Copy)]
pub struct Tick {
    pub open_time: i64,
    pub close_time: i64,
}

/// Montant minimal d'un ordre (ordre de grandeur du minimum Binance spot).
pub const MIN_NOTIONAL: f64 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PendingKind {
    Enter,
    Add,
    Exit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pending {
    pub kind: PendingKind,
    pub reason: String,
    /// ATR(14) à la clôture de la décision, pour placer le stop.
    pub atr: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SymbolState {
    pub pending: Option<Pending>,
    /// Ouverture de la dernière bougie clôturée traitée.
    pub last_bar_open_time: Option<i64>,
    pub trailing_active: bool,
    /// Force d'entrée à la dernière clôture (voir `entry_strength`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strength: Option<f64>,
}

/// Une crypto qui a donné un signal d'entrée à la clôture, en attente de place.
struct Candidate {
    symbol: String,
    strength: f64,
    atr: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Engine {
    pub preset: Preset,
    pub costs: CostModel,
    pub symbols: Vec<String>,
    pub portfolio: Portfolio,
    pub states: BTreeMap<String, SymbolState>,
    pub marks: BTreeMap<String, f64>,
    /// Aucune entrée sur une bougie ouverte avant cet instant (préchauffage).
    pub trade_from: i64,
}

impl Engine {
    pub fn new(preset: Preset, symbols: Vec<String>, initial_cash: f64, costs: CostModel, trade_from: i64) -> Self {
        let states = symbols.iter().map(|s| (s.clone(), SymbolState::default())).collect();
        Engine {
            preset,
            costs,
            symbols,
            portfolio: Portfolio::new(initial_cash),
            states,
            marks: BTreeMap::new(),
            trade_from,
        }
    }

    pub fn equity(&self) -> f64 {
        self.portfolio.equity(&self.marks)
    }

    /// LE point d'entrée unique, en backtest comme en direct : traite, dans l'ordre
    /// du temps et pour tous les symboles, les bougies clôturées pas encore vues,
    /// puis exécute chaque décision à l'ouverture de la bougie suivante (clôturée
    /// ou en cours). `on_tick` est appelé après chaque instant, avant exécution.
    /// Rend le nombre de bougies traitées.
    pub fn advance(
        &mut self,
        feeds: &BTreeMap<String, Feed>,
        ext: &External,
        mut on_tick: impl FnMut(Tick, &Engine),
    ) -> usize {
        struct Prepared<'a> {
            feed: &'a Feed<'a>,
            signals: Vec<Signal>,
            strength: Vec<f64>,
            atr: Vec<f64>,
            by_time: BTreeMap<i64, usize>,
        }
        let mut prepared: BTreeMap<&String, Prepared> = BTreeMap::new();
        let mut times = BTreeSet::new();
        for (sym, feed) in feeds {
            let last = self.states.get(sym).and_then(|s| s.last_bar_open_time);
            let start = last.map_or(0, |t| feed.closed.partition_point(|c| c.open_time <= t));
            if start >= feed.closed.len() {
                continue;
            }
            let by_time: BTreeMap<i64, usize> =
                feed.closed.iter().enumerate().skip(start).map(|(i, c)| (c.open_time, i)).collect();
            times.extend(by_time.keys().copied());
            prepared.insert(
                sym,
                Prepared {
                    feed,
                    signals: compute_signals(&self.preset.rule, feed.closed, ext, self.preset.trend_sma),
                    strength: entry_strength(feed.closed),
                    atr: atr(feed.closed, 14),
                    by_time,
                },
            );
        }
        let mut processed = 0;
        let warmup = self.preset.warmup();
        for t in times {
            let mut close_time = t;
            let mut done = Vec::new();
            let mut candidates = Vec::new();
            for (sym, p) in &prepared {
                if let Some(&i) = p.by_time.get(&t) {
                    let bar = &p.feed.closed[i];
                    let mut sig = p.signals[i];
                    // Préchauffage propre à chaque crypto : cotée tard, elle entre tard.
                    sig.enter &= i >= warmup;
                    if let Some(c) = self.close_bar(sym, bar, sig, p.atr[i], p.strength[i], true) {
                        candidates.push(c);
                    }
                    close_time = close_time.max(bar.close_time);
                    done.push((*sym, i));
                    processed += 1;
                }
            }
            let closed_now: BTreeSet<String> = done.iter().map(|(s, _)| (*s).clone()).collect();
            self.allocate(candidates, &closed_now);
            on_tick(Tick { open_time: t, close_time }, self);
            // Les sorties d'abord : la vente d'un changement libère la trésorerie de l'achat.
            let is_exit = |e: &Engine, s: &String| matches!(e.states.get(s).and_then(|st| st.pending.as_ref()), Some(p) if p.kind == PendingKind::Exit);
            done.sort_by_key(|(s, _)| !is_exit(self, s));
            for (sym, i) in done {
                let feed = prepared[sym].feed;
                let next = feed.closed.get(i + 1).or(feed.forming);
                if let Some(n) = next {
                    self.on_next_open(sym, n.open_time, n.open);
                }
            }
        }
        for (sym, feed) in feeds {
            if let Some(f) = feed.forming {
                self.marks.insert(sym.clone(), f.close);
            }
        }
        processed
    }

    fn max_positions(&self) -> usize {
        self.preset.max_positions.unwrap_or(self.symbols.len()).max(1)
    }

    fn pending_entries(&self) -> usize {
        self.states.values().filter(|s| matches!(&s.pending, Some(p) if p.kind == PendingKind::Enter)).count()
    }

    /// Étape 1 : la bougie `bar` de `symbol` vient de se clôturer.
    /// Applique les stops/objectifs touchés pendant la bougie, puis décide.
    pub fn on_bar_close(&mut self, symbol: &str, bar: &Candle, sig: Signal, atr: f64) {
        self.close_bar(symbol, bar, sig, atr, f64::NAN, false);
    }

    /// Répartit les places entre les cryptos qui ont donné un signal d'entrée au
    /// même instant : les plus fortes d'abord. Places pleines et `switch_margin`
    /// réglée : une candidate remplace la position la plus faible (clôturée au
    /// même instant) si sa force la dépasse d'au moins la marge.
    fn allocate(&mut self, mut candidates: Vec<Candidate>, closed_now: &BTreeSet<String>) {
        let key = |x: f64| if x.is_nan() { f64::NEG_INFINITY } else { x };
        candidates.sort_by(|a, b| key(b.strength).total_cmp(&key(a.strength)).then_with(|| a.symbol.cmp(&b.symbol)));
        let mut free = self.max_positions().saturating_sub(self.portfolio.positions.len() + self.pending_entries());
        for c in candidates {
            if free > 0 {
                free -= 1;
                self.states.get_mut(&c.symbol).expect("état").pending =
                    Some(Pending { kind: PendingKind::Enter, reason: "SIGNAL D'ENTRÉE".into(), atr: c.atr });
                continue;
            }
            let Some(margin) = self.preset.switch_margin else { break };
            if c.strength.is_nan() {
                break;
            }
            let weakest = self
                .portfolio
                .positions
                .keys()
                .filter(|s| closed_now.contains(*s))
                .filter_map(|s| {
                    let st = self.states.get(s)?;
                    if st.pending.as_ref().is_some_and(|p| p.kind == PendingKind::Exit) {
                        return None;
                    }
                    st.strength.map(|v| (s.clone(), v))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
            let Some((weak, weak_strength)) = weakest else { break };
            if c.strength < weak_strength + margin {
                break; // candidates triées : les suivantes ne feront pas mieux
            }
            self.states.get_mut(&weak).expect("état").pending =
                Some(Pending { kind: PendingKind::Exit, reason: format!("CHANGEMENT → {}", c.symbol), atr: c.atr });
            self.states.get_mut(&c.symbol).expect("état").pending =
                Some(Pending { kind: PendingKind::Enter, reason: format!("CHANGEMENT (remplace {weak})"), atr: c.atr });
        }
    }

    /// `defer_entry` : l'entrée n'est pas décidée ici mais rendue comme candidate,
    /// pour être classée avec celles des autres cryptos (voir `allocate`).
    fn close_bar(
        &mut self,
        symbol: &str,
        bar: &Candle,
        sig: Signal,
        atr: f64,
        strength: f64,
        defer_entry: bool,
    ) -> Option<Candidate> {
        let state = self.states.entry(symbol.to_string()).or_default();
        if let Some(last) = state.last_bar_open_time {
            if bar.open_time <= last {
                return None; // déjà traitée : rejouer une bougie ne doit rien changer
            }
        }
        state.strength = strength.is_finite().then_some(strength);

        // 1. Sorties touchées pendant la bougie.
        if let Some(pos) = self.portfolio.positions.get(symbol) {
            if pos.entry_time <= bar.open_time {
                let stop_label = if state.trailing_active { "STOP SUIVEUR" } else { "STOP" };
                let mut exit: Option<(f64, String)> = None;
                if let Some(stop) = pos.stop {
                    if bar.open <= stop {
                        exit = Some((bar.open, format!("{stop_label} (ouverture sous le stop)")));
                    } else if bar.low <= stop {
                        exit = Some((stop, stop_label.to_string()));
                    }
                }
                if exit.is_none() {
                    if let Some(tp) = pos.take_profit {
                        if bar.open >= tp {
                            exit = Some((bar.open, "OBJECTIF (ouverture au-dessus)".to_string()));
                        } else if bar.high >= tp {
                            exit = Some((tp, "OBJECTIF".to_string()));
                        }
                    }
                }
                if let Some((price, reason)) = exit {
                    self.portfolio
                        .sell_all(symbol, price, bar.close_time, &self.costs, &reason)
                        .expect("position présente");
                    state.pending = None;
                    state.trailing_active = false;
                }
            }
        }

        self.marks.insert(symbol.to_string(), bar.close);
        state.last_bar_open_time = Some(bar.open_time);

        // 2. Décision à la clôture.
        let exits = self.preset.exits.clone();
        if let Some(pos) = self.portfolio.positions.get_mut(symbol) {
            pos.bars_held += 1;
            pos.highest = pos.highest.max(bar.high);
            if let Some(tr) = exits.trailing {
                let gain_pct = (pos.highest / pos.avg_price - 1.0) * 100.0;
                if gain_pct >= tr.activation_pct {
                    if let Some(level) = tr.distance.below(pos.highest, atr) {
                        if pos.stop.is_none_or(|s| level > s) {
                            pos.stop = Some(level);
                            state.trailing_active = true;
                        }
                    }
                }
            }
            let pending = if exits.max_bars.is_some_and(|m| pos.bars_held >= m) {
                Some(Pending {
                    kind: PendingKind::Exit, reason: format!("DURÉE MAX ({} bougies)", pos.bars_held), atr
                })
            } else if sig.exit {
                Some(Pending { kind: PendingKind::Exit, reason: "SIGNAL DE SORTIE".into(), atr })
            } else if let Some(py) = self.preset.pyramid {
                (pos.layers < py.max_layers && bar.close <= pos.last_fill_price * (1.0 - py.step_pct / 100.0)).then(
                    || Pending {
                        kind: PendingKind::Add,
                        reason: format!("RENFORCEMENT {}/{}", pos.layers + 1, py.max_layers),
                        atr,
                    },
                )
            } else {
                None
            };
            let state = self.states.get_mut(symbol).expect("état");
            state.pending = pending;
        } else if defer_entry {
            let state = self.states.get_mut(symbol).expect("état");
            state.pending = None;
            if sig.enter && bar.open_time >= self.trade_from {
                return Some(Candidate { symbol: symbol.to_string(), strength, atr });
            }
        } else {
            let can_enter = sig.enter
                && bar.open_time >= self.trade_from
                && self.portfolio.positions.len() + self.pending_entries() < self.max_positions();
            let state = self.states.get_mut(symbol).expect("état");
            state.pending =
                can_enter.then(|| Pending { kind: PendingKind::Enter, reason: "SIGNAL D'ENTRÉE".into(), atr });
        }
        None
    }

    /// Étape 2 : la bougie suivante s'ouvre à `price` (instant `time`).
    /// Exécute la décision prise à la clôture précédente.
    pub fn on_next_open(&mut self, symbol: &str, time: i64, price: f64) {
        self.marks.insert(symbol.to_string(), price);
        let Some(pending) = self.states.get_mut(symbol).and_then(|s| s.pending.take()) else {
            return;
        };
        match pending.kind {
            PendingKind::Exit => {
                if self.portfolio.positions.contains_key(symbol) {
                    self.portfolio
                        .sell_all(symbol, price, time, &self.costs, &pending.reason)
                        .expect("position présente");
                    self.states.get_mut(symbol).expect("état").trailing_active = false;
                }
            }
            PendingKind::Enter => {
                if self.portfolio.positions.contains_key(symbol) {
                    return;
                }
                let fill = self.costs.buy_price(price);
                let stop = self.preset.exits.stop.and_then(|d| d.below(fill, pending.atr));
                let notional = self.entry_notional(fill, stop).min(self.portfolio.cash);
                if notional < MIN_NOTIONAL {
                    return;
                }
                self.portfolio
                    .buy(symbol, notional, price, time, &self.costs, &pending.reason)
                    .expect("trésorerie vérifiée");
                self.set_levels(symbol, pending.atr);
            }
            PendingKind::Add => {
                let Some(pos) = self.portfolio.positions.get(symbol) else { return };
                let notional = pos.layer_notional.min(self.portfolio.cash);
                if notional < MIN_NOTIONAL {
                    return;
                }
                self.portfolio
                    .buy(symbol, notional, price, time, &self.costs, &pending.reason)
                    .expect("trésorerie vérifiée");
                self.set_levels(symbol, pending.atr);
            }
        }
    }

    fn entry_notional(&self, fill: f64, stop: Option<f64>) -> f64 {
        let equity = self.equity();
        match self.preset.sizing {
            Sizing::Risk { risk_pct, max_position_pct } => {
                let cap = equity * max_position_pct / 100.0;
                match stop {
                    Some(s) if s < fill => (equity * risk_pct / 100.0 / ((fill - s) / fill)).min(cap),
                    _ => cap,
                }
            }
            Sizing::Fixed { position_pct } => equity * position_pct / 100.0,
            Sizing::EqualWeight => equity / self.symbols.len().min(self.max_positions()).max(1) as f64,
        }
    }

    /// Place stop et objectif à partir du prix moyen de la position.
    fn set_levels(&mut self, symbol: &str, atr: f64) {
        let exits = self.preset.exits.clone();
        let pos = self.portfolio.positions.get_mut(symbol).expect("position");
        let base = pos.avg_price;
        let initial_stop = exits.stop.and_then(|d| d.below(base, atr));
        pos.stop = match (pos.stop, initial_stop) {
            // Après un renforcement, le stop suit le nouveau prix moyen.
            (_, Some(s)) => Some(s),
            (old, None) => old,
        };
        pos.take_profit = match exits.take_profit {
            Some(TakeProfit::Percent(x)) => Some(base * (1.0 + x / 100.0)),
            Some(TakeProfit::RMultiple(r)) => pos.stop.map(|s| base + r * (base - s)),
            None => None,
        };
        if pos.layers == 1 {
            pos.highest = pos.last_fill_price;
        }
        self.states.get_mut(symbol).expect("état").trailing_active = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::find;
    use crate::strategy::{Distance, ExitPolicy, Pyramid, Rule};
    use crate::testutil::candle;

    fn bh_engine() -> Engine {
        Engine::new(find("buy_hold").unwrap(), vec!["A".into()], 1_000.0, CostModel::default(), 0)
    }

    #[test]
    fn decision_at_close_fills_at_next_open() {
        let mut e = bh_engine();
        let b0 = candle(0, 100.0, 101.0, 99.0, 100.0);
        e.on_bar_close("A", &b0, Signal { enter: true, exit: false }, 1.0);
        assert!(e.portfolio.positions.is_empty(), "aucun achat avant l'ouverture suivante");
        e.on_next_open("A", b0.close_time + 1, 105.0);
        let pos = &e.portfolio.positions["A"];
        assert!((pos.avg_price - e.costs.buy_price(105.0)).abs() < 1e-9, "exécuté à l'ouverture suivante");
    }

    #[test]
    fn buy_and_hold_never_sells() {
        let mut e = bh_engine();
        let mut p = 100.0;
        for i in 0..500 {
            let b = candle(i, p, p * 1.03, p * 0.9, p * 0.97);
            e.on_bar_close("A", &b, Signal { enter: true, exit: false }, p * 0.05);
            e.on_next_open("A", b.close_time + 1, p * 0.97);
            p *= 0.97 + 0.06 * ((i % 3) as f64 / 2.0);
        }
        assert_eq!(e.portfolio.closed.len(), 0, "la référence ne vend jamais");
        assert_eq!(e.portfolio.fills.len(), 1, "et n'achète qu'une fois");
    }

    fn stop_engine(stop: f64, tp: Option<f64>) -> Engine {
        let mut preset = find("ema_9_21_1h").unwrap();
        preset.exits = ExitPolicy {
            stop: Some(Distance::Percent(stop)),
            take_profit: tp.map(crate::strategy::TakeProfit::Percent),
            trailing: None,
            max_bars: None,
        };
        preset.sizing = Sizing::Fixed { position_pct: 50.0 };
        Engine::new(preset, vec!["A".into()], 1_000.0, CostModel::zero(), 0)
    }

    #[test]
    fn stop_is_hit_inside_the_bar_and_wins_ties() {
        let mut e = stop_engine(5.0, Some(5.0));
        let b0 = candle(0, 100.0, 100.0, 100.0, 100.0);
        e.on_bar_close("A", &b0, Signal { enter: true, exit: false }, 1.0);
        e.on_next_open("A", b0.close_time + 1, 100.0);
        // La bougie suivante touche le stop (95) ET l'objectif (105) : le stop gagne.
        let b1 = candle(1, 100.0, 106.0, 94.0, 101.0);
        e.on_bar_close("A", &b1, Signal::default(), 1.0);
        let t = &e.portfolio.closed[0];
        assert_eq!(t.exit_reason, "STOP");
        assert!((t.exit_price - 95.0).abs() < 1e-9);
        e.portfolio.check_invariants().unwrap();
    }

    #[test]
    fn gap_below_stop_fills_at_open_not_at_stop() {
        let mut e = stop_engine(5.0, None);
        let b0 = candle(0, 100.0, 100.0, 100.0, 100.0);
        e.on_bar_close("A", &b0, Signal { enter: true, exit: false }, 1.0);
        e.on_next_open("A", b0.close_time + 1, 100.0);
        let b1 = candle(1, 90.0, 91.0, 88.0, 89.0);
        e.on_bar_close("A", &b1, Signal::default(), 1.0);
        assert!((e.portfolio.closed[0].exit_price - 90.0).abs() < 1e-9, "un trou d'ouverture se paie");
    }

    #[test]
    fn no_second_buy_on_a_held_symbol() {
        let mut e = stop_engine(20.0, None);
        for i in 0..50 {
            let b = candle(i, 100.0, 101.0, 99.0, 100.0);
            e.on_bar_close("A", &b, Signal { enter: true, exit: false }, 1.0);
            e.on_next_open("A", b.close_time + 1, 100.0);
        }
        assert_eq!(e.portfolio.fills.len(), 1, "l'ancien bot rachetait toutes les 9 minutes");
    }

    #[test]
    fn pyramid_is_bounded() {
        let mut preset = find("dip_buyer_4h").unwrap();
        preset.exits.stop = Some(Distance::Percent(90.0));
        preset.exits.take_profit = None;
        preset.pyramid = Some(Pyramid { step_pct: 5.0, max_layers: 3 });
        preset.rule = Rule::DipBuy { lookback: 3, dip_pct: 1.0 };
        let mut e = Engine::new(preset, vec!["A".into()], 1_000.0, CostModel::default(), 0);
        let mut p = 100.0;
        for i in 0..10 {
            let b = candle(i, p, p, p * 0.9, p * 0.9);
            e.on_bar_close("A", &b, Signal { enter: i == 0, exit: false }, 1.0);
            p *= 0.9;
            e.on_next_open("A", b.close_time + 1, p);
        }
        assert_eq!(e.portfolio.positions["A"].layers, 3);
        e.portfolio.check_invariants().unwrap();
    }

    #[test]
    fn trailing_stop_ratchets_up_only() {
        let mut preset = find("ema_9_21_1h").unwrap();
        preset.sizing = Sizing::Fixed { position_pct: 50.0 };
        let mut e = Engine::new(preset, vec!["A".into()], 1_000.0, CostModel::zero(), 0);
        let b0 = candle(0, 100.0, 100.0, 100.0, 100.0);
        e.on_bar_close("A", &b0, Signal { enter: true, exit: false }, 2.0);
        e.on_next_open("A", b0.close_time + 1, 100.0);
        let mut last_stop = e.portfolio.positions["A"].stop.unwrap();
        for (i, p) in [104.0, 110.0, 108.0, 115.0, 112.0].iter().enumerate() {
            let b = candle(i as i64 + 1, *p, p + 1.0, p - 1.0, *p);
            e.on_bar_close("A", &b, Signal::default(), 2.0);
            let s = e.portfolio.positions["A"].stop.unwrap();
            assert!(s >= last_stop, "le stop ne redescend jamais");
            last_stop = s;
        }
        assert!(last_stop > 100.0, "le stop suiveur protège le gain");
    }

    /// Avancer bougie par bougie, comme en direct (avec la bougie en cours connue),
    /// doit donner EXACTEMENT le même état que le backtest d'un bloc.
    #[test]
    fn live_increments_equal_one_shot_backtest() {
        use crate::testutil::synthetic;
        let a = synthetic(1600, 21);
        let b = synthetic(1550, 22);
        let b: Vec<Candle> = b
            .into_iter()
            .map(|mut c| {
                c.open_time += 50 * 3_600_000;
                c.close_time += 50 * 3_600_000;
                c
            })
            .collect(); // second symbol listé plus tard
        let ext = External::default();
        for id in ["supertrend_7_2_1h", "bollinger_reversion_1h", "dip_buyer_4h", "buy_hold"] {
            let preset = find(id).unwrap();
            let syms = vec!["A".to_string(), "B".to_string()];
            let mut one = Engine::new(preset.clone(), syms.clone(), 10_000.0, CostModel::default(), 200 * 3_600_000);
            let feeds: BTreeMap<String, Feed> = [
                ("A".to_string(), Feed { closed: &a, forming: None }),
                ("B".to_string(), Feed { closed: &b, forming: None }),
            ]
            .into();
            one.advance(&feeds, &ext, |_, _| {});

            let mut live = Engine::new(preset, syms, 10_000.0, CostModel::default(), 200 * 3_600_000);
            let end = a.last().unwrap().open_time;
            let mut t = a[0].open_time;
            while t <= end {
                let ka = a.partition_point(|c| c.open_time <= t);
                let kb = b.partition_point(|c| c.open_time <= t);
                let feeds: BTreeMap<String, Feed> = [
                    ("A".to_string(), Feed { closed: &a[..ka], forming: a.get(ka) }),
                    ("B".to_string(), Feed { closed: &b[..kb], forming: b.get(kb) }),
                ]
                .into();
                live.advance(&feeds, &ext, |_, _| {});
                t += 3_600_000 * if t % 7 == 0 { 3 } else { 1 }; // pas irréguliers, comme des passages manqués
            }
            let feeds: BTreeMap<String, Feed> = [
                ("A".to_string(), Feed { closed: &a, forming: None }),
                ("B".to_string(), Feed { closed: &b, forming: None }),
            ]
            .into();
            live.advance(&feeds, &ext, |_, _| {});
            assert_eq!(one.portfolio, live.portfolio, "{id} : le direct diverge du backtest");
            assert_eq!(one.states, live.states, "{id}");
            assert!(!one.portfolio.fills.is_empty(), "{id} : aucun trade, test sans valeur");
        }
    }

    #[test]
    fn replaying_a_bar_is_a_no_op() {
        let mut e = stop_engine(5.0, None);
        let b0 = candle(0, 100.0, 100.0, 100.0, 100.0);
        e.on_bar_close("A", &b0, Signal { enter: true, exit: false }, 1.0);
        e.on_next_open("A", b0.close_time + 1, 100.0);
        let before = e.clone();
        e.on_bar_close("A", &b0, Signal { enter: false, exit: true }, 1.0);
        assert_eq!(before, e);
    }

    fn multi(max: usize, margin: Option<f64>) -> Engine {
        let mut p = find("buy_hold").unwrap();
        p.max_positions = Some(max);
        p.switch_margin = margin;
        Engine::new(p, vec!["A".into(), "B".into(), "C".into()], 1_000.0, CostModel::default(), 0)
    }

    fn close_all(e: &mut Engine, i: i64, sigs: &[(&str, bool, f64)]) {
        let b = candle(i, 100.0, 101.0, 99.0, 100.0);
        let mut cands = Vec::new();
        for (s, enter, st) in sigs {
            if let Some(c) = e.close_bar(s, &b, Signal { enter: *enter, exit: false }, 1.0, *st, true) {
                cands.push(c);
            }
        }
        let now: BTreeSet<String> = sigs.iter().map(|x| x.0.to_string()).collect();
        e.allocate(cands, &now);
        let mut order: Vec<&str> = sigs.iter().map(|x| x.0).collect();
        order.sort_by_key(|s| !matches!(e.states[*s].pending.as_ref(), Some(p) if p.kind == PendingKind::Exit));
        for s in order {
            e.on_next_open(s, b.close_time + 1, 100.0);
        }
    }

    #[test]
    fn simultaneous_signals_take_the_strongest() {
        let mut e = multi(2, None);
        close_all(&mut e, 0, &[("A", true, 1.0), ("B", true, 3.0), ("C", true, 2.0)]);
        let held: Vec<&String> = e.portfolio.positions.keys().collect();
        assert_eq!(held, ["B", "C"], "les 2 plus fortes, pas les 2 premières");
    }

    #[test]
    fn stronger_signal_switches_the_weakest_position() {
        let mut e = multi(1, Some(1.0));
        close_all(&mut e, 0, &[("A", true, 1.0), ("B", false, 0.0), ("C", false, 0.0)]);
        assert!(e.portfolio.positions.contains_key("A"));
        close_all(&mut e, 1, &[("A", false, 1.0), ("B", true, 2.5), ("C", false, 0.0)]);
        let held: Vec<&String> = e.portfolio.positions.keys().collect();
        assert_eq!(held, ["B"], "A vendue, B achetée à la même ouverture");
    }

    #[test]
    fn signal_below_margin_does_not_switch() {
        let mut e = multi(1, Some(1.0));
        close_all(&mut e, 0, &[("A", true, 1.0), ("B", false, 0.0), ("C", false, 0.0)]);
        close_all(&mut e, 1, &[("A", false, 1.0), ("B", true, 1.9), ("C", false, 0.0)]);
        let held: Vec<&String> = e.portfolio.positions.keys().collect();
        assert_eq!(held, ["A"], "écart 0,9 sous la marge 1 : rien ne bouge");
    }

    #[test]
    fn without_switch_margin_a_full_book_never_switches() {
        let mut e = multi(1, None);
        close_all(&mut e, 0, &[("A", true, 1.0), ("B", false, 0.0), ("C", false, 0.0)]);
        close_all(&mut e, 1, &[("A", false, 1.0), ("B", true, 50.0), ("C", false, 0.0)]);
        let held: Vec<&String> = e.portfolio.positions.keys().collect();
        assert_eq!(held, ["A"]);
    }
}
