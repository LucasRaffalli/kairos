//! Chiffres de largeur fixe pour le titre de l'icône. La police de la barre de menus a des chiffres
//! proportionnels (un « 1 » est plus étroit) : à chaque seconde, le titre change de largeur et
//! décale toutes les icônes voisines. On pose donc sur le bouton de l'icône la même police, en
//! chiffres tabulaires (`monospacedDigitSystemFont`).
//!
//! `tray-icon` écrit le titre avec `setTitle`, qui garde la police du bouton : un appel suffit,
//! mais on le répète à chaque mise à jour du titre, pour rester juste si le bouton était recréé.
//! Si quoi que ce soit échoue, le titre reste tel quel : rien ne panique, rien n'est perdu.

use tauri::tray::TrayIcon;
use tauri::Runtime;

/// Passe le titre de `tray` en chiffres de largeur fixe (macOS ; sans effet ailleurs).
#[cfg(target_os = "macos")]
pub fn use_fixed_width_digits<R: Runtime>(tray: &TrayIcon<R>) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSFont, NSFontWeightRegular};

    // `with_inner_tray_icon` exécute la fermeture sur le fil principal, seul à toucher AppKit.
    let _ = tray.with_inner_tray_icon(|inner| {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let Some(button) = inner.ns_status_item().and_then(|item| item.button(mtm)) else {
            return;
        };
        // Taille 0 : celle que le système donne à la barre de menus.
        let size = NSFont::menuBarFontOfSize(0.0).pointSize();
        let font =
            NSFont::monospacedDigitSystemFontOfSize_weight(size, unsafe { NSFontWeightRegular });
        button.setFont(Some(&font));
    });
}

#[cfg(not(target_os = "macos"))]
pub fn use_fixed_width_digits<R: Runtime>(_tray: &TrayIcon<R>) {}
