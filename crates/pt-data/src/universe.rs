//! Paires achetables : univers figé, forme des symboles et contrôle de cotation.
//!
//! Une paire n'entre dans un portefeuille que si elle s'achète VRAIMENT en euros
//! sur la plateforme dont viennent les prix (Bitvavo) : un gain simulé sur une paire
//! qu'on ne peut pas acheter ne se reproduit pas avec de l'argent réel.

use serde_json::Value;
use std::collections::BTreeSet;

/// Devise de cotation de toutes les paires.
pub const QUOTE: &str = "EUR";

/// Univers figé et relu (`crates/pt-data/univers.txt`) : `TOP{n}` le désigne partout.
pub const UNIVERSE: &str = include_str!("../univers.txt");

/// Stablecoins, or et jetons enveloppés : pas des cryptos à trader pour leur prix.
pub const NOT_CRYPTO: &[&str] = &[
    "USDC", "USDT", "EURC", "EURI", "EURCV", "EURQ", "EURR", "DAI", "USDE", "USDS", "PYUSD", "RLUSD", "USD1", "FDUSD",
    "TUSD", "USDP", "USDG", "USDQ", "USDR", "PAXG", "XAUT", "WBTC", "WETH", "CBBTC", "STETH", "WSTETH",
];

/// Les `n` premiers symboles de l'univers figé.
pub fn frozen_universe(n: usize) -> Vec<String> {
    UNIVERSE.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).take(n).map(String::from).collect()
}

/// Taille de l'univers figé (pastille `TOP{n}` des écrans).
pub fn universe_size() -> usize {
    frozen_universe(usize::MAX).len()
}

/// `TOP44` → `Some(44)`.
pub fn top_alias(s: &str) -> Option<usize> {
    s.strip_prefix("TOP").and_then(|x| x.parse::<usize>().ok())
}

/// Forme d'un symbole saisi : majuscules, espaces et tiret retirés (`btc-eur` → `BTCEUR`).
/// Refuse tout autre caractère, plus de 20 caractères, et toute paire qui n'est pas en EUR.
/// `TOP{n}` passe tel quel. `Ok(None)` : saisie vide, à ignorer.
pub fn normalize_symbol(raw: &str) -> Result<Option<String>, String> {
    let s: String = raw.trim().to_uppercase().chars().filter(|c| *c != '-').collect();
    if s.is_empty() {
        return Ok(None);
    }
    if !s.chars().all(|c| c.is_ascii_alphanumeric()) || s.len() > 20 {
        return Err(format!("symbole invalide : {}", raw.trim()));
    }
    if top_alias(&s).is_some() {
        return Ok(Some(s));
    }
    match s.strip_suffix(QUOTE) {
        Some(base) if !base.is_empty() => Ok(Some(s)),
        _ => Err(format!("{s} refusé : seules les paires en EUR sont achetables (ex. BTCEUR)")),
    }
}

/// Paires en EUR ouvertes au négoce dans une réponse `/v2/markets` : `BTC-EUR` → `BTCEUR`.
pub fn tradable_eur(markets: &Value) -> BTreeSet<String> {
    markets
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m.get("quote").and_then(Value::as_str) == Some(QUOTE))
        .filter(|m| m.get("status").and_then(Value::as_str) == Some("trading"))
        .filter_map(|m| m.get("base").and_then(Value::as_str).map(|b| format!("{b}{QUOTE}")))
        .collect()
}

