//! Passage des portefeuilles en paires USDT à des paires EUR réellement achetables.
//!
//! Pourquoi : jusqu'au 2026-10-08, chaque portefeuille suivait des paires en USDT,
//! qu'un résident de l'EEE ne peut plus acheter depuis MiCA. Leurs gains simulés ne
//! se reproduisaient donc pas avec de vrais euros (audit conv-111 : tout le gain du
//! meilleur portefeuille venait d'OGNUSDT). Avec les prix EUR, ces portefeuilles ne
//! trouvent plus leurs bougies : il faut les remplacer, sans rien effacer.
//!
//! Au premier démarrage de cette version, UNE fois :
//! 1. copie cohérente de la base (`VACUUM INTO`, journal WAL compris). Si la copie
//!    échoue, rien n'est écrit ;
//! 2. chaque portefeuille ACTIF dont une paire n'est pas en EUR est ARCHIVÉ : inactif,
//!    motif dans `last_error`. Ses exécutions, ses trades et sa courbe restent
//!    intacts, et il se réactive d'un clic ;
//! 3. un portefeuille équivalent est recréé : même stratégie, même référence, même
//!    capital, sur des paires EUR cotées. Plus de 20 paires = liste construite sur
//!    l'univers figé (la saisie en accepte 20 au plus), donc l'univers figé EUR ;
//!    sinon chaque `XUSDT` devient `XEUR` s'il est coté, et les autres partent.
//!
//! Archivage, recréations et drapeau tiennent dans UNE transaction : un arrêt en
//! cours de route ne laisse ni doublon ni portefeuille archivé sans remplaçant.
//! Un portefeuille que l'utilisateur avait mis en pause n'est pas touché.

use crate::Store;
use anyhow::{bail, Context, Result};
use pt_core::{CostModel, Engine, Preset};
use rusqlite::params;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Drapeau dans `settings` : la migration ne se rejoue jamais.
pub const EUR_MIGRATION_FLAG: &str = "migration_paires_eur_v1";
/// Motif affiché sur un portefeuille archivé.
pub const ARCHIVED_REASON: &str = "archivé : ancien univers USDT, non achetable en euros";
const LEGACY_QUOTE: &str = "USDT";
const QUOTE: &str = "EUR";
/// Au-delà, la liste ne peut venir que de l'univers figé : la saisie en accepte 20 au plus.
const MAX_MANUAL_SYMBOLS: usize = 20;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct EurMigrationReport {
    /// Copie de la base prise avant toute écriture (absente s'il n'y avait rien à migrer).
    pub backup: Option<PathBuf>,
    pub archived: Vec<i64>,
    /// (ancien id, nouvel id).
    pub recreated: Vec<(i64, i64)>,
    /// Archivés sans remplaçant, avec le motif.
    pub not_recreated: Vec<(i64, String)>,
}

/// Un portefeuille suit l'ancien univers dès qu'une de ses paires n'est pas en EUR.
pub fn is_legacy(symbols: &[String]) -> bool {
    symbols.iter().any(|s| !s.ends_with(QUOTE))
}

/// Paires EUR remplaçantes, triées et sans doublon.
pub fn eur_symbols(old: &[String], universe: &[String], tradable: &BTreeSet<String>) -> Vec<String> {
    let mut out: Vec<String> = if old.len() > MAX_MANUAL_SYMBOLS {
        universe.iter().filter(|s| tradable.contains(*s)).cloned().collect()
    } else {
        old.iter()
            .map(|s| match s.strip_suffix(LEGACY_QUOTE) {
                Some(base) => format!("{base}{QUOTE}"),
                None => s.clone(),
            })
            .filter(|s| tradable.contains(s))
            .collect()
    };
    out.sort();
    out.dedup();
    out
}

/// « … · 100 cryptos » devient « … · 50 cryptos » : le nom dit le vrai nombre.
pub fn renamed(name: &str, old_n: usize, new_n: usize) -> String {
    let from = format!("{old_n} cryptos");
    if name.contains(&from) {
        name.replacen(&from, &format!("{new_n} cryptos"), 1)
    } else {
        name.to_string()
    }
}

