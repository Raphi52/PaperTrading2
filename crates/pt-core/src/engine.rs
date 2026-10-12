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
use crate::indicators::{atr, closes, rsi, sma};
use crate::portfolio::{CostModel, FeeTierMemo, Portfolio};
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

/// Montant minimal d'un ordre, en euros : `minOrderInQuoteAsset` de Bitvavo, identique
/// (5,00 €) pour les 426 paires EUR ouvertes au négoce le 2026-10-08 (`/v2/markets`).
pub const MIN_NOTIONAL: f64 = 5.0;

/// En direct, une exécution prévue plus de 5 minutes avant l'instant présent a été manquée :
/// l'application était fermée. Mesuré le 2026-10-10 sur 4 906 exécutions en marche normale :
/// 0,86 min au plus entre l'ouverture d'une bougie et son traitement.
pub const MISSED_AFTER_MS: i64 = 5 * 60_000;

/// Mode direct : l'instant présent et le prix du moment de chaque crypto, posés par
/// [`Engine::advance_live`] le temps d'un passage.
#[derive(Debug, Clone, Default, PartialEq)]
struct LiveClock {
    now: i64,
    prices: BTreeMap<String, f64>,
    /// Le passage précédent date de moins de [`MISSED_AFTER_MS`] : l'application tournait.
    running: bool,
}

/// LA règle de fermeture : en direct, ce qui devait se passer à `planned` est manqué si
/// l'application a été fermée depuis le passage précédent et que `planned` précède l'instant
/// présent de plus de [`MISSED_AFTER_MS`]. Rend alors l'instant présent et le prix du moment
/// de `symbol`. Toujours `None` en backtest.
fn missed(live: Option<&LiveClock>, symbol: &str, planned: i64) -> Option<(i64, f64)> {
    let live = live?;
    // fix-ok: l'âge de la bougie seul prenait un marché calme pour une fermeture : une bougie
    // dont le premier échange arrive 7 min après l'ouverture partait au prix du moment, notée
    // « à la réouverture » (test quiet_market_first_trade_after_seven_minutes_is_not_a_closure).
    if live.running || planned + MISSED_AFTER_MS >= live.now {
        return None;
    }
    Some((live.now, *live.prices.get(symbol)?))
}

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
    /// Euros échangés sur les 24 h avant la décision (voir [`volume_24h_eur`]), pour le
    /// coût d'exécution. Absent (historique trop court, ordre enregistré avant le
    /// 2026-10-09) : glissement minimal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_24h: Option<f64>,
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
    time: i64,
    price: f64,
    indicators: BTreeMap<String, f64>,
    volume_24h: Option<f64>,
}

// fix-ok: le moteur exécutait sans regarder le volume (ancien engine.rs:535-537), d'où un coût
// identique sur BTC-EUR (93,7 M€/24 h) et NPC-EUR (0,45 M€/24 h) ; mesuré le 2026-10-09 :
// 7,3 % des quarts d'heure absents des bougies Bitvavo, d'où une fenêtre comptée en temps.
/// Euros échangés (volume × clôture) par les bougies ouvertes dans les 24 h qui précèdent
/// `end`, parmi `closed` (triées). Fenêtre en TEMPS, pas en nombre de bougies : Bitvavo ne
/// rend aucune bougie pour un quart d'heure sans échange, un marché qui n'échange pas
/// paraît donc moins liquide. Un volume illisible ou négatif compte pour zéro. `None` si
/// l'historique ne couvre pas les 24 h : liquidité inconnue.
pub fn volume_24h_eur(closed: &[Candle], end: i64) -> Option<f64> {
    let start = end - crate::portfolio::DAY_MS;
    if closed.first()?.open_time > start {
        return None;
    }
    let mut sum = 0.0;
    for c in closed.iter().rev() {
        if c.open_time < start {
            break;
        }
        let v = c.volume * c.close;
        if c.open_time < end && v.is_finite() && v > 0.0 {
            sum += v;
        }
    }
    Some(sum)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecisionKind {
    /// Entrée décidée (exécutée à l'ouverture suivante).
    Entry,
    /// Renforcement décidé.
    Add,
    /// Sortie décidée, ou stop/objectif touché dans la bougie.
    Exit,
    /// Signal (ou décision) écarté, avec sa raison.
    Skipped,
}

impl DecisionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DecisionKind::Entry => "ENTRY",
            DecisionKind::Add => "ADD",
            DecisionKind::Exit => "EXIT",
            DecisionKind::Skipped => "SKIPPED",
        }
    }
}

/// Instantané d'une décision : quand, quoi, pourquoi, et l'état des indicateurs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub time: i64,
    pub symbol: String,
    pub kind: DecisionKind,
    pub reason: String,
    pub price: f64,
    pub indicators: BTreeMap<String, f64>,
}

