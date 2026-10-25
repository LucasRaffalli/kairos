//! L'icône de la barre de menus : un clic montre, ramène au premier plan ou range la fenêtre, le
//! titre affiche la durée du lot, « Quitter Kairos » demande quoi faire d'un lot en cours.

use std::sync::atomic::{AtomicBool, Ordering};

use kairos_core::clock::{Clock, SystemClock};
use kairos_core::model::Status;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow, WindowEvent, Wry,
};

use crate::bridge;
use crate::chord::{chord_title, Chords};
use crate::inflight::{InFlight, QUIT_WAIT};
use crate::placement::{fit_height, popover_position, Rect, Size};
use crate::tray_font;

const TRAY_ID: &str = "kairos";
pub const POPOVER: &str = "popover";
/// L'élément « Quitter Kairos », dans le menu de l'icône comme dans celui de l'app.
const QUIT_ID: &str = "quit";
/// L'élément « Fermer la fenêtre » (⌘W) du menu de l'app : il range la fenêtre, jamais ne la détruit.
const CLOSE_ID: &str = "close-window";

/// Émis quand on quitte avec un lot en cours : le popover affiche le dialogue « Quitter ».
pub const QUIT_REQUESTED: &str = "kairos://quit-requested";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleAction {
    /// La fenêtre est rangée : l'afficher et lui donner le focus.
    Show,
    /// Elle est visible mais derrière d'autres fenêtres : la ramener devant, sans la ranger.
    Raise,
    /// Elle est visible et au premier plan : la ranger.
    Hide,
}

/// Que faire d'un clic sur l'icône ? `focused` : la fenêtre est au premier plan, avec le focus.
pub fn toggle_action(visible: bool, focused: bool) -> ToggleAction {
    match (visible, focused) {
        (false, _) => ToggleAction::Show,
        (true, false) => ToggleAction::Raise,
        (true, true) => ToggleAction::Hide,
    }
}

/// Vrai une fois la fenêtre posée sous l'icône. Ensuite elle garde la place où l'utilisateur l'a
/// mise, le temps de la session : rien n'est écrit sur disque.
#[derive(Default)]
pub struct Placed(AtomicBool);

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let quit = MenuItem::with_id(app, QUIT_ID, "Quitter Kairos", true, Some("cmd+q"))?;
    let menu = Menu::with_items(app, &[&quit])?;
    let icon = Image::from_bytes(include_bytes!("../icons/sources/tray-template.png"))?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(true)
        .tooltip("Kairos")
        .menu(&menu)
        // Le menu s'ouvre au clic droit seulement : le clic gauche est pour le popover.
        .show_menu_on_left_click(false)
        // Ce gestionnaire est global : il reçoit aussi les éléments du menu de l'app.
        .on_menu_event(|app, event| {
            if event.id() == QUIT_ID {
                request_quit(app);
            } else if event.id() == CLOSE_ID {
                if let Some(window) = app.get_webview_window(POPOVER) {
                    let _ = window.hide();
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                let app = tray.app_handle();
                let Some(window) = app.get_webview_window(POPOVER) else {
                    return;
                };
                let visible = window.is_visible().unwrap_or(false);
                let focused = window.is_focused().unwrap_or(false);
                match toggle_action(visible, focused) {
                    ToggleAction::Hide => {
                        let _ = window.hide();
                    }
                    ToggleAction::Raise => {
                        let _ = window.set_focus();
                    }
                    ToggleAction::Show => reveal(&window, Some(rect)),
                }
            }
        })
        .build(app)?;
    Ok(())
}

/// Affiche la fenêtre et lui donne le focus (seconde instance, dialogue « Quitter »). Elle reste
/// où elle est si elle a déjà été posée.
pub fn show_popover(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(POPOVER) {
        reveal(&window, tray_rect(app));
    }
}

/// Montre la fenêtre. Au tout premier affichage, elle se centre sous l'icône ; les suivants la
/// laissent où l'utilisateur l'a mise.
fn reveal(window: &WebviewWindow, tray: Option<tauri::Rect>) {
    let placed = window.state::<Placed>();
    if !placed.0.load(Ordering::Relaxed) {
        if let Some(tray) = tray {
            if place_under_icon(window, tray) {
                placed.0.store(true, Ordering::Relaxed);
            }
        }
    }
    let _ = window.show();
    let _ = window.set_focus();
}

/// Ce que « Quitter Kairos » fait d'après l'état du cœur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitDecision {
    /// Quitter tout de suite.
    Exit,
    /// Un lot tourne : ouvrir le popover et demander quoi en faire.
    Ask,
}

