//! Boucle du mode direct (paper trading sur les prix réels).
//!
//! Corrections par rapport à l'ancien bot :
//! - un seul processus écrit l'état, dans une base transactionnelle ;
//! - au redémarrage, les bougies clôturées pendant l'arrêt sont rejouées dans
//!   l'ordre (l'ancien bot ignorait tout ce qui s'était passé pendant l'arrêt) ;
//! - le moteur est EXACTEMENT celui du backtest (`Engine::advance`) ;
//! - un verrou de fichier garantit qu'une seule instance fait avancer les
//!   portefeuilles : deux fenêtres ouvertes ne traitent jamais deux fois la même bougie.

use crate::state::AppState;
use anyhow::Result;
use pt_core::engine::Feed;
use pt_core::{External, Timeframe};
use pt_data::Recent;
use pt_store::{EquityRow, LivePortfolio};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// Fenêtre de bougies récupérée à chaque passage.
const WINDOW: usize = 1000;
/// Un point de courbe de valeur au plus toutes les 5 minutes, sauf nouvelle bougie.
const EQUITY_EVERY_MS: i64 = 5 * 60_000;

/// Verrou exclusif du mode direct, tenu tant que le fichier rendu est ouvert
/// (le système le libère aussi si l'application plante). `None` : une autre
/// instance le détient déjà.
pub fn acquire_live_lock(path: &Path) -> Option<File> {
    let file = OpenOptions::new().create(true).write(true).truncate(false).open(path).ok()?;
    file.try_lock().ok().map(|()| file)
}

pub async fn run_loop(state: Arc<AppState>, app: AppHandle) {
    let lock_path = state.data_dir.join("mode-direct.lock");
    let mut lock: Option<File> = None;
    loop {
        if lock.is_none() {
            lock = acquire_live_lock(&lock_path);
            let elsewhere = lock.is_none();
            if !elsewhere {
                // Seule l'instance qui fait avancer les portefeuilles a une icône près de l'horloge.
                let handle = app.clone();
                let _ = app.run_on_main_thread(move || {
                    if let Err(e) = crate::tray::install(&handle) {
                        eprintln!("icône de notification indisponible : {e}");
                    }
                });
            }
            let mut st = state.status.lock().expect("statut");
            if st.elsewhere != elsewhere {
                st.elsewhere = elsewhere;
                let snapshot = st.clone();
                drop(st);
                let _ = app.emit("engine-status", snapshot);
            }
        }
        let running = state.status.lock().expect("statut").running;
        if lock.is_none() {
            // Affichage seul : l'autre instance écrit dans la base, on relit.
            let _ = app.emit("portfolios-changed", ());
        } else if running {
            state.status.lock().expect("statut").busy = true;
            let _ = app.emit("engine-status", state.status.lock().expect("statut").clone());
            let started = Instant::now();
            let result = tick(&state).await;
            let mut st = state.status.lock().expect("statut");
            st.busy = false;
            st.ticks += 1;
            st.last_tick = Some(pt_data::now_ms());
            st.last_duration_ms = Some(started.elapsed().as_millis() as u64);
            match result {
                Ok((bars, errors)) => {
                    st.bars_processed += bars;
                    st.last_error = (!errors.is_empty()).then(|| errors.join(" · "));
                }
                Err(e) => st.last_error = Some(format!("{e:#}")),
            }
            let snapshot = st.clone();
            drop(st);
            let _ = app.emit("engine-status", snapshot);
            let _ = app.emit("portfolios-changed", ());
        }
        let secs = state.settings().tick_seconds.max(10);
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(secs)) => {},
            _ = state.wake.notified() => {},
        }
    }
}

