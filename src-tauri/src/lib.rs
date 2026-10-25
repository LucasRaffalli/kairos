//! Kairos : l'enveloppe Tauri. L'icône de la barre de menus, son popover et le pont vers
//! `kairos-core`, où vivent toutes les règles métier.

mod bridge;
mod chord;
mod inflight;
mod placement;
mod tray;
mod tray_font;
mod watcher;

use tauri::{ActivationPolicy, Manager};

pub fn run() {
    let (shortcuts, chord_worker, chords) = chord::plugin();
    tauri::Builder::default()
        // En premier : une seconde instance se ferme aussitôt et montre la première.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_popover(app);
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(shortcuts)
        .manage(chords)
        .manage(tray::Placed::default())
        .manage(inflight::InFlight::default())
        // Remplace le menu par défaut de macOS : son « Quitter » (⌘Q) fermerait l'app sans
        // passer par le dialogue, son « Fermer la fenêtre » (⌘W) détruirait la fenêtre.
        .menu(tray::app_menu)
        .invoke_handler(tauri::generate_handler![
            bridge::commands::snapshot,
            bridge::commands::subtask_choices,
            bridge::commands::start,
            bridge::commands::switch,
            bridge::commands::stop,
            bridge::commands::pause,
            bridge::commands::resume,
            bridge::commands::quit,
        ])
        .setup(|app| {
            // Une app de barre de menus : pas d'icône dans le Dock, pas de fenêtre au démarrage.
            app.set_activation_policy(ActivationPolicy::Accessory);
            tray::build(app.handle())?;
            chord::spawn(app.handle().clone(), chord_worker)?;
            watcher::spawn(app.handle().clone())?;

            if let Some(window) = app.get_webview_window(tray::POPOVER) {
                tray::watch_popover(&window);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erreur au lancement de Kairos");
}
