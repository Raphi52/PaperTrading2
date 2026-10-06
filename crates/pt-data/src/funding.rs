//! Taux de financement réglés des contrats perpétuels Binance USDⓈ-M.
//!
//! Données publiques, sans clé : `GET /fapi/v1/fundingRate` rend au plus 1000
//! taux par appel, un toutes les 8 h par symbole. Un taux réglé ne change plus :
//! le cache disque ne retélécharge que ce qui manque.

use anyhow::{bail, Context, Result};
use pt_core::carry::FundingEvent;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Point d'accès public des contrats perpétuels. Modifiable par `PT_BINANCE_FUTURES_URL`.
pub const DEFAULT_FUTURES_URL: &str = "https://fapi.binance.com";
const LIMIT: usize = 1000;

#[derive(Clone)]
pub struct FundingClient {
    http: reqwest::Client,
    base: String,
}

impl Default for FundingClient {
    fn default() -> Self {
        Self::new()
    }
}

impl FundingClient {
    pub fn new() -> Self {
        let base = std::env::var("PT_BINANCE_FUTURES_URL").unwrap_or_else(|_| DEFAULT_FUTURES_URL.to_string());
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("PaperTrading2")
            .build()
            .expect("client HTTP");
        FundingClient { http, base: base.trim_end_matches('/').to_string() }
    }

    /// Taux réglés depuis `from` (inclus), triés et sans doublon.
    pub async fn since(&self, symbol: &str, from: i64) -> Result<Vec<FundingEvent>> {
        let mut out: Vec<FundingEvent> = Vec::new();
        let mut start = from;
        loop {
            let url = format!("{}/fapi/v1/fundingRate?symbol={symbol}&startTime={start}&limit={LIMIT}", self.base);
            let resp = self.http.get(&url).send().await.with_context(|| format!("financement {symbol}"))?;
            let status = resp.status();
            let body: Value = resp.json().await.with_context(|| format!("réponse illisible pour {symbol}"))?;
            if !status.is_success() {
                bail!("financement {symbol} : {status} {body}");
            }
            let batch = parse(&body).with_context(|| format!("financement {symbol}"))?;
            let n = batch.len();
            let last = batch.last().map(|e| e.time);
            out.extend(batch.into_iter().filter(|e| e.time >= start));
            match last {
                Some(t) if n == LIMIT => start = t + 1,
                _ => break,
            }
        }
        out.sort_by_key(|e| e.time);
        out.dedup_by_key(|e| e.time);
        Ok(out)
    }
}

fn parse(body: &Value) -> Result<Vec<FundingEvent>> {
    let arr = body.as_array().context("tableau attendu")?;
    arr.iter()
        .map(|x| {
            let time = x["fundingTime"].as_i64().context("fundingTime absent")?;
            let rate: f64 = x["fundingRate"].as_str().context("fundingRate absent")?.parse().context("fundingRate")?;
            Ok(FundingEvent { time, rate })
        })
        .collect()
}

/// Cache disque : un fichier `SYMBOLE_financement.csv` par symbole (`temps,taux`).
pub struct FundingCache {
    dir: PathBuf,
    client: FundingClient,
}

impl FundingCache {
    pub fn new(dir: impl AsRef<Path>, client: FundingClient) -> Self {
        FundingCache { dir: dir.as_ref().to_path_buf(), client }
    }

    fn file(&self, symbol: &str) -> PathBuf {
        self.dir.join(format!("{}_financement.csv", symbol.to_uppercase()))
    }

    fn requested_file(&self, symbol: &str) -> PathBuf {
        self.dir.join(format!("{}_financement.depuis", symbol.to_uppercase()))
    }

    fn read(path: &Path) -> Vec<FundingEvent> {
        let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
        text.lines()
            .filter_map(|l| {
                let (t, r) = l.split_once(',')?;
                Some(FundingEvent { time: t.parse().ok()?, rate: r.parse().ok()? })
            })
            .collect()
    }

    fn write(path: &Path, events: &[FundingEvent]) -> Result<()> {
        let mut s = String::with_capacity(events.len() * 28);
        for e in events {
            s.push_str(&format!("{},{}\n", e.time, e.rate));
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, s).with_context(|| format!("écriture {}", tmp.display()))?;
        std::fs::rename(&tmp, path).with_context(|| format!("remplacement {}", path.display()))?;
        Ok(())
    }

    /// Taux réglés depuis `start_ms`, complétés depuis Binance si besoin.
    pub async fn history(&self, symbol: &str, start_ms: i64) -> Result<Vec<FundingEvent>> {
        std::fs::create_dir_all(&self.dir).with_context(|| format!("création {}", self.dir.display()))?;
        let path = self.file(symbol);
        let requested_path = self.requested_file(symbol);
        let mut cached = Self::read(&path);
        let already = std::fs::read_to_string(&requested_path)
            .ok()
            .and_then(|s| s.trim().parse::<i64>().ok())
            .is_some_and(|from| from <= start_ms);
        let covers = !cached.is_empty() && (already || cached[0].time <= start_ms + 8 * 3_600_000);
        if !covers {
            cached = self.client.since(symbol, start_ms).await?;
        } else if let Some(last) = cached.last().copied() {
            let fresh = self.client.since(symbol, last.time + 1).await?;
            cached.extend(fresh.into_iter().filter(|e| e.time > last.time));
        }
        if cached.is_empty() {
            bail!("{symbol} : aucun taux de financement (pas de contrat perpétuel USDⓈ-M ?)");
        }
        Self::write(&path, &cached)?;
        if !covers {
            std::fs::write(&requested_path, start_ms.to_string())
                .with_context(|| format!("écriture {}", requested_path.display()))?;
        }
        Ok(cached.into_iter().filter(|e| e.time >= start_ms).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_binance_funding_rows() {
        let body: Value = serde_json::from_str(
            r#"[{"symbol":"BTCUSDT","fundingTime":1568102400000,"fundingRate":"0.00010000","markPrice":""},
                {"symbol":"BTCUSDT","fundingTime":1568131200000,"fundingRate":"-0.00002695","markPrice":""}]"#,
        )
        .unwrap();
        let v = parse(&body).unwrap();
        assert_eq!(
            v,
            vec![
                FundingEvent { time: 1568102400000, rate: 0.0001 },
                FundingEvent { time: 1568131200000, rate: -0.00002695 },
            ]
        );
        assert!(parse(&serde_json::json!({"code": -1121})).is_err());
    }

    /// Réseau réel : `cargo test -p pt-data -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn real_funding_history_is_complete() {
        let v = FundingClient::new().since("BTCUSDT", 1_567_296_000_000).await.unwrap();
        assert!(v.len() > 7000, "{} taux", v.len());
        assert!(v.windows(2).all(|w| w[1].time > w[0].time));
    }
}
