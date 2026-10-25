//! L'icône de la barre de menus : un clic montre ou cache le popover, le titre affiche la
//! durée du lot, « Quitter Kairos » demande quoi faire d'un lot en cours.

use std::time::{Duration, Instant};

use kairos_core::clock::{Clock, SystemClock};
use kairos_core::model::Status;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow, WindowEvent, Wry,
};

use crate::bridge;
use crate::inflight::{InFlight, QUIT_WAIT};
use crate::placement::{popover_height, popover_position, Rect, Size};

const TRAY_ID: &str = "kairos";
pub const POPOVER: &str = "popover";
/// L'élément « Quitter Kairos », dans le menu de l'icône comme dans celui de l'app.
const QUIT_ID: &str = "quit";

/// Émis quand on quitte avec un lot en cours : le popover affiche le dialogue « Quitter ».
pub const QUIT_REQUESTED: &str = "kairos://quit-requested";

/// Un clic sur l'icône retire d'abord le focus au popover (il se masque), puis
/// arrive : sans cette fenêtre, le même clic le fermerait puis le rouvrirait.
pub const BLUR_CLICK_WINDOW: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleAction {
    Show,
    Hide,
}

/// Que faire d'un clic sur l'icône ? `just_blurred` : le popover vient de
/// perdre le focus à cause de ce même clic.
pub fn toggle_action(visible: bool, just_blurred: bool) -> ToggleAction {
    if visible || just_blurred {
        ToggleAction::Hide
    } else {
        ToggleAction::Show
    }
}

/// Instant où le popover a perdu le focus pour la dernière fois.
#[derive(Default)]
pub struct LastBlur(pub std::sync::Mutex<Option<Instant>>);

impl LastBlur {
    pub fn mark(&self) {
        *self.0.lock().expect("verrou LastBlur") = Some(Instant::now());
    }

