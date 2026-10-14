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
//! - au plus 1 000 bougies par requête (`limit` plus grand : refus 400). Le 2026-10-11,
//!   1 001, 1 440 et 1 441 ont été acceptés ; le client reste à 1 000 (`MAX_LIMIT`) ;
//! - les bougies arrivent de la plus RÉCENTE à la plus ancienne ;
//! - la réponse rend toujours les `limit` bougies les plus récentes de la période
//!   demandée : on remonte donc le temps en déplaçant la fin de période (`end`) ;
//! - (sondé le 2026-10-09 et le 2026-10-11) une bougie close n'est rendue que si
//!   `start ≤ ouverture` et `ouverture + durée ≤ end` ; la bougie en cours, si
//!   `start ≤ ouverture ≤ end` ; une fenêtre où aucune ne peut l'être est refusée
//!   (400, errorCode 205) ; un créneau sans échange n'est pas rendu ;
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
        self.closed_until(symbol, tf, start_ms, crate::now_ms()).await
    }

    /// `closed_since` vu à l'heure `now` (les tests sans réseau fixent l'heure).
    ///
    /// Bitvavo ne rend une bougie close que si `start ≤ ouverture` et `ouverture + Q ≤ end`
    /// (Q = durée d'une bougie), et refuse (400, errorCode 205) une fenêtre où aucune ne
    /// peut l'être. Après une page pleine dont la plus ancienne ouverture est F, la page
    /// suivante se termine donc à F (elle rend F − Q), et la lecture s'arrête dès que
    /// F − Q < `start` : il ne reste aucun créneau entier à lire.
    async fn closed_until(&self, symbol: &str, tf: Timeframe, start_ms: i64, now: i64) -> Result<Vec<Candle>> {
        let q = tf.millis();
        let start = start_ms.max(FIRST_CANDLE_MS);
        // Premier créneau entier à partir de `start`. S'il n'est pas clos à `now`, aucune
        // bougie close n'existe ; `[start, now]` serait même refusée si `start` tombe dans
        // la bougie en cours sans être aligné.
        let first_slot = start.div_euclid(q) * q + if start.rem_euclid(q) == 0 { 0 } else { q };
        if first_slot + q > now {
            return Ok(Vec::new());
        }
        let mut pages: Vec<Vec<Candle>> = Vec::new();
        let mut end = now;
        while end >= start {
            let batch = self.candles(symbol, tf, Some(start), Some(end), MAX_LIMIT).await?;
            let Some(first) = batch.first().copied() else { break };
            let full = batch.len() == MAX_LIMIT;
            pages.push(batch);
            // fix-ok: cause mesurée — la page suivante finissait à `F − 1` ; Bitvavo ne rendant une bougie close que si ouverture + Q ≤ end, la bougie F − Q était perdue à chaque raccord (cache BTCEUR_15m : 196 trous alignés sur les pages de 1 000 ; BTC-EUR 15m réel avant correction : 2 créneaux absents sur 2 600, tous deux détenus par Bitvavo, reseau-rouge-ffadd6f.log). La fenêtre de 21e2f5a, [S−1, S+Q−1], est refusée (400/205) par le vrai Bitvavo. Rouge sur d55849a et 21e2f5a, vert ici (preuves/2026-10-11-pagination/).
            // Page incomplète : la période est épuisée (ou la paire n'était pas encore cotée).
            // `>=` et non plus `>` : `end` ne recule plus de lui-même, une réponse
            // anormale (F = end) ferait boucler.
            if !full || first.open_time - q < start || first.open_time >= end {
                break;
            }
            end = first.open_time;
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

/// Adresse d'une page de bougies : jamais plus de 1 000 par requête (`MAX_LIMIT`).
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
    use crate::faux_bitvavo;

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

    /// Heure fixe des tests sans réseau : bougie en cours ouverte à `c`, `now` en son milieu.
    /// En 1j, `c` tombe le 2026-09-01, avant la date réelle.
    fn horloge(tf: Timeframe) -> (i64, i64) {
        let q = tf.millis();
        let c = FIRST_CANDLE_MS + 2800 * q;
        (c, c + q / 2 + 7)
    }

    /// Ce que `closed_since` doit rendre : chaque bougie que le serveur détient, ouverte à
    /// partir de `start` et close à `now`, par ouverture croissante.
    fn attendu(ouvertures: &[i64], q: i64, start: i64, now: i64) -> Vec<i64> {
        let mut v: Vec<i64> = ouvertures.iter().copied().filter(|&o| o >= start && o + q <= now).collect();
        v.sort_unstable();
        v
    }

    struct Cas {
        nom: &'static str,
        ouvertures: Vec<i64>,
        start: i64,
        /// Garde contre un cas mal construit (qui ne testerait plus rien).
        min_attendu: usize,
    }

    /// Les cas limites d'entrée de `closed_since` (consigne du 2026-10-11).
    fn cas_limites(tf: Timeframe) -> Vec<Cas> {
        let q = tf.millis();
        let (c, _) = horloge(tf);
        let cote = FIRST_CANDLE_MS + 20 * q; // cotation de la paire
        let creneaux = |de: i64| -> Vec<i64> { (0..).map(|i| de + i * q).take_while(|&o| o <= c).collect() };
        // Créneaux sans échange épars : un sur 97 (la bougie en cours s'échange).
        let epars: Vec<i64> = creneaux(cote).into_iter().filter(|o| ((o - cote) / q) % 97 != 5).collect();
        // En plus, 1 200 créneaux vides d'affilée : plus long qu'une page.
        let coupure: Vec<i64> = epars.iter().copied().filter(|o| !(300..1500).contains(&((o - cote) / q))).collect();
        vec![
            Cas { nom: "nominal : plusieurs pages, créneaux vides épars", ouvertures: epars.clone(), start: cote + 3 * q, min_attendu: 2500 },
            Cas {
                nom: "page pleine qui s'arrête un créneau après start (cas ONDO)",
                ouvertures: creneaux(cote),
                start: c - 1000 * q,
                min_attendu: 1000,
            },
            Cas { nom: "start non aligné", ouvertures: epars.clone(), start: cote + 500 * q + 1, min_attendu: 2000 },
            Cas {
                nom: "start non aligné, page pleine qui commence au premier créneau entier",
                ouvertures: creneaux(cote),
                start: c - 1000 * q + 1,
                min_attendu: 999,
            },
            Cas {
                nom: "créneau vide pile à start, coupure plus longue qu'une page",
                ouvertures: coupure,
                start: cote + 5 * q,
                min_attendu: 1500,
            },
            Cas { nom: "start avant FIRST_CANDLE_MS et avant la cotation", ouvertures: epars.clone(), start: 0, min_attendu: 2500 },
            Cas {
                nom: "cotation pile sur un raccord de page",
                ouvertures: creneaux(c - 1999 * q),
                start: FIRST_CANDLE_MS,
                min_attendu: 1999,
            },
            Cas { nom: "paire sans aucune bougie", ouvertures: Vec::new(), start: c - 10 * q, min_attendu: 0 },
            Cas { nom: "start dans la bougie en cours, non aligné", ouvertures: epars.clone(), start: c + 1, min_attendu: 0 },
            Cas { nom: "start = ouverture de la bougie en cours", ouvertures: epars.clone(), start: c, min_attendu: 0 },
            Cas { nom: "start une ms après la dernière ouverture close", ouvertures: epars, start: c - q + 1, min_attendu: 0 },
        ]
    }

    /// `closed_since` rend exactement les bougies que Bitvavo détient, en 15m et en 1j :
    /// aucune perdue aux raccords de pages, aucun doublon, aucune requête refusée, pas de
    /// boucle. Faux serveur : `crate::faux_bitvavo` (règles sondées sur la vraie API).
    #[tokio::test]
    async fn rend_exactement_les_bougies_du_serveur() {
        for tf in [Timeframe::M15, Timeframe::D1] {
            let q = tf.millis();
            let (_, now) = horloge(tf);
            for cas in cas_limites(tf) {
                let faux = faux_bitvavo::lancer(q, cas.ouvertures.clone(), now);
                let attendu = attendu(&cas.ouvertures, q, cas.start, now);
                assert!(attendu.len() >= cas.min_attendu, "{tf} {} : cas mal construit ({} bougies)", cas.nom, attendu.len());
                let v = BitvavoClient::with_base(&faux.base).closed_until("BTCEUR", tf, cas.start, now).await;
                let requetes = faux.requetes();
                let v = v.unwrap_or_else(|e| panic!("{tf} {} : {e:#} ; requêtes {requetes:?}", cas.nom));
                let obtenu: Vec<i64> = v.iter().map(|c| c.open_time).collect();
                if obtenu != attendu {
                    let recus: BTreeSet<i64> = obtenu.iter().copied().collect();
                    let perdues: Vec<i64> = attendu.iter().copied().filter(|o| !recus.contains(o)).collect();
                    panic!(
                        "{tf} {} : {} bougies au lieu de {}, {} perdues (ex. {:?}) ; requêtes {requetes:?}",
                        cas.nom,
                        obtenu.len(),
                        attendu.len(),
                        perdues.len(),
                        &perdues[..perdues.len().min(3)]
                    );
                }
                assert!(requetes.iter().all(|r| !r.refusee), "{tf} {} : requête refusée {requetes:?}", cas.nom);
                assert!(requetes.len() <= attendu.len() / 1000 + 2, "{tf} {} : {} requêtes", cas.nom, requetes.len());
                pt_core::candle::validate_series(&v).unwrap();
            }
        }
    }

    /// Fenêtres exactes demandées pour 1 000 bougies closes plus celle en cours : la page
    /// pleine s'arrête un créneau après `start`, la suivante se termine à sa première
    /// ouverture (`[start, start+Q]`, qui rend `start`), puis plus rien.
    #[tokio::test]
    async fn page_suivante_finit_a_la_premiere_ouverture() {
        let tf = Timeframe::M15;
        let q = tf.millis();
        let (c, now) = horloge(tf);
        let start = c - 1000 * q;
        let faux = faux_bitvavo::lancer(q, (0..=1000).map(|i| start + i * q).collect(), now);
        let v = BitvavoClient::with_base(&faux.base).closed_until("ONDOEUR", tf, start, now).await;
        let fenetres: Vec<(i64, i64)> = faux.requetes().iter().map(|r| (r.start, r.end)).collect();
        assert_eq!(fenetres, [(start, now), (start, start + q)]);
        assert_eq!(v.unwrap().len(), 1000);
    }

    /// Une période de 1 001 bougies (1 000 clôturées + celle en cours) : la page pleine
    /// s'arrête une bougie après le début, et la page suivante ne doit pas demander une
    /// fenêtre de moins d'un intervalle (cas ONDO-EUR 15m du 2026-10-09). Entrée qui ferait
    /// échouer une correction fausse : `start = ouverture en cours − 1 000 intervalles`
    /// — arrêter la boucle au lieu d'élargir la fenêtre perdrait la bougie ouverte à `start`.
    #[tokio::test]
    async fn jamais_de_fenetre_plus_etroite_qu_un_intervalle() {
        for tf in [Timeframe::M15, Timeframe::D1] {
            let step = tf.millis();
            let (forming, now) = horloge(tf);
            // Bougies cotées depuis FIRST_CANDLE_MS, jusqu'à celle en cours.
            let cotees: Vec<i64> = (0..).map(|i| FIRST_CANDLE_MS + i * step).take_while(|&o| o <= forming).collect();
            let faux = faux_bitvavo::lancer(step, cotees, now);
            let start = forming - 1000 * step;
            let v = BitvavoClient::with_base(&faux.base).closed_until("ONDOEUR", tf, start, now).await.unwrap();
            let windows: Vec<(i64, i64)> = faux.requetes().iter().map(|r| (r.start, r.end)).collect();
            assert!(windows.iter().all(|(s, e)| e - s >= step), "{tf} : fenêtres demandées {windows:?}");
            assert_eq!(v.first().map(|c| c.open_time), Some(start), "{tf} : bougie de début perdue");
            assert_eq!(v.len(), 1000, "{tf}");
            pt_core::candle::validate_series(&v).unwrap();
        }
    }

    /// Test réseau : chaque fenêtre sondée, rejouée sur Bitvavo (BTC-EUR 15m, qui s'échange
    /// dans chaque créneau), reçoit la même réponse que du faux serveur des tests sans réseau.
    #[tokio::test]
    #[ignore]
    async fn live_regles_de_fenetre_bitvavo() {
        let client = BitvavoClient::new();
        let tf = Timeframe::M15;
        let q = tf.millis();
        // Loin d'un changement de bougie, pour que la bougie en cours ne change pas pendant le test.
        while {
            let t = crate::now_ms() % q;
            t < 60_000 || t > q - 60_000
        } {
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
        let now = crate::now_ms();
        let c = now / q * q;
        let s = c - 10 * q;
        let tout: Vec<i64> = (0..40).map(|i| c - i * q).collect();
        let fenetres = [
            (s, s + q),
            (s - 1, s + q),
            (s - q, s + q - 1),
            (s + 1, s + 2 * q),
            (s - 3 * q, s - 1),
            (s - 3 * q, s),
            (s, s + q - 1),
            (s - 1, s + q - 1),
            (s + 1, s + q + 1),
            (s + 1, s + 2 * q - 1),
            (s - q + 1, s + q - 1),
            (c, now),
            (c - q, now),
            (c, c + q - 1),
            (c - q + 1, now),
            (c + 1, now),
            (c - q, c - 1),
        ];
        for (a, b) in fenetres {
            let reel = match client.candles("BTCEUR", tf, Some(a), Some(b), MAX_LIMIT).await {
                Ok(v) => Some(v.iter().rev().map(|c| c.open_time).collect::<Vec<i64>>()),
                Err(e) => {
                    let e = format!("{e:#}");
                    assert!(e.contains("400"), "[{a}, {b}] : {e}");
                    None
                }
            };
            let faux = faux_bitvavo::repondre(&tout, q, now, a, b, MAX_LIMIT);
            let rel = |v: &Vec<i64>| v.iter().map(|o| (o - s) / q).collect::<Vec<_>>();
            eprintln!("[S{:+}, S{:+}] : Bitvavo {:?}, faux serveur {:?}", a - s, b - s, reel.as_ref().map(rel), faux.as_ref().map(rel));
            assert_eq!(reel, faux, "[{a}, {b}] (S = {s}, C = {c})");
        }
    }

    /// Test réseau : au moins 2 500 créneaux de BTC-EUR 15m ; chaque créneau absent du
    /// résultat est redemandé seul (`[o, o+Q]`), et Bitvavo doit le rendre vide. Une bougie
    /// perdue à un raccord de page, elle, serait rendue.
    #[tokio::test]
    #[ignore]
    async fn live_creneaux_absents_vraiment_vides() {
        let client = BitvavoClient::new();
        let tf = Timeframe::M15;
        let q = tf.millis();
        let avant = crate::now_ms();
        let start = (avant / q - 2600) * q;
        let v = client.closed_since("BTCEUR", tf, start).await.unwrap();
        pt_core::candle::validate_series(&v).unwrap();
        let recus: BTreeSet<i64> = v.iter().map(|c| c.open_time).collect();
        // Créneaux clos avant l'appel : closed_since devait tous les voir.
        let absents: Vec<i64> =
            (0..).map(|i| start + i * q).take_while(|&o| o + q <= avant).filter(|o| !recus.contains(o)).collect();
        assert!(recus.len() >= 2500, "{} bougies", recus.len());
        let mut perdus = Vec::new();
        for &o in &absents {
            if !client.candles("BTCEUR", tf, Some(o), Some(o + q), MAX_LIMIT).await.unwrap().is_empty() {
                perdus.push(o);
            }
        }
        eprintln!(
            "BTC-EUR 15m : {} bougies, {} créneaux absents, dont {} détenus par Bitvavo",
            recus.len(),
            absents.len(),
            perdus.len()
        );
        assert!(perdus.is_empty(), "créneaux absents du résultat mais détenus par Bitvavo : {perdus:?}");
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
