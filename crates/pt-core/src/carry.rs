//! Portage du financement des contrats perpétuels (« funding carry »).
//!
//! On achète un symbole au comptant et on vend en même quantité son contrat
//! perpétuel : la variation du prix s'annule entre les deux jambes. Il reste le
//! financement, versé toutes les 8 h par les acheteurs de perpétuels aux vendeurs
//! quand le taux est positif. C'est une prime documentée, pas un pari sur la
//! direction du prix (Schmeling, Schrimpf & Todorov, « Crypto carry », BIS
//! Working Paper 1087, 2023).
//!
//! Modèle volontairement prudent :
//! - chaque symbole reçoit une poche égale ; la moitié est investie au comptant,
//!   l'autre moitié sert de marge au perpétuel (levier 1×) ;
//! - frais : le comptant paie le taux des réglages, le perpétuel 0,05 % par côté
//!   (preneur, utilisateur standard Binance :
//!   <https://www.binance.com/fr/support/articles/360033544231>) ; glissement sur
//!   chaque jambe ;
//! - quand la hausse ronge la marge du perpétuel sous `rebalance_below` de son
//!   notionnel, on revend du comptant pour la reconstituer, frais payés ;
//! - si une bougie monte assez pour épuiser la marge, la jambe perpétuelle est
//!   liquidée (marge perdue) et la poche revend son comptant ;
//! - l'écart entre le prix du perpétuel et le comptant (la « base ») n'est pas
//!   modélisé : les deux jambes sont valorisées au prix comptant ;
//! - la trésorerie ne rapporte aucun intérêt, comme pour toutes les stratégies.
//!
//! Comme pour `Engine` : décision à la clôture d'une bougie, exécution à
//! l'ouverture de la suivante, et le même code sert au backtest et au mode direct.

