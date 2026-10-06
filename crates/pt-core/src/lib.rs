//! # pt-core
//!
//! Le cœur de PaperTrading2 : indicateurs, comptabilité, stratégies, moteur et
//! backtest. Ce crate ne fait AUCUN accès réseau ni disque : tout ce qui décide
//! d'un trade est une fonction pure, donc testable et rejouable.

pub mod backtest;
pub mod candle;
pub mod carry;
pub mod catalog;
pub mod engine;
pub mod essais;
pub mod indicators;
pub mod portfolio;
pub mod strategy;
pub mod validation;
pub mod walkforward;

#[cfg(test)]
pub(crate) mod testutil;

pub use backtest::{backtest, BacktestReport, CurvePoint, Metrics, Verdict};
pub use candle::{Candle, Timeframe};
pub use catalog::{catalog, find};
pub use engine::Engine;
pub use portfolio::{ClosedTrade, CostModel, Fill, Portfolio, Position, Side};
pub use strategy::{compute_signals, External, Preset, Signal};
pub use validation::{rolling_validation, Robustness, RollingConfig, RollingReport};
