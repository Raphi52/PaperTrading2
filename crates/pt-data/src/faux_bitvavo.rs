//! Faux Bitvavo local, sans réseau, pour les tests de `closed_since` et du cache.
//!
//! Il applique les règles de `/v2/{marché}/candles` sondées sur la vraie API (ONDO-EUR et
//! BTC-EUR 15m le 2026-10-09, BTC-EUR 15m le 2026-10-11), et confrontées à elle par le test
//! réseau `live_regles_de_fenetre_bitvavo`. Notations : Q = durée d'une bougie,
//! C = ouverture de la bougie en cours.
//! 1. Une bougie close n'est rendue que si `start ≤ ouverture` ET `ouverture + Q ≤ end`.
//!    La bougie en cours l'est si `start ≤ C ≤ end` (`[C, maintenant]` la rend).
//! 2. Une fenêtre sans aucun créneau rendable selon la règle 1 est refusée : HTTP 400,
//!    errorCode 205 (`[S, S+Q−1]`, `[S−1, S+Q−1]`, `[C+1, maintenant]` refusés).
//! 3. Un créneau sans échange n'est pas rendu : une fenêtre valide peut rendre `[]` (200).
//! 4. Les `limit` bougies les plus récentes de la fenêtre, la plus récente d'abord.
//! 5. Une fenêtre plus large que `PLAFOND_CRENEAUX` (1 050) créneaux est d'abord ramenée à
//!    ses 1 050 derniers : `start` devient `end − 1 050 Q`. Sondé le 2026-10-19 sur SYN-EUR
//!    15m, fin E alignée (`data/agents/strategies/work/2026-10-19/telechargeur/sonde-*.txt`) :
//!    `[E−1048Q, E]` rend jusqu'à E−1048 ; `[E−1052Q, E]`, `[E−1440Q, E]` et `[E−5000Q, E]`
//!    rendent les mêmes 855 bougies, la plus ancienne en E−1050. Le plafond ne dépend pas de
//!    `limit` : `[E−2000Q, E]` avec `limit=100` rend les 100 plus récentes (jusqu'à E−144).
//!    C'est lui qui coupait les paires peu échangées : moins de 1 000 bougies sur 1 050
//!    créneaux, la page paraissait « incomplète ». Fin non alignée : supposée de même.
//!
//! Il ne refuse pas un `limit` au-delà de 1 000 : le 2026-10-11, Bitvavo a accepté 1 001,
//! 1 440 et 1 441 (le client, lui, n'en demande jamais plus de 1 000).

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

/// Une requête reçue par le faux serveur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Requete {
    pub start: i64,
    pub end: i64,
    pub refusee: bool,
}

pub struct FauxBitvavo {
    pub base: String,
    requetes: Arc<Mutex<Vec<Requete>>>,
}

impl FauxBitvavo {
    /// Les requêtes reçues jusqu'ici, dans l'ordre.
    pub fn requetes(&self) -> Vec<Requete> {
        self.requetes.lock().unwrap().clone()
    }

    /// Oublie les requêtes reçues (pour compter celles d'un second appel).
    pub fn oublier(&self) {
        self.requetes.lock().unwrap().clear();
    }
}

/// Largeur maximale d'une fenêtre, en créneaux (règle 5).
pub const PLAFOND_CRENEAUX: i64 = 1050;

/// Le créneau `o` peut-il être rendu pour la fenêtre `[start, end]` (règle 1) ?
fn rendable(o: i64, q: i64, c: i64, start: i64, end: i64) -> bool {
    o >= start && o <= c && (o + q <= end || (o == c && o <= end))
}

/// Réponse de Bitvavo à `[start, end]` : les ouvertures rendues, de la plus récente à la
/// plus ancienne, ou `None` pour un refus 400 (errorCode 205). `ouvertures` = créneaux
/// où il y a eu au moins un échange ; `now` fixe la bougie en cours.
pub fn repondre(ouvertures: &[i64], q: i64, now: i64, start: i64, end: i64, limit: usize) -> Option<Vec<i64>> {
    let c = now.div_euclid(q) * q;
    let start = start.max(end - PLAFOND_CRENEAUX * q); // règle 5
    let premier = start.div_euclid(q) * q + if start.rem_euclid(q) == 0 { 0 } else { q };
    if !rendable(premier, q, c, start, end) {
        return None;
    }
    let mut rendues: Vec<i64> = ouvertures.iter().copied().filter(|&o| rendable(o, q, c, start, end)).collect();
    rendues.sort_unstable_by(|a, b| b.cmp(a));
    rendues.dedup();
    rendues.truncate(limit);
    Some(rendues)
}

