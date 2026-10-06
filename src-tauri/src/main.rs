// Pas de console noire derrière la fenêtre en version publiée.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod live;
mod state;

use state::AppState;
use std::sync::Arc;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // `PT_DATA_DIR` permet de lancer une instance isolée (démo, tests).
            let dir = match std::env::var("PT_DATA_DIR") {
                Ok(d) => std::path::PathBuf::from(d),
                Err(_) => app.path().app_data_dir()?,
            };
            std::fs::create_dir_all(&dir)?;
            let store = pt_store::Store::open(dir.join("papertrading2.sqlite"))?;
            let state = Arc::new(AppState::new(store, dir));
            app.manage(state.clone());
            tauri::async_runtime::spawn(live::run_loop(state, app.handle().clone()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::catalog,
            commands::get_settings,
            commands::save_settings,
            commands::engine_status,
            commands::set_engine_running,
            commands::tick_now,
            commands::list_portfolios,
            commands::portfolio_detail,
            commands::create_portfolio,
            commands::set_portfolio_active,
            commands::delete_portfolio,
            commands::run_backtest,
            commands::run_comparison,
            commands::run_validation,
            commands::app_info,
        ])
        .run(tauri::generate_context!())
        .expect("impossible de démarrer PaperTrading2");
}