/// Un passage : récupère les bougies nécessaires, fait avancer chaque portefeuille
/// actif et enregistre. Rend (bougies traitées, erreurs par portefeuille).
pub async fn tick(state: &AppState) -> Result<(u64, Vec<String>)> {
    let mut errors = Vec::new();
    let ids = state.store.lock().expect("base").ids()?;
    let mut portfolios: Vec<LivePortfolio> = Vec::new();
    for id in ids {
        match state.store.lock().expect("base").load(id) {
            Ok(p) if p.active => portfolios.push(p),
            Ok(_) => {}
            Err(e) => errors.push(format!("{e:#}")),
        }
    }
    if portfolios.is_empty() {
        return Ok((0, errors));
    }

    // Quelle profondeur d'historique faut-il pour chaque (symbole, unité de temps) ?
    let mut needs: BTreeMap<(String, Timeframe), Option<i64>> = BTreeMap::new();
    for p in &portfolios {
        let tf = p.engine.preset.timeframe;
        let warm = (p.engine.preset.warmup() as i64 + 5) * tf.millis();
        for s in &p.engine.symbols {
            let last = p.engine.states.get(s).and_then(|x| x.last_bar_open_time);
            let from = last.map(|t| t - warm);
            let e = needs.entry((s.clone(), tf)).or_insert(from);
            *e = match (*e, from) {
                (Some(a), Some(b)) => Some(a.min(b)),
                _ => None,
            };
        }
    }
    let mut market: BTreeMap<(String, Timeframe), Recent> = BTreeMap::new();
    for ((symbol, tf), from) in &needs {
        let fetched = async {
            let mut r = state.client.recent(symbol, *tf, WINDOW).await?;
            // Rattrapage : l'application a été arrêtée plus longtemps que la fenêtre.
            if let (Some(from), Some(first)) = (from, r.closed.first()) {
                if first.open_time > *from {
                    r.closed = state.client.closed_since(symbol, *tf, *from).await?;
                }
            }
            anyhow::Ok(r)
        }
        .await;
        match fetched {
            Ok(r) => {
                market.insert((symbol.clone(), *tf), r);
            }
            Err(e) => errors.push(format!("{symbol} {tf} : {e:#}")),
        }
    }

    let mut ext = External::default();
    if portfolios.iter().any(|p| p.engine.preset.needs_fear_greed()) {
        match state.fear_greed().await {
            Ok(v) => ext.fear_greed = v,
            Err(e) => errors.push(format!("Fear & Greed : {e:#}")),
        }
    }

    let now = pt_data::now_ms();
    let mut total = 0u64;
    for mut p in portfolios {
        let tf = p.engine.preset.timeframe;
        let missing: BTreeSet<&String> =
            p.engine.symbols.iter().filter(|s| !market.contains_key(&((*s).clone(), tf))).collect();
        if !missing.is_empty() {
            p.last_error = Some(format!(
                "données indisponibles : {}",
                missing.into_iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        } else {
            let feeds: BTreeMap<String, Feed> = p
                .engine
                .symbols
                .iter()
                .map(|s| {
                    let r = &market[&(s.clone(), tf)];
                    (s.clone(), Feed { closed: &r.closed, forming: r.forming.as_ref() })
                })
                .collect();
            // Journal des décisions : instantané des indicateurs à chaque entrée,
            // sortie et signal écarté, enregistré par `save_progress`.
            p.engine.record = true;
            let n = p.engine.advance(&feeds, &ext, |_, _| {});
            p.benchmark.advance(&feeds, &ext, |_, _| {});
            total += n as u64;
            p.last_error = None;
            let row = EquityRow {
                time: now,
                equity: p.engine.equity(),
                benchmark: p.benchmark.equity(),
                exposure: p.engine.portfolio.exposure(&p.engine.marks),
            };
            let saved = (|| -> Result<()> {
                let mut store = state.store.lock().expect("base");
                // L'état est enregistré AVANT le point de courbe : c'est lui qui fait foi.
                store.save_progress_and_flush(&mut p)?;
                let last = store.last_equity_time(p.id)?.unwrap_or(0);
                if n > 0 || now - last >= EQUITY_EVERY_MS {
                    store.push_equity(p.id, row)?;
                }
                Ok(())
            })();
            if let Err(e) = saved {
                errors.push(format!("{} : {e:#}", p.name));
            }
            continue;
        }
        if let Err(e) = state.store.lock().expect("base").save_progress_and_flush(&mut p) {
            errors.push(format!("{} : {e:#}", p.name));
        }
    }
    Ok((total, errors))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pt_core::{find, CostModel, Engine};
    use pt_store::Store;

    /// Deux instances ouvertes : la seconde n'obtient pas le verrou, puis le
    /// reprend dès que la première se ferme.
    #[test]
    fn only_one_instance_runs_the_live_loop() {
        let dir = std::env::temp_dir().join(format!("pt-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mode-direct.lock");
        let first = acquire_live_lock(&path).expect("première instance");
        assert!(acquire_live_lock(&path).is_none(), "la seconde instance ne doit pas avancer les portefeuilles");
        drop(first);
        assert!(acquire_live_lock(&path).is_some(), "le relais doit être possible après fermeture");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Passage complet sur le VRAI marché (réseau) : `cargo test -p papertrading -- --ignored`.
    /// Le départ est volontairement antidaté de 10 jours pour que le moteur ait des bougies à traiter.
    #[tokio::test]
    #[ignore]
    async fn live_tick_on_real_market_keeps_books_consistent() {
        let dir = std::env::temp_dir().join("pt-live-test");
        let state = AppState::new(Store::in_memory().unwrap(), dir);
        let now = pt_data::now_ms();
        let from = now - 10 * 86_400_000;
        let syms = vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()];
        let e = Engine::new(find("supertrend_7_2_1h").unwrap(), syms.clone(), 10_000.0, CostModel::default(), from);
        let b = Engine::new(find("buy_hold").unwrap(), syms, 10_000.0, CostModel::default(), from);
        let id = state.store.lock().unwrap().create("test", now, &e, &b).unwrap();

        let (bars, errors) = tick(&state).await.unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        assert!(bars >= 2 * 200, "fenêtre complète attendue, {bars} bougies traitées");
        // `load` revérifie la comptabilité contre les exécutions enregistrées.
        let p = state.store.lock().unwrap().load(id).unwrap();
        assert_eq!(p.benchmark.portfolio.positions.len(), 2, "la référence détient les deux symboles");
        assert!(p.benchmark.portfolio.closed.is_empty(), "la référence ne vend jamais");
        assert!(p.engine.marks.len() == 2 && p.engine.equity() > 0.0);
        let fills_after_first = p.engine.portfolio.fills.len();

        // Un second passage immédiat ne retraite rien (au plus une bougie fraîchement close).
        let (bars2, errors2) = tick(&state).await.unwrap();
        assert!(errors2.is_empty(), "{errors2:?}");
        assert!(bars2 <= 2, "{bars2} bougies retraitées");
        let p2 = state.store.lock().unwrap().load(id).unwrap();
        assert!(p2.engine.portfolio.fills.len() >= fills_after_first);
        assert!(!state.store.lock().unwrap().equity_curve(id).unwrap().is_empty());
        println!(
            "stratégie : {} exécutions, valeur {:.2} ; référence : valeur {:.2}",
            p2.engine.portfolio.fills.len(),
            p2.engine.equity(),
            p2.benchmark.equity()
        );
    }
}
