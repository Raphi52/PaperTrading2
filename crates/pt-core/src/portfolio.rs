//! Comptabilité du portefeuille.
//!
//! Corrections par rapport à l'ancien bot :
//! - le coût d'une position INCLUT les frais d'achat (l'ancien PnL les oubliait) ;
//! - le glissement est fixe et documenté, pas tiré au hasard : un backtest rejoué
//!   donne exactement le même résultat ;
//! - l'historique des exécutions n'est jamais tronqué (l'ancien gardait 500 trades) ;
//! - un invariant vérifiable relie la trésorerie aux exécutions.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct CostModel {
    /// Frais par côté, en fraction (0.0025 = 0,25 %). Par défaut : tarif Bitvavo d'un ordre
    /// exécuté immédiatement (« taker »), sous 100 000 € échangés sur 30 jours, sur les
    /// paires crypto en EUR (<https://bitvavo.com/en/fees>, relevé du 2026-10-08). Grille confirmée le
    /// 2026-10-08 par <https://www.cafedelabourse.com/fiches-courtiers/bitvavo> : paires EUR, maker 0,15 %,
    /// taker 0,25 % sous 100 000 € sur 30 jours glissants.
    pub fee_rate: f64,
    /// Glissement par côté, en points de base (2 = 0,02 %). Avec un modèle de liquidité,
    /// c'est le MINIMUM payé par un ordre, même sur le marché le plus profond.
    pub slippage_bps: f64,
    /// Coût d'exécution selon la liquidité du marché (voir [`Liquidity`]). Absent d'un
    /// réglage enregistré avant le 2026-10-09 : le modèle calibré s'applique. `None` :
    /// glissement fixe de `slippage_bps`.
    #[serde(default = "Liquidity::calibrated_option")]
    pub liquidity: Option<Liquidity>,
    /// Paliers de frais Bitvavo selon le volume des 30 jours (voir [`BITVAVO_TAKER_TIERS`]).
    /// Absent d'un réglage enregistré avant le 2026-10-12 : activés. `false` : `fee_rate`
    /// sur chaque ordre, quel que soit le volume.
    #[serde(default = "fee_tiers_on")]
    pub fee_tiers: bool,
}

fn fee_tiers_on() -> bool {
    true
}

/// Durée de la fenêtre de volume : 24 h.
pub const DAY_MS: i64 = 86_400_000;

/// Grille « taker » de Bitvavo par volume échangé sur 30 jours, du palier le plus haut au plus
/// bas : (volume minimal en €, frais par côté). Sous 100 000 € : `fee_rate` (0,25 % par défaut).
/// Source : <https://bitvavo.com/en/fees>, relevée le 2026-10-10 par moteur de recherche (la page
/// répond HTTP 403 à une lecture directe) ; 0,20 % et 0,18 % revérifiés de même le 2026-10-12.
/// Le volume des paires de stablecoins ne compte pas chez Bitvavo : l'univers n'en contient pas.
pub const BITVAVO_TAKER_TIERS: [(f64, f64); 9] = [
    (100_000_000.0, 0.0002),
    (25_000_000.0, 0.0004),
    (10_000_000.0, 0.0008),
    (5_000_000.0, 0.0010),
    (2_500_000.0, 0.0012),
    (1_000_000.0, 0.0014),
    (500_000.0, 0.0016),
    (250_000.0, 0.0018),
    (100_000.0, 0.0020),
];

/// Fenêtre de volume des paliers : 30 jours.
pub const TIER_WINDOW_MS: i64 = 30 * DAY_MS;

/// Heure du recalcul quotidien du palier : 01:00 UTC (article d'aide Bitvavo 4405175148689,
/// lu par moteur de recherche le 2026-10-12).
pub const TIER_RESET_OFFSET_MS: i64 = 3_600_000;

/// Dernier recalcul du palier (01:00 UTC) antérieur ou égal à `t`.
pub fn tier_reset_at(t: i64) -> i64 {
    (t - TIER_RESET_OFFSET_MS).div_euclid(DAY_MS) * DAY_MS + TIER_RESET_OFFSET_MS
}

/// Euros échangés par les exécutions `fills` datées de `[reset − 30 j, reset)`.
pub fn volume_30d_eur(fills: &[Fill], reset: i64) -> f64 {
    add_tier_volume(0.0, fills, reset)
}

/// Ajoute à `v`, une par une et dans l'ordre, les exécutions de la fenêtre : la mémoire et le
/// recalcul complet font exactement les mêmes additions, donc le même résultat au bit près.
/// Un montant illisible (non fini, négatif) ne compte pas : `buy` les refuse déjà, seule une
/// base modifiée à la main peut en contenir.
fn add_tier_volume(mut v: f64, fills: &[Fill], reset: i64) -> f64 {
    for f in fills {
        let eur = f.qty * f.price;
        if (reset - TIER_WINDOW_MS..reset).contains(&f.time) && eur.is_finite() && eur > 0.0 {
            v += eur;
        }
    }
    v
}

/// Mémoire du volume des 30 jours d'UN compte, pour ne pas tout re-sommer à chaque ordre.
/// Jamais enregistrée ni comparée : deux moteurs égaux restent égaux quel que soit leur cache.
#[derive(Debug, Clone, Default)]
pub struct FeeTierMemo {
    reset: Option<i64>,
    seen: usize,
    volume: f64,
}

