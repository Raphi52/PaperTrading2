//! Bougies et unités de temps.
//!
//! Règle de conception n°1 : le moteur ne voit QUE des bougies clôturées.
//! L'ancien bot calculait ses indicateurs sur la bougie Binance encore en cours :
//! un signal pouvait s'allumer en milieu de bougie puis disparaître.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Candle {
    /// Ouverture, en millisecondes UTC.
    pub open_time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    /// Clôture, en millisecondes UTC (dernière milliseconde de la bougie).
    pub close_time: i64,
}

impl Candle {
    pub fn is_closed_at(&self, now_ms: i64) -> bool {
        self.close_time < now_ms
    }

    pub fn typical_price(&self) -> f64 {
        (self.high + self.low + self.close) / 3.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Timeframe {
    #[serde(rename = "15m")]
    M15,
    #[serde(rename = "30m")]
    M30,
    #[serde(rename = "1h")]
    H1,
    #[serde(rename = "4h")]
    H4,
    #[serde(rename = "1d")]
    D1,
}

impl Timeframe {
    pub const ALL: [Timeframe; 5] = [Timeframe::M15, Timeframe::M30, Timeframe::H1, Timeframe::H4, Timeframe::D1];

    /// Code d'intervalle Binance.
    pub fn as_str(&self) -> &'static str {
        match self {
            Timeframe::M15 => "15m",
            Timeframe::M30 => "30m",
            Timeframe::H1 => "1h",
            Timeframe::H4 => "4h",
            Timeframe::D1 => "1d",
        }
    }

    pub fn millis(&self) -> i64 {
        match self {
            Timeframe::M15 => 15 * 60_000,
            Timeframe::M30 => 30 * 60_000,
            Timeframe::H1 => 3_600_000,
            Timeframe::H4 => 4 * 3_600_000,
            Timeframe::D1 => 24 * 3_600_000,
        }
    }

    /// Nombre de bougies par an (marché crypto ouvert 24h/24, 365 jours).
    pub fn bars_per_year(&self) -> f64 {
        365.0 * 24.0 * 3_600_000.0 / self.millis() as f64
    }

    pub fn parse(s: &str) -> Option<Timeframe> {
        match s {
            "15m" => Some(Timeframe::M15),
            "30m" => Some(Timeframe::M30),
            "1h" => Some(Timeframe::H1),
            "4h" => Some(Timeframe::H4),
            "1d" => Some(Timeframe::D1),
            _ => None,
        }
    }
}

impl std::fmt::Display for Timeframe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Vérifie qu'une série est exploitable : triée, sans trou de prix, cohérente.
pub fn validate_series(candles: &[Candle]) -> Result<(), String> {
    for (i, c) in candles.iter().enumerate() {
        let values = [c.open, c.high, c.low, c.close];
        if values.iter().any(|v| !v.is_finite() || *v <= 0.0) {
            return Err(format!("bougie {i} : prix invalide"));
        }
        if c.high < c.low || c.high < c.open.max(c.close) - 1e-9 || c.low > c.open.min(c.close) + 1e-9 {
            return Err(format!("bougie {i} : high/low incohérents"));
        }
        if i > 0 && c.open_time <= candles[i - 1].open_time {
            return Err(format!("bougie {i} : série non triée"));
        }
    }
    Ok(())
}