/// Lance le faux serveur pour une unité de temps de `q` ms. Sans `start` ni `end`
/// (requête de `recent`), la fenêtre va de 0 à `now`.
pub fn lancer(q: i64, ouvertures: Vec<i64>, now: i64) -> FauxBitvavo {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let requetes = Arc::new(Mutex::new(Vec::new()));
    let journal = requetes.clone();
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
            let start = param("start").unwrap_or(0);
            let end = param("end").unwrap_or(now);
            let limit = param("limit").unwrap_or(1440).max(1) as usize;
            let reponse = repondre(&ouvertures, q, now, start, end, limit);
            journal.lock().unwrap().push(Requete { start, end, refusee: reponse.is_none() });
            let (status, body) = match reponse {
                None => (
                    "400 Bad Request",
                    r#"{"errorCode":205,"error":"Invalid parameters to get candlesticks, please review your request. parameter is invalid."}"#
                        .to_string(),
                ),
                Some(o) => {
                    let rows: Vec<String> = o.iter().map(|o| format!(r#"[{o},"1","1","1","1","1"]"#)).collect();
                    ("200 OK", format!("[{}]", rows.join(",")))
                }
            };
            let _ = write!(
                s,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    FauxBitvavo { base, requetes }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le faux serveur rend, pour chaque fenêtre sondée sur la vraie API, la même réponse
    /// qu'elle (BTC-EUR, qui s'échange dans chaque créneau). Sondes du 2026-10-09 (consigne
    /// `2026-10-11.correction-demandee.md`) et du 2026-10-11.
    #[test]
    fn le_faux_serveur_rend_ce_que_bitvavo_a_rendu() {
        let q = 900_000;
        let c = 1_791_700_200_000 + 10 * q; // bougie en cours
        let now = c + 449_000;
        let tout: Vec<i64> = (0..40).map(|i| c - i * q).collect();
        let s = c - 10 * q;
        let r = |start: i64, end: i64| repondre(&tout, q, now, start, end, 1000);
        // Règle 1.
        assert_eq!(r(s, s + q), Some(vec![s]));
        assert_eq!(r(s - 1, s + q), Some(vec![s]));
        assert_eq!(r(s - q, s + q - 1), Some(vec![s - q]));
        assert_eq!(r(s + 1, s + 2 * q), Some(vec![s + q]));
        assert_eq!(r(s - 3 * q, s - 1), Some(vec![s - 2 * q, s - 3 * q]));
        assert_eq!(r(s - 3 * q, s), Some(vec![s - q, s - 2 * q, s - 3 * q]));
        // Règle 2 : aucune fenêtre complète.
        for (a, b) in [(s, s + q - 1), (s - 1, s + q - 1), (s + 1, s + q + 1), (s + 1, s + 2 * q - 1), (s - q + 1, s + q - 1)] {
            assert_eq!(r(a, b), None, "[{a}, {b}]");
        }
        // Bougie en cours (sondes du 2026-10-11).
        assert_eq!(r(c, now), Some(vec![c]));
        assert_eq!(r(c - q, now), Some(vec![c, c - q]));
        assert_eq!(r(c, c + q - 1), Some(vec![c]));
        assert_eq!(r(c - q + 1, now), Some(vec![c]));
        assert_eq!(r(c - 1, now - 1), Some(vec![c]));
        assert_eq!(r(c + 1, now), None);
        assert_eq!(r(c - q, c - 1), None);
        // Règle 3 : créneau sans échange, fenêtre valide, réponse vide.
        let troue: Vec<i64> = tout.iter().copied().filter(|&o| o != s).collect();
        assert_eq!(repondre(&troue, q, now, s, s + q, 1000), Some(vec![]));
        // Règle 4 : les plus récentes d'abord, `limit` au plus.
        assert_eq!(r(s - 5 * q, s + q).map(|v| v.len()), Some(6));
        assert_eq!(repondre(&tout, q, now, s - 5 * q, s + q, 2), Some(vec![s, s - q]));
        // Règle 5 (sondes du 2026-10-19, fin E alignée, paire où un créneau sur 6 est vide).
        let e = c - 10 * q;
        let peu: Vec<i64> = (1..6000).map(|i| e - i * q).filter(|o| ((e - o) / q) % 6 != 3).collect();
        let plus_ancienne = |a: i64, lim: usize| repondre(&peu, q, now, a, e, lim).and_then(|v| v.last().copied());
        assert_eq!(plus_ancienne(e - 1048 * q, 1000), Some(e - 1048 * q));
        for large in [1052, 1440, 1441, 5000] {
            assert_eq!(plus_ancienne(e - large * q, 1000), Some(e - 1050 * q), "[E−{large}Q, E]");
        }
        assert_eq!(repondre(&peu, q, now, e - 5000 * q, e, 1000).map(|v| v.len()), Some(875));
        // `limit` sous le plafond : les 100 plus récentes, au-delà des 100 derniers créneaux.
        let cent = repondre(&peu, q, now, e - 2000 * q, e, 100).unwrap();
        assert_eq!(cent.len(), 100);
        assert!(*cent.last().unwrap() < e - 100 * q);
    }
}
