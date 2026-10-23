//! Kairos : l'enveloppe Tauri. L'icône de la barre de menus et son popover ;
//! toutes les règles métier vivent dans `kairos-core`.

mod placement;
mod tray;

use tauri::{ActivationPolicy, Manager, WindowEvent};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(tray::LastBlur::default())
        .setup(|app| {
            // Une app de barre de menus : pas d'icône dans le Dock, pas de fenêtre au démarrage.
            app.set_activation_policy(ActivationPolicy::Accessory);
            tray::build(app.handle())?;

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
        .run(tauri::generate_context!())
        .expect("erreur au lancement de Kairos");
}