use crate::candle::Candle;
use crate::portfolio::CostModel;
use crate::validation::{judge_rolling, median, worst_sign_test, Outcome, Robustness, SignTest, ValidationError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 86_400_000;
/// Marge de maintien sous laquelle la jambe perpétuelle est liquidée (part du notionnel).
const MAINTENANCE: f64 = 0.005;
/// Écart (en points de %) sous lequel une fenêtre est déclarée nulle.
const TIE_EPSILON_PCT: f64 = 1e-6;

/// Un taux de financement réglé : `rate` est versé par les acheteurs aux vendeurs
/// (négatif : l'inverse), en fraction du notionnel, à l'instant `time`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FundingEvent {
    pub time: i64,
    pub rate: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CarryEntry {
    /// Couverture tenue en permanence.
    Always,
    /// Couverture tenue tant que la moyenne des `settlements` derniers taux réglés
    /// est positive ; en liquide sinon.
    TrailingMean { settlements: usize },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CarryPreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub entry: CarryEntry,
    /// Frais du perpétuel par côté, en fraction (0.0005 = 0,05 %).
    pub perp_fee_rate: f64,
    /// Reconstitue la marge quand elle tombe sous cette part du notionnel.
    pub rebalance_below: f64,
}

impl CarryPreset {
    fn warmup_settlements(&self) -> usize {
        match self.entry {
            CarryEntry::Always => 1,
            CarryEntry::TrailingMean { settlements } => settlements.max(1),
        }
    }
}

/// Les stratégies de portage. Toute entrée ici doit figurer au registre des essais.
pub fn carry_catalog() -> Vec<CarryPreset> {
    vec![
        CarryPreset {
            id: "portage_toujours".into(),
            name: "Portage du financement, permanent".into(),
            description: "Achète chaque symbole au comptant et vend la même quantité en perpétuel, en permanence : le prix s'annule, le financement versé toutes les 8 h reste. La moitié de chaque poche sert de marge (levier 1×).".into(),
            entry: CarryEntry::Always,
            perp_fee_rate: 0.0005,
            rebalance_below: 0.5,
        },
        CarryPreset {
            id: "portage_filtre_7j".into(),
            name: "Portage du financement, filtré sur 7 jours".into(),
            description: "Même couverture, tenue seulement tant que la moyenne des 21 derniers taux réglés (7 jours) est positive ; en liquide sinon.".into(),
            entry: CarryEntry::TrailingMean { settlements: 21 },
            perp_fee_rate: 0.0005,
            rebalance_below: 0.5,
        },
    ]
}

pub fn find_carry(id: &str) -> Option<CarryPreset> {
    carry_catalog().into_iter().find(|p| p.id == id)
}

/// Empreinte au registre des essais : les réglages exacts, sans nom ni description.
pub fn carry_fingerprint(p: &CarryPreset) -> String {
    #[derive(Serialize)]
    struct Settings {
        r#type: &'static str,
        entry: CarryEntry,
        perp_fee_rate: f64,
        rebalance_below: f64,
    }
    serde_json::to_string(&Settings {
        r#type: "portage",
        entry: p.entry,
        perp_fee_rate: p.perp_fee_rate,
        rebalance_below: p.rebalance_below,
    })
    .expect("réglages sérialisables")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CarryAction {
    Open,
    Close,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CarryEvent {
    pub time: i64,
    pub symbol: String,
    /// OUVERTURE, FERMETURE, RÉÉQUILIBRAGE ou LIQUIDATION.
    pub kind: String,
    pub price: f64,
    /// Quantité couverte après l'événement.
    pub qty: f64,
    pub fees: f64,
    pub equity_after: f64,
}

/// Une poche : un symbole, sa jambe au comptant et sa jambe perpétuelle.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Pocket {
    pub cash: f64,
    /// Quantité détenue au comptant = quantité vendue en perpétuel.
    pub qty: f64,
    /// Prix de vente du perpétuel (référence de son gain ou de sa perte).
    pub perp_entry: f64,
    /// Marge du perpétuel, financements compris.
    pub margin: f64,
    pub funding_received: f64,
    pub fees_paid: f64,
    pub trades: usize,
    pub last_hour: Option<i64>,
    pub last_funding: Option<i64>,
    /// Derniers taux réglés (au plus le nombre exigé par le filtre).
    pub recent_rates: Vec<f64>,
    pub pending: Option<CarryAction>,
}

impl Pocket {
    pub fn is_open(&self) -> bool {
        self.qty > 0.0
    }

    pub fn equity(&self, price: f64) -> f64 {
        self.cash + self.qty * price + self.perp_equity(price)
    }

    fn perp_equity(&self, price: f64) -> f64 {
        if self.is_open() {
            self.margin + self.qty * (self.perp_entry - price)
        } else {
            0.0
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CarryBook {
    pub preset: CarryPreset,
    pub costs: CostModel,
    pub symbols: Vec<String>,
    pub initial_cash: f64,
    /// Aucune exécution avant cet instant (le préchauffage peut le précéder).
    pub trade_from: i64,
    pub pockets: BTreeMap<String, Pocket>,
    /// Dernier prix connu de chaque symbole.
    pub marks: BTreeMap<String, f64>,
    pub events: Vec<CarryEvent>,
}

/// Données d'un symbole pour un passage : bougies 1h clôturées, bougie en cours,
/// et taux de financement réglés.
pub struct CarryFeed<'a> {
    pub closed: &'a [Candle],
    pub forming: Option<&'a Candle>,
    pub funding: &'a [FundingEvent],
}

impl CarryBook {
    pub fn new(
        preset: CarryPreset,
        symbols: Vec<String>,
        initial_cash: f64,
        costs: CostModel,
        trade_from: i64,
    ) -> Self {
        let share = initial_cash / symbols.len().max(1) as f64;
        let pockets = symbols.iter().map(|s| (s.clone(), Pocket { cash: share, ..Pocket::default() })).collect();
        CarryBook {
            preset,
            costs,
            symbols,
            initial_cash,
            trade_from,
            pockets,
            marks: BTreeMap::new(),
            events: Vec::new(),
        }
    }

    pub fn equity(&self) -> f64 {
        self.pockets
            .iter()
            .map(|(s, p)| match self.marks.get(s) {
                Some(&m) => p.equity(m),
                None => p.cash,
            })
            .sum()
    }

    pub fn funding_received(&self) -> f64 {
        self.pockets.values().map(|p| p.funding_received).sum()
    }

    pub fn fees_paid(&self) -> f64 {
        self.pockets.values().map(|p| p.fees_paid).sum()
    }

    pub fn trades(&self) -> usize {
        self.pockets.values().map(|p| p.trades).sum()
    }

    /// Part du capital couverte (au comptant), entre 0 et 1.
    pub fn hedged_fraction(&self) -> f64 {
        let eq = self.equity();
        if eq <= 0.0 {
            return 0.0;
        }
        let spot: f64 = self.pockets.iter().map(|(s, p)| p.qty * self.marks.get(s).copied().unwrap_or(0.0)).sum();
        spot / eq
    }

    /// Fait avancer chaque poche sur les bougies non encore traitées. Rend le nombre
    /// de bougies traitées. `on_hour(t, book)` est appelé après chaque heure, avec
    /// `t` = fin de l'heure. Rejouer les mêmes données ne change rien.
    pub fn advance(&mut self, feeds: &BTreeMap<String, CarryFeed>, mut on_hour: impl FnMut(i64, &CarryBook)) -> usize {
        let mut index: BTreeMap<&String, BTreeMap<i64, usize>> = BTreeMap::new();
        let mut times = BTreeSet::new();
        for (sym, feed) in feeds {
            let Some(pocket) = self.pockets.get(sym) else { continue };
            let start = pocket.last_hour.map_or(0, |t| feed.closed.partition_point(|c| c.open_time <= t));
            let by_time: BTreeMap<i64, usize> =
                feed.closed.iter().enumerate().skip(start).map(|(i, c)| (c.open_time, i)).collect();
            times.extend(by_time.keys().copied());
            index.insert(sym, by_time);
        }
        let mut processed = 0;
        for t in times {
            let mut done = Vec::new();
            for (sym, by_time) in &index {
                if let Some(&i) = by_time.get(&t) {
                    let feed = &feeds[*sym];
                    self.on_candle(sym, &feed.closed[i], feed.funding);
                    done.push((*sym, i));
                    processed += 1;
                }
            }
            on_hour(t + HOUR_MS, self);
            for (sym, i) in done {
                let feed = &feeds[sym];
                if let Some(next) = feed.closed.get(i + 1).or(feed.forming) {
                    self.execute_pending(sym, next.open_time, next.open);
                }
            }
        }
        for (sym, feed) in feeds {
            if let Some(f) = feed.forming {
                if self.pockets.contains_key(sym) {
                    self.marks.insert(sym.clone(), f.close);
                }
            }
        }
        processed
    }

    fn log(&mut self, time: i64, symbol: &str, kind: &str, price: f64, fees: f64) {
        let p = &self.pockets[symbol];
        let ev = CarryEvent {
            time,
            symbol: symbol.to_string(),
            kind: kind.to_string(),
            price,
            qty: p.qty,
            fees,
            equity_after: p.equity(price),
        };
        self.events.push(ev);
    }

    /// La bougie `c` de `symbol` vient de se clôturer.
    fn on_candle(&mut self, symbol: &str, c: &Candle, funding: &[FundingEvent]) {
        let preset = self.preset.clone();
        let costs = self.costs;
        let pocket = self.pockets.get_mut(symbol).expect("poche");
        if pocket.last_hour.is_some_and(|t| c.open_time <= t) {
            return;
        }
        let mut log: Vec<(&str, f64, f64)> = Vec::new();

        // 1. Liquidation si le plus haut de la bougie a épuisé la marge du perpétuel.
        if pocket.is_open() && pocket.perp_equity(c.high) <= MAINTENANCE * pocket.qty * c.high {
            pocket.margin = 0.0;
            pocket.perp_entry = 0.0;
            let slip = costs.slippage_bps / 10_000.0;
            let proceeds = pocket.qty * c.close * (1.0 - slip) * (1.0 - costs.fee_rate);
            let fees = pocket.qty * c.close - proceeds;
            pocket.cash += proceeds;
            pocket.fees_paid += fees;
            pocket.qty = 0.0;
            pocket.trades += 1;
            pocket.pending = None;
            log.push(("LIQUIDATION", c.close, fees));
        }

        // 2. Financements réglés jusqu'à la fin de cette bougie, au prix de clôture.
        let from = pocket.last_funding.map_or(0, |t| funding.partition_point(|f| f.time <= t));
        let until = funding.partition_point(|f| f.time <= c.close_time + 1);
        for f in &funding[from.min(until)..until] {
            if pocket.is_open() {
                let amount = pocket.qty * c.close * f.rate;
                pocket.margin += amount;
                pocket.funding_received += amount;
            }
            pocket.recent_rates.push(f.rate);
            let keep = preset.warmup_settlements();
            if pocket.recent_rates.len() > keep {
                let excess = pocket.recent_rates.len() - keep;
                pocket.recent_rates.drain(..excess);
            }
            pocket.last_funding = Some(f.time);
        }

        // 3. Rééquilibrage si la hausse a rongé la marge.
        if pocket.is_open() && pocket.perp_equity(c.close) < preset.rebalance_below * pocket.qty * c.close {
            let slip = costs.slippage_bps / 10_000.0;
            let total = pocket.qty * c.close + pocket.perp_equity(c.close);
            let target = total / (2.0 * c.close);
            let traded = (pocket.qty - target).max(0.0);
            let fees = traded * c.close * (costs.fee_rate + preset.perp_fee_rate + 2.0 * slip);
            pocket.qty = target;
            pocket.perp_entry = c.close;
            pocket.margin = total - target * c.close - fees;
            pocket.fees_paid += fees;
            pocket.trades += 1;
            log.push(("RÉÉQUILIBRAGE", c.close, fees));
        }

        // 4. Décision, exécutée à l'ouverture suivante.
        let want_open = match preset.entry {
            CarryEntry::Always => true,
            CarryEntry::TrailingMean { settlements } => {
                pocket.recent_rates.len() >= settlements
                    && pocket.recent_rates.iter().sum::<f64>() / pocket.recent_rates.len() as f64 > 0.0
            }
        };
        pocket.pending = match (want_open, pocket.is_open()) {
            (true, false) if c.open_time + HOUR_MS >= self.trade_from && pocket.cash > 1.0 => Some(CarryAction::Open),
            (false, true) => Some(CarryAction::Close),
            _ => None,
        };
        pocket.last_hour = Some(c.open_time);
        self.marks.insert(symbol.to_string(), c.close);
        for (kind, price, fees) in log {
            self.log(c.close_time + 1, symbol, kind, price, fees);
        }
    }

    /// Exécute la décision prise à la clôture précédente, au prix d'ouverture.
    fn execute_pending(&mut self, symbol: &str, time: i64, price: f64) {
        let preset = self.preset.clone();
        let costs = self.costs;
        let slip = costs.slippage_bps / 10_000.0;
        let pocket = self.pockets.get_mut(symbol).expect("poche");
        self.marks.insert(symbol.to_string(), price);
        let Some(action) = pocket.pending.take() else { return };
        if time < self.trade_from {
            return;
        }
        let (kind, fees) = match action {
            CarryAction::Open => {
                if pocket.is_open() {
                    return;
                }
                let e = pocket.cash;
                let spot_px = price * (1.0 + slip);
                let perp_px = price * (1.0 - slip);
                // Moitié au comptant, moitié en marge, frais déduits.
                let q = e / (spot_px * (1.0 + costs.fee_rate) + perp_px * preset.perp_fee_rate + price);
                let spot_cost = q * spot_px * (1.0 + costs.fee_rate);
                let perp_fee = q * perp_px * preset.perp_fee_rate;
                pocket.qty = q;
                pocket.perp_entry = perp_px;
                pocket.margin = e - spot_cost - perp_fee;
                pocket.cash = 0.0;
                let fees = (spot_cost - q * price) + perp_fee + q * (price - perp_px);
                ("OUVERTURE", fees)
            }
            CarryAction::Close => {
                if !pocket.is_open() {
                    return;
                }
                let q = pocket.qty;
                let spot = q * price * (1.0 - slip) * (1.0 - costs.fee_rate);
                let perp_px = price * (1.0 + slip);
                let perp_fee = q * perp_px * preset.perp_fee_rate;
                let perp_pnl = q * (pocket.perp_entry - perp_px);
                pocket.cash += spot + pocket.margin + perp_pnl - perp_fee;
                pocket.qty = 0.0;
                pocket.margin = 0.0;
                pocket.perp_entry = 0.0;
                let fees = (q * price - spot) + perp_fee + q * (perp_px - price);
                ("FERMETURE", fees)
            }
        };
        pocket.fees_paid += fees;
        pocket.trades += 1;
        self.log(time, symbol, kind, price, fees);
    }
}

// ---------------------------------------------------------------------------
// Backtest et validation sur fenêtres glissantes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CarryWindow {
    pub start: i64,
    pub end: i64,
    pub symbols: Vec<String>,
    pub return_pct: f64,
    /// « Acheter et garder » sur les mêmes symboles, pour information : le portage
    /// n'est pas exposé au prix, sa référence à exposition égale vaut 0 %.
    pub benchmark_return_pct: f64,
    pub funding_pct: f64,
    pub fees_pct: f64,
    pub max_drawdown_pct: f64,
    pub benchmark_max_drawdown_pct: f64,
    /// Part moyenne du capital couverte.
    pub hedged_pct: f64,
    pub trades: usize,
    pub liquidations: usize,
    pub outcome: Outcome,
}

fn max_drawdown_pct(curve: &[f64]) -> f64 {
    let mut peak = f64::MIN;
    let mut worst: f64 = 0.0;
    for &v in curve {
        peak = peak.max(v);
        if peak > 0.0 {
            worst = worst.max((peak - v) / peak);
        }
    }
    worst * 100.0
}

/// Bougies `[from, end)` d'un symbole.
fn slice(c: &[Candle], from: i64, end: i64) -> &[Candle] {
    let a = c.partition_point(|x| x.open_time < from);
    let b = c.partition_point(|x| x.open_time < end);
    &c[a..b]
}

/// Rejoue `preset` sur `[start, end)`. Un symbole entre dans la fenêtre si son
/// perpétuel a déjà réglé assez de taux avant `start` et s'il a un prix au départ.
pub fn run_carry_window(
    preset: &CarryPreset,
    prices: &BTreeMap<String, Vec<Candle>>,
    funding: &BTreeMap<String, Vec<FundingEvent>>,
    costs: CostModel,
    initial_cash: f64,
    start: i64,
    end: i64,
) -> Option<(CarryWindow, CarryBook)> {
    let need = preset.warmup_settlements();
    let symbols: Vec<String> = prices
        .iter()
        .filter(|(s, c)| {
            let ready = funding.get(*s).is_some_and(|f| f.partition_point(|x| x.time < start) >= need);
            let priced =
                c.first().is_some_and(|x| x.open_time <= start) && c.last().is_some_and(|x| x.open_time >= start);
            ready && priced
        })
        .map(|(s, _)| s.clone())
        .collect();
    if symbols.is_empty() {
        return None;
    }
    let sliced: BTreeMap<String, &[Candle]> =
        symbols.iter().map(|s| (s.clone(), slice(&prices[s], start, end))).collect();
    // Le financement d'avant la fenêtre ne sert qu'à préchauffer le filtre.
    let fund: BTreeMap<String, &[FundingEvent]> = symbols
        .iter()
        .map(|s| {
            let f = &funding[s];
            (s.clone(), &f[..f.partition_point(|x| x.time < end)])
        })
        .collect();
    let feeds: BTreeMap<String, CarryFeed> =
        symbols.iter().map(|s| (s.clone(), CarryFeed { closed: sliced[s], forming: None, funding: fund[s] })).collect();
    let mut book = CarryBook::new(preset.clone(), symbols.clone(), initial_cash, costs, start);
    let mut curve = vec![initial_cash];
    let mut hedged = Vec::new();
    book.advance(&feeds, |_, b| {
        curve.push(b.equity());
        hedged.push(b.hedged_fraction());
    });
    let final_equity = book.equity();
    // Référence : parts égales achetées à la première ouverture, valorisées en continu.
    let mut bh_curve = Vec::new();
    let firsts: BTreeMap<&String, f64> = sliced.iter().filter_map(|(s, c)| c.first().map(|x| (s, x.open))).collect();
    let share = initial_cash / symbols.len() as f64;
    let times: BTreeSet<i64> = sliced.values().flat_map(|c| c.iter().map(|x| x.open_time)).collect();
    let mut last: BTreeMap<&String, f64> = firsts.clone();
    let lookup: BTreeMap<&String, BTreeMap<i64, f64>> =
        sliced.iter().map(|(s, c)| (s, c.iter().map(|x| (x.open_time, x.close)).collect())).collect();
    for t in times {
        for (s, m) in &lookup {
            if let Some(&px) = m.get(&t) {
                last.insert(*s, px);
            }
        }
        bh_curve.push(last.iter().map(|(s, px)| share * px / firsts[s]).sum::<f64>());
    }
    let bh_final = bh_curve.last().copied().unwrap_or(initial_cash);
    let return_pct = (final_equity / initial_cash - 1.0) * 100.0;
    let outcome = if return_pct.abs() <= TIE_EPSILON_PCT {
        Outcome::Tie
    } else if return_pct > 0.0 {
        Outcome::Win
    } else {
        Outcome::Loss
    };
    let liquidations = book.events.iter().filter(|e| e.kind == "LIQUIDATION").count();
    let hedged_pct = if hedged.is_empty() { 0.0 } else { hedged.iter().sum::<f64>() / hedged.len() as f64 * 100.0 };
    let w = CarryWindow {
        start,
        end,
        symbols,
        return_pct,
        benchmark_return_pct: (bh_final / initial_cash - 1.0) * 100.0,
        funding_pct: book.funding_received() / initial_cash * 100.0,
        fees_pct: book.fees_paid() / initial_cash * 100.0,
        max_drawdown_pct: max_drawdown_pct(&curve),
        benchmark_max_drawdown_pct: max_drawdown_pct(&bh_curve),
        hedged_pct,
        trades: book.trades(),
        liquidations,
        outcome,
    };
    Some((w, book))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CarryReport {
    pub preset_id: String,
    pub preset_name: String,
    pub window_days: i64,
    pub step_days: i64,
    pub tested_strategies: usize,
    pub costs: CostModel,
    pub windows: Vec<CarryWindow>,
    pub stride: usize,
    pub sign_test: SignTest,
    pub p_adjusted: f64,
    pub median_return_pct: f64,
    pub worst_return_pct: f64,
    /// Rendement annualisé, fenêtres indépendantes mises bout à bout.
    pub annualized_pct: f64,
    pub verdict: Robustness,
    pub verdict_reason: String,
}

/// Validation du portage : chaque fenêtre doit finir en gain, puisque sa référence
/// à exposition égale (aucune exposition au prix) vaut 0 %. Même test du signe,
/// même découpage le moins favorable et même correction que pour les autres stratégies.
#[allow(clippy::too_many_arguments)]
pub fn carry_validation(
    preset: &CarryPreset,
    prices: &BTreeMap<String, Vec<Candle>>,
    funding: &BTreeMap<String, Vec<FundingEvent>>,
    costs: CostModel,
    initial_cash: f64,
    window_days: i64,
    step_days: i64,
    tested_strategies: usize,
) -> Result<CarryReport, ValidationError> {
    if window_days < 30 || step_days < 1 || step_days > window_days {
        return Err(ValidationError::InvalidConfig(
            "la fenêtre doit durer au moins 30 jours, et le pas être compris entre 1 jour et la durée d'une fenêtre"
                .into(),
        ));
    }
    let need = preset.warmup_settlements();
    let first = funding
        .iter()
        .filter(|(s, f)| prices.contains_key(*s) && f.len() >= need)
        .map(|(s, f)| {
            let ready = f[need - 1].time + 1;
            let priced = prices[s].first().map_or(i64::MAX, |c| c.open_time);
            ready.max(priced)
        })
        .min();
    let last = prices.values().filter_map(|c| c.last()).map(|c| c.close_time + 1).max();
    let (Some(first), Some(last)) = (first, last) else {
        return Err(ValidationError::NoWindow(window_days));
    };
    // Fenêtres alignées sur le jour, comme les bougies journalières des autres validations.
    let mut start = (first + DAY_MS - 1) / DAY_MS * DAY_MS;
    let (len, step) = (window_days * DAY_MS, step_days * DAY_MS);
    let mut windows = Vec::new();
    while start + len <= last {
        if let Some((w, _)) = run_carry_window(preset, prices, funding, costs, initial_cash, start, start + len) {
            windows.push(w);
        }
        start += step;
    }
    if windows.is_empty() {
        return Err(ValidationError::NoWindow(window_days));
    }
    let stride = ((window_days + step_days - 1) / step_days) as usize;
    let outcomes: Vec<Outcome> = windows.iter().map(|w| w.outcome).collect();
    let test = worst_sign_test(&outcomes, stride);
    let (verdict, p_adjusted, verdict_reason) = judge_rolling(&test, tested_strategies);
    let chained: f64 = windows.iter().skip(test.phase).step_by(stride).map(|w| 1.0 + w.return_pct / 100.0).product();
    let years = windows.iter().skip(test.phase).step_by(stride).count() as f64 * window_days as f64 / 365.25;
    let annualized_pct = if years > 0.0 { (chained.powf(1.0 / years) - 1.0) * 100.0 } else { 0.0 };
    let mut returns: Vec<f64> = windows.iter().map(|w| w.return_pct).collect();
    Ok(CarryReport {
        preset_id: preset.id.clone(),
        preset_name: preset.name.clone(),
        window_days,
        step_days,
        tested_strategies,
        costs,
        stride,
        sign_test: test,
        p_adjusted,
        median_return_pct: median(&mut returns),
        worst_return_pct: windows.iter().map(|w| w.return_pct).fold(f64::INFINITY, f64::min),
        annualized_pct,
        verdict,
        verdict_reason,
        windows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hourly(prices: &[f64], t0: i64) -> Vec<Candle> {
        prices
            .iter()
            .enumerate()
            .map(|(i, &p)| Candle {
                open_time: t0 + i as i64 * HOUR_MS,
                open: p,
                high: p,
                low: p,
                close: p,
                volume: 1.0,
                close_time: t0 + (i as i64 + 1) * HOUR_MS - 1,
            })
            .collect()
    }

    /// Un taux toutes les 8 h, du début de `candles` à leur fin (et `before` taux d'avance).
    fn every_8h(candles: &[Candle], rate: impl Fn(usize) -> f64, before: usize) -> Vec<FundingEvent> {
        let t0 = candles[0].open_time - before as i64 * 8 * HOUR_MS;
        let end = candles.last().unwrap().close_time + 1;
        (0..)
            .map(|k| t0 + k * 8 * HOUR_MS)
            .take_while(|t| *t <= end)
            .enumerate()
            .map(|(k, t)| FundingEvent { time: t, rate: rate(k) })
            .collect()
    }

    fn run_all(p: &CarryPreset, c: &[Candle], f: &[FundingEvent], costs: CostModel) -> CarryBook {
        let mut book = CarryBook::new(p.clone(), vec!["X".into()], 10_000.0, costs, c[0].open_time);
        let feeds = BTreeMap::from([("X".to_string(), CarryFeed { closed: c, forming: None, funding: f })]);
        book.advance(&feeds, |_, _| {});
        book
    }

    fn always(perp_fee: f64) -> CarryPreset {
        CarryPreset { perp_fee_rate: perp_fee, ..find_carry("portage_toujours").unwrap() }
    }

    /// Sans frais ni financement, la valeur ne bouge pas, même si le prix triple
    /// puis s'effondre : les deux jambes s'annulent (rééquilibrages compris).
    #[test]
    fn carry_is_price_neutral() {
        let mut px = Vec::new();
        for i in 0..600 {
            px.push(100.0 * (1.0 + 2.0 * (i as f64 / 600.0)));
        }
        for i in 0..400 {
            px.push(300.0 * (1.0 - 0.8 * (i as f64 / 400.0)));
        }
        let c = hourly(&px, 0);
        let f = every_8h(&c, |_| 0.0, 1);
        let book = run_all(&always(0.0), &c, &f, CostModel::zero());
        assert!(
            book.events.iter().any(|e| e.kind == "RÉÉQUILIBRAGE"),
            "le triplement doit déclencher un rééquilibrage"
        );
        assert!(!book.events.iter().any(|e| e.kind == "LIQUIDATION"));
        assert!((book.equity() - 10_000.0).abs() < 1e-6, "valeur {}", book.equity());
    }

    /// Un taux constant `r` rapporte r × notionnel couvert (la moitié du capital) à chaque règlement.
    #[test]
    fn carry_collects_funding_on_half_the_capital() {
        let c = hourly(&[100.0; 24 * 30], 0);
        let f = every_8h(&c, |_| 0.0001, 1);
        let book = run_all(&always(0.0), &c, &f, CostModel::zero());
        // Ouverture à l'ouverture de la 2e bougie : 89 règlements sur 30 jours (le premier est manqué).
        let settled = f.iter().filter(|e| e.time > HOUR_MS).count() as f64;
        let expected = 10_000.0 * (1.0 + 0.5 * 0.0001 * settled);
        assert!((book.equity() - expected).abs() < 0.05, "{} vs {expected}", book.equity());
        assert!((book.funding_received() - (expected - 10_000.0)).abs() < 0.05);
    }

    /// Les frais sont payés à l'ouverture et à la fermeture, sur les deux jambes.
    #[test]
    fn carry_pays_both_legs_fees() {
        let c = hourly(&[100.0; 24 * 10], 0);
        // Taux positifs pendant 3 jours puis négatifs : le filtre ouvre puis ferme.
        let f = every_8h(&c, |k| if k < 30 { 0.0002 } else { -0.0004 }, 21);
        let mut p = find_carry("portage_filtre_7j").unwrap();
        p.perp_fee_rate = 0.0005;
        let book = run_all(&p, &c, &f, CostModel::default());
        let kinds: Vec<&str> = book.events.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds, ["OUVERTURE", "FERMETURE"], "{kinds:?}");
        // Comptant 0,1 % + perpétuel 0,05 % + glissement 0,02 % par jambe, sur la moitié du capital, deux fois.
        let fees = book.fees_paid();
        let expected = 2.0 * 5_000.0 * (0.001 + 0.0005 + 2.0 * 0.0002);
        assert!((fees - expected).abs() / expected < 0.01, "frais {fees} vs {expected}");
        assert!(!book.pockets["X"].is_open());
    }

    /// Le filtre ne regarde que des taux déjà réglés : couper les données après un
    /// instant ne change aucune décision prise avant.
    #[test]
    fn carry_filter_never_uses_future_rates() {
        let px: Vec<f64> = (0..24 * 40).map(|i| 100.0 + ((i * 37) % 23) as f64).collect();
        let c = hourly(&px, 0);
        let f = every_8h(&c, |k| ((k * 13 % 7) as f64 - 3.0) * 0.0001, 21);
        let p = find_carry("portage_filtre_7j").unwrap();
        let full = run_all(&p, &c, &f, CostModel::default());
        for cut in [300usize, 555, 800] {
            let end = c[cut].close_time + 1;
            let part_f: Vec<FundingEvent> = f.iter().copied().filter(|e| e.time <= end).collect();
            let part = run_all(&p, &c[..=cut], &part_f, CostModel::default());
            // Ce qui s'exécute à `end` dépend de la bougie suivante, absente de la version coupée.
            let before: Vec<&CarryEvent> = full.events.iter().filter(|e| e.time < end).collect();
            let got: Vec<&CarryEvent> = part.events.iter().filter(|e| e.time < end).collect();
            assert_eq!(before, got, "décisions différentes avec des données coupées à {cut}");
        }
    }

    /// Avancer heure par heure (mode direct) donne exactement le résultat d'un seul passage (backtest).
    #[test]
    fn carry_live_increments_equal_one_shot() {
        let px: Vec<f64> = (0..24 * 20).map(|i| 100.0 * (1.0 + 0.002 * ((i % 50) as f64 - 25.0))).collect();
        let c = hourly(&px, 0);
        let f = every_8h(&c, |k| if k % 9 < 6 { 0.0001 } else { -0.0002 }, 21);
        let p = find_carry("portage_filtre_7j").unwrap();
        let one = run_all(&p, &c, &f, CostModel::default());
        let mut inc = CarryBook::new(p.clone(), vec!["X".into()], 10_000.0, CostModel::default(), c[0].open_time);
        for k in 1..c.len() {
            let end = c[k - 1].close_time + 1;
            let fs: Vec<FundingEvent> = f.iter().copied().filter(|e| e.time <= end).collect();
            let feeds =
                BTreeMap::from([("X".to_string(), CarryFeed { closed: &c[..k], forming: Some(&c[k]), funding: &fs })]);
            inc.advance(&feeds, |_, _| {});
        }
        let feeds = BTreeMap::from([("X".to_string(), CarryFeed { closed: &c, forming: None, funding: &f })]);
        inc.advance(&feeds, |_, _| {});
        assert_eq!(one.events, inc.events);
        assert!((one.equity() - inc.equity()).abs() < 1e-9);
    }

    /// Une bougie qui fait plus que doubler épuise la marge : la jambe perpétuelle est liquidée.
    #[test]
    fn a_spike_beyond_the_margin_liquidates() {
        let mut c = hourly(&[100.0, 100.0, 100.0, 100.0], 0);
        c[2].high = 250.0;
        let f = every_8h(&c, |_| 0.0, 1);
        let book = run_all(&always(0.0), &c, &f, CostModel::zero());
        let kinds: Vec<&str> = book.events.iter().map(|e| e.kind.as_str()).collect();
        // La variante permanente rouvre une couverture neuve à l'ouverture suivante.
        assert_eq!(kinds, ["OUVERTURE", "LIQUIDATION", "OUVERTURE"], "{kinds:?}");
        assert!(book.equity() < 10_000.0 * 0.6, "la marge doit être perdue : {}", book.equity());
    }

    #[test]
    fn every_carry_preset_is_registered() {
        let reg = crate::essais::registry();
        for p in carry_catalog() {
            let fp = carry_fingerprint(&p);
            assert!(
                reg.iter().any(|t| t.id == p.id && t.fingerprint == fp),
                "{} absent du registre des essais ; ligne à ajouter :
2026-10-06	strategie	{}	{fp}",
                p.id,
                p.id
            );
        }
    }

    #[test]
    fn carry_validation_on_steady_positive_funding_wins_every_window() {
        let px: Vec<f64> = (0..24 * 800).map(|i| 100.0 + 30.0 * ((i as f64) / 500.0).sin()).collect();
        let c = hourly(&px, 0);
        let f = every_8h(&c, |_| 0.0001, 30);
        let prices = BTreeMap::from([("X".to_string(), c)]);
        let funding = BTreeMap::from([("X".to_string(), f)]);
        let p = find_carry("portage_toujours").unwrap();
        let r = carry_validation(&p, &prices, &funding, CostModel::default(), 10_000.0, 60, 30, 1).unwrap();
        assert!(
            r.windows.iter().all(|w| w.outcome == Outcome::Win),
            "{:?}",
            r.windows.iter().map(|w| w.return_pct).collect::<Vec<_>>()
        );
        assert_eq!(r.verdict, Robustness::PasDeLaChance);
        // Et sur des taux nuls, les frais font perdre chaque fenêtre.
        let zero = BTreeMap::from([("X".to_string(), every_8h(&prices["X"], |_| 0.0, 30))]);
        let r0 = carry_validation(&p, &prices, &zero, CostModel::default(), 10_000.0, 60, 30, 1).unwrap();
        assert!(r0.windows.iter().all(|w| w.outcome == Outcome::Loss));
        assert_eq!(r0.verdict, Robustness::CompatibleAvecLaChance);
    }
}