/// Seul un lot en cours demande confirmation. Une base illisible (verrouillée, trop récente)
/// ne bloque pas la sortie : le dialogue n'aurait aucun résumé à montrer, et un lot ouvert reste
/// compté depuis son heure de départ, rien n'est perdu.
pub fn quit_decision(status: &bridge::BridgeResult<Status>) -> QuitDecision {
    match status {
        Ok(status) if bridge::lot_is_running(status) => QuitDecision::Ask,
        _ => QuitDecision::Exit,
    }
}

/// « Quitter Kairos » (menu de l'icône ou ⌘Q) : sans lot en cours, l'app quitte ; avec un lot
/// en cours, le popover s'ouvre et l'interface affiche le dialogue.
pub fn request_quit(app: &AppHandle) {
    let app = app.clone();
    // Ouvrir la base peut attendre un verrou : jamais sur le fil principal.
    std::thread::spawn(move || {
        // D'abord laisser finir les actions en vol : un `start` en cours serait lu comme « rien
        // en cours », et l'app quitterait sans dialogue.
        app.state::<InFlight>().wait_idle(QUIT_WAIT);
        let status = bridge::open().and_then(|k| Ok(k.status()?));
        match quit_decision(&status) {
            QuitDecision::Exit => exit_when_idle(&app),
            QuitDecision::Ask => {
                show_popover(&app);
                let _ = app.emit(QUIT_REQUESTED, ());
            }
        }
    });
}

/// Quitte l'app une fois les actions en vol terminées (au plus [`QUIT_WAIT`]) : chaque action
/// est une transaction, qu'on ne coupe pas. Bloquant : jamais sur le fil principal.
pub fn exit_when_idle(app: &AppHandle) {
    app.state::<InFlight>().wait_idle(QUIT_WAIT);
    app.exit(0);
}

/// Branche la fermeture sur la fenêtre : le bouton rouge (ou ⌘W, ou Échap) la range sans jamais
/// la détruire, car sans elle l'icône ne répondrait plus. Perdre le focus ne fait rien.
pub fn watch_popover(window: &WebviewWindow) {
    let handle = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = handle.hide();
        }
    });
}

/// Le menu de l'app (barre de menus macOS, active quand la fenêtre a le focus) : « Quitter
/// Kairos » (⌘Q) passe par [`request_quit`], comme le menu de l'icône ; « Fermer la fenêtre » (⌘W)
/// la range ; le menu Édition fait marcher ⌘C, ⌘V, ⌘A, ⌘Z dans les champs.
pub fn app_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let quit = MenuItem::with_id(app, QUIT_ID, "Quitter Kairos", true, Some("cmd+q"))?;
    let close = MenuItem::with_id(app, CLOSE_ID, "Fermer la fenêtre", true, Some("cmd+w"))?;
    let kairos = Submenu::with_items(app, "Kairos", true, &[&close, &quit])?;
    let edit = Submenu::with_items(
        app,
        "Édition",
        true,
        &[
            &PredefinedMenuItem::undo(app, Some("Annuler"))?,
            &PredefinedMenuItem::redo(app, Some("Rétablir"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, Some("Couper"))?,
            &PredefinedMenuItem::copy(app, Some("Copier"))?,
            &PredefinedMenuItem::paste(app, Some("Coller"))?,
            &PredefinedMenuItem::select_all(app, Some("Tout sélectionner"))?,
        ],
    )?;
    Menu::with_items(app, &[&kairos, &edit])
}

/// Le titre à côté de l'icône, tenu à jour par le fil de surveillance.
#[derive(Default)]
pub struct MenuTitle {
    /// Le dernier état lu dans la base, avec les réglages internes.
    state: Option<bridge::MenuState>,
    /// Le réglage `global_shortcuts` déjà transmis au fil des raccourcis.
    shortcuts_applied: Option<bool>,
    /// La minute (depuis l'epoch) de cette lecture.
    read_at_minute: Option<i64>,
    /// Le titre affiché, pour ne le poser que s'il change.
    shown: Option<Option<String>>,
}

impl MenuTitle {
    /// Appelé chaque seconde. Relit l'état quand la base a changé et à chaque minute pile (la
    /// coupure de minuit, par exemple), puis recalcule le titre depuis l'heure de départ.
    pub fn tick(&mut self, app: &AppHandle, base_changed: bool) {
        let now = SystemClock.now();
        let minute = now.div_euclid(60);
        if base_changed || self.read_at_minute != Some(minute) {
            // Base illisible : on garde le dernier titre et on réessaie à la seconde suivante.
            if let Ok(state) = bridge::read_menu_state() {
                self.state = Some(state);
                self.read_at_minute = Some(minute);
            }
        }
        let Some(state) = &self.state else {
            return;
        };
        let chords = app.state::<Chords>();
        // Allumer ou éteindre ⌘K quand le réglage change (et au premier relevé), sans relancer l'app.
        if self.shortcuts_applied != Some(state.global_shortcuts) {
            chords.enable(state.global_shortcuts);
            self.shortcuts_applied = Some(state.global_shortcuts);
        }
        let title = chord_title(
            bridge::menu_title(&state.status, now, state.seconds),
            chords.is_armed(),
        );
        if self.shown.as_ref() != Some(&title) {
            if let Some(tray) = app.tray_by_id(TRAY_ID) {
                let _ = tray.set_title(title.as_deref());
                tray_font::use_fixed_width_digits(&tray);
            }
            self.shown = Some(title);
        }
    }
}

