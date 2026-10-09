//! Bougies au comptant de Bitvavo, en paires EUR.
//!
//! La bougie en cours est TOUJOURS séparée des bougies clôturées : le moteur ne
//! décide que sur des bougies terminées, et n'utilise la bougie en cours que pour
//! son prix d'ouverture (exécution) et son dernier prix (valorisation).
//!
//! Le symbole interne reste collé (`BTCEUR`) : c'est lui qu'on trouve dans la base,
//! les noms des fichiers du cache et les écrans. Seul ce client le traduit en
//! identifiant de marché Bitvavo (`BTC-EUR`).
//!
//! Comportements de l'API mesurés le 2026-10-08 (`/v2/{marché}/candles`) :
//! - au plus 1 000 bougies par requête (`limit` plus grand : refus 400) ;
//! - les bougies arrivent de la plus RÉCENTE à la plus ancienne ;
//! - la réponse rend toujours les `limit` bougies les plus récentes de la période
//!   demandée : on remonte donc le temps en déplaçant la fin de période (`end`) ;
//! - une bougie n'a pas d'heure de clôture : elle vaut ouverture + durée − 1 ms ;
//! - prix et volumes arrivent en texte.

use anyhow::{bail, Context, Result};
use pt_core::{Candle, Timeframe};
use serde_json::Value;
use std::collections::BTreeSet;
use std::time::Duration;

/// Point d'accès public, en lecture seule et sans clé, des données de marché Bitvavo.
/// Modifiable par la variable d'environnement `PT_MARKET_URL`.
pub const DEFAULT_BASE_URL: &str = "https://api.bitvavo.com";
const MAX_LIMIT: usize = 1000;
/// 2019-01-01 UTC, avant la première bougie de Bitvavo (BTC-EUR : 2019-03-08).
/// Une période qui commence plus tôt part de là.
const FIRST_CANDLE_MS: i64 = 1_546_300_800_000;

/// Source des bougies : celle du client par défaut.
pub fn market_url() -> String {
    std::env::var("PT_MARKET_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string())
}

#[derive(Clone)]
pub struct BitvavoClient {
    http: reqwest::Client,
    base: String,
}

/// Dernières bougies d'un symbole : les clôturées, et celle en cours.
#[derive(Debug, Clone)]
pub struct Recent {
    pub closed: Vec<Candle>,
    pub forming: Option<Candle>,
}

impl Default for BitvavoClient {
    fn default() -> Self {
        Self::new()
    }
}

/// `BTCEUR` → `BTC-EUR`. Seules les paires en EUR sont acceptées.
pub fn market_id(symbol: &str) -> Result<String> {
    match symbol.strip_suffix(crate::universe::QUOTE) {
        Some(base) if !base.is_empty() => Ok(format!("{base}-{}", crate::universe::QUOTE)),
        _ => bail!("{symbol} : seules les paires en EUR sont achetables (ex. BTCEUR)"),
    }
}

impl BitvavoClient {
    pub fn new() -> Self {
        Self::with_base(&market_url())
    }

    pub fn with_base(base: &str) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("PaperTrading")
            .build()
            .expect("client HTTP");
        BitvavoClient { http, base: base.trim_end_matches('/').to_string() }
    }

    async fn get(&self, url: &str, what: &str) -> Result<Value> {
        let resp = self.http.get(url).send().await.with_context(|| format!("requête Bitvavo {what}"))?;
        let status = resp.status();
        let body: Value = resp.json().await.with_context(|| format!("réponse Bitvavo illisible pour {what}"))?;
        if !status.is_success() {
            bail!(
                "Bitvavo a refusé {what} ({status}) : {}",
                body.get("error").and_then(|m| m.as_str()).unwrap_or("?")
            );
        }
        Ok(body)
    }

    /// Les `limit` bougies les plus récentes entre `start` et `end`, triées par ouverture croissante.
    async fn candles(
        &self,
        symbol: &str,
        tf: Timeframe,
        start: Option<i64>,
        end: Option<i64>,
        limit: usize,
    ) -> Result<Vec<Candle>> {
        let market = market_id(symbol)?;
        let url = candles_url(&self.base, &market, tf, start, end, limit);
        let body = self.get(&url, &format!("{market} {tf}")).await?;
        parse_candles(&body, tf)
    }

    /// Toutes les bougies CLÔTURÉES entre `start_ms` et maintenant, par ouverture croissante.
    pub async fn closed_since(&self, symbol: &str, tf: Timeframe, start_ms: i64) -> Result<Vec<Candle>> {
        let now = crate::now_ms();
        let start = start_ms.max(FIRST_CANDLE_MS);
        let mut pages: Vec<Vec<Candle>> = Vec::new();
        let mut end = now;
        while end >= start {
            let batch = self.candles(symbol, tf, Some(page_start(start, end, tf)), Some(end), MAX_LIMIT).await?;
            let Some(first) = batch.first().copied() else { break };
            let full = batch.len() == MAX_LIMIT;
            pages.push(batch);
            // Page incomplète : la période est épuisée (ou la paire n'était pas encore cotée).
            if !full || first.open_time <= start || first.open_time > end {
                break;
            }
            end = first.open_time - 1;
        }
        Ok(assemble(pages, start_ms, now))
    }

    /// Les `limit` dernières bougies clôturées, plus la bougie en cours.
    pub async fn recent(&self, symbol: &str, tf: Timeframe, limit: usize) -> Result<Recent> {
        let now = crate::now_ms();
        let mut all = self.candles(symbol, tf, None, None, limit + 1).await?;
        let forming = match all.last() {
            Some(c) if !c.is_closed_at(now) => all.pop(),
            _ => None,
        };
        Ok(Recent { closed: all, forming })
    }

    /// Paires en EUR ouvertes au négoce maintenant (`/v2/markets`, `status = trading`).
    pub async fn eur_markets(&self) -> Result<BTreeSet<String>> {
        let body = self.get(&format!("{}/v2/markets", self.base), "liste des marchés").await?;
        Ok(crate::universe::tradable_eur(&body))
    }
}