/// Refuse la première paire qui ne s'achète pas en EUR maintenant.
pub fn check_tradable(symbols: &[String], tradable: &BTreeSet<String>) -> Result<(), String> {
    match symbols.iter().find(|s| !tradable.contains(*s)) {
        Some(s) => Err(format!("{s} refusé : non coté en EUR sur Bitvavo (absent ou suspendu)")),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Relevé de `/v2/markets` du 2026-10-08 (champs utiles seulement).
    const MARKETS: &str = include_str!("../fixtures/bitvavo-markets.json");

    fn markets() -> BTreeSet<String> {
        tradable_eur(&serde_json::from_str(MARKETS).unwrap())
    }

    /// Chaque crypto de l'univers s'achète en euros (relevé figé du 2026-10-08).
    #[test]
    fn univers_tout_achetable() {
        let m = markets();
        assert!(m.len() > 400, "{} marchés", m.len());
        let u = frozen_universe(usize::MAX);
        let absent: Vec<&String> = u.iter().filter(|s| !m.contains(*s)).collect();
        assert!(absent.is_empty(), "non achetables en EUR : {absent:?}");
    }

    #[test]
    fn univers_sans_doublon_ni_stablecoin() {
        let u = frozen_universe(usize::MAX);
        assert!(u.len() >= 30, "{}", u.len());
        assert_eq!(u.iter().collect::<BTreeSet<_>>().len(), u.len());
        for s in &u {
            let base = s.strip_suffix(QUOTE).expect("paire EUR");
            assert!(!base.is_empty() && !NOT_CRYPTO.contains(&base), "{s}");
        }
        assert_eq!(&frozen_universe(2)[..], ["BTCEUR", "XRPEUR"]);
        assert_eq!(universe_size(), u.len());
    }

    /// Cas 1, 2, 4, 5, 6, 7, 10 : forme du symbole saisi.
    #[test]
    fn forme_des_symboles() {
        let ok = |s: &str| normalize_symbol(s).unwrap();
        let err = |s: &str| normalize_symbol(s).unwrap_err();
        assert_eq!(ok(" btceur "), Some("BTCEUR".into()));
        assert_eq!(ok("BTC-EUR"), Some("BTCEUR".into()));
        assert_eq!(ok("top44"), Some("TOP44".into()));
        assert_eq!(ok("TOP0"), Some("TOP0".into())); // borne contrôlée par l'appelant
        assert_eq!(ok(""), None);
        assert_eq!(ok("   "), None);
        assert!(err("BTC/EUR!").starts_with("symbole invalide"));
        assert!(err(&"A".repeat(18).add_eur()).starts_with("symbole invalide"));
        assert_eq!(ok(&"A".repeat(17).add_eur()), Some(format!("{}EUR", "A".repeat(17))));
        for s in ["BTCUSDT", "ETHUSDC", "OGN-USDT", "BTC", "EUR"] {
            assert!(err(s).contains("seules les paires en EUR"), "{s}");
        }
    }

    /// Cas 8 et 9 : cotée ailleurs mais pas sur Bitvavo, ou marché suspendu.
    #[test]
    fn refuse_les_paires_non_cotees_ou_suspendues() {
        let m = markets();
        for s in ["TUTEUR", "ZECEUR", "DASHEUR", "OGNEUR0"] {
            let e = check_tradable(&[s.to_string()], &m).unwrap_err();
            assert!(e.contains("non coté en EUR sur Bitvavo"), "{s} : {e}");
        }
        assert!(check_tradable(&["BTCEUR".into(), "OGNEUR".into()], &m).is_ok());
        let halted = serde_json::json!([
            {"market": "AAA-EUR", "status": "halted", "base": "AAA", "quote": "EUR"},
            {"market": "BBB-EUR", "status": "trading", "base": "BBB", "quote": "EUR"},
            {"market": "BBB-USDC", "status": "trading", "base": "BBB", "quote": "USDC"}
        ]);
        let t = tradable_eur(&halted);
        assert_eq!(t.iter().collect::<Vec<_>>(), ["BBBEUR"]);
        assert!(check_tradable(&["AAAEUR".into()], &t).is_err());
    }

    trait AddEur {
        fn add_eur(self) -> String;
    }
    impl AddEur for String {
        fn add_eur(self) -> String {
            self + "EUR"
        }
    }
}