impl PartialEq for FeeTierMemo {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl FeeTierMemo {
    /// Volume des 30 jours au dernier recalcul antérieur ou égal à `time`. Recalcul complet
    /// au changement de recalcul ou si l'historique a raccourci ; sinon, seules les exécutions
    /// ajoutées depuis l'appel précédent sont lues (dans n'importe quel ordre de dates).
    pub fn volume(&mut self, fills: &[Fill], time: i64) -> f64 {
        let reset = tier_reset_at(time);
        if self.reset != Some(reset) || fills.len() < self.seen {
            *self = FeeTierMemo { reset: Some(reset), seen: 0, volume: 0.0 };
        }
        self.volume = add_tier_volume(self.volume, &fills[self.seen..], reset);
        self.seen = fills.len();
        self.volume
    }

    /// Les coûts d'UN ordre passé à `time` par le compte dont l'historique est `fills` :
    /// glissement selon la liquidité ([`CostModel::for_order`]), frais selon le palier.
    pub fn order_costs(
        &mut self,
        costs: &CostModel,
        fills: &[Fill],
        volume_24h: Option<f64>,
        notional: f64,
        time: i64,
    ) -> CostModel {
        let mut order = costs.for_order(volume_24h, notional);
        if costs.fee_tiers {
            order.fee_rate = costs.tier_rate(self.volume(fills, time));
        }
        order
    }
}

/// Coût d'un ordre au marché, en points de base par côté :
/// `spread_bps × (V / 1 M€)^−spread_exponent + impact_bps × √(M / (V / 96))`, borné
/// entre `slippage_bps` et `cap_bps`. `V` : euros échangés sur les 24 h précédant la
/// décision ; `M` : montant de l'ordre ; `V / 96` : volume moyen d'un quart d'heure.
///
/// Calibration du 2026-10-09 (audit de réalisme) : 50 paires EUR de Bitvavo, carnets
/// `/v2/<marché>/book?depth=500` et bougies 15 min relevés vers 09:00 UTC, ordres de la
/// taille réelle des 26 171 ordres du journal archivé. Coût réel moyen : 5,49 pb ; ce
/// modèle : 5,32 pb ; écart moyen par paire : 2,2 pb. L'impact est rapporté au volume
/// moyen des 24 h et non à la seule bougie de décision : rapporté à la bougie, le modèle
/// facturait 15 à 30 pb en moyenne, 3 à 5 fois le coût réel (une bougie calme n'a pas
/// vidé le carnet). Limite connue : un marché au carnet mince pour son volume (NPC-EUR,
/// 30 pb réels) est sous-estimé (6,6 pb) ; aucun modèle fondé sur le volume ne le voit.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Liquidity {
    /// Demi-écart achat/vente d'un marché qui échange 1 M€ par jour, en pb.
    pub spread_bps: f64,
    /// Décroissance du demi-écart avec le volume (0,4 : volume ×10 → écart ÷2,5).
    pub spread_exponent: f64,
    /// Impact d'un ordre égal au volume moyen d'un quart d'heure, en pb.
    pub impact_bps: f64,
    /// Plafond : coût d'un marché sans volume connu (zéro, illisible) ou d'un ordre démesuré.
    pub cap_bps: f64,
}

impl Liquidity {
    pub const CALIBRATED: Liquidity =
        Liquidity { spread_bps: 4.0, spread_exponent: 0.4, impact_bps: 2.0, cap_bps: 100.0 };

    fn calibrated_option() -> Option<Liquidity> {
        Some(Liquidity::CALIBRATED)
    }

    /// Rend une erreur lisible si un réglage sort de ses bornes.
    pub fn validate(&self) -> Result<(), String> {
        let ok = |v: f64, max: f64| v.is_finite() && (0.0..=max).contains(&v);
        if !ok(self.spread_bps, 500.0) || !ok(self.impact_bps, 500.0) {
            return Err("l'écart et l'impact de liquidité doivent être entre 0 et 500 points de base".into());
        }
        if !ok(self.spread_exponent, 2.0) {
            return Err("l'exposant de liquidité doit être entre 0 et 2".into());
        }
        if !ok(self.cap_bps, 1_000.0) {
            return Err("le plafond de liquidité doit être entre 0 et 1 000 points de base".into());
        }
        Ok(())
    }
}

impl Default for CostModel {
    fn default() -> Self {
        CostModel { fee_rate: 0.0025, slippage_bps: 2.0, liquidity: Some(Liquidity::CALIBRATED), fee_tiers: true }
    }
}

impl CostModel {
    pub fn zero() -> Self {
        CostModel { fee_rate: 0.0, slippage_bps: 0.0, liquidity: None, fee_tiers: false }
    }

    // fix-ok: le glissement était fixé à 2 pb pour toute paire (ancien portfolio.rs:27, appliqué
    // tel quel par buy_price/sell_price). Mesuré sur les carnets Bitvavo du 2026-10-09 pour un
    // ordre de 1 446 € : 0,1 pb sur BTC-EUR, 28,7 pb sur NPC-EUR, 5,49 pb en moyenne sur 50 paires.
    /// Glissement d'un ordre de `notional` € sur un marché qui a échangé `volume_24h` €
    /// sur les 24 h précédentes. Liquidité inconnue (`None`, historique trop court) ou
    /// modèle absent : `slippage_bps`. Volume nul ou illisible : le plafond. Toujours fini.
    pub fn order_slippage_bps(&self, volume_24h: Option<f64>, notional: f64) -> f64 {
        let (Some(l), Some(v)) = (self.liquidity, volume_24h) else {
            return self.slippage_bps;
        };
        let cap = l.cap_bps.max(self.slippage_bps);
        if !(v.is_finite() && v > 0.0) {
            return cap;
        }
        let m = if notional.is_finite() { notional.max(0.0) } else { f64::INFINITY };
        let x = l.spread_bps * (v / 1e6).powf(-l.spread_exponent) + l.impact_bps * (m / (v / 96.0)).sqrt();
        if x.is_nan() {
            return cap;
        }
        x.clamp(self.slippage_bps, cap)
    }