/// Début demandé pour une page `[start, end]` : Bitvavo refuse (400, errorCode 205) une
/// fenêtre plus étroite qu'un intervalle (sondé le 2026-10-09 sur BTC-EUR 15m :
/// `[t, t+899999]` refusé, `[t, t+900000]` accepté). C'est le cas après une page pleine qui
/// s'arrête une bougie après `start`. Le début est alors reculé d'assez pour couvrir un
/// intervalle, ce qui garde la bougie ouverte à `start` ; les plus anciennes sont écartées
/// par `assemble`.
fn page_start(start: i64, end: i64, tf: Timeframe) -> i64 {
    start.min(end - tf.millis())
}

/// Adresse d'une page de bougies : jamais plus de 1 000 par requête (au-delà, refus 400).
fn candles_url(base: &str, market: &str, tf: Timeframe, start: Option<i64>, end: Option<i64>, limit: usize) -> String {
    let mut url = format!("{base}/v2/{market}/candles?interval={}&limit={}", tf.as_str(), limit.clamp(1, MAX_LIMIT));
    if let Some(s) = start {
        url.push_str(&format!("&start={s}"));
    }
    if let Some(e) = end {
        url.push_str(&format!("&end={e}"));
    }
    url
}

/// Réunit des pages lues en remontant le temps : ordre croissant, sans doublon,
/// clôturées à `now`, ouvertes à partir de `start_ms`.
fn assemble(pages: Vec<Vec<Candle>>, start_ms: i64, now: i64) -> Vec<Candle> {
    let mut out: Vec<Candle> =
        pages.into_iter().flatten().filter(|c| c.open_time >= start_ms && c.is_closed_at(now)).collect();
    out.sort_by_key(|c| c.open_time);
    out.dedup_by_key(|c| c.open_time);
    out
}

