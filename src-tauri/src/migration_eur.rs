//! Au démarrage : conversion UNIQUE des portefeuilles en paires USDT vers des paires
//! EUR achetables (règles et garanties : `pt_store::migration`).
//!
//! La liste des marchés vient du relevé figé du dépôt et non de l'API : le démarrage
//! ne dépend pas du réseau, et ce relevé est celui contre lequel l'univers est testé.

use crate::state::Settings;
use pt_store::migration::{backup_path, EurMigrationReport};
use pt_store::Store;
use std::path::Path;

/// Ne bloque jamais le démarrage : un échec est signalé, et les anciens portefeuilles
/// restent tels quels (le mode direct les marque « données indisponibles »).
pub fn run(store: &mut Store, dir: &Path) {
    match convert(store, dir, pt_data::now_ms()) {
        Ok(Some(r)) if !r.archived.is_empty() => eprintln!(
            "portefeuilles USDT : {} archivés, {} recréés en EUR, {} sans remplaçant ; copie de la base : {}",
            r.archived.len(),
            r.recreated.len(),
            r.not_recreated.len(),
            r.backup.as_deref().map(|p| p.display().to_string()).unwrap_or_default()
        ),
        Ok(_) => {}
        Err(e) => eprintln!("conversion des portefeuilles USDT non faite : {e:#}"),
    }
}

pub fn convert(store: &mut Store, dir: &Path, now: i64) -> anyhow::Result<Option<EurMigrationReport>> {
    let costs = store.get_setting::<Settings>("settings").ok().flatten().unwrap_or_default().costs;
    store.migrate_to_eur(
        &backup_path(dir, now),
        &pt_data::universe::frozen_universe(usize::MAX),
        &pt_data::universe::snapshot_tradable_eur(),
        costs,
        now,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use pt_store::migration::ARCHIVED_REASON;

    /// Rejoue la conversion sur une COPIE de la vraie base, jamais sur l'originale :
    /// `PT_MIGRATION_DB=<copie.sqlite> cargo test -p papertrading migration_eur -- --ignored --nocapture`
    #[test]
    #[ignore = "a besoin d'une copie de la vraie base dans PT_MIGRATION_DB"]
    fn conversion_sur_copie_de_la_vraie_base() {
        let src = std::env::var("PT_MIGRATION_DB").expect("PT_MIGRATION_DB : chemin d'une copie de la base");
        let dir = std::env::temp_dir().join(format!("pt-migration-reelle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(&src, dir.join(pt_store::DB_FILE)).unwrap();
        let mut store = Store::open(dir.join(pt_store::DB_FILE)).unwrap();

        let ids_avant = store.ids().unwrap();
        let fills_avant = store.fill_count().unwrap();
        let now = pt_data::now_ms();
        let r = convert(&mut store, &dir, now).unwrap().expect("première exécution");

        let tradable = pt_data::universe::snapshot_tradable_eur();
        for (old, new) in &r.recreated {
            let o = store.load(*old).unwrap();
            assert!(!o.active);
            assert_eq!(o.last_error.as_deref(), Some(ARCHIVED_REASON));
            let n = store.load(*new).unwrap();
            assert!(n.active);
            assert!(n.engine.symbols.iter().all(|s| tradable.contains(s)), "{:?}", n.engine.symbols);
            assert_eq!(n.engine.preset.id, o.engine.preset.id);
            assert_eq!(n.benchmark.preset.id, o.benchmark.preset.id);
            assert_eq!(n.engine.portfolio.initial_cash, o.engine.portfolio.initial_cash);
        }
        assert_eq!(store.fill_count().unwrap(), fills_avant, "aucune exécution perdue");
        assert_eq!(store.ids().unwrap().len(), ids_avant.len() + r.recreated.len());

        let backup = r.backup.clone().expect("copie prise");
        let copie = Store::open(&backup).unwrap();
        assert_eq!(copie.ids().unwrap(), ids_avant, "la copie contient l'état d'avant");
        assert_eq!(copie.fill_count().unwrap(), fills_avant);

        assert!(convert(&mut store, &dir, now + 1).unwrap().is_none(), "seconde exécution sans effet");
        let noms: Vec<String> = r.recreated.iter().take(3).map(|(_, id)| store.load(*id).unwrap().name).collect();
        eprintln!(
            "RAPPORT : {} portefeuilles, {} exécutions ; {} archivés, {} recréés, sans remplaçant {:?} ; copie {} ; exemples {:?}",
            ids_avant.len(),
            fills_avant,
            r.archived.len(),
            r.recreated.len(),
            r.not_recreated,
            backup.display(),
            noms
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