/// `papertrading.avant-bitvavo-AAAA-MM-JJ.sqlite` dans `dir`, suffixé `-2`, `-3`… s'il existe.
pub fn backup_path(dir: &Path, now_ms: i64) -> PathBuf {
    let (y, m, d) = civil_date(now_ms.div_euclid(86_400_000));
    let base = format!("papertrading.avant-bitvavo-{y:04}-{m:02}-{d:02}");
    let mut p = dir.join(format!("{base}.sqlite"));
    let mut i = 2;
    while p.exists() {
        p = dir.join(format!("{base}-{i}.sqlite"));
        i += 1;
    }
    p
}

/// Jours depuis le 1970-01-01 → (année, mois, jour), calendrier grégorien (algorithme de H. Hinnant).
fn civil_date(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

struct Legacy {
    id: i64,
    name: String,
    engine: Value,
    benchmark: Value,
    symbols: Vec<String>,
}

impl Store {
    /// Copie cohérente de toute la base (journal WAL compris) dans `dest`, qui ne doit pas exister.
    pub fn backup_to(&self, dest: &Path) -> Result<()> {
        if dest.exists() {
            bail!("copie de sauvegarde refusée : {} existe déjà", dest.display());
        }
        self.conn
            .execute("VACUUM INTO ?1", params![dest.to_string_lossy()])
            .with_context(|| format!("copie de sauvegarde vers {}", dest.display()))?;
        Ok(())
    }

    /// Voir l'en-tête du module. `Ok(None)` : déjà faite, rien n'est touché.
    pub fn migrate_to_eur(
        &mut self,
        backup: &Path,
        universe: &[String],
        tradable: &BTreeSet<String>,
        costs: CostModel,
        now: i64,
    ) -> Result<Option<EurMigrationReport>> {
        if self.get_setting::<bool>(EUR_MIGRATION_FLAG)?.unwrap_or(false) {
            return Ok(None);
        }
        let mut legacy = Vec::new();
        {
            let mut st =
                self.conn.prepare("SELECT id, name, engine, benchmark FROM portfolios WHERE active=1 ORDER BY id")?;
            let rows = st.query_map([], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))
            })?;
            for row in rows {
                let (id, name, e, b) = row?;
                let engine: Value =
                    serde_json::from_str(&e).with_context(|| format!("état du portefeuille {id} illisible"))?;
                let benchmark: Value =
                    serde_json::from_str(&b).with_context(|| format!("référence du portefeuille {id} illisible"))?;
                let symbols: Vec<String> =
                    serde_json::from_value(engine.get("symbols").cloned().unwrap_or(Value::Null)).unwrap_or_default();
                if is_legacy(&symbols) {
                    legacy.push(Legacy { id, name, engine, benchmark, symbols });
                }
            }
        }

        let mut report = EurMigrationReport::default();
        if !legacy.is_empty() {
            // AVANT toute écriture : sans copie, on n'archive rien.
            self.backup_to(backup)?;
            report.backup = Some(backup.to_path_buf());
        }

        let tx = self.conn.transaction()?;
        for old in legacy {
            tx.execute("UPDATE portfolios SET active=0, last_error=?2 WHERE id=?1", params![old.id, ARCHIVED_REASON])?;
            report.archived.push(old.id);

            let symbols = eur_symbols(&old.symbols, universe, tradable);
            if symbols.is_empty() {
                report.not_recreated.push((old.id, "aucune de ses paires n'est cotée en EUR".into()));
                continue;
            }
            let cash = old.engine.pointer("/portfolio/initial_cash").and_then(Value::as_f64);
            let preset = old.engine.get("preset").cloned().and_then(|p| serde_json::from_value::<Preset>(p).ok());
            let bench = old.benchmark.get("preset").cloned().and_then(|p| serde_json::from_value::<Preset>(p).ok());
            let (Some(cash), Some(preset), Some(bench)) = (cash, preset, bench) else {
                report.not_recreated.push((old.id, "stratégie ou capital illisible".into()));
                continue;
            };
            let engine = Engine::new(preset, symbols.clone(), cash, costs, now);
            let benchmark = Engine::new(bench, symbols.clone(), cash, costs, now);
            tx.execute(
                "INSERT INTO portfolios (name, created_at, active, engine, benchmark) VALUES (?1, ?2, 1, ?3, ?4)",
                params![
                    renamed(&old.name, old.symbols.len(), symbols.len()),
                    now,
                    serde_json::to_string(&engine)?,
                    serde_json::to_string(&benchmark)?
                ],
            )?;
            report.recreated.push((old.id, tx.last_insert_rowid()));
        }
        tx.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![EUR_MIGRATION_FLAG, serde_json::to_string(&true)?],
        )?;
        tx.commit()?;
        Ok(Some(report))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pt_core::find;

    fn usdt(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("C{i}USDT")).collect()
    }

    fn tradable() -> BTreeSet<String> {
        ["BTCEUR", "ETHEUR", "SOLEUR"].iter().map(|s| s.to_string()).collect()
    }

    fn universe() -> Vec<String> {
        vec!["SOLEUR".into(), "BTCEUR".into(), "ETHEUR".into()]
    }

    fn create(s: &mut Store, name: &str, symbols: Vec<String>) -> i64 {
        let preset = find("sma_trend_200_1h").expect("preset");
        let engine = Engine::new(preset, symbols.clone(), 10_000.0, CostModel::default(), 1);
        let bench = Engine::new(find("buy_hold").expect("référence"), symbols, 10_000.0, CostModel::default(), 1);
        s.create(name, 1, &engine, &bench).unwrap()
    }

    fn temp_backup(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pt-migration-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("avant.sqlite")
    }

    #[test]
    fn date_civile_du_nom_de_sauvegarde() {
        assert_eq!(civil_date(0), (1970, 1, 1));
        // 2026-10-08T00:00:00Z
        assert_eq!(civil_date(1_791_417_600_000 / 86_400_000), (2026, 10, 8));
        assert_eq!(civil_date(11_016), (2000, 2, 29));
    }

    #[test]
    fn grand_univers_usdt_devient_l_univers_eur() {
        assert_eq!(eur_symbols(&usdt(100), &universe(), &tradable()), vec!["BTCEUR", "ETHEUR", "SOLEUR"]);
    }

    #[test]
    fn petite_liste_garde_les_paires_cotees_en_eur() {
        let old = vec!["BTCUSDT".to_string(), "OGNUSDT".into(), "ETHUSDT".into()];
        assert_eq!(eur_symbols(&old, &universe(), &tradable()), vec!["BTCEUR", "ETHEUR"]);
    }

    #[test]
    fn le_nom_dit_le_vrai_nombre() {
        assert_eq!(renamed("SMA 200 (1h) · 100 cryptos", 100, 50), "SMA 200 (1h) · 50 cryptos");
        assert_eq!(renamed("Tendance SMA 50 (1j)", 5, 2), "Tendance SMA 50 (1j)");
    }

    #[test]
    fn archive_recree_et_ne_rejoue_pas() {
        let mut s = Store::in_memory().unwrap();
        let grand = create(&mut s, "SMA · 100 cryptos", usdt(100));
        let petit = create(&mut s, "Petit", vec!["BTCUSDT".into(), "OGNUSDT".into()]);
        let perdu = create(&mut s, "Perdu", vec!["OGNUSDT".into()]);
        let deja_eur = create(&mut s, "Déjà en euros", vec!["BTCEUR".into()]);
        let en_pause = create(&mut s, "En pause", vec!["ETHUSDT".into()]);
        s.set_active(en_pause, false).unwrap();
        // Une exécution sur l'ancien portefeuille : elle doit survivre.
        s.conn
            .execute(
                "INSERT INTO fills (portfolio_id, book, seq, time, symbol, side, qty, price, fee, cash_delta, reason) \
                 VALUES (?1, 'strategy', 0, 2, 'BTCUSDT', 'BUY', 1.0, 1.0, 0.0, 0.0, 'test')",
                params![petit],
            )
            .unwrap();
        let fills = |s: &Store, id: i64| -> i64 {
            s.conn.query_row("SELECT COUNT(*) FROM fills WHERE portfolio_id=?1", params![id], |r| r.get(0)).unwrap()
        };
        let etat = |s: &Store, id: i64| -> (bool, Option<String>) {
            s.conn
                .query_row("SELECT active, last_error FROM portfolios WHERE id=?1", params![id], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .unwrap()
        };

        let backup = temp_backup("unit");
        let r = s.migrate_to_eur(&backup, &universe(), &tradable(), CostModel::default(), 99).unwrap().unwrap();

        assert_eq!(r.backup.as_deref(), Some(backup.as_path()));
        assert!(Store::open(&backup).unwrap().ids().unwrap().len() == 5, "la copie contient l'état d'avant");
        assert_eq!(r.archived, vec![grand, petit, perdu]);
        assert_eq!(r.recreated.len(), 2);
        assert_eq!(r.not_recreated.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![perdu]);

        assert_eq!(etat(&s, petit), (false, Some(ARCHIVED_REASON.to_string())));
        assert_eq!(fills(&s, petit), 1, "exécutions intactes");
        assert_eq!(etat(&s, deja_eur), (true, None), "un portefeuille déjà en EUR n'est pas touché");
        assert_eq!(etat(&s, en_pause), (false, None), "un portefeuille en pause n'est pas touché");

        let (_, new_grand) = r.recreated[0];
        let n = s.load(new_grand).unwrap();
        assert!(n.active);
        assert_eq!(n.name, "SMA · 3 cryptos");
        assert_eq!(n.engine.symbols, vec!["BTCEUR", "ETHEUR", "SOLEUR"]);
        assert_eq!(n.benchmark.symbols, n.engine.symbols);
        assert_eq!(n.engine.preset.id, "sma_trend_200_1h");
        assert_eq!(n.benchmark.preset.id, "buy_hold");
        assert_eq!(n.engine.portfolio.initial_cash, 10_000.0);
        assert_eq!(n.engine.costs, CostModel::default());
        assert_eq!(n.engine.trade_from, 99);
        let (_, new_petit) = r.recreated[1];
        assert_eq!(s.load(new_petit).unwrap().engine.symbols, vec!["BTCEUR"]);

        // Seconde exécution : aucun effet, aucune nouvelle copie.
        let backup2 = temp_backup("unit-bis");
        let ids = s.ids().unwrap();
        assert!(s.migrate_to_eur(&backup2, &universe(), &tradable(), CostModel::default(), 100).unwrap().is_none());
        assert_eq!(s.ids().unwrap(), ids);
        assert!(!backup2.exists());
    }

    #[test]
    fn sans_copie_rien_n_est_ecrit() {
        let mut s = Store::in_memory().unwrap();
        let id = create(&mut s, "Ancien", vec!["BTCUSDT".into()]);
        let backup = temp_backup("refus");
        std::fs::write(&backup, b"occupe").unwrap();
        assert!(s.migrate_to_eur(&backup, &universe(), &tradable(), CostModel::default(), 9).is_err());
        assert!(s.load(id).unwrap().active, "aucun archivage sans copie");
        assert_eq!(s.ids().unwrap(), vec![id]);
        assert_eq!(s.get_setting::<bool>(EUR_MIGRATION_FLAG).unwrap(), None);
    }

    #[test]
    fn rien_a_migrer_pose_le_drapeau_sans_copie() {
        let mut s = Store::in_memory().unwrap();
        create(&mut s, "Euros", vec!["BTCEUR".into()]);
        let backup = temp_backup("vide");
        let r = s.migrate_to_eur(&backup, &universe(), &tradable(), CostModel::default(), 9).unwrap().unwrap();
        assert_eq!(r, EurMigrationReport::default());
        assert!(!backup.exists());
        assert_eq!(s.get_setting::<bool>(EUR_MIGRATION_FLAG).unwrap(), Some(true));
    }
}
