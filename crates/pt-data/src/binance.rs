//! Bougies Binance spot.
//!
//! La bougie en cours est TOUJOURS séparée des bougies clôturées : le moteur ne
//! décide que sur des bougies terminées, et n'utilise la bougie en cours que pour
//! son prix d'ouverture (exécution) et son dernier prix (valorisation).

use anyhow::{bail, Context, Result};
use pt_core::{Candle, Timeframe};
use serde_json::Value;
use std::time::Duration;

/// Point d'accès public, en lecture seule, des données de marché Binance.
/// Modifiable par la variable d'environnement `PT_BINANCE_URL`.
pub const DEFAULT_BASE_URL: &str = "https://data-api.binance.vision";
const MAX_LIMIT: usize = 1000;

#[derive(Clone)]
pub struct BinanceClient {
    http: reqwest::Client,
    base: String,
}

/// Dernières bougies d'un symbole : les clôturées, et celle en cours.
#[derive(Debug, Clone)]
pub struct Recent {
    pub closed: Vec<Candle>,
    pub forming: Option<Candle>,
}

impl Default for BinanceClient {
    fn default() -> Self {
        Self::new()
    }
}

impl BinanceClient {
    pub fn new() -> Self {
        let base = std::env::var("PT_BINANCE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
        Self::with_base(&base)
    }

    pub fn with_base(base: &str) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("PaperTrading2")
            .build()
            .expect("client HTTP");
        BinanceClient { http, base: base.trim_end_matches('/').to_string() }
    }

    async fn klines(&self, symbol: &str, tf: Timeframe, start: Option<i64>, limit: usize) -> Result<Vec<Candle>> {
        let mut url = format!("{}/api/v3/klines?symbol={}&interval={}&limit={}", self.base, symbol, tf.as_str(), limit);
        if let Some(s) = start {
            url.push_str(&format!("&startTime={s}"));
        }
        let resp = self.http.get(&url).send().await.with_context(|| format!("requête Binance {symbol} {tf}"))?;
        let status = resp.status();
        let body: Value = resp.json().await.with_context(|| format!("réponse Binance illisible pour {symbol}"))?;
        if !status.is_success() {
            bail!(
                "Binance a refusé {symbol} {tf} ({status}) : {}",
                body.get("msg").and_then(|m| m.as_str()).unwrap_or("?")
            );
        }
        parse_klines(&body)
    }

    /// Toutes les bougies CLÔTURÉES entre `start_ms` et maintenant.
    pub async fn closed_since(&self, symbol: &str, tf: Timeframe, start_ms: i64) -> Result<Vec<Candle>> {
        let now = crate::now_ms();
        let mut out: Vec<Candle> = Vec::new();
        let mut cursor = start_ms;
        loop {
            let batch = self.klines(symbol, tf, Some(cursor), MAX_LIMIT).await?;
            let n = batch.len();
            for c in batch {
                if c.is_closed_at(now) && out.last().is_none_or(|l| c.open_time > l.open_time) {
                    out.push(c);
                }
            }
            match out.last() {
                Some(last) if n == MAX_LIMIT && last.close_time + 1 < now => cursor = last.close_time + 1,
                _ => break,
            }
        }
        Ok(out)
    }

    /// Les `limit` dernières bougies clôturées, plus la bougie en cours.
    pub async fn recent(&self, symbol: &str, tf: Timeframe, limit: usize) -> Result<Recent> {
        let now = crate::now_ms();
        let mut all = self.klines(symbol, tf, None, (limit + 1).min(MAX_LIMIT)).await?;
        let forming = match all.last() {
            Some(c) if !c.is_closed_at(now) => all.pop(),
            _ => None,
        };
        Ok(Recent { closed: all, forming })
    }
}

pub fn parse_klines(body: &Value) -> Result<Vec<Candle>> {
    let rows = body.as_array().context("format de bougies inattendu")?;
    let num = |v: &Value| -> Result<f64> {
        match v {
            Value::String(s) => s.parse::<f64>().context("nombre invalide"),
            Value::Number(n) => n.as_f64().context("nombre invalide"),
            _ => bail!("champ numérique attendu"),
        }
    };
    rows.iter()
        .map(|r| {
            let a = r.as_array().context("bougie invalide")?;
            if a.len() < 7 {
                bail!("bougie incomplète");
            }
            Ok(Candle {
                open_time: a[0].as_i64().context("open_time")?,
                open: num(&a[1])?,
                high: num(&a[2])?,
                low: num(&a[3])?,
                close: num(&a[4])?,
                volume: num(&a[5])?,
                close_time: a[6].as_i64().context("close_time")?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_binance_rows() {
        let body: Value = serde_json::from_str(
            r#"[[1502928000000,"4261.48","4485.39","4200.74","4285.08","795.15",1503014399999,"0",0,"0","0","0"]]"#,
        )
        .unwrap();
        let c = parse_klines(&body).unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].close, 4285.08);
        assert_eq!(c[0].close_time, 1503014399999);
    }

    /// Test réseau, lancé seulement avec `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn live_recent_separates_forming_candle() {
        let r = BinanceClient::new().recent("BTCUSDT", Timeframe::H1, 50).await.unwrap();
        let now = crate::now_ms();
        assert!(r.closed.iter().all(|c| c.is_closed_at(now)));
        assert!(r.forming.is_none_or(|c| !c.is_closed_at(now)));
        assert_eq!(r.closed.len(), 50);
    }
}