    pub fn is_recent(&self) -> bool {
        self.0
            .lock()
            .expect("verrou LastBlur")
            .is_some_and(|at| at.elapsed() < BLUR_CLICK_WINDOW)
    }
}

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
        // Ce gestionnaire est global : il reçoit aussi le « Quitter Kairos » du menu de l'app.
        .on_menu_event(|app, event| {
            if event.id() == QUIT_ID {
                request_quit(app);
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
                let blurred = app.state::<LastBlur>().is_recent();
                let visible = window.is_visible().unwrap_or(false);
                match toggle_action(visible, blurred) {
                    ToggleAction::Hide => {
                        let _ = window.hide();
                    }
                    ToggleAction::Show => {
                        place(&window, rect);
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            }
        })
        .build(app)?;
    Ok(())
}

/// Montre le popover sous l'icône et lui donne le focus (seconde instance, dialogue « Quitter »).
pub fn show_popover(app: &AppHandle) {
    let Some(window) = app.get_webview_window(POPOVER) else {
        return;
    };
    if let Some(rect) = tray_rect(app) {
        place(&window, rect);
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

/// Ce qui arrive à la fenêtre du popover, réduit à ce qui compte pour elle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopoverEvent {
    LostFocus,
    CloseRequested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopoverReaction {
    /// Perdre le focus ferme le popover, comme tout popover de macOS (et on note l'instant,
    /// voir [`BLUR_CLICK_WINDOW`]).
    HideAfterBlur,
    /// Le popover n'est jamais détruit : sans lui, l'icône ne répondrait plus. On le masque.
    KeepAndHide,
}

pub fn popover_reaction(event: PopoverEvent) -> PopoverReaction {
    match event {
        PopoverEvent::LostFocus => PopoverReaction::HideAfterBlur,
        PopoverEvent::CloseRequested => PopoverReaction::KeepAndHide,
    }
}

/// Branche ces réactions sur la fenêtre du popover.
pub fn watch_popover(window: &WebviewWindow) {
    let handle = window.clone();
    window.on_window_event(move |event| {
        let (event, api) = match event {
            WindowEvent::Focused(false) => (PopoverEvent::LostFocus, None),
            WindowEvent::CloseRequested { api, .. } => (PopoverEvent::CloseRequested, Some(api)),
            _ => return,
        };
        match popover_reaction(event) {
            PopoverReaction::HideAfterBlur => handle.app_handle().state::<LastBlur>().mark(),
            PopoverReaction::KeepAndHide => {
                if let Some(api) = api {
                    api.prevent_close();
                }
            }
        }
        let _ = handle.hide();
    });
}

/// Le menu de l'app (barre de menus macOS, active quand le popover a le focus) : « Quitter
/// Kairos » (⌘Q) passe par [`request_quit`], comme le menu de l'icône ; le menu Édition fait
/// marcher ⌘C, ⌘V, ⌘A, ⌘Z dans les champs. Pas de « Fermer la fenêtre » : ⌘W ne fait rien.
pub fn app_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let quit = MenuItem::with_id(app, QUIT_ID, "Quitter Kairos", true, Some("cmd+q"))?;
    let kairos = Submenu::with_items(app, "Kairos", true, &[&quit])?;
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

/// Ajuste la hauteur du popover à son contenu (`pixels` : pixels CSS), plafonnée à l'écran,
/// en le gardant sous l'icône.
pub fn resize_popover(window: &WebviewWindow, pixels: f64) {
    let scale = window.scale_factor().unwrap_or(1.0);
    let (Ok(size), Ok(position)) = (window.outer_size(), window.outer_position()) else {
        return;
    };
    let tray = tray_rect(window.app_handle());
    let screen = tray
        .and_then(|rect| screen_of(window, physical_rect(rect, scale)))
        .or_else(|| current_screen(window));
    let Some(screen) = screen else {
        return;
    };
    let height = (popover_height(pixels, screen.height / scale) * scale).round();
    let popover = Size {
        width: f64::from(size.width),
        height,
    };
    let _ = window.set_size(PhysicalSize::new(popover.width, popover.height));
    match tray {
        Some(rect) => place_sized(window, rect, popover),
        // Sans la position de l'icône, on garde le coin haut-gauche où il est.
        None => {
            let _ = window.set_position(position);
        }
    }
}

/// Le titre à côté de l'icône, tenu à jour par le fil de surveillance.
#[derive(Default)]
pub struct MenuTitle {
    /// Le dernier état lu dans la base.
    status: Option<Status>,
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
            if let Ok(status) = bridge::open().and_then(|k| Ok(k.status()?)) {
                self.status = Some(status);
                self.read_at_minute = Some(minute);
            }
        }
        let Some(status) = &self.status else {
            return;
        };
        let title = bridge::menu_title(status, now);
        if self.shown.as_ref() != Some(&title) {
            if let Some(tray) = app.tray_by_id(TRAY_ID) {
                let _ = tray.set_title(title.as_deref());
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

fn current_screen(window: &WebviewWindow) -> Option<Rect> {
    let monitor = window.current_monitor().ok()??;
    Some(monitor_rect(&monitor))
}

/// Pose le popover sous l'icône, dans l'écran où se trouve l'icône.
fn place(window: &WebviewWindow, tray_rect: tauri::Rect) {
    let Ok(size) = window.outer_size() else {
        return;
    };
    let popover = Size {
        width: f64::from(size.width),
        height: f64::from(size.height),
    };
    place_sized(window, tray_rect, popover);
}

/// Comme [`place`], pour un popover de taille connue (pixels physiques) : juste après un
/// redimensionnement, la taille de la fenêtre n'est pas encore à jour.
fn place_sized(window: &WebviewWindow, tray_rect: tauri::Rect, popover: Size) {
    let scale = window.scale_factor().unwrap_or(1.0);
    let tray = physical_rect(tray_rect, scale);
    let Some(screen) = screen_of(window, tray) else {
        return;
    };
    let at = popover_position(tray, popover, screen);
    let _ = window.set_position(PhysicalPosition::new(at.x, at.y));
}

#[cfg(test)]
mod tests {
    use super::*;
    use kairos_core::error::Error;
    use kairos_core::model::LotView;

    #[test]
    fn a_visible_popover_is_hidden_by_a_click() {
        assert_eq!(toggle_action(true, false), ToggleAction::Hide);
    }

    #[test]
    fn a_hidden_popover_is_shown_by_a_click() {
        assert_eq!(toggle_action(false, false), ToggleAction::Show);
    }

    #[test]
    fn a_popover_that_just_lost_focus_to_this_click_stays_closed() {
        assert_eq!(toggle_action(false, true), ToggleAction::Hide);
    }

    #[test]
    fn closing_the_popover_only_hides_it() {
        // ⌘W ou une demande de fermeture : détruire la fenêtre laisserait l'icône sans popover.
        assert_eq!(
            popover_reaction(PopoverEvent::CloseRequested),
            PopoverReaction::KeepAndHide
        );
        assert_eq!(
            popover_reaction(PopoverEvent::LostFocus),
            PopoverReaction::HideAfterBlur
        );
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

    #[test]
    fn the_blur_is_remembered_only_briefly() {
        let blur = LastBlur::default();
        assert!(!blur.is_recent());
        blur.mark();
        assert!(blur.is_recent());
    }
}
