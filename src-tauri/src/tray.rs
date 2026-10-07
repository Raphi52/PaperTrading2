//! Icône près de l'horloge (zone de notification) : fermer la fenêtre la range
//! ici au lieu d'arrêter le mode direct. « Quitter », dans le menu de l'icône,
//! arrête vraiment l'application.

use crate::state::AppState;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Window, WindowEvent};

const TRAY_ID: &str = "principal";

/// Ranger la fenêtre au lieu de la fermer : seulement si cette instance fait avancer
/// les portefeuilles, si Windows affiche réellement l'icône (sinon la fenêtre cachée
/// ne pourrait plus être rouverte) et si l'utilisateur n'a pas demandé à quitter.
pub fn hide_instead_of_close(runs_live_loop: bool, icon_shown: bool, quitting: bool) -> bool {
    runs_live_loop && icon_shown && !quitting
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn quit_app(app: &AppHandle) {
    if let Some(state) = app.try_state::<Arc<AppState>>() {
        state.quitting.store(true, Ordering::SeqCst);
    }
    app.exit(0);
}

/// Crée l'icône. Appelée par l'instance qui fait avancer les portefeuilles, sur le
/// fil principal. Sans zone de notification (session sans barre des tâches), la
/// création réussit mais l'icône n'apparaît pas : voir [`icon_shown`].
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "ouvrir", "Ouvrir PaperTrading", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quitter", "Quitter (arrête le mode direct)", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("PaperTrading · mode direct actif")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "ouvrir" => show_main(app),
            "quitter" => quit_app(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// L'icône est-elle réellement affichée ? Windows ne rend sa position que dans ce cas.
fn icon_shown(app: &AppHandle) -> bool {
    app.tray_by_id(TRAY_ID).and_then(|t| t.rect().ok().flatten()).is_some()
}

pub fn on_window_event(window: &Window, event: &WindowEvent) {
    let WindowEvent::CloseRequested { api, .. } = event else { return };
    let app = window.app_handle();
    let Some(state) = app.try_state::<Arc<AppState>>() else { return };
    let runs_live_loop = !state.status.lock().expect("statut").elsewhere;
    if hide_instead_of_close(runs_live_loop, icon_shown(app), state.quitting.load(Ordering::SeqCst)) {
        api.prevent_close();
        let _ = window.hide();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_hides_only_when_the_icon_can_bring_it_back() {
        assert!(hide_instead_of_close(true, true, false));
        // Sans icône affichée, la fenêtre cachée serait impossible à rouvrir : on ferme.
        assert!(!hide_instead_of_close(true, false, false));
        // Une seconde fenêtre « affichage seul » se ferme normalement.
        assert!(!hide_instead_of_close(false, true, false));
        // « Quitter » dans le menu de l'icône ferme vraiment.
        assert!(!hide_instead_of_close(true, true, true));
    }
}
