//! Cache disque des bougies clôturées (un fichier CSV par symbole et unité de temps).
//! Une bougie clôturée ne change plus : on ne télécharge que ce qui manque.

use crate::bitvavo::BitvavoClient;
use anyhow::{Context, Result};
use pt_core::{Candle, Timeframe};
use std::path::{Path, PathBuf};

pub struct HistoryCache {
    dir: PathBuf,
    client: BitvavoClient,
}

impl HistoryCache {
    pub fn new(dir: impl AsRef<Path>, client: BitvavoClient) -> Self {
        HistoryCache { dir: dir.as_ref().to_path_buf(), client }
    }

    fn file(&self, symbol: &str, tf: Timeframe) -> PathBuf {
        self.dir.join(format!("{}_{}.csv", symbol.to_uppercase(), tf.as_str()))
    }

    fn read(path: &Path) -> Vec<Candle> {
        let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
        text.lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.split(',').collect();
                if f.len() != 7 {
                    return None;
                }
                Some(Candle {
                    open_time: f[0].parse().ok()?,
                    open: f[1].parse().ok()?,
                    high: f[2].parse().ok()?,
                    low: f[3].parse().ok()?,
                    close: f[4].parse().ok()?,
                    volume: f[5].parse().ok()?,
                    close_time: f[6].parse().ok()?,
                })
            })
            .collect()
    }

    fn write(path: &Path, candles: &[Candle]) -> Result<()> {
        let mut s = String::with_capacity(candles.len() * 80);
        for c in candles {
            s.push_str(&format!(
                "{},{},{},{},{},{},{}\n",
                c.open_time, c.open, c.high, c.low, c.close, c.volume, c.close_time
            ));
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, s).with_context(|| format!("écriture {}", tmp.display()))?;
        std::fs::rename(&tmp, path).with_context(|| format!("remplacement {}", path.display()))?;
        Ok(())
    }

    /// Fichier qui retient la date la plus ancienne déjà demandée à la plateforme pour ce
    /// symbole. Sans lui, un symbole coté APRÈS la date demandée (SOL en 2020 pour un
    /// historique de 10 ans) paraissait toujours incomplet et était retéléchargé en
    /// entier à chaque appel.
    fn requested_file(&self, symbol: &str, tf: Timeframe) -> PathBuf {
        self.dir.join(format!("{}_{}.depuis", symbol.to_uppercase(), tf.as_str()))
    }

    /// Bougies clôturées depuis `start_ms`, complétées depuis Bitvavo si besoin.
    pub async fn history(&self, symbol: &str, tf: Timeframe, start_ms: i64) -> Result<Vec<Candle>> {
        std::fs::create_dir_all(&self.dir).with_context(|| format!("création {}", self.dir.display()))?;
        let path = self.file(symbol, tf);
        let requested_path = self.requested_file(symbol, tf);
        let mut cached = Self::read(&path);
        let already_requested = std::fs::read_to_string(&requested_path)
            .ok()
            .and_then(|s| s.trim().parse::<i64>().ok())
            .is_some_and(|from| from <= start_ms);
        let covers_start = !cached.is_empty() && (already_requested || cached[0].open_time <= start_ms + tf.millis());
        if !covers_start {
            cached = self.client.closed_since(symbol, tf, start_ms).await?;
        } else if let Some(last) = cached.last().copied() {
            let fresh = self.client.closed_since(symbol, tf, last.close_time + 1).await?;
            cached.extend(fresh.into_iter().filter(|c| c.open_time > last.open_time));
        }
        pt_core::candle::validate_series(&cached).map_err(|e| anyhow::anyhow!("{symbol} {tf} : {e}"))?;
        if cached.is_empty() {
            anyhow::bail!("{symbol} {tf} : aucune bougie (paire non cotée en EUR sur Bitvavo ?)");
        }
        Self::write(&path, &cached)?;
        // Écrit APRÈS les bougies : un téléchargement interrompu ne marque jamais
        // comme couverte une période absente du fichier.
        if !covers_start {
            std::fs::write(&requested_path, start_ms.to_string())
                .with_context(|| format!("écriture {}", requested_path.display()))?;
        }
        Ok(cached.into_iter().filter(|c| c.open_time >= start_ms).collect())
    }

    /// Historique de plusieurs symboles, indexé par symbole.
    pub async fn series(
        &self,
        symbols: &[String],
        tf: Timeframe,
        start_ms: i64,
    ) -> Result<std::collections::BTreeMap<String, Vec<Candle>>> {
        let mut out = std::collections::BTreeMap::new();
        for s in symbols {
            out.insert(s.clone(), self.history(s, tf, start_ms).await?);
        }
        Ok(out)
    }
}