fn tray_rect(app: &AppHandle) -> Option<tauri::Rect> {
    app.tray_by_id(TRAY_ID)?.rect().ok().flatten()
}

fn physical_rect(rect: tauri::Rect, scale: f64) -> Rect {
    let position: PhysicalPosition<f64> = rect.position.to_physical(scale);
    let size: PhysicalSize<f64> = rect.size.to_physical(scale);
    Rect {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    }
}

fn monitor_rect(monitor: &tauri::Monitor) -> Rect {
    let position = monitor.position();
    let size = monitor.size();
    Rect {
        x: f64::from(position.x),
        y: f64::from(position.y),
        width: f64::from(size.width),
        height: f64::from(size.height),
    }
}

/// L'écran qui contient l'icône.
fn screen_of(window: &WebviewWindow, tray: Rect) -> Option<Rect> {
    let monitor = window.monitor_from_point(tray.x, tray.y).ok()??;
    Some(monitor_rect(&monitor))
}

/// Pose la fenêtre sous l'icône, dans l'écran où se trouve l'icône, après avoir ramené sa hauteur
/// à l'écran si celui-ci est trop petit. Rend `false` si l'écran ou la taille de la fenêtre restent
/// inconnus : elle n'est alors pas posée.
fn place_under_icon(window: &WebviewWindow, tray_rect: tauri::Rect) -> bool {
    let scale = window.scale_factor().unwrap_or(1.0);
    let tray = physical_rect(tray_rect, scale);
    let (Ok(size), Some(screen)) = (window.inner_size(), screen_of(window, tray)) else {
        return false;
    };
    let height =
        (fit_height(f64::from(size.height) / scale, screen.height / scale) * scale).round();
    if height != f64::from(size.height) {
        let _ = window.set_size(PhysicalSize::new(f64::from(size.width), height));
    }
    let popover = Size {
        width: f64::from(size.width),
        height,
    };
    let at = popover_position(tray, popover, screen);
    window
        .set_position(PhysicalPosition::new(at.x, at.y))
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kairos_core::error::Error;
    use kairos_core::model::LotView;

    #[test]
    fn a_hidden_window_is_shown_by_a_click() {
        assert_eq!(toggle_action(false, false), ToggleAction::Show);
        // Une fenêtre masquée n'a pas le focus ; si le système dit autre chose, elle reste à montrer.
        assert_eq!(toggle_action(false, true), ToggleAction::Show);
    }

    #[test]
    fn a_window_behind_others_is_brought_forward_not_hidden() {
        assert_eq!(toggle_action(true, false), ToggleAction::Raise);
    }

    #[test]
    fn a_window_in_front_is_hidden_by_a_click() {
        assert_eq!(toggle_action(true, true), ToggleAction::Hide);
    }

    #[test]
    fn a_running_lot_asks_before_quitting() {
        assert_eq!(quit_decision(&Ok(running())), QuitDecision::Ask);
    }

    #[test]
    fn nothing_running_or_a_pause_quits_at_once() {
        assert_eq!(quit_decision(&Ok(Status::Idle)), QuitDecision::Exit);
        let paused = Status::Paused {
            lot: lot(),
            since: 0,
            duration: 60,
            launched_by: None,
            workdir: None,
        };
        assert_eq!(quit_decision(&Ok(paused)), QuitDecision::Exit);
    }

    #[test]
    fn an_unreadable_database_still_quits() {
        // Le dialogue n'aurait pas de résumé à montrer : un lot ouvert reste compté depuis son
        // heure de départ, rien n'est perdu.
        for error in [Error::DbUnavailable, Error::DbNewer] {
            let status = Err(bridge::BridgeError::from(error));
            assert_eq!(quit_decision(&status), QuitDecision::Exit);
        }
    }

    fn lot() -> LotView {
        LotView {
            id: 1,
            parent_monday_id: Some("1".into()),
            parent_label: None,
            subtask: None,
            subtask_monday_id: None,
            name: None,
        }
    }

    fn running() -> Status {
        Status::Running {
            lot: lot(),
            since: 0,
            duration: 0,
            today_total: 0,
            launched_by: None,
            workdir: None,
        }
    }
}