/// Indicateurs communs à toutes les stratégies, à la dernière bougie de `c`.
/// Les valeurs non définies (historique trop court) sont omises.
pub fn indicator_snapshot(c: &[Candle], atr14: f64, strength: f64) -> BTreeMap<String, f64> {
    let mut m = BTreeMap::new();
    let Some(last) = c.last() else { return m };
    let cl = closes(c);
    let i = c.len() - 1;
    let mut put = |k: &str, v: f64| {
        if v.is_finite() {
            m.insert(k.to_string(), v);
        }
    };
    put("close", last.close);
    put("volume", last.volume);
    put("atr14", atr14);
    put("strength", strength);
    put("rsi14", rsi(&cl, 14)[i]);
    put("sma20", sma(&cl, 20)[i]);
    put("sma50", sma(&cl, 50)[i]);
    put("sma200", sma(&cl, 200)[i]);
    if i >= 1 {
        put("change_pct", (last.close / c[i - 1].close - 1.0) * 100.0);
    }
    m
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
    /// Active le journal des décisions (mode direct). Jamais sérialisé.
    #[serde(skip)]
    pub record: bool,
    /// Décisions pas encore enregistrées. Jamais sérialisé : la base les garde
    /// dans sa propre table, l'état du moteur ne grossit donc pas.
    #[serde(skip)]
    pub decisions: Vec<Decision>,
    /// Dernier instantané d'indicateurs par crypto (clôture la plus récente),
    /// pour les décisions prises hors de la clôture (exécution à l'ouverture,
    /// sortie par remplacement). Jamais sérialisé.
    #[serde(skip)]
    pub last_indicators: BTreeMap<String, BTreeMap<String, f64>>,
    /// Volume des 24 h à la dernière clôture de chaque crypto, pour une sortie décidée
    /// par remplacement (`allocate`). Jamais sérialisé : rempli dans le même passage.
    #[serde(skip)]
    pub last_volume_24h: BTreeMap<String, Option<f64>>,
    /// Posé par [`Engine::advance_live`] le temps d'un passage, absent en backtest.
    /// Jamais sérialisé.
    #[serde(skip)]
    live: Option<LiveClock>,
    /// Instant du dernier passage en direct : à moins de [`MISSED_AFTER_MS`] du passage
    /// suivant, l'application n'a pas été fermée entre les deux. Absent en backtest et dans
    /// les états enregistrés avant le 2026-10-11 (règle de fermeture d'avant).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_live_pass: Option<i64>,
    /// Volume des 30 jours au dernier recalcul du palier de frais, pour ne pas re-sommer tout
    /// l'historique à chaque ordre. Jamais sérialisé ; toujours égal dans les comparaisons.
    #[serde(skip)]
    pub fee_memo: FeeTierMemo,
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
            record: false,
            decisions: Vec::new(),
            last_indicators: BTreeMap::new(),
            last_volume_24h: BTreeMap::new(),
            live: None,
            last_live_pass: None,
            fee_memo: FeeTierMemo::default(),
        }
    }

    fn log(&mut self, time: i64, symbol: &str, kind: DecisionKind, reason: &str, price: f64, indicators: BTreeMap<String, f64>) {
        if self.record {
            self.decisions.push(Decision {
                time,
                symbol: symbol.to_string(),
                kind,
                reason: reason.to_string(),
                price,
                indicators,
            });
        }
    }

    fn last_snap(&self, symbol: &str) -> BTreeMap<String, f64> {
        if self.record { self.last_indicators.get(symbol).cloned().unwrap_or_default() } else { BTreeMap::new() }
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
        // fix-ok: un ordre décidé à un passage précédent, quand la bougie suivante n'avait encore
        // aucun échange (Bitvavo ne rend pas de bougie sans échange), était sauté tant qu'aucune
        // nouvelle bougie ne clôturait, puis écrasé par la décision de la clôture suivante.
        // Mesuré le 2026-10-11 sur 24 h de direct : 8 191 entrées jamais exécutées, 515 achats de
        // référence en attente. Il part maintenant à l'ouverture de la première bougie qui existe,
        // en cours ou close, avant toute nouvelle décision, comme le backtest (`closed[i + 1]`) ;
        // tests gapped_feed_live_equals_one_shot_backtest et suivants.
        let mut carried: Vec<(&String, &Candle, bool, bool)> = Vec::new();
        for (sym, feed) in feeds {
            let Some(st) = self.states.get(sym) else { continue };
            let (Some(p), Some(last)) = (st.pending.as_ref(), st.last_bar_open_time) else { continue };
            let start = feed.closed.partition_point(|c| c.open_time <= last);
            let next = match feed.closed.get(start) {
                Some(c) => Some((c, true)),
                None => feed.forming.filter(|f| f.open_time > last).map(|f| (f, false)),
            };
            if let Some((n, closed)) = next {
                carried.push((sym, n, closed, p.kind == PendingKind::Exit));
            }
        }
        // Dans l'ordre du temps, les sorties d'abord (elles libèrent la trésorerie).
        carried.sort_by_key(|&(_, n, _, exit)| (n.open_time, !exit));
        for (sym, n, closed, _) in carried {
            self.fill_pending(sym, n, closed);
        }

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
                    if let Some(c) = self.close_bar(sym, &p.feed.closed[..=i], sig, p.atr[i], p.strength[i], true) {
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
                    self.fill_pending(sym, n, i + 1 < feed.closed.len());
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

    /// Exécute l'ordre en attente de `symbol` à l'ouverture de `next`, première bougie après
    /// sa décision (`closed` : déjà close à ce passage).
    fn fill_pending(&mut self, symbol: &str, next: &Candle, closed: bool) {
        // fix-ok: au redémarrage, chaque ordre décidé pendant la fermeture partait à
        // l'ouverture suivante, déjà passée (mesuré le 2026-10-09 : 852 exécutions de
        // stratégie et 14 900 de référence datées pendant 96,7 min de fermeture) ;
        // test restart_after_closure_never_fills_in_the_past.
        match missed(self.live.as_ref(), symbol, next.open_time) {
            None => self.on_next_open(symbol, next.open_time, next.open),
            // Bougie suivante déjà close : l'ordre n'a jamais pu partir.
            Some(_) if closed => self.drop_missed(symbol, next.open_time, next.open),
            Some((now, price)) => {
                if let Some(p) = self.states.get_mut(symbol).and_then(|s| s.pending.as_mut()) {
                    p.reason.push_str(" (à la réouverture)");
                }
                self.on_next_open(symbol, now, price);
            }
        }
    }

    /// [`Engine::advance`] en direct, à l'instant `now` (horloge réelle). Même moteur, une
    /// seule différence : un vrai bot éteint ne trade pas. Après une fermeture (passage
    /// précédent à plus de [`MISSED_AFTER_MS`], ou inconnu), une exécution prévue plus de
    /// [`MISSED_AFTER_MS`] avant `now` n'a pas lieu à son prix passé : l'ordre est abandonné
    /// si la bougie suivante est déjà close, sinon il part au prix du moment, daté `now`. Un
    /// stop ou un objectif touché pendant la fermeture est vendu au prix du moment, daté
    /// `now`. Les bougies manquées sont rejouées pour les indicateurs et l'état. Sans
    /// fermeture, identique à `advance` (tests `live_mode_without_closure_matches_backtest`,
    /// `gapped_feed_live_equals_one_shot_backtest`).
    pub fn advance_live(
        &mut self,
        feeds: &BTreeMap<String, Feed>,
        ext: &External,
        now: i64,
        on_tick: impl FnMut(Tick, &Engine),
    ) -> usize {
        // Prix du moment : dernier prix de la bougie en cours, sinon dernière clôture.
        let prices = feeds.iter().filter_map(|(s, f)| f.forming.or(f.closed.last()).map(|c| (s.clone(), c.close))).collect();
        let running = self.last_live_pass.is_some_and(|last| now - last <= MISSED_AFTER_MS);
        self.live = Some(LiveClock { now, prices, running });
        let n = self.advance(feeds, ext, on_tick);
        self.live = None;
        self.last_live_pass = Some(now);
        n
    }

    /// Application fermée quand l'ordre devait partir : il est abandonné, et noté.
    fn drop_missed(&mut self, symbol: &str, time: i64, price: f64) {
        self.marks.insert(symbol.to_string(), price);
        let Some(p) = self.states.get_mut(symbol).and_then(|s| s.pending.take()) else {
            return;
        };
        let why = format!("ORDRE MANQUÉ (application fermée) : {}", p.reason);
        let ind = self.last_snap(symbol);
        self.log(time, symbol, DecisionKind::Skipped, &why, price, ind);
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
        self.close_bar(symbol, std::slice::from_ref(bar), sig, atr, f64::NAN, false);
    }

    /// Répartit les places entre les cryptos qui ont donné un signal d'entrée au
    /// même instant : les plus fortes d'abord. Places pleines et `switch_margin`
    /// réglée : une candidate remplace la position la plus faible (clôturée au
    /// même instant) si sa force la dépasse d'au moins la marge.
    fn allocate(&mut self, mut candidates: Vec<Candidate>, closed_now: &BTreeSet<String>) {
        let key = |x: f64| if x.is_nan() { f64::NEG_INFINITY } else { x };
        candidates.sort_by(|a, b| key(b.strength).total_cmp(&key(a.strength)).then_with(|| a.symbol.cmp(&b.symbol)));
        let mut free = self.max_positions().saturating_sub(self.portfolio.positions.len() + self.pending_entries());
        let skip = |e: &mut Engine, c: Candidate, why: &str| {
            e.log(c.time, &c.symbol, DecisionKind::Skipped, why, c.price, c.indicators);
        };
        let mut rest = candidates.into_iter();
        while let Some(c) = rest.next() {
            if free > 0 {
                free -= 1;
                self.states.get_mut(&c.symbol).expect("état").pending = Some(Pending {
                    kind: PendingKind::Enter,
                    reason: "SIGNAL D'ENTRÉE".into(),
                    atr: c.atr,
                    volume_24h: c.volume_24h,
                });
                self.log(c.time, &c.symbol, DecisionKind::Entry, "SIGNAL D'ENTRÉE", c.price, c.indicators);
                continue;
            }
            let Some(margin) = self.preset.switch_margin else {
                skip(self, c, "PLUS DE PLACE");
                rest.for_each(|c| skip(self, c, "PLUS DE PLACE"));
                break;
            };
            if c.strength.is_nan() {
                skip(self, c, "PLUS DE PLACE (force inconnue)");
                rest.for_each(|c| skip(self, c, "PLUS DE PLACE (force inconnue)"));
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
            let Some((weak, weak_strength)) = weakest else {
                skip(self, c, "PLUS DE PLACE (aucune position remplaçable)");
                rest.for_each(|c| skip(self, c, "PLUS DE PLACE (aucune position remplaçable)"));
                break;
            };
            if c.strength < weak_strength + margin {
                // candidates triées : les suivantes ne feront pas mieux
                let why = |c: &Candidate| {
                    format!("PLUS DE PLACE (force {:.2} < {weak} {:.2} + marge {margin})", c.strength, weak_strength)
                };
                let w = why(&c);
                skip(self, c, &w);
                rest.for_each(|c| {
                    let w = why(&c);
                    skip(self, c, &w)
                });
                break;
            }
            let out_reason = format!("CHANGEMENT → {}", c.symbol);
            let in_reason = format!("CHANGEMENT (remplace {weak})");
            let weak_volume = self.last_volume_24h.get(&weak).copied().flatten();
            self.states.get_mut(&weak).expect("état").pending = Some(Pending {
                kind: PendingKind::Exit,
                reason: out_reason.clone(),
                atr: c.atr,
                volume_24h: weak_volume,
            });
            self.states.get_mut(&c.symbol).expect("état").pending = Some(Pending {
                kind: PendingKind::Enter,
                reason: in_reason.clone(),
                atr: c.atr,
                volume_24h: c.volume_24h,
            });
            let weak_price = self.marks.get(&weak).copied().unwrap_or(f64::NAN);
            let mut weak_ind = self.last_snap(&weak);
            weak_ind.insert("strength".to_string(), weak_strength);
            self.log(c.time, &weak, DecisionKind::Exit, &out_reason, weak_price, weak_ind);
            self.log(c.time, &c.symbol, DecisionKind::Entry, &in_reason, c.price, c.indicators);
        }
    }

    /// `defer_entry` : l'entrée n'est pas décidée ici mais rendue comme candidate,
    /// pour être classée avec celles des autres cryptos (voir `allocate`).
    fn close_bar(
        &mut self,
        symbol: &str,
        hist: &[Candle],
        sig: Signal,
        atr: f64,
        strength: f64,
        defer_entry: bool,
    ) -> Option<Candidate> {
        let bar = hist.last().expect("au moins la bougie clôturée");
        let record = self.record;
        let snap_now = if record { indicator_snapshot(hist, atr, strength) } else { BTreeMap::new() };
        if record {
            self.last_indicators.insert(symbol.to_string(), snap_now.clone());
        }
        let snap = || snap_now.clone();
        let state = self.states.entry(symbol.to_string()).or_default();
        if let Some(last) = state.last_bar_open_time {
            if bar.open_time <= last {
                return None; // déjà traitée : rejouer une bougie ne doit rien changer
            }
        }
        state.strength = strength.is_finite().then_some(strength);
        // Liquidité connue à la clôture : elle fixera le coût de l'ordre décidé ici.
        let volume_24h = volume_24h_eur(hist, bar.close_time + 1);
        self.last_volume_24h.insert(symbol.to_string(), volume_24h);

        // 1. Sorties touchées pendant la bougie.
        let mut in_bar_exit: Option<(f64, String, i64)> = None;
        if let Some(pos) = self.portfolio.positions.get(symbol) {
            if pos.entry_time <= bar.open_time {
                let qty = pos.qty;
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
                if let Some((mut price, mut reason)) = exit {
                    let mut time = bar.close_time;
                    // Application fermée à la clôture de cette bougie : aucun ordre stop n'est
                    // posé chez la plateforme, le bot ne l'a donc vu qu'à la réouverture.
                    if let Some((now, now_price)) = missed(self.live.as_ref(), symbol, bar.close_time + 1) {
                        (price, time) = (now_price, now);
                        reason.push_str(" (touché pendant la fermeture, vendu à la réouverture)");
                    }
                    // Pendant la bougie, seules les bougies déjà closes sont connues.
                    let before = volume_24h_eur(&hist[..hist.len() - 1], bar.open_time);
                    let costs = self.fee_memo.order_costs(&self.costs, &self.portfolio.fills, before, qty * price, time);
                    self.portfolio.sell_all(symbol, price, time, &costs, &reason).expect("position présente");
                    state.pending = None;
                    state.trailing_active = false;
                    in_bar_exit = Some((price, reason, time));
                }
            }
        }

        self.marks.insert(symbol.to_string(), bar.close);
        state.last_bar_open_time = Some(bar.open_time);
        if let (true, Some((price, reason, time))) = (record, in_bar_exit) {
            // Champ distinct de `states` : `state` reste emprunté plus bas.
            self.decisions.push(Decision {
                time,
                symbol: symbol.to_string(),
                kind: DecisionKind::Exit,
                reason,
                price,
                indicators: snap(),
            });
        }

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
                    kind: PendingKind::Exit,
                    reason: format!("DURÉE MAX ({} bougies)", pos.bars_held),
                    atr,
                    volume_24h,
                })
            } else if sig.exit {
                Some(Pending { kind: PendingKind::Exit, reason: "SIGNAL DE SORTIE".into(), atr, volume_24h })
            } else if let Some(py) = self.preset.pyramid {
                (pos.layers < py.max_layers && bar.close <= pos.last_fill_price * (1.0 - py.step_pct / 100.0)).then(
                    || Pending {
                        kind: PendingKind::Add,
                        reason: format!("RENFORCEMENT {}/{}", pos.layers + 1, py.max_layers),
                        atr,
                        volume_24h,
                    },
                )
            } else {
                None
            };
            if let Some(p) = &pending {
                let kind = if p.kind == PendingKind::Add { DecisionKind::Add } else { DecisionKind::Exit };
                let reason = p.reason.clone();
                self.log(bar.close_time, symbol, kind, &reason, bar.close, snap());
            }
            let state = self.states.get_mut(symbol).expect("état");
            state.pending = pending;
        } else if defer_entry {
            let state = self.states.get_mut(symbol).expect("état");
            state.pending = None;
            if sig.enter && bar.open_time >= self.trade_from {
                return Some(Candidate {
                    symbol: symbol.to_string(),
                    strength,
                    atr,
                    time: bar.close_time,
                    price: bar.close,
                    indicators: snap(),
                    volume_24h,
                });
            }
        } else {
            let in_window = sig.enter && bar.open_time >= self.trade_from;
            let can_enter =
                in_window && self.portfolio.positions.len() + self.pending_entries() < self.max_positions();
            let state = self.states.get_mut(symbol).expect("état");
            state.pending = can_enter.then(|| Pending {
                kind: PendingKind::Enter,
                reason: "SIGNAL D'ENTRÉE".into(),
                atr,
                volume_24h,
            });
            if can_enter {
                self.log(bar.close_time, symbol, DecisionKind::Entry, "SIGNAL D'ENTRÉE", bar.close, snap());
            } else if in_window {
                self.log(bar.close_time, symbol, DecisionKind::Skipped, "PLUS DE PLACE", bar.close, snap());
            }
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
                if let Some(pos) = self.portfolio.positions.get(symbol) {
                    let costs =
                        self.fee_memo.order_costs(&self.costs, &self.portfolio.fills, pending.volume_24h, pos.qty * price, time);
                    self.portfolio.sell_all(symbol, price, time, &costs, &pending.reason).expect("position présente");
                    self.states.get_mut(symbol).expect("état").trailing_active = false;
                }
            }
            PendingKind::Enter => {
                if self.portfolio.positions.contains_key(symbol) {
                    let why = "ENTRÉE ANNULÉE (position déjà ouverte)";
                    let ind = self.last_snap(symbol);
                    self.log(time, symbol, DecisionKind::Skipped, why, price, ind);
                    return;
                }
                let fill = self.costs.buy_price(price);
                let stop = self.preset.exits.stop.and_then(|d| d.below(fill, pending.atr));
                let notional = self.entry_notional(fill, stop).min(self.portfolio.cash);
                if notional < MIN_NOTIONAL {
                    let why = format!("ENTRÉE ANNULÉE (montant {notional:.2} < minimum {MIN_NOTIONAL})");
                    let ind = self.last_snap(symbol);
                    self.log(time, symbol, DecisionKind::Skipped, &why, price, ind);
                    return;
                }
                let costs = self.fee_memo.order_costs(&self.costs, &self.portfolio.fills, pending.volume_24h, notional, time);
                self.portfolio
                    .buy(symbol, notional, price, time, &costs, &pending.reason)
                    .expect("trésorerie vérifiée");
                self.set_levels(symbol, pending.atr);
            }
            PendingKind::Add => {
                let Some(pos) = self.portfolio.positions.get(symbol) else { return };
                let notional = pos.layer_notional.min(self.portfolio.cash);
                if notional < MIN_NOTIONAL {
                    let why = format!("RENFORCEMENT ANNULÉ (montant {notional:.2} < minimum {MIN_NOTIONAL})");
                    let ind = self.last_snap(symbol);
                    self.log(time, symbol, DecisionKind::Skipped, &why, price, ind);
                    return;
                }
                let costs = self.fee_memo.order_costs(&self.costs, &self.portfolio.fills, pending.volume_24h, notional, time);
                self.portfolio
                    .buy(symbol, notional, price, time, &costs, &pending.reason)
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
    fn decisions_record_entry_exit_and_skipped_with_indicators() {
        let mut e = stop_engine(5.0, None);
        e.preset.max_positions = Some(1);
        e.symbols = vec!["A".into(), "B".into()];
        e.record = true;
        let hist: Vec<Candle> = (0..30).map(|i| candle(i, 100.0, 101.0, 99.0, 100.0 + i as f64 * 0.1)).collect();
        let last = hist.last().unwrap();
        e.close_bar("A", &hist, Signal { enter: true, exit: false }, 1.0, f64::NAN, false);
        e.close_bar("B", &hist, Signal { enter: true, exit: false }, 1.0, f64::NAN, false);
        e.on_next_open("A", last.close_time + 1, 103.0);
        let next = candle(30, 103.0, 104.0, 102.0, 103.5);
        e.on_bar_close("A", &next, Signal { enter: false, exit: true }, 1.0);
        let kinds: Vec<_> = e.decisions.iter().map(|d| (d.symbol.as_str(), d.kind, d.reason.as_str())).collect();
        assert_eq!(
            kinds,
            vec![
                ("A", DecisionKind::Entry, "SIGNAL D'ENTRÉE"),
                ("B", DecisionKind::Skipped, "PLUS DE PLACE"),
                ("A", DecisionKind::Exit, "SIGNAL DE SORTIE"),
            ]
        );
        assert!(e.decisions[0].indicators.contains_key("rsi14"), "{:?}", e.decisions[0].indicators);
        assert!(e.decisions[0].indicators.contains_key("sma20"));
        assert!(e.decisions.iter().all(|d| d.indicators.contains_key("close")));
        // Une entrée annulée à l'ouverture garde l'instantané de la dernière clôture.
        let mut e2 = stop_engine(5.0, None);
        e2.record = true;
        e2.close_bar("A", &hist, Signal { enter: true, exit: false }, 1.0, f64::NAN, false);
        e2.portfolio.cash = 1.0;
        e2.on_next_open("A", last.close_time + 1, 103.0);
        let skip = e2.decisions.last().unwrap();
        assert_eq!(skip.kind, DecisionKind::Skipped, "{:?}", skip.reason);
        assert!(skip.reason.starts_with("ENTRÉE ANNULÉE"), "{}", skip.reason);
        assert!(skip.indicators.contains_key("rsi14"), "{:?}", skip.indicators);
        // Jumeau : sans `record`, rien n'est journalisé (backtests inchangés).
        let mut quiet = stop_engine(5.0, None);
        quiet.on_bar_close("A", &next, Signal { enter: true, exit: false }, 1.0);
        assert!(quiet.decisions.is_empty());
        // Et le journal n'entre jamais dans l'état sérialisé.
        let json = serde_json::to_string(&e).unwrap();
        assert!(!json.contains("decisions") && !json.contains("rsi14"), "{json}");
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

    /// Avancer par morceaux (avec la bougie en cours connue), sans horloge, doit donner
    /// EXACTEMENT le même état que le backtest d'un bloc. Le mode direct, lui, est couvert
    /// par `live_mode_without_closure_matches_backtest`.
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
                // Pas irréguliers : `advance` (sans horloge) ne dépend pas du découpage. En
                // direct, un passage manqué de plus de 5 min est une fermeture et change les
                // exécutions, à dessein (restart_after_closure_never_fills_in_the_past).
                t += 3_600_000 * if t % 7 == 0 { 3 } else { 1 };
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

    /// Un vrai bot éteint ne trade pas. Au redémarrage, les bougies manquées sont rejouées
    /// (indicateurs, état), mais aucune exécution n'est datée pendant la fermeture : ce qui
    /// tient encore part au prix du moment, à la réouverture.
    #[test]
    fn restart_after_closure_never_fills_in_the_past() {
        use crate::portfolio::Side;
        const H: i64 = 3_600_000;
        // 0-51 plates à 100 ; 52 touche 80 (stop à 90) et clôt à 85 ; remontée ; 56 en cours.
        let mut c: Vec<Candle> = (0..52).map(|i| candle(i, 100.0, 100.0, 100.0, 100.0)).collect();
        c.push(candle(52, 100.0, 100.0, 80.0, 85.0));
        c.push(candle(53, 85.0, 95.0, 85.0, 95.0));
        c.push(candle(54, 95.0, 110.0, 95.0, 110.0));
        c.push(candle(55, 110.0, 118.0, 110.0, 118.0));
        let forming = candle(56, 119.0, 120.0, 119.0, 120.0);
        let ext = External::default();

        // Dernier passage avant la fermeture : 0-49 closes, 50 en cours depuis 30 s.
        let mut p = find("buy_hold").unwrap();
        p.exits.stop = Some(Distance::Percent(10.0));
        let mut e = Engine::new(p, vec!["A".into()], 10_000.0, CostModel::zero(), 49 * H);
        e.record = true;
        let last_pass = 50 * H + 30_000;
        let feeds: BTreeMap<String, Feed> = [("A".to_string(), Feed { closed: &c[..50], forming: Some(&c[50]) })].into();
        e.advance_live(&feeds, &ext, last_pass, |_, _| {});
        let first: Vec<_> = e.portfolio.fills.iter().map(|f| (f.side, f.time, f.price)).collect();
        assert_eq!(first, vec![(Side::Buy, 50 * H, 100.0)], "sans fermeture : à l'ouverture suivante, comme le backtest");

        // Réouverture 20 min après l'ouverture de 56 : 50-55 closes pendant la fermeture.
        let now = 56 * H + 20 * 60_000;
        let feeds: BTreeMap<String, Feed> = [("A".to_string(), Feed { closed: &c[..56], forming: Some(&forming) })].into();
        e.advance_live(&feeds, &ext, now, |_, _| {});
        for f in &e.portfolio.fills {
            assert!(!(f.time > last_pass && f.time < now), "exécution datée pendant la fermeture : {f:?}");
        }
        let sell = e.portfolio.fills.iter().find(|f| f.side == Side::Sell).expect("stop touché pendant la fermeture");
        assert_eq!((sell.time, sell.price), (now, 120.0), "vendu au prix du moment, à la réouverture, pas au stop (90)");
        assert!(
            e.decisions.iter().any(|d| d.kind == DecisionKind::Exit && d.reason.contains("fermeture")),
            "le journal dit pourquoi : {:?}",
            e.decisions
        );

        // « Acheter et garder » créé pendant la fermeture (entrées permises dès 53).
        let mut b = Engine::new(find("buy_hold").unwrap(), vec!["A".into()], 10_000.0, CostModel::zero(), 53 * H);
        b.advance_live(&feeds, &ext, now, |_, _| {});
        let got: Vec<_> = b.portfolio.fills.iter().map(|f| (f.side, f.time, f.price)).collect();
        assert_eq!(got, vec![(Side::Buy, now, 120.0)], "un seul achat, au prix du moment, à la réouverture");
    }

    /// La borne de 5 minutes, des deux côtés : à 5 min pile, l'ordre part normalement à
    /// l'ouverture ; une milliseconde de plus, il est manqué et part au prix du moment.
    #[test]
    fn missed_order_threshold_is_five_minutes() {
        const H: i64 = 3_600_000;
        let c: Vec<Candle> = (0..50).map(|i| candle(i, 100.0, 100.0, 100.0, 100.0)).collect();
        let forming = candle(50, 101.0, 103.0, 101.0, 103.0);
        let feeds: BTreeMap<String, Feed> = [("A".to_string(), Feed { closed: &c, forming: Some(&forming) })].into();
        let at_limit = 50 * H + MISSED_AFTER_MS;
        for (now, expected) in [(at_limit, (50 * H, 101.0)), (at_limit + 1, (at_limit + 1, 103.0))] {
            let mut e = Engine::new(find("buy_hold").unwrap(), vec!["A".into()], 10_000.0, CostModel::zero(), 49 * H);
            e.advance_live(&feeds, &External::default(), now, |_, _| {});
            let got: Vec<_> = e.portfolio.fills.iter().map(|f| (f.time, f.price)).collect();
            assert_eq!(got, vec![expected], "passage à {} ms de l'ouverture", now - 50 * H);
        }
    }

    /// En marche normale (un passage peu après chaque ouverture), le mode direct reste
    /// EXACTEMENT le backtest d'un bloc : la règle de fermeture ne touche rien d'autre.
    #[test]
    fn live_mode_without_closure_matches_backtest() {
        use crate::testutil::synthetic;
        const H: i64 = 3_600_000;
        let a = synthetic(1600, 21);
        let b: Vec<Candle> = synthetic(1550, 22)
            .into_iter()
            .map(|mut c| {
                c.open_time += 50 * H;
                c.close_time += 50 * H;
                c
            })
            .collect();
        let ext = External::default();
        for id in ["supertrend_7_2_1h", "bollinger_reversion_1h", "dip_buyer_4h", "buy_hold"] {
            let preset = find(id).unwrap();
            let syms = vec!["A".to_string(), "B".to_string()];
            let mut one = Engine::new(preset.clone(), syms.clone(), 10_000.0, CostModel::default(), 200 * H);
            let all: BTreeMap<String, Feed> = [
                ("A".to_string(), Feed { closed: &a, forming: None }),
                ("B".to_string(), Feed { closed: &b, forming: None }),
            ]
            .into();
            one.advance(&all, &ext, |_, _| {});

            let mut live = Engine::new(preset, syms, 10_000.0, CostModel::default(), 200 * H);
            for ka in 1..=a.len() {
                let open = a[ka - 1].close_time + 1; // ouverture de la bougie en cours
                let kb = b.partition_point(|c| c.close_time < open);
                let feeds: BTreeMap<String, Feed> = [
                    ("A".to_string(), Feed { closed: &a[..ka], forming: a.get(ka) }),
                    ("B".to_string(), Feed { closed: &b[..kb], forming: b.get(kb) }),
                ]
                .into();
                live.advance_live(&feeds, &ext, open + 30_000, |_, _| {});
            }
            assert_eq!(one.portfolio, live.portfolio, "{id} : sans fermeture, le direct diverge du backtest");
            assert_eq!(one.states, live.states, "{id}");
            assert!(!one.portfolio.fills.is_empty(), "{id} : aucun trade, test sans valeur");
        }
    }

    // --- Bougie suivante pas encore cotée (2026-10-11) ---------------------------------------
    // Bitvavo ne rend une bougie qu'après son premier échange (`/{market}/candles` : « zero
    // trades are represented by zero candlesticks »). Mesuré le 2026-10-11 sur 24 h de direct :
    // 8 191 entrées décidées jamais exécutées, 515 achats de référence en attente.

    const M: i64 = 60_000;

    fn single<'a>(closed: &'a [Candle], forming: Option<&'a Candle>) -> BTreeMap<String, Feed<'a>> {
        [("A".to_string(), Feed { closed, forming })].into()
    }

    /// Toutes les cryptos avec la même bougie en cours, sauf `quiet` (pas encore d'échange).
    fn basket<'a>(syms: &[String], closed: &'a [Candle], next: &'a Candle, quiet: Option<&str>) -> BTreeMap<String, Feed<'a>> {
        syms.iter().map(|s| (s.clone(), Feed { closed, forming: (Some(s.as_str()) != quiet).then_some(next) })).collect()
    }

    /// Une heure sur 7 sans aucun échange (bougie absente).
    fn gapped(n: usize, seed: u64, shift_h: i64) -> Vec<Candle> {
        crate::testutil::synthetic(n, seed)
            .into_iter()
            .map(|mut c| {
                c.open_time += shift_h * 3_600_000;
                c.close_time += shift_h * 3_600_000;
                c
            })
            .filter(|c| c.open_time == shift_h * 3_600_000 || (c.open_time / 3_600_000) % 7 != 3)
            .collect()
    }

    /// Ce que Bitvavo montre à `now` : les bougies closes, et la bougie en cours seulement
    /// après son premier échange — 7 min après l'ouverture pour une heure sur 11.
    fn seen_at(s: &[Candle], now: i64) -> (usize, Option<&Candle>) {
        let k = s.partition_point(|c| c.close_time < now);
        let forming = s.get(k).filter(|c| {
            let first_trade = c.open_time + if (c.open_time / 3_600_000) % 11 == 5 { 7 * M } else { 0 };
            first_trade <= now
        });
        (k, forming)
    }

    /// Passages d'une application allumée : toutes les 30 s pendant les 10 premières minutes
    /// de chaque heure, puis toutes les 4 min. Jamais plus de 5 min entre deux passages.
    fn running_passes(first: i64, last: i64) -> Vec<i64> {
        let mut v = Vec::new();
        let mut h = first - first.rem_euclid(3_600_000);
        while h <= last {
            v.extend((1..=20).map(|j| h + j * 30_000));
            v.extend((1..=12).map(|j| h + 10 * M + j * 4 * M));
            h += 3_600_000;
        }
        v
    }

    /// Une seule crypto, des heures sans échange, des premiers échanges tardifs : le direct
    /// (avec horloge, application allumée) et l'avance par morceaux (sans horloge) doivent
    /// rendre EXACTEMENT le backtest d'un bloc. Aujourd'hui, un ordre décidé avant une heure
    /// sans échange est sauté puis écrasé.
    #[test]
    fn gapped_feed_live_equals_one_shot_backtest() {
        let a = gapped(1600, 21, 0);
        assert!(a.len() < 1500, "des heures sans échange : {}", a.len());
        let ext = External::default();
        let mut waited: BTreeMap<&str, usize> = BTreeMap::new();
        for id in ["supertrend_7_2_1h", "bollinger_reversion_1h", "dip_buyer_4h", "buy_hold"] {
            let preset = find(id).unwrap();
            let syms = vec!["A".to_string()];
            let mut one = Engine::new(preset.clone(), syms.clone(), 10_000.0, CostModel::default(), 200 * 3_600_000);
            let all: BTreeMap<String, Feed> = [("A".to_string(), Feed { closed: &a, forming: None })].into();
            one.advance(&all, &ext, |_, _| {});

            let mut live = Engine::new(preset.clone(), syms.clone(), 10_000.0, CostModel::default(), 200 * 3_600_000);
            let mut plain = Engine::new(preset, syms, 10_000.0, CostModel::default(), 200 * 3_600_000);
            for now in running_passes(a[0].open_time, a.last().unwrap().close_time) {
                let (k, forming) = seen_at(&a, now);
                let feeds: BTreeMap<String, Feed> = [("A".to_string(), Feed { closed: &a[..k], forming })].into();
                live.advance_live(&feeds, &ext, now, |_, _| {});
                plain.advance(&feeds, &ext, |_, _| {});
                if forming.is_none() && live.states["A"].pending.is_some() {
                    *waited.entry(id).or_insert(0) += 1;
                }
            }
            let end = a.last().unwrap().close_time + 30_000;
            live.advance_live(&all, &ext, end, |_, _| {});
            plain.advance(&all, &ext, |_, _| {});
            assert!(!one.portfolio.fills.is_empty(), "{id} : aucun trade, test sans valeur");
            assert_eq!(one.portfolio, plain.portfolio, "{id} : par morceaux, des ordres en attente sont perdus");
            assert_eq!(one.states, plain.states, "{id}");
            assert_eq!(one.portfolio, live.portfolio, "{id} : en direct, des ordres en attente sont perdus");
            assert_eq!(one.states, live.states, "{id}");
        }
        // Une stratégie qui trade peu peut ne jamais décider juste avant une heure sans
        // échange (bollinger_reversion_1h, 2026-10-11) : la garde porte sur l'ensemble.
        assert!(waited.values().sum::<usize>() > 0, "aucun ordre n'a attendu sa bougie suivante, test sans valeur : {waited:?}");
    }

    /// Plusieurs cryptos aux trous différents : sans connaître les prix futurs, le direct ne
    /// peut pas dépenser la trésorerie dans l'ordre du backtest (qui exécute un ordre à
    /// l'ouverture suivante AVANT les bougies des autres cryptos de l'heure sans échange) ;
    /// les quantités peuvent donc différer. Mais ce sont les MÊMES ordres : crypto, sens,
    /// heure et prix. Prix à 1e-9 près : un stop posé sur le prix moyen de plusieurs couches
    /// (dip_buyer_4h) hérite de quantités différentes, donc d'un autre arrondi (mesuré le
    /// 2026-10-11 : 29,216307936827036 contre 29,21630793682703, 1 ordre sur 67).
    #[test]
    fn gapped_multi_symbol_live_has_same_orders_as_backtest() {
        let a = gapped(1600, 21, 0);
        let b = gapped(1550, 22, 50);
        let ext = External::default();
        let key = |e: &Engine| -> Vec<(String, crate::portfolio::Side, i64, i64)> {
            e.portfolio.fills.iter().map(|f| (f.symbol.clone(), f.side, f.time, (f.price * 1e9).round() as i64)).collect()
        };
        for id in ["supertrend_7_2_1h", "bollinger_reversion_1h", "dip_buyer_4h", "buy_hold"] {
            let preset = find(id).unwrap();
            let syms = vec!["A".to_string(), "B".to_string()];
            let mut one = Engine::new(preset.clone(), syms.clone(), 10_000.0, CostModel::zero(), 200 * 3_600_000);
            let all: BTreeMap<String, Feed> = [
                ("A".to_string(), Feed { closed: &a, forming: None }),
                ("B".to_string(), Feed { closed: &b, forming: None }),
            ]
            .into();
            one.advance(&all, &ext, |_, _| {});

            let mut live = Engine::new(preset, syms, 10_000.0, CostModel::zero(), 200 * 3_600_000);
            for now in running_passes(a[0].open_time, a.last().unwrap().close_time) {
                let (ka, fa) = seen_at(&a, now);
                let (kb, fb) = seen_at(&b, now);
                let feeds: BTreeMap<String, Feed> = [
                    ("A".to_string(), Feed { closed: &a[..ka], forming: fa }),
                    ("B".to_string(), Feed { closed: &b[..kb], forming: fb }),
                ]
                .into();
                live.advance_live(&feeds, &ext, now, |_, _| {});
            }
            live.advance_live(&all, &ext, a.last().unwrap().close_time + 30_000, |_, _| {});
            assert!(!one.portfolio.fills.is_empty(), "{id} : aucun trade, test sans valeur");
            assert_eq!(key(&one), key(&live), "{id} : le direct n'a pas passé les ordres du backtest");
        }
    }

    /// Premier passage : bougie de décision close, aucune bougie en cours (pas encore
    /// d'échange). Second passage : la bougie en cours existe. L'entrée part à son ouverture.
    #[test]
    fn entry_waits_for_first_trade_of_next_bar() {
        const H: i64 = 3_600_000;
        let c: Vec<Candle> = (0..50).map(|i| candle(i, 100.0, 100.0, 100.0, 100.0)).collect();
        let next = candle(50, 101.0, 103.0, 101.0, 103.0);
        let ext = External::default();
        let mut e = Engine::new(find("buy_hold").unwrap(), vec!["A".into()], 10_000.0, CostModel::zero(), 49 * H);
        e.advance(&single(&c, None), &ext, |_, _| {});
        assert!(e.portfolio.fills.is_empty(), "rien avant la bougie suivante");
        e.advance(&single(&c, Some(&next)), &ext, |_, _| {});
        let got: Vec<_> = e.portfolio.fills.iter().map(|f| (f.time, f.price)).collect();
        assert_eq!(got, vec![(50 * H, 101.0)], "à l'ouverture de la première bougie qui existe");
    }

    /// La bougie qui suit la décision arrive déjà close au passage suivant (premier échange
    /// juste avant sa clôture). L'ordre part à son ouverture, AVANT la décision de sa clôture,
    /// comme dans le backtest d'un bloc ; aujourd'hui la clôture le remplace sans l'exécuter.
    #[test]
    fn pending_order_fills_before_next_close_overwrites_it() {
        const H: i64 = 3_600_000;
        let mut c: Vec<Candle> = (0..50).map(|i| candle(i, 100.0, 100.0, 100.0, 100.0)).collect();
        c.extend((50..53).map(|i| candle(i, 51.0 + i as f64, 51.5 + i as f64, 50.5 + i as f64, 51.0 + i as f64)));
        let mut p = find("buy_hold").unwrap();
        p.exits.max_bars = Some(2);
        let ext = External::default();
        let mut e = Engine::new(p.clone(), vec!["A".into()], 10_000.0, CostModel::zero(), 49 * H);
        e.record = true;
        e.advance(&single(&c[..50], Some(&c[50])), &ext, |_, _| {}); // achat à 50H
        e.advance(&single(&c[..52], None), &ext, |_, _| {}); // DURÉE MAX à la clôture de 51, rien de coté
        assert!(e.states["A"].pending.as_ref().is_some_and(|q| q.kind == PendingKind::Exit));
        e.advance(&single(&c[..53], None), &ext, |_, _| {}); // 52 arrive déjà close
        let sell = e.portfolio.fills.iter().find(|f| f.side == crate::portfolio::Side::Sell);
        assert_eq!(
            sell.map(|f| (f.time, f.price, f.reason.as_str())),
            Some((52 * H, 103.0, "DURÉE MAX (2 bougies)")),
            "vendu à l'ouverture de 52, avec le motif de la décision d'origine"
        );
        let mut one = Engine::new(p, vec!["A".into()], 10_000.0, CostModel::zero(), 49 * H);
        one.advance(&single(&c[..53], None), &ext, |_, _| {});
        assert_eq!(one.portfolio, e.portfolio, "comme le backtest d'un bloc");
        assert_eq!(one.states, e.states);
        let after_sell = e.decisions.iter().any(|d| d.time == c[52].close_time && d.kind == DecisionKind::Entry);
        assert!(after_sell, "la clôture de 52 décide à nouveau, après la vente : {:?}", e.decisions);
    }

    /// « Acheter et garder » sur 5 cryptos : la cinquième n'a pas encore d'échange au passage
    /// qui suit la décision. Elle est achetée au passage suivant ; la référence est investie.
    #[test]
    fn buy_and_hold_fills_every_symbol_after_late_first_trade() {
        const H: i64 = 3_600_000;
        let c: Vec<Candle> = (0..50).map(|i| candle(i, 100.0, 100.0, 100.0, 100.0)).collect();
        let next = candle(50, 101.0, 101.0, 101.0, 101.0);
        let syms: Vec<String> = ["A", "B", "C", "D", "E"].iter().map(|s| s.to_string()).collect();
        let ext = External::default();
        let mut e = Engine::new(find("buy_hold").unwrap(), syms.clone(), 10_000.0, CostModel::zero(), 49 * H);
        e.advance_live(&basket(&syms, &c, &next, Some("E")), &ext, 50 * H + 30_000, |_, _| {});
        assert_eq!(e.portfolio.positions.len(), 4);
        e.advance_live(&basket(&syms, &c, &next, None), &ext, 50 * H + 60_000, |_, _| {});
        assert_eq!(e.portfolio.positions.len(), 5, "la référence doit tenir les 5 cryptos");
        let fill = e.portfolio.fills.iter().find(|f| f.symbol == "E").expect("E achetée");
        assert_eq!((fill.time, fill.price), (50 * H, 101.0));
    }

    /// Application allumée (un passage toutes les 30 s), marché calme : le premier échange
    /// de la bougie suivante arrive 7 min après son ouverture. Ce n'est PAS une fermeture :
    /// l'ordre part à l'ouverture, sans « réouverture » ni « ORDRE MANQUÉ » au journal.
    #[test]
    fn quiet_market_first_trade_after_seven_minutes_is_not_a_closure() {
        const H: i64 = 3_600_000;
        let c: Vec<Candle> = (0..50).map(|i| candle(i, 100.0, 100.0, 100.0, 100.0)).collect();
        let next = candle(50, 101.0, 103.0, 101.0, 103.0);
        let ext = External::default();
        let mut e = Engine::new(find("buy_hold").unwrap(), vec!["A".into()], 10_000.0, CostModel::zero(), 49 * H);
        e.record = true;
        let mut now = 50 * H + 30_000;
        while now <= 50 * H + 7 * M + 30_000 {
            let forming = (now >= 50 * H + 7 * M).then_some(&next);
            e.advance_live(&single(&c, forming), &ext, now, |_, _| {});
            now += 30_000;
        }
        let got: Vec<_> = e.portfolio.fills.iter().map(|f| (f.time, f.price, f.reason.clone())).collect();
        assert_eq!(got, vec![(50 * H, 101.0, "SIGNAL D'ENTRÉE".to_string())], "à l'ouverture, pas au prix du moment");
        assert!(
            e.decisions.iter().all(|d| !d.reason.contains("réouverture") && !d.reason.contains("MANQUÉ")),
            "{:?}",
            e.decisions
        );
    }

    /// Un ordre en attente enregistré avant cette correction (état JSON sans le dernier
    /// passage), puis une vraie fermeture de 2 h : il ne part jamais à un prix passé, et le
    /// journal dit ce qu'il est devenu à l'heure où il devait partir (aujourd'hui : écrasé
    /// à la clôture suivante, sans trace).
    #[test]
    fn old_saved_pending_after_real_closure_never_fills_in_the_past() {
        const H: i64 = 3_600_000;
        let mut c: Vec<Candle> = (0..50).map(|i| candle(i, 100.0, 100.0, 100.0, 100.0)).collect();
        c.push(candle(50, 101.0, 101.0, 101.0, 101.0));
        c.push(candle(51, 102.0, 102.0, 102.0, 102.0));
        let forming = candle(52, 110.0, 120.0, 110.0, 120.0);
        let ext = External::default();
        let mut e = Engine::new(find("buy_hold").unwrap(), vec!["A".into()], 10_000.0, CostModel::zero(), 49 * H);
        let last_pass = 50 * H + 30_000;
        e.advance_live(&single(&c[..50], None), &ext, last_pass, |_, _| {});
        assert!(e.states["A"].pending.is_some(), "ordre en attente à l'enregistrement");

        let mut saved = serde_json::to_value(&e).unwrap();
        let had = saved.as_object_mut().unwrap().remove("last_live_pass").is_some();
        let mut e: Engine = serde_json::from_value(saved).expect("l'ancien format se relit");
        e.record = true;
        let now = 52 * H + 20 * M;
        e.advance_live(&single(&c[..52], Some(&forming)), &ext, now, |_, _| {});
        for f in &e.portfolio.fills {
            assert!(!(f.time > last_pass && f.time < now), "exécution datée pendant la fermeture : {f:?}");
        }
        let trace = e.decisions.iter().any(|d| d.time == 50 * H && d.kind == DecisionKind::Skipped && d.reason.contains("ORDRE MANQUÉ"));
        assert!(trace, "l'ordre prévu à 50H est noté manqué : {:?}", e.decisions);
        assert_eq!(e.portfolio.fills.len(), 1, "un seul achat, au prix du moment");
        assert_eq!((e.portfolio.fills[0].time, e.portfolio.fills[0].price), (now, 120.0));
        assert!(had, "le dernier passage est bien enregistré dans le nouveau format");
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
            if let Some(c) = e.close_bar(s, std::slice::from_ref(&b), Signal { enter: *enter, exit: false }, 1.0, *st, true) {
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

    /// Mêmes prix, volumes multipliés par `factor` : seul le carnet change.
    fn scaled(c: &[Candle], factor: f64) -> Vec<Candle> {
        c.iter().map(|x| Candle { volume: x.volume * factor, ..*x }).collect()
    }

    /// Audit de réalisme du 2026-10-09 : sur Bitvavo, un ordre de 1 446 € coûte 0,1 pb sur
    /// BTC-EUR et 28,7 pb sur NPC-EUR (carnets relevés vers 08:30 UTC). Le simulateur
    /// facturait 2 pb partout. Un marché mince doit coûter plus cher qu'un marché profond.
    #[test]
    fn thin_market_entry_pays_more_than_liquid() {
        use crate::testutil::synthetic;
        let base = synthetic(400, 21);
        let first_buy = |factor: f64| {
            let series = scaled(&base, factor);
            let mut e = Engine::new(find("buy_hold").unwrap(), vec!["A".into()], 10_000.0, CostModel::default(), 0);
            let feeds: BTreeMap<String, Feed> = [("A".to_string(), Feed { closed: &series, forming: None })].into();
            e.advance(&feeds, &External::default(), |_, _| {});
            let f = e.portfolio.fills.first().expect("un achat").clone();
            let open = series.iter().find(|c| c.open_time == f.time).expect("ouverture d'exécution").open;
            (f.price, open)
        };
        let (thin, open_thin) = first_buy(0.001);
        let (deep, open_deep) = first_buy(1_000.0);
        assert_eq!(open_thin, open_deep, "mêmes prix des deux côtés");
        assert!(
            (deep - open_deep * (1.0 + 2.0 / 10_000.0)).abs() < 1e-9,
            "marché profond : le minimum de 2 pb, {deep}"
        );
        assert!(thin > deep, "marché mince : l'achat doit coûter plus cher ({thin} contre {deep})");
    }

    /// Jumeau côté vente : un stop touché sur un marché mince se vend moins bien.
    #[test]
    fn thin_market_stop_exit_sells_lower() {
        let hist: Vec<Candle> = (0..30).map(|i| candle(i, 100.0, 101.0, 99.0, 100.0)).collect();
        let exit_vs_stop = |factor: f64| {
            let mut e = stop_engine(5.0, None);
            e.costs = CostModel::default();
            let h = scaled(&hist, factor);
            e.close_bar("A", &h, Signal { enter: true, exit: false }, 1.0, f64::NAN, false);
            e.on_next_open("A", h.last().unwrap().close_time + 1, 100.0);
            let stop = e.portfolio.positions["A"].stop.expect("stop placé");
            let mut h2 = h.clone();
            h2.push(Candle { volume: h[0].volume, ..candle(30, 100.0, 100.0, 90.0, 92.0) });
            e.close_bar("A", &h2, Signal::default(), 1.0, f64::NAN, false);
            let t = e.portfolio.closed.first().expect("stop touché").clone();
            assert_eq!(t.exit_reason, "STOP");
            t.exit_price / stop
        };
        let thin = exit_vs_stop(0.001);
        let deep = exit_vs_stop(1_000.0);
        assert!((deep - (1.0 - 2.0 / 10_000.0)).abs() < 1e-9, "marché profond : le minimum de 2 pb, {deep}");
        assert!(thin < deep, "marché mince : le stop doit se vendre moins bien ({thin} contre {deep})");
    }

    /// Cas 3 : liquidité inconnue (historique de moins de 24 h, ordre enregistré avant le
    /// modèle) : le minimum de 2 pb, comme avant.
    #[test]
    fn unknown_liquidity_pays_the_floor() {
        let thin: Vec<Candle> =
            scaled(&(0..24).map(|i| candle(i, 100.0, 101.0, 99.0, 100.0)).collect::<Vec<_>>(), 0.001);
        assert_eq!(volume_24h_eur(&thin[..23], thin[22].close_time + 1), None, "23 h d'historique : inconnue");
        assert!(volume_24h_eur(&thin, thin[23].close_time + 1).is_some(), "jumeau : 24 h couvertes");
        let fill = |hist: &[Candle]| {
            let mut e = stop_engine(5.0, None);
            e.costs = CostModel::default();
            e.close_bar("A", hist, Signal { enter: true, exit: false }, 1.0, f64::NAN, false);
            e.on_next_open("A", hist.last().unwrap().close_time + 1, 100.0);
            e.portfolio.fills[0].price
        };
        assert!((fill(&thin[..23]) - 100.0 * (1.0 + 2.0 / 10_000.0)).abs() < 1e-9);
        assert!(fill(&thin) > 100.0 * (1.0 + 2.0 / 10_000.0), "jumeau : 24 h d'un marché mince coûtent plus");
        // Ordre en attente enregistré avant le modèle : relu sans liquidité.
        let old: Pending = serde_json::from_str(r#"{"kind":"Enter","reason":"SIGNAL D'ENTRÉE","atr":1.0}"#).unwrap();
        assert_eq!(old.volume_24h, None);
        let mut e = stop_engine(5.0, None);
        e.costs = CostModel::default();
        e.states.get_mut("A").unwrap().pending = Some(old);
        e.on_next_open("A", 0, 100.0);
        assert!((e.portfolio.fills[0].price - 100.0 * (1.0 + 2.0 / 10_000.0)).abs() < 1e-9);
    }

    /// Cas 4 et 6 : la fenêtre compte 24 h de TEMPS ; une bougie absente (aucun échange
    /// sur Bitvavo) ou au volume illisible n'ajoute rien.
    #[test]
    fn liquidity_window_is_24h_of_time_not_bar_count() {
        let full: Vec<Candle> = (0..49).map(|i| candle(i, 100.0, 100.0, 100.0, 100.0)).collect();
        let eur = |c: &Candle| c.volume * c.close;
        let end = full[48].close_time + 1;
        let expected: f64 = full[25..49].iter().map(eur).sum();
        assert_eq!(volume_24h_eur(&full, end), Some(expected), "24 bougies horaires");
        let gaps: Vec<Candle> =
            full.iter().filter(|c| !(30..40).contains(&(c.open_time / 3_600_000))).copied().collect();
        let with_gaps: f64 =
            full[25..49].iter().filter(|c| !(30..40).contains(&(c.open_time / 3_600_000))).map(eur).sum();
        assert_eq!(
            volume_24h_eur(&gaps, end),
            Some(with_gaps),
            "10 heures sans échange : 14 bougies, pas 24 plus anciennes"
        );
        let mut bad = full.clone();
        bad[47].volume = f64::NAN;
        bad[46].volume = -5.0;
        let without: f64 = full[25..46].iter().chain(&full[48..49]).map(eur).sum();
        assert_eq!(volume_24h_eur(&bad, end), Some(without), "volume illisible ou négatif : zéro");
        // Une bougie de 1 jour couvre seule sa fenêtre.
        let day = Candle { open_time: 0, close_time: crate::portfolio::DAY_MS - 1, ..full[0] };
        assert_eq!(volume_24h_eur(&[day], crate::portfolio::DAY_MS), Some(eur(&day)));
        assert_eq!(volume_24h_eur(&[], end), None, "aucun historique : inconnue");
    }

    /// R4 : un stop touché pendant une bougie ne lit pas le volume de cette bougie (encore
    /// inconnu à l'instant du stop), seulement celui des bougies déjà closes.
    #[test]
    fn in_bar_exit_liquidity_ignores_the_bar_in_progress() {
        let hist: Vec<Candle> =
            scaled(&(0..30).map(|i| candle(i, 100.0, 101.0, 99.0, 100.0)).collect::<Vec<_>>(), 0.001);
        let exit_price = |hist: &[Candle], stop_bar_volume: f64| {
            let mut e = stop_engine(5.0, None);
            e.costs = CostModel::default();
            e.close_bar("A", hist, Signal { enter: true, exit: false }, 1.0, f64::NAN, false);
            e.on_next_open("A", hist.last().unwrap().close_time + 1, 100.0);
            let stop = e.portfolio.positions["A"].stop.expect("stop placé");
            let mut h2 = hist.to_vec();
            h2.push(Candle { volume: stop_bar_volume, ..candle(30, 100.0, 100.0, 90.0, 92.0) });
            e.close_bar("A", &h2, Signal::default(), 1.0, f64::NAN, false);
            e.portfolio.closed[0].exit_price / stop
        };
        assert_eq!(exit_price(&hist, 1.0), exit_price(&hist, 1e9), "le volume de la bougie en cours ne compte pas");
        // Jumeau : le volume des bougies déjà closes, lui, compte.
        let mut busier = hist.clone();
        for c in busier.iter_mut().skip(10) {
            c.volume *= 1_000.0;
        }
        assert!(exit_price(&busier, 1.0) > exit_price(&hist, 1.0));
    }

    /// Cas 12 côté moteur : sans coûts, l'achat se fait au prix d'ouverture exact.
    #[test]
    fn zero_costs_fill_at_open_on_thin_market() {
        use crate::testutil::synthetic;
        let series = scaled(&synthetic(200, 21), 1e-6);
        let mut e = Engine::new(find("buy_hold").unwrap(), vec!["A".into()], 10_000.0, CostModel::zero(), 0);
        let feeds: BTreeMap<String, Feed> = [("A".to_string(), Feed { closed: &series, forming: None })].into();
        e.advance(&feeds, &External::default(), |_, _| {});
        let f = &e.portfolio.fills[0];
        let open = series.iter().find(|c| c.open_time == f.time).unwrap().open;
        assert_eq!(f.price, open);
    }

    // --- Paliers de frais Bitvavo (audit de réalisme du 2026-10-12) ---

    const TIER_H: i64 = 3_600_000;
    /// Minuit UTC du jour 100 depuis 1970.
    const TIER_D: i64 = 100 * crate::portfolio::DAY_MS;

    /// Capital de 200 000 € ; 120 000 € échangés le jour D à 02:00 et 03:00 par de vrais
    /// achat et vente sans frais (invariants tenus), sur une autre crypto.
    fn tier_engine_with_volume() -> Engine {
        let mut e = Engine::new(find("buy_hold").unwrap(), vec!["A".into()], 200_000.0, CostModel::default(), 0);
        let z = CostModel::zero();
        e.portfolio.buy("B", 60_000.0, 100.0, TIER_D + 2 * TIER_H, &z, "volume").unwrap();
        e.portfolio.sell_all("B", 100.0, TIER_D + 3 * TIER_H, &z, "volume").unwrap();
        e
    }

    fn tier_order(e: &mut Engine, kind: PendingKind, time: i64) -> f64 {
        let n = e.portfolio.fills.len();
        let reason = "test".to_string();
        e.states.get_mut("A").expect("état").pending = Some(Pending { kind, reason, atr: 1.0, volume_24h: None });
        e.on_next_open("A", time, 100.0);
        assert_eq!(e.portfolio.fills.len(), n + 1, "l'ordre est exécuté");
        e.portfolio.check_invariants().unwrap();
        let f = e.portfolio.fills.last().unwrap();
        match f.side {
            crate::portfolio::Side::Buy => f.fee / -f.cash_delta,
            crate::portfolio::Side::Sell => f.fee / (f.qty * f.price),
        }
    }

    /// Cas C1 (T1) : 120 000 € dans les 30 jours du recalcul de 01:00 → 0,20 % sur l'entrée
    /// et sur la sortie de 01:30 ; 0,25 % à 00:30 le même jour (recalcul de la veille, avant
    /// ce volume).
    #[test]
    fn tier_applies_after_01h_recalc() {
        let mut e = tier_engine_with_volume();
        let mut before = e.clone();
        let day1 = TIER_D + crate::portfolio::DAY_MS;
        let r = tier_order(&mut e, PendingKind::Enter, day1 + TIER_H + 30 * 60_000);
        assert!((r - 0.0020).abs() < 1e-12, "entrée à 01:30 : {r}");
        let r = tier_order(&mut e, PendingKind::Exit, day1 + TIER_H + 45 * 60_000);
        assert!((r - 0.0020).abs() < 1e-12, "sortie à 01:45 : {r}");
        let r = tier_order(&mut before, PendingKind::Enter, day1 + 30 * 60_000);
        assert!((r - 0.0025).abs() < 1e-12, "jumeau, entrée à 00:30 : {r}");
    }

    /// Cas C14 (T12) : après plusieurs nuits de fermeture, l'ordre prend le recalcul de 01:00
    /// qui le précède, sans traitement particulier ; 30 jours plus tard, le volume est sorti.
    #[test]
    fn tier_after_several_closed_nights() {
        let mut e = tier_engine_with_volume();
        let mut late = e.clone();
        let day = crate::portfolio::DAY_MS;
        let r = tier_order(&mut e, PendingKind::Enter, TIER_D + 3 * day + TIER_H + 30 * 60_000);
        assert!((r - 0.0020).abs() < 1e-12, "trois nuits plus tard : {r}");
        let r = tier_order(&mut late, PendingKind::Enter, TIER_D + 31 * day + TIER_H + 30 * 60_000);
        assert!((r - 0.0025).abs() < 1e-12, "jumeau, 31 nuits plus tard : {r}");
    }

    /// Cas C2 (T8) : un compte sans aucune exécution paie le taux de base.
    #[test]
    fn tier_empty_book_pays_base_rate() {
        let mut e = Engine::new(find("buy_hold").unwrap(), vec!["A".into()], 200_000.0, CostModel::default(), 0);
        let r = tier_order(&mut e, PendingKind::Enter, TIER_D + TIER_H + 30 * 60_000);
        assert!((r - 0.0025).abs() < 1e-12, "{r}");
    }
}
