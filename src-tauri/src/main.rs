// Pas de console noire derrière la fenêtre en version publiée.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod accounts;
mod commands;
mod live;
mod state;
mod tray;

use state::AppState;
use std::sync::Arc;
use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;

/// Argument de l'entrée de démarrage de Windows : l'application s'ouvre alors
/// réduite dans la barre des tâches, sans fenêtre devant l'utilisateur.
const LOGIN_ARG: &str = "--demarrage";

fn launched_at_login(args: impl IntoIterator<Item = String>) -> bool {
    args.into_iter().skip(1).any(|a| a == LOGIN_ARG)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec![LOGIN_ARG])))
        .on_window_event(tray::on_window_event)
        .setup(|app| {
            // `PT_DATA_DIR` permet de lancer une instance isolée (démo, tests).
            let dir = match std::env::var("PT_DATA_DIR") {
                Ok(d) => std::path::PathBuf::from(d),
                Err(_) => app.path().app_data_dir()?,
            };
            std::fs::create_dir_all(&dir)?;
            let store = pt_store::open_app_store(&dir)?;
            let state = Arc::new(AppState::new(store, dir));
            app.manage(state.clone());
            tauri::async_runtime::spawn(live::run_loop(state, app.handle().clone()));
            if launched_at_login(std::env::args()) {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.minimize();
                }
            }
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
            commands::app_info,
            commands::get_autostart,
            commands::set_autostart,
            commands::trade_candles,
            commands::portfolio_decisions,
            accounts::list_accounts,
            accounts::save_account,
            accounts::delete_account,
        ])
        .run(tauri::generate_context!())
        .expect("impossible de démarrer PaperTrading");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_login_entry_opens_minimized() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(launched_at_login(args(&["papertrading.exe", "--demarrage"])));
        assert!(!launched_at_login(args(&["papertrading.exe"])));
        // Le chemin de l'exécutable n'est jamais pris pour l'argument.
        assert!(!launched_at_login(args(&["--demarrage"])));
    }
}
