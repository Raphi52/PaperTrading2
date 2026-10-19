//! Cache disque des bougies clôturées (un fichier CSV par symbole et unité de temps).
//! Une bougie clôturée ne change plus : on ne télécharge que ce qui manque.

use crate::bitvavo::BitvavoClient;
use anyhow::{Context, Result};
use pt_core::{Candle, Timeframe};
use std::path::{Path, PathBuf};

/// Marque des fichiers écrits par le téléchargeur courant. `history` ne fait que compléter
/// la fin d'un fichier : un défaut de lecture passé y resterait pour toujours, d'où un
/// retéléchargement complet, une fois, de tout fichier d'une autre marque.
/// - « 2 » (2026-10-11) : correction des raccords de pages (une bougie perdue toutes les 1 000) ;
/// - « 3 » (2026-10-19) : la lecture ne s'arrête plus à la première page de moins de 1 000
///   bougies (une paire peu échangée ne gardait que ses ~1 050 derniers créneaux :
///   LTCEUR_15m, 990 bougies au lieu de plus de 240 000).
const CONTENT_VERSION: &str = "3";

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

    /// Fichier qui porte la marque `CONTENT_VERSION` du fichier de bougies.
    fn version_file(&self, symbol: &str, tf: Timeframe) -> PathBuf {
        self.dir.join(format!("{}_{}.version", symbol.to_uppercase(), tf.as_str()))
    }

    /// Bougies clôturées depuis `start_ms`, complétées depuis Bitvavo si besoin.
    pub async fn history(&self, symbol: &str, tf: Timeframe, start_ms: i64) -> Result<Vec<Candle>> {
        std::fs::create_dir_all(&self.dir).with_context(|| format!("création {}", self.dir.display()))?;
        let path = self.file(symbol, tf);
        let requested_path = self.requested_file(symbol, tf);
        let version_path = self.version_file(symbol, tf);
        let mut cached = Self::read(&path);
        let requested_from = std::fs::read_to_string(&requested_path).ok().and_then(|s| s.trim().parse::<i64>().ok());
        let already_requested = requested_from.is_some_and(|from| from <= start_ms);
        // fix-ok: cause mesurée — `history` ne fait que compléter la fin d'un fichier ; les bougies perdues aux raccords de pages avant la correction de `closed_since` (data/cache/BTCEUR_15m.csv : 196 trous alignés sur les pages de 1 000, BTCEUR_4h : 16) y seraient restées pour toujours. Test : un_cache_d_avant_les_raccords_est_retelecharge_en_entier, rouge avant (rouge-*.log), vert ici.
        // Fichier sans la marque : retéléchargé en entier une fois, depuis le plus ancien
        // début connu, puis marqué.
        let marked = std::fs::read_to_string(&version_path).is_ok_and(|v| v.trim() == CONTENT_VERSION);
        let from = if marked {
            start_ms
        } else {
            [Some(start_ms), requested_from, cached.first().map(|c| c.open_time)].into_iter().flatten().min().unwrap_or(start_ms)
        };
        let covers_start =
            marked && !cached.is_empty() && (already_requested || cached[0].open_time <= start_ms + tf.millis());
        if !covers_start {
            cached = self.client.closed_since(symbol, tf, from).await?;
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
            std::fs::write(&requested_path, from.to_string())
                .with_context(|| format!("écriture {}", requested_path.display()))?;
            std::fs::write(&version_path, CONTENT_VERSION)
                .with_context(|| format!("écriture {}", version_path.display()))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::faux_bitvavo;

    /// Dossier de cache neuf, propre à ce test.
    fn dossier(nom: &str) -> PathBuf {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("pt-data-test-{nom}-{}-{n}", std::process::id()))
    }

    fn bougie(o: i64, q: i64) -> Candle {
        Candle { open_time: o, open: 1.0, high: 1.0, low: 1.0, close: 1.0, volume: 1.0, close_time: o + q - 1 }
    }

    fn ouvertures(v: &[Candle]) -> Vec<i64> {
        v.iter().map(|c| c.open_time).collect()
    }

    /// Un cache écrit avant la correction des raccords de pages (une bougie perdue toutes
    /// les 1 000) est retéléchargé en entier une fois, depuis le plus ancien début connu,
    /// puis marqué. Jumeau : un cache marqué n'est plus que complété par la fin.
    #[tokio::test]
    async fn un_cache_d_avant_les_raccords_est_retelecharge_en_entier() {
        let tf = Timeframe::M15;
        let q = tf.millis();
        let now = crate::now_ms();
        // Le marché s'arrête 10 créneaux avant maintenant : la lecture parcourt chaque
        // créneau jusqu'à maintenant, un marché éteint depuis des années coûterait une
        // requête par tranche de 1 000 créneaux vides.
        let t0 = (now / q - 2510) * q;
        let marche: Vec<i64> = (0..2500).map(|i| t0 + i * q).collect(); // tout est clos
        let faux = faux_bitvavo::lancer(q, marche.clone(), now);
        let cache = HistoryCache::new(dossier("raccords"), BitvavoClient::with_base(&faux.base));
        std::fs::create_dir_all(&cache.dir).unwrap();
        // Fichier d'avant la correction : deux bougies perdues à des raccords, demandé depuis t0.
        let troue: Vec<Candle> =
            marche.iter().copied().filter(|&o| o != t0 + 499 * q && o != t0 + 1499 * q).map(|o| bougie(o, q)).collect();
        HistoryCache::write(&cache.file("BTCEUR", tf), &troue).unwrap();
        std::fs::write(cache.requested_file("BTCEUR", tf), t0.to_string()).unwrap();

        let start = t0 + 1000 * q;
        let v = cache.history("BTCEUR", tf, start).await.unwrap();
        assert_eq!(ouvertures(&v), marche[1000..], "bougies rendues");
        assert_eq!(ouvertures(&HistoryCache::read(&cache.file("BTCEUR", tf))), marche, "fichier réécrit en entier");
        assert_eq!(
            faux.requetes().iter().map(|r| r.start).min(),
            Some(t0),
            "retéléchargé depuis le plus ancien début connu"
        );

        faux.oublier();
        let v2 = cache.history("BTCEUR", tf, start).await.unwrap();
        assert_eq!(ouvertures(&v2), marche[1000..]);
        let r = faux.requetes();
        assert_eq!(r.len(), 1, "un cache marqué ne fait qu'une requête : {r:?}");
        assert_eq!(r[0].start, t0 + 2500 * q, "seule la fin est demandée");
        std::fs::remove_dir_all(&cache.dir).ok();
    }

    /// Un cache marqué « 2 » a été écrit par le téléchargeur qui s'arrêtait à la première
    /// page de moins de 1 000 bougies (avant le 2026-10-19) : pour une paire peu échangée,
    /// il ne garde que les ~1 050 derniers créneaux (data/cache/LTCEUR_15m.csv : 990
    /// bougies depuis le 2026-10-08). Il est retéléchargé en entier une fois.
    #[tokio::test]
    async fn un_cache_tronque_a_la_premiere_page_incomplete_est_retelecharge() {
        let tf = Timeframe::M15;
        let q = tf.millis();
        let now = crate::now_ms();
        let c = now / q * q;
        let t0 = c - 3000 * q;
        // Un créneau sur 6 sans échange ; dernière bougie bien avant maintenant.
        let marche: Vec<i64> = (0..2990).map(|i| t0 + i * q).filter(|o| ((o - t0) / q) % 6 != 2).collect();
        let faux = faux_bitvavo::lancer(q, marche.clone(), now);
        let cache = HistoryCache::new(dossier("tronque"), BitvavoClient::with_base(&faux.base));
        std::fs::create_dir_all(&cache.dir).unwrap();
        let tronque: Vec<Candle> = marche.iter().copied().filter(|&o| o >= c - 1050 * q).map(|o| bougie(o, q)).collect();
        HistoryCache::write(&cache.file("LTCEUR", tf), &tronque).unwrap();
        std::fs::write(cache.requested_file("LTCEUR", tf), t0.to_string()).unwrap();
        std::fs::write(cache.version_file("LTCEUR", tf), "2").unwrap();

        let v = cache.history("LTCEUR", tf, t0).await.unwrap();
        assert_eq!(ouvertures(&v).len(), marche.len(), "bougies rendues");
        assert_eq!(ouvertures(&v), marche);
        assert_eq!(ouvertures(&HistoryCache::read(&cache.file("LTCEUR", tf))), marche, "fichier réécrit en entier");
        assert_eq!(std::fs::read_to_string(cache.version_file("LTCEUR", tf)).unwrap(), CONTENT_VERSION);
        std::fs::remove_dir_all(&cache.dir).ok();
    }
}