/// Lignes `[ouverture, "o", "h", "l", "c", "volume"]`, de la plus récente à la plus
/// ancienne : rendues par ouverture croissante, avec l'heure de clôture calculée.
pub fn parse_candles(body: &Value, tf: Timeframe) -> Result<Vec<Candle>> {
    let rows = body.as_array().context("format de bougies inattendu")?;
    let num = |v: &Value| -> Result<f64> {
        match v {
            Value::String(s) => s.parse::<f64>().with_context(|| format!("nombre invalide : {s}")),
            Value::Number(n) => n.as_f64().context("nombre invalide"),
            _ => bail!("champ numérique attendu"),
        }
    };
    let mut out = rows
        .iter()
        .map(|r| {
            let a = r.as_array().context("bougie invalide")?;
            if a.len() < 6 {
                bail!("bougie incomplète");
            }
            let open_time = a[0].as_i64().context("heure d'ouverture")?;
            Ok(Candle {
                open_time,
                open: num(&a[1])?,
                high: num(&a[2])?,
                low: num(&a[3])?,
                close: num(&a[4])?,
                volume: num(&a[5])?,
                close_time: open_time + tf.millis() - 1,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    out.sort_by_key(|c| c.open_time);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: i64 = 3_600_000;

    fn c(t: i64) -> Candle {
        Candle { open_time: t, open: 1.0, high: 1.0, low: 1.0, close: 1.0, volume: 1.0, close_time: t + H - 1 }
    }

    #[test]
    fn traduit_le_symbole_en_marche() {
        assert_eq!(market_id("BTCEUR").unwrap(), "BTC-EUR");
        assert_eq!(market_id("PEPEEUR").unwrap(), "PEPE-EUR");
        for bad in ["BTCUSDT", "EUR", "BTC"] {
            let e = market_id(bad).unwrap_err().to_string();
            assert!(e.contains("seules les paires en EUR"), "{bad} : {e}");
        }
    }

    /// Cas 16 et 17 : réponse la plus récente d'abord, prix en texte.
    #[test]
    fn trie_croissant_et_lit_les_prix_texte() {
        let body: Value = serde_json::from_str(
            r#"[[1791489600000,"72904","72995","72840","72907","18.8"],
                [1791486000000,"72800","72950","72700","72904","11.5"]]"#,
        )
        .unwrap();
        let v = parse_candles(&body, Timeframe::H1).unwrap();
        assert_eq!(v.iter().map(|c| c.open_time).collect::<Vec<_>>(), [1791486000000, 1791489600000]);
        assert_eq!(v[1].close, 72907.0);
        assert_eq!(v[0].close_time, 1791486000000 + H - 1);
        pt_core::candle::validate_series(&v).unwrap();
    }

    /// Cas 17 : un prix illisible ou nul fait échouer la série, jamais une valeur inventée.
    #[test]
    fn refuse_les_prix_invalides() {
        let txt: Value = serde_json::from_str(r#"[[1791489600000,"abc","1","1","1","1"]]"#).unwrap();
        assert!(parse_candles(&txt, Timeframe::H1).unwrap_err().to_string().contains("nombre invalide"));
        let zero: Value = serde_json::from_str(r#"[[1791489600000,"0","1","0","1","1"]]"#).unwrap();
        let v = parse_candles(&zero, Timeframe::H1).unwrap();
        assert!(pt_core::candle::validate_series(&v).unwrap_err().contains("prix invalide"));
        let short: Value = serde_json::from_str(r#"[[1791489600000,"1","1","1"]]"#).unwrap();
        assert!(parse_candles(&short, Timeframe::H1).is_err());
    }

    /// Cas 15 : une demande de plus de 1 000 bougies est découpée, jamais envoyée telle quelle.
    #[test]
    fn pages_de_1000_bougies_au_plus() {
        let u = candles_url("https://x", "BTC-EUR", Timeframe::M15, Some(1), Some(2), 5000);
        assert_eq!(u, "https://x/v2/BTC-EUR/candles?interval=15m&limit=1000&start=1&end=2");
        assert!(candles_url("https://x", "BTC-EUR", Timeframe::D1, None, None, 51).ends_with("interval=1d&limit=51"));
    }

    /// Cas 14 et 15 : pages lues à rebours, recouvrement, bougie en cours et début demandé.
    #[test]
    fn assemble_les_pages_a_rebours() {
        let now = 100 * H + 10;
        let recent: Vec<Candle> = (50..=100).map(|i| c(i * H)).collect(); // la 100e est en cours
        let older: Vec<Candle> = (10..=50).map(|i| c(i * H)).collect(); // recouvre la 50e
        let v = assemble(vec![recent, older], 20 * H, now);
        assert_eq!(v.first().unwrap().open_time, 20 * H);
        assert_eq!(v.last().unwrap().open_time, 99 * H);
        assert_eq!(v.len(), 80);
        pt_core::candle::validate_series(&v).unwrap();
        // Aucune page (paire pas encore cotée) : série vide, pas d'erreur.
        assert!(assemble(Vec::new(), 0, now).is_empty());
    }

    /// Une fenêtre d'au moins un intervalle est envoyée telle quelle ; une plus étroite est
    /// élargie vers le passé jusqu'à un intervalle exactement, jamais au-delà.
    #[test]
    fn fenetre_d_au_moins_un_intervalle() {
        let q = Timeframe::M15.millis();
        let t = 1_788_862_500_000; // ONDO-EUR 15m, 2026-10-09
        assert_eq!(page_start(t, t + q, Timeframe::M15), t); // limite acceptée par Bitvavo
        assert_eq!(page_start(t, t + 5 * q, Timeframe::M15), t);
        assert_eq!(page_start(t, t + q - 1, Timeframe::M15), t - 1); // refusée sinon
        assert_eq!(page_start(t, t, Timeframe::M15), t - q);
    }

    /// Faux Bitvavo local, sans réseau. Il applique les règles mesurées sur l'API :
    /// au plus 1 000 bougies, la plus récente d'abord, et refus 400 (errorCode 205)
    /// d'une fenêtre plus étroite qu'un intervalle (sondé le 2026-10-09 sur BTC-EUR 15m :
    /// `[t, t+899999]` refusé, `[t, t+900000]` accepté). Rend l'adresse et les fenêtres reçues.
    fn faux_bitvavo(tf: Timeframe) -> (String, std::sync::Arc<std::sync::Mutex<Vec<(i64, i64)>>>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = format!("http://{}", listener.local_addr().unwrap());
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { continue };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    match s.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                let req = String::from_utf8_lossy(&buf);
                let path = req.split_whitespace().nth(1).unwrap_or("");
                let param = |k: &str| -> Option<i64> {
                    path.split(['?', '&']).find_map(|kv| kv.strip_prefix(&format!("{k}="))?.parse().ok())
                };
                let now = crate::now_ms();
                let (start, end, limit) = (param("start").unwrap(), param("end").unwrap(), param("limit").unwrap());
                log.lock().unwrap().push((start, end));
                let step = tf.millis();
                let (status, body) = if limit > MAX_LIMIT as i64 || end - start < step {
                    ("400 Bad Request", r#"{"errorCode":205,"error":"Invalid parameters to get candlesticks."}"#.to_string())
                } else {
                    // Bougies cotées depuis FIRST_CANDLE_MS, jusqu'à celle en cours.
                    let from = start.max(FIRST_CANDLE_MS) - FIRST_CANDLE_MS;
                    let first = (from + step - 1) / step * step + FIRST_CANDLE_MS;
                    let last = end.min(now);
                    let mut t: Vec<i64> = (0..).map(|i| first + i * step).take_while(|&t| t <= last).collect();
                    t.reverse();
                    t.truncate(limit as usize);
                    let rows: Vec<String> = t.iter().map(|o| format!(r#"[{o},"1","1","1","1","1"]"#)).collect();
                    ("200 OK", format!("[{}]", rows.join(",")))
                };
                let _ = write!(
                    s,
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        (addr, seen)
    }

    /// Une période de 1 001 bougies (1 000 clôturées + celle en cours) : la page pleine
    /// s'arrête une bougie après le début, et la page suivante ne doit pas demander une
    /// fenêtre de moins d'un intervalle (cas ONDO-EUR 15m du 2026-10-09). Entrée qui ferait
    /// échouer une correction fausse : `start = ouverture en cours − 1 000 intervalles`
    /// — arrêter la boucle au lieu d'élargir la fenêtre perdrait la bougie ouverte à `start`.
    #[tokio::test]
    async fn jamais_de_fenetre_plus_etroite_qu_un_intervalle() {
        for tf in [Timeframe::M15, Timeframe::D1] {
            let (base, seen) = faux_bitvavo(tf);
            let step = tf.millis();
            let forming = (crate::now_ms() - FIRST_CANDLE_MS) / step * step + FIRST_CANDLE_MS;
            let start = forming - 1000 * step;
            let v = BitvavoClient::with_base(&base).closed_since("ONDOEUR", tf, start).await.unwrap();
            let windows = seen.lock().unwrap().clone();
            assert!(windows.iter().all(|(s, e)| e - s >= step), "{tf} : fenêtres demandées {windows:?}");
            assert_eq!(v.first().map(|c| c.open_time), Some(start), "{tf} : bougie de début perdue");
            assert_eq!(v.len(), 1000, "{tf}");
            pt_core::candle::validate_series(&v).unwrap();
        }
    }

    /// Test réseau, lancé seulement avec `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn live_recent_separates_forming_candle() {
        let client = BitvavoClient::new();
        for tf in Timeframe::ALL {
            let r = client.recent("BTCEUR", tf, 50).await.unwrap();
            let now = crate::now_ms();
            assert!(r.closed.iter().all(|c| c.is_closed_at(now)), "{tf}");
            assert!(r.forming.is_none_or(|c| !c.is_closed_at(now)), "{tf}");
            assert!(r.closed.len() >= 50, "{tf} : {}", r.closed.len());
            pt_core::candle::validate_series(&r.closed).unwrap();
        }
    }

    /// Test réseau : deux ans d'historique horaire, sans désordre, et sans trou laissé par
    /// la pagination. Bitvavo a lui-même de courtes interruptions (mesuré le 2026-10-08 sur
    /// BTC-EUR : 32 trous de 2 à 6 h en deux ans, la plupart un dimanche dans la nuit, absents
    /// aussi d'une requête ciblée sur l'intervalle) : on n'y tradait pas non plus en vrai.
    /// Une page perdue, elle, ferait un trou de 1 000 h.
    #[tokio::test]
    #[ignore]
    async fn live_two_years_hourly_history_is_continuous() {
        let start = crate::now_ms() - 730 * 86_400_000;
        let v = BitvavoClient::new().closed_since("BTCEUR", Timeframe::H1, start).await.unwrap();
        pt_core::candle::validate_series(&v).unwrap();
        assert!(v[0].open_time - start < H, "première bougie trop tardive");
        let max_gap = v.windows(2).map(|w| w[1].open_time - w[0].open_time).max().unwrap();
        // fix-ok: le 1er passage (t10.log) échouait sur « trou de 6 h » ; cause mesurée par requête ciblée sur l'API : interruption réelle de Bitvavo, pas la pagination. Seuil 12 h ; une page perdue (1 000 h) reste détectée (défaut réinjecté, t12r.log rouge).
        assert!(max_gap <= 12 * H, "trou de {} h", max_gap / H);
        let expected = (v.last().unwrap().open_time - v[0].open_time) / H + 1;
        assert!(v.len() as f64 >= 0.99 * expected as f64, "{} bougies sur {expected} heures", v.len());
    }

    /// Test réseau : une période qui commence avant la cotation part de la cotation.
    #[tokio::test]
    #[ignore]
    async fn live_start_before_listing_is_not_an_error() {
        let v = BitvavoClient::new().closed_since("TAOEUR", Timeframe::D1, 0).await.unwrap();
        assert!(!v.is_empty());
        pt_core::candle::validate_series(&v).unwrap();
    }

    /// Test réseau : la liste des marchés en EUR contient BTC et pas TUT.
    #[tokio::test]
    #[ignore]
    async fn live_eur_markets() {
        let m = BitvavoClient::new().eur_markets().await.unwrap();
        assert!(m.contains("BTCEUR") && !m.contains("TUTEUR"), "{} marchés", m.len());
    }
}
