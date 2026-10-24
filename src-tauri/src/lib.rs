//! Kairos : l'enveloppe Tauri. L'icône de la barre de menus, son popover et le pont vers
//! `kairos-core`, où vivent toutes les règles métier.

mod bridge;
mod placement;
mod tray;
mod watcher;

use tauri::{ActivationPolicy, Manager, RunEvent, WindowEvent};

pub fn run() {
    tauri::Builder::default()
        // En premier : une seconde instance se ferme aussitôt et montre la première.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_popover(app);
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(tray::LastBlur::default())
        .invoke_handler(tauri::generate_handler![
            bridge::commands::snapshot,
            bridge::commands::subtask_choices,
            bridge::commands::start,
            bridge::commands::switch,
            bridge::commands::stop,
            bridge::commands::pause,
            bridge::commands::resume,
            bridge::commands::set_popover_height,
            bridge::commands::quit,
        ])
        .setup(|app| {
            // Une app de barre de menus : pas d'icône dans le Dock, pas de fenêtre au démarrage.
            app.set_activation_policy(ActivationPolicy::Accessory);
            tray::build(app.handle())?;
            watcher::spawn(app.handle().clone())?;

            if let Some(window) = app.get_webview_window("popover") {
                let handle = window.clone();
                window.on_window_event(move |event| {
                    // Perdre le focus ferme le popover, comme tout popover de macOS.
                    if let WindowEvent::Focused(false) = event {
                        handle.app_handle().state::<tray::LastBlur>().mark();
                        let _ = handle.hide();
                    }
                });
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("erreur au lancement de Kairos")
        .run(|app, event| {
            // ⌘Q (menu de l'app) demande à quitter sans code : on passe par le même chemin que
            // « Quitter Kairos ». `app.exit(0)`, lui, porte un code et quitte vraiment.
            if let RunEvent::ExitRequested {
                code: None, api, ..
            } = event
            {
                api.prevent_exit();
                tray::request_quit(app);
            }
        });
}
