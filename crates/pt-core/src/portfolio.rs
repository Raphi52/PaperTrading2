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
    /// paires crypto en EUR (<https://bitvavo.com/en/fees>, relevé du 2026-10-08).
    pub fee_rate: f64,
    /// Glissement par côté, en points de base (2 = 0,02 %).
    pub slippage_bps: f64,
}

impl Default for CostModel {
    fn default() -> Self {
        CostModel { fee_rate: 0.0025, slippage_bps: 2.0 }
    }
}

impl CostModel {
    pub fn zero() -> Self {
        CostModel { fee_rate: 0.0, slippage_bps: 0.0 }
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
        let costs = CostModel { fee_rate: 0.001, slippage_bps: 0.0 };
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
}