    /// Les coûts d'UN ordre : glissement fixé par la liquidité, sans modèle.
    pub fn for_order(&self, volume_24h: Option<f64>, notional: f64) -> CostModel {
        CostModel {
            fee_rate: self.fee_rate,
            slippage_bps: self.order_slippage_bps(volume_24h, notional),
            liquidity: None,
            fee_tiers: self.fee_tiers,
        }
    }

    // fix-ok: chaque ordre payait `fee_rate` (0,25 %) quel que soit le volume : `for_order`
    // (portfolio.rs:117-121 avant ce commit) le recopiait, `buy` et `sell_all` l'appliquaient.
    // Mesuré le 2026-10-12 dans la base : 107 stratégies sur 321 dépassaient 100 000 € sur
    // 30 jours au recalcul de 01:00 UTC (92 à 0,20 %, 15 à 0,18 %).
    /// Frais par côté d'un compte qui a échangé `volume_30d` € sur les 30 jours du dernier
    /// recalcul (voir [`BITVAVO_TAKER_TIERS`]) : `min(fee_rate, palier)` dès 100 000 €, jamais
    /// plus que le réglage ; sous 100 000 €, volume illisible ou paliers coupés : `fee_rate`.
    /// `round_trip` et `breakeven` restent au taux de base : hypothèse prudente (un objectif
    /// placé pour couvrir 0,25 % couvre a fortiori un palier plus bas).
    pub fn tier_rate(&self, volume_30d: f64) -> f64 {
        if !self.fee_tiers {
            return self.fee_rate;
        }
        match BITVAVO_TAKER_TIERS.iter().find(|(min, _)| volume_30d >= *min) {
            Some(&(_, tier)) => self.fee_rate.min(tier),
            None => self.fee_rate,
        }
    }
    pub fn buy_price(&self, p: f64) -> f64 {
        p * (1.0 + self.slippage_bps / 10_000.0)
    }
    pub fn sell_price(&self, p: f64) -> f64 {
        p * (1.0 - self.slippage_bps / 10_000.0)
    }
    /// Coût d'un aller-retour complet, en fraction.
    pub fn round_trip(&self) -> f64 {
        2.0 * self.fee_rate + 2.0 * self.slippage_bps / 10_000.0
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Fill {
    pub time: i64,
    pub symbol: String,
    pub side: Side,
    pub qty: f64,
    /// Prix d'exécution, glissement compris.
    pub price: f64,
    pub fee: f64,
    /// Variation de trésorerie : négative à l'achat, positive à la vente.
    pub cash_delta: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Position {
    pub symbol: String,
    pub qty: f64,
    /// Trésorerie totale engagée, frais d'achat compris.
    pub cost: f64,
    pub entry_time: i64,
    /// Prix moyen d'exécution (hors frais).
    pub avg_price: f64,
    pub layers: u32,
    pub last_fill_price: f64,
    /// Montant de chaque couche, pour les stratégies à renforcement borné.
    pub layer_notional: f64,
    pub highest: f64,
    pub stop: Option<f64>,
    pub take_profit: Option<f64>,
    pub bars_held: u32,
}

impl Position {
    /// Prix auquel la position est à l'équilibre, frais de sortie compris.
    pub fn breakeven(&self, costs: &CostModel) -> f64 {
        self.cost / (self.qty * (1.0 - costs.fee_rate) * (1.0 - costs.slippage_bps / 10_000.0))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClosedTrade {
    pub symbol: String,
    pub entry_time: i64,
    pub exit_time: i64,
    pub avg_entry_price: f64,
    pub exit_price: f64,
    pub qty: f64,
    pub cost: f64,
    pub proceeds: f64,
    /// Gain net : frais d'achat ET de vente déduits.
    pub pnl: f64,
    pub return_pct: f64,
    pub bars_held: u32,
    pub layers: u32,
    pub exit_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Portfolio {
    pub initial_cash: f64,
    pub cash: f64,
    pub positions: BTreeMap<String, Position>,
    pub fills: Vec<Fill>,
    pub closed: Vec<ClosedTrade>,
    pub fees_paid: f64,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum AccountError {
    #[error("montant invalide : {0}")]
    InvalidAmount(f64),
    #[error("trésorerie insuffisante : {needed:.2} demandés, {available:.2} disponibles")]
    InsufficientCash { needed: f64, available: f64 },
    #[error("aucune position ouverte sur {0}")]
    NoPosition(String),
}

impl Portfolio {
    pub fn new(initial_cash: f64) -> Self {
        Portfolio {
            initial_cash,
            cash: initial_cash,
            positions: BTreeMap::new(),
            fills: Vec::new(),
            closed: Vec::new(),
            fees_paid: 0.0,
        }
    }

    /// Achète pour `notional` de trésorerie (frais compris) au prix de marché `market_price`.
    pub fn buy(
        &mut self,
        symbol: &str,
        notional: f64,
        market_price: f64,
        time: i64,
        costs: &CostModel,
        reason: &str,
    ) -> Result<&Position, AccountError> {
        if !(notional.is_finite() && notional > 0.0 && market_price > 0.0) {
            return Err(AccountError::InvalidAmount(notional));
        }
        if notional > self.cash + 1e-9 {
            return Err(AccountError::InsufficientCash { needed: notional, available: self.cash });
        }
        let price = costs.buy_price(market_price);
        let fee = notional * costs.fee_rate;
        let qty = (notional - fee) / price;
        self.cash -= notional;
        self.fees_paid += fee;
        self.fills.push(Fill {
            time,
            symbol: symbol.to_string(),
            side: Side::Buy,
            qty,
            price,
            fee,
            cash_delta: -notional,
            reason: reason.to_string(),
        });
        let pos = self.positions.entry(symbol.to_string()).or_insert_with(|| Position {
            symbol: symbol.to_string(),
            qty: 0.0,
            cost: 0.0,
            entry_time: time,
            avg_price: 0.0,
            layers: 0,
            last_fill_price: price,
            layer_notional: notional,
            highest: price,
            stop: None,
            take_profit: None,
            bars_held: 0,
        });
        pos.avg_price = (pos.avg_price * pos.qty + price * qty) / (pos.qty + qty);
        pos.qty += qty;
        pos.cost += notional;
        pos.layers += 1;
        pos.last_fill_price = price;
        Ok(pos)
    }

    /// Vend la TOTALITÉ de la position au prix de marché `market_price`.
    pub fn sell_all(
        &mut self,
        symbol: &str,
        market_price: f64,
        time: i64,
        costs: &CostModel,
        reason: &str,
    ) -> Result<ClosedTrade, AccountError> {
        let pos = self.positions.remove(symbol).ok_or_else(|| AccountError::NoPosition(symbol.to_string()))?;
        let price = costs.sell_price(market_price);
        let gross = pos.qty * price;
        let fee = gross * costs.fee_rate;
        let proceeds = gross - fee;
        self.cash += proceeds;
        self.fees_paid += fee;
        self.fills.push(Fill {
            time,
            symbol: symbol.to_string(),
            side: Side::Sell,
            qty: pos.qty,
            price,
            fee,
            cash_delta: proceeds,
            reason: reason.to_string(),
        });
        let pnl = proceeds - pos.cost;
        let trade = ClosedTrade {
            symbol: symbol.to_string(),
            entry_time: pos.entry_time,
            exit_time: time,
            avg_entry_price: pos.avg_price,
            exit_price: price,
            qty: pos.qty,
            cost: pos.cost,
            proceeds,
            pnl,
            return_pct: pnl / pos.cost * 100.0,
            bars_held: pos.bars_held,
            layers: pos.layers,
            exit_reason: reason.to_string(),
        };
        self.closed.push(trade.clone());
        Ok(trade)
    }

    /// Valeur totale au prix `marks` (prix de marché par symbole).
    pub fn equity(&self, marks: &BTreeMap<String, f64>) -> f64 {
        self.cash
            + self
                .positions
                .values()
                .map(|p| p.qty * marks.get(&p.symbol).copied().unwrap_or(p.last_fill_price))
                .sum::<f64>()
    }

    pub fn exposure(&self, marks: &BTreeMap<String, f64>) -> f64 {
        let eq = self.equity(marks);
        if eq <= 0.0 {
            return 0.0;
        }
        (eq - self.cash) / eq
    }

    /// Invariant comptable : la trésorerie se reconstruit exactement à partir des
    /// exécutions, et à partir des gains réalisés et des coûts encore engagés.
    pub fn check_invariants(&self) -> Result<(), String> {
        let from_fills = self.initial_cash + self.fills.iter().map(|f| f.cash_delta).sum::<f64>();
        let from_pnl = self.initial_cash + self.closed.iter().map(|t| t.pnl).sum::<f64>()
            - self.positions.values().map(|p| p.cost).sum::<f64>();
        let tol = 1e-6 * self.initial_cash.max(1.0);
        if (from_fills - self.cash).abs() > tol {
            return Err(format!("trésorerie {:.6} ≠ somme des exécutions {:.6}", self.cash, from_fills));
        }
        if (from_pnl - self.cash).abs() > tol {
            return Err(format!("trésorerie {:.6} ≠ capital + gains − coûts engagés {:.6}", self.cash, from_pnl));
        }
        if self.cash < -tol {
            return Err(format!("trésorerie négative : {:.6}", self.cash));
        }
        let fees: f64 = self.fills.iter().map(|f| f.fee).sum();
        if (fees - self.fees_paid).abs() > tol {
            return Err(format!("frais cumulés {:.6} ≠ somme des frais {:.6}", self.fees_paid, fees));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Règles réelles de la plateforme : 0,25 % par côté, 5 € d'ordre minimal.
    #[test]
    fn default_costs_are_the_real_platform_ones() {
        assert_eq!(CostModel::default().fee_rate, 0.0025);
        assert_eq!(CostModel::default().slippage_bps, 2.0);
        assert_eq!(crate::engine::MIN_NOTIONAL, 5.0);
    }

    #[test]
    fn pnl_includes_both_fees() {
        let costs = CostModel { fee_rate: 0.001, slippage_bps: 0.0, liquidity: None, fee_tiers: false };
        let mut p = Portfolio::new(10_000.0);
        p.buy("BTCEUR", 1_000.0, 100.0, 0, &costs, "test").unwrap();
        // Prix inchangé : on doit perdre exactement les deux frais.
        let t = p.sell_all("BTCEUR", 100.0, 1, &costs, "test").unwrap();
        let expected = -(1.0 + 999.0 * 0.001);
        assert!((t.pnl - expected).abs() < 1e-9, "pnl={} attendu={}", t.pnl, expected);
        assert!((p.cash - (10_000.0 + expected)).abs() < 1e-9);
        p.check_invariants().unwrap();
    }

    #[test]
    fn breakeven_covers_round_trip() {
        let costs = CostModel::default();
        let mut p = Portfolio::new(10_000.0);
        p.buy("ETHEUR", 2_000.0, 50.0, 0, &costs, "t").unwrap();
        let be = p.positions["ETHEUR"].breakeven(&costs);
        let t = p.sell_all("ETHEUR", be, 1, &costs, "t").unwrap();
        assert!(t.pnl.abs() < 1e-6, "au point mort le gain doit être nul, obtenu {}", t.pnl);
        assert!(be > 50.0 * (1.0 + costs.round_trip() * 0.9));
    }

    #[test]
    fn layering_averages_and_keeps_invariants() {
        let costs = CostModel::default();
        let mut p = Portfolio::new(10_000.0);
        p.buy("SOLEUR", 1_000.0, 100.0, 0, &costs, "l1").unwrap();
        p.buy("SOLEUR", 1_000.0, 80.0, 1, &costs, "l2").unwrap();
        let pos = &p.positions["SOLEUR"];
        assert_eq!(pos.layers, 2);
        assert!(pos.avg_price < 100.0 && pos.avg_price > 80.0);
        assert!((pos.cost - 2_000.0).abs() < 1e-9);
        p.check_invariants().unwrap();
        p.sell_all("SOLEUR", 90.0, 2, &costs, "exit").unwrap();
        p.check_invariants().unwrap();
        assert!(p.positions.is_empty());
    }

    #[test]
    fn refuses_overspending() {
        let mut p = Portfolio::new(100.0);
        let err = p.buy("X", 150.0, 1.0, 0, &CostModel::default(), "t").unwrap_err();
        assert!(matches!(err, AccountError::InsufficientCash { .. }));
        assert_eq!(p.cash, 100.0);
    }

    /// Relevé du 2026-10-09 vers 08:50 UTC sur les 50 paires EUR des portefeuilles : euros
    /// échangés sur 24 h (bougies 15 min de Bitvavo, volume × clôture) et coût réel, en pb
    /// par côté, d'un ordre au marché de 1 448 € (taille médiane des 26 171 ordres du
    /// journal archivé), moyenne achat/vente en parcourant `/v2/<marché>/book?depth=500`.
    const BOOKS_2026_10_09: [(&str, f64, f64); 50] = [
        ("AAVEEUR", 1052070.0, 2.9),
        ("ADAEUR", 31920989.0, 1.2),
        ("ARBEUR", 979059.0, 4.98),
        ("AVAXEUR", 3263464.0, 1.89),
        ("BCHEUR", 1649837.0, 3.06),
        ("BNBEUR", 461877.0, 2.32),
        ("BTCEUR", 93849928.0, 0.07),
        ("COTIEUR", 148742.0, 9.38),
        ("DOGEEUR", 1872008.0, 1.01),
        ("DOTEUR", 1403126.0, 6.87),
        ("ENAEUR", 1734551.0, 3.75),
        ("EPICEUR", 103705.0, 11.03),
        ("ETHEUR", 35387635.0, 0.04),
        ("ETHFIEUR", 892403.0, 9.85),
        ("FARTCOINEUR", 872674.0, 2.93),
        ("FETEUR", 4575868.0, 6.29),
        ("HBAREUR", 2648768.0, 4.87),
        ("HYPEEUR", 13763933.0, 0.86),
        ("INJEUR", 978821.0, 12.75),
        ("JUPEUR", 1424147.0, 8.3),
        ("LINKEUR", 4869707.0, 1.27),
        ("LTCEUR", 1322634.0, 3.36),
        ("NEAREUR", 14016298.0, 3.13),
        ("NPCEUR", 460531.0, 30.02),
        ("ONDOEUR", 6868192.0, 1.55),
        ("PENGUEUR", 836868.0, 6.1),
        ("PEPEEUR", 1753261.0, 3.06),
        ("PROMEUR", 451484.0, 11.82),
        ("PUMPEUR", 3172900.0, 2.74),
        ("PYTHEUR", 1754804.0, 9.09),
        ("QNTEUR", 11299083.0, 4.46),
        ("RENDEREUR", 2089159.0, 7.08),
        ("SHIBEUR", 572015.0, 5.03),
        ("SOLEUR", 27542262.0, 0.1),
        ("SPXEUR", 696955.0, 6.62),
        ("SUIEUR", 10174336.0, 2.23),
        ("SYNEUR", 548499.0, 10.4),
        ("TAOEUR", 8933500.0, 2.18),
        ("TIAEUR", 2803139.0, 6.65),
        ("TRUMPEUR", 643367.0, 5.95),
        ("TRXEUR", 334763.0, 2.0),
        ("UNIEUR", 2204145.0, 5.06),
        ("USELESSEUR", 3157855.0, 10.97),
        ("VETEUR", 930299.0, 9.44),
        ("VIRTUALEUR", 880260.0, 9.64),
        ("WLDEUR", 3315583.0, 4.34),
        ("XLMEUR", 1627261.0, 2.51),
        ("XPLEUR", 529339.0, 5.95),
        ("XRPEUR", 44550422.0, 0.14),
        ("ZROEUR", 1019080.0, 6.67),
    ];

    /// Cas 1 (nominal) : le modèle colle aux carnets réels mieux que les 2 pb fixes.
    /// Mesuré sur ce relevé : réel 5,48 pb en moyenne ; modèle 4,21 (biais −1,27, écart
    /// moyen 2,71) ; 2 pb fixes : biais −3,48, écart moyen 3,95. Limite connue : NPC-EUR,
    /// 30 pb réels, carnet mince pour son volume, reste sous-estimé (6,6 pb).
    #[test]
    fn liquidity_cost_matches_bitvavo_books_2026_10_09() {
        let costs = CostModel::default();
        let fixed = CostModel { liquidity: None, ..costs };
        let n = BOOKS_2026_10_09.len() as f64;
        let real = BOOKS_2026_10_09.iter().map(|r| r.2).sum::<f64>() / n;
        let fit = |c: &CostModel| {
            let model: Vec<f64> = BOOKS_2026_10_09.iter().map(|r| c.order_slippage_bps(Some(r.1), 1_448.0)).collect();
            let bias = model.iter().sum::<f64>() / n - real;
            let mae = model.iter().zip(&BOOKS_2026_10_09).map(|(m, r)| (m - r.2).abs()).sum::<f64>() / n;
            (bias, mae)
        };
        let (bias, mae) = fit(&costs);
        let (fixed_bias, fixed_mae) = fit(&fixed);
        assert!(bias.abs() <= 1.5, "biais {bias:.2} pb");
        assert!(mae <= 3.0, "écart moyen {mae:.2} pb");
        assert!(fixed_bias.abs() > 1.5, "jumeau : les 2 pb fixes échouent à cette borne ({fixed_bias:.2})");
        assert!(mae < fixed_mae, "{mae:.2} contre {fixed_mae:.2} pb");
        let at = |name: &str| {
            let r = BOOKS_2026_10_09.iter().find(|r| r.0 == name).expect("paire");
            costs.order_slippage_bps(Some(r.1), 1_448.0)
        };
        for deep in ["BTCEUR", "ETHEUR", "SOLEUR", "XRPEUR"] {
            assert!(at(deep) <= 3.0, "{deep} : {} pb", at(deep));
        }
        assert!((4.0..=15.0).contains(&at("JUPEUR")), "JUP : {} pb (réel 8,3)", at("JUPEUR"));
        assert!(at("EPICEUR") > at("JUPEUR") && at("JUPEUR") > at("BTCEUR"), "plus mince, plus cher");
    }

    /// Cas 2 : un réglage enregistré avant le modèle de liquidité se relit, modèle ACTIVÉ.
    #[test]
    fn saved_costs_without_liquidity_fields_load_with_model_on() {
        let old: CostModel = serde_json::from_str(r#"{"fee_rate":0.0025,"slippage_bps":2.0}"#).unwrap();
        assert_eq!(old, CostModel::default());
        assert_eq!(old.liquidity, Some(Liquidity::CALIBRATED));
        // Jumeau : un modèle explicitement coupé reste coupé une fois relu.
        let zero: CostModel = serde_json::from_str(&serde_json::to_string(&CostModel::zero()).unwrap()).unwrap();
        assert_eq!(zero, CostModel::zero());
    }

    /// Cas 5, 6 et 8 : volume nul, illisible ou négatif, ordre démesuré : le plafond, fini.
    #[test]
    fn liquidity_cost_is_finite_and_capped() {
        let c = CostModel::default();
        let cap = Liquidity::CALIBRATED.cap_bps;
        assert!(cap >= 50.0, "au-dessus du pire coût mesuré (49,2 pb, 5 000 € sur NPC-EUR)");
        for v in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(c.order_slippage_bps(Some(v), 1_000.0), cap, "volume {v}");
        }
        assert_eq!(c.order_slippage_bps(Some(1e6), 1e12), cap, "ordre démesuré");
        assert_eq!(c.order_slippage_bps(Some(1e6), f64::NAN), cap, "montant illisible");
        let mut last = 0.0;
        for m in [0.0, 10.0, 1e3, 1e5, 1e7, 1e9, 1e15] {
            let x = c.order_slippage_bps(Some(1e6), m);
            assert!(x.is_finite() && x >= last && x <= cap, "{m} € : {x} pb");
            last = x;
        }
        let p = c.for_order(Some(0.0), 1_000.0).buy_price(100.0);
        assert!((p - 100.0 * (1.0 + cap / 10_000.0)).abs() < 1e-9, "prix fini et plafonné : {p}");
        // Jumeau : un volume normal ne touche pas le plafond.
        assert!(c.order_slippage_bps(Some(1e6), 1_000.0) < cap);
    }

    /// Cas 7 : un marché très profond paie exactement le minimum, jamais moins.
    #[test]
    fn deep_market_pays_exactly_the_floor() {
        let c = CostModel::default();
        assert_eq!(c.order_slippage_bps(Some(1e12), 1_000.0), 2.0);
        assert_eq!(c.order_slippage_bps(Some(93.7e6), 1_446.0), 2.0, "profil BTC-EUR");
        // Jumeau : un marché mince paie plus que le minimum.
        assert!(c.order_slippage_bps(Some(1e5), 1_446.0) > 2.0);
        // Liquidité inconnue (cas 3) : le minimum aussi.
        assert_eq!(c.order_slippage_bps(None, 1e9), 2.0);
    }

    /// Cas 12 : `zero()` reste sans aucun coût, même sur un marché sans volume.
    #[test]
    fn zero_costs_stay_zero_on_thin_market() {
        let z = CostModel::zero();
        for v in [Some(1.0), Some(0.0), None] {
            assert_eq!(z.order_slippage_bps(v, 1e6), 0.0);
            assert_eq!(z.for_order(v, 1e6).buy_price(100.0), 100.0);
        }
    }

    /// Cas 10 : un réglage de liquidité hors bornes est refusé ; le calibré est accepté.
    #[test]
    fn liquidity_settings_out_of_bounds_are_refused() {
        assert!(Liquidity::CALIBRATED.validate().is_ok());
        let l = Liquidity::CALIBRATED;
        for bad in [
            Liquidity { spread_bps: -1.0, ..l },
            Liquidity { impact_bps: -0.1, ..l },
            Liquidity { spread_exponent: -0.4, ..l },
            Liquidity { cap_bps: f64::NAN, ..l },
            Liquidity { spread_bps: 501.0, ..l },
        ] {
            assert!(bad.validate().is_err(), "{bad:?}");
        }
        assert!(Liquidity { spread_bps: 0.0, impact_bps: 500.0, ..l }.validate().is_ok(), "bornes incluses");
    }

    // --- Paliers de frais Bitvavo (audit de réalisme du 2026-10-12) ---

    const H: i64 = 3_600_000;
    /// 01:00 UTC du jour 100 depuis 1970 : un recalcul.
    const R: i64 = 100 * DAY_MS + TIER_RESET_OFFSET_MS;

    fn tier_fill(time: i64, eur: f64) -> Fill {
        Fill {
            time,
            symbol: "BTCEUR".into(),
            side: Side::Buy,
            qty: eur / 100.0,
            price: 100.0,
            fee: 0.0,
            cash_delta: -eur,
            reason: "volume".into(),
        }
    }

    /// Cas C8 et C9 : seuils de la grille, bornes atteintes comprises ; jamais sous 0,02 %.
    #[test]
    fn tier_grid_thresholds() {
        let c = CostModel::default();
        for (v, want) in [
            (0.0, 0.0025),
            (99_999.99, 0.0025),
            (100_000.0, 0.0020),
            (249_999.99, 0.0020),
            (250_000.0, 0.0018),
            (500_000.0, 0.0016),
            (1_000_000.0, 0.0014),
            (2_500_000.0, 0.0012),
            (5_000_000.0, 0.0010),
            (10_000_000.0, 0.0008),
            (25_000_000.0, 0.0004),
            (99_999_999.0, 0.0004),
            (100_000_000.0, 0.0002),
            (1e15, 0.0002),
        ] {
            assert_eq!(c.tier_rate(v), want, "{v} €");
        }
    }

    /// Cas C10 et C12 : le palier ne fait jamais payer plus que le réglage.
    #[test]
    fn tier_rate_never_above_setting() {
        let base = CostModel::default();
        let zero = CostModel { fee_rate: 0.0, ..base };
        let low = CostModel { fee_rate: 0.001, ..base };
        let high = CostModel { fee_rate: 0.005, ..base };
        for v in [0.0, 120_000.0, 1e6, 1e9] {
            assert_eq!(zero.tier_rate(v), 0.0, "sans frais reste sans frais, {v} €");
        }
        assert_eq!(low.tier_rate(120_000.0), 0.001, "un réglage sous la grille reste plus bas");
        assert_eq!(low.tier_rate(1e8), 0.0002, "jumeau : la grille passe dessous quand elle descend plus bas");
        assert_eq!(base.tier_rate(120_000.0), 0.0020);
        assert_eq!(high.tier_rate(50_000.0), 0.005, "sous le premier palier, le réglage reste tel quel");
        assert_eq!(high.tier_rate(120_000.0), 0.0020);
    }

    /// Cas C3 et C15 : un réglage enregistré sans le champ se charge, paliers ACTIVÉS ;
    /// un réglage coupé reste coupé.
    #[test]
    fn tier_old_settings_load_with_tiers_on() {
        let old: CostModel = serde_json::from_str(r#"{"fee_rate":0.0025,"slippage_bps":2.0}"#).unwrap();
        assert!(old.fee_tiers);
        assert_eq!(old.tier_rate(1_000_000.0), 0.0014);
        let off = CostModel { fee_tiers: false, ..old };
        let back: CostModel = serde_json::from_str(&serde_json::to_string(&off).unwrap()).unwrap();
        assert!(!back.fee_tiers, "un réglage coupé reste coupé une fois relu");
        for v in [0.0, 120_000.0, 1e6, 1e9] {
            assert_eq!(back.tier_rate(v), 0.0025, "coupé : {v} €");
        }
    }

    /// Cas C4 : un champ mal typé est refusé, jamais remplacé par la valeur par défaut.
    #[test]
    fn tier_flag_wrong_type_is_refused() {
        for bad in [r#""oui""#, "1", "null"] {
            let json = format!(r#"{{"fee_rate":0.0025,"slippage_bps":2.0,"fee_tiers":{bad}}}"#);
            assert!(serde_json::from_str::<CostModel>(&json).is_err(), "{json}");
        }
        // Jumeau : la même entrée bien typée est acceptée.
        let ok: CostModel =
            serde_json::from_str(r#"{"fee_rate":0.0025,"slippage_bps":2.0,"fee_tiers":false}"#).unwrap();
        assert!(!ok.fee_tiers);
    }

    /// Cas C6 : le recalcul de 01:00:00,000 vaut pour l'ordre de 01:00:00,000.
    #[test]
    fn tier_reset_is_01h_utc() {
        assert_eq!(tier_reset_at(R), R, "01:00:00,000 pile : recalcul du jour");
        assert_eq!(tier_reset_at(R - 1), R - DAY_MS, "00:59:59,999 : recalcul de la veille");
        assert_eq!(tier_reset_at(R + 22 * H), R, "23:00 : recalcul du jour");
        assert_eq!(tier_reset_at(R - H), R - DAY_MS, "minuit : recalcul de la veille");
        for t in [-1, 0, 1, R + 1, R + DAY_MS - 1] {
            let r = tier_reset_at(t);
            assert!(r <= t && t - r < DAY_MS, "{t} → {r}");
        }
    }

    /// Cas C7 : fenêtre `[r − 30 j, r)` ; un volume vieux de plus de 30 jours ne compte plus.
    #[test]
    fn tier_volume_window_bounds() {
        let fills = vec![
            tier_fill(R - TIER_WINDOW_MS, 1_000.0),
            tier_fill(R - TIER_WINDOW_MS - 1, 20_000.0),
            tier_fill(R, 300_000.0),
            tier_fill(R - 1, 4.0),
        ];
        assert_eq!(volume_30d_eur(&fills, R), 1_004.0);
        // Jumeau, recalcul suivant : l'exécution datée de R compte, celle de R − 30 j est sortie.
        assert_eq!(volume_30d_eur(&fills, R + DAY_MS), 300_004.0);
        assert_eq!(volume_30d_eur(&[], R), 0.0);
    }

    /// Cas C5 : une exécution au montant illisible est ignorée ; le taux reste fini.
    #[test]
    fn tier_non_finite_fill_is_ignored() {
        let mut fills = vec![tier_fill(R - 10, 120_000.0)];
        let mut nan = tier_fill(R - 5, 1.0);
        nan.price = f64::NAN;
        let mut inf = tier_fill(R - 4, 1.0);
        inf.qty = f64::INFINITY;
        fills.extend([nan, inf]);
        let v = volume_30d_eur(&fills, R);
        assert_eq!(v, 120_000.0);
        assert_eq!(CostModel::default().tier_rate(v), 0.0020);
        let r = CostModel::default().tier_rate(f64::NAN);
        assert!(r.is_finite() && r == 0.0025, "volume illisible : taux de base, {r}");
    }

    /// Cas C13 : la mémoire donne le volume d'un recalcul complet, quel que soit l'ordre
    /// d'arrivée des exécutions ; une exécution datée de R ou après attend le recalcul suivant.
    #[test]
    fn tier_cache_equals_full_recompute() {
        let t = R + 30 * 60_000; // 01:30
        let all: Vec<Fill> = (1..=12).map(|i| tier_fill(R - i * DAY_MS, 10_000.0)).collect();
        let mut m = FeeTierMemo::default();
        for k in 0..=all.len() {
            assert_eq!(m.volume(&all[..k], t), volume_30d_eur(&all[..k], R), "après {k} exécutions");
        }
        assert_eq!(m.volume(&all, t), 120_000.0);
        let rev: Vec<Fill> = all.iter().rev().cloned().collect();
        let mut m2 = FeeTierMemo::default();
        for k in 0..rev.len() {
            m2.volume(&rev[..k], t);
        }
        assert!((m2.volume(&rev, t) - 120_000.0).abs() < 1e-9 * 120_000.0, "à l'envers");
        // Exécution arrivée tard mais datée d'avant R : comptée.
        let mut more = all.clone();
        more.push(tier_fill(R - 1, 5_000.0));
        assert_eq!(m.volume(&more, t), 125_000.0);
        // Datée de R ou après : rien ne change avant le recalcul suivant.
        more.push(tier_fill(R, 1_000_000.0));
        more.push(tier_fill(t, 1_000_000.0));
        assert_eq!(m.volume(&more, t + 60_000), 125_000.0);
        let c = CostModel::default();
        let oc = m.order_costs(&c, &more, None, 1_000.0, t + 60_000);
        assert_eq!(oc.fee_rate, 0.0020);
        assert_eq!(oc.slippage_bps, c.for_order(None, 1_000.0).slippage_bps, "le glissement ne change pas");
        // Recalcul suivant : les deux millions comptent (2 125 000 €, entre 1 M€ et 2,5 M€ : 0,14 %).
        assert_eq!(m.volume(&more, R + DAY_MS), volume_30d_eur(&more, R + DAY_MS));
        assert_eq!(m.order_costs(&c, &more, None, 1_000.0, R + DAY_MS).fee_rate, 0.0014);
        // Historique raccourci (rechargement) : recalcul complet.
        assert_eq!(m.volume(&all[..3], t), 30_000.0);
        // Paliers coupés : le réglage, quel que soit le volume.
        let off = CostModel { fee_tiers: false, ..c };
        assert_eq!(m.order_costs(&off, &more, None, 1_000.0, R + DAY_MS).fee_rate, 0.0025);
    }
}
