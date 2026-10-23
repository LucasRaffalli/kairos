//! L'icône de la barre de menus : un clic montre ou cache le popover.

use std::time::{Duration, Instant};

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize};

use crate::placement::{popover_position, Rect, Size};

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
    let quit = MenuItem::with_id(app, "quit", "Quitter Kairos", true, Some("cmd+q"))?;
    let menu = Menu::with_items(app, &[&quit])?;
    let icon = Image::from_bytes(include_bytes!("../icons/sources/tray-template.png"))?;

    TrayIconBuilder::with_id("kairos")
        .icon(icon)
        .icon_as_template(true)
        .tooltip("Kairos")
        .menu(&menu)
        // Le menu s'ouvre au clic droit seulement : le clic gauche est pour le popover.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            if event.id() == "quit" {
                app.exit(0);
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
                let Some(window) = app.get_webview_window("popover") else {
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

/// Pose le popover sous l'icône, dans l'écran où se trouve l'icône.
fn place(window: &tauri::WebviewWindow, tray_rect: tauri::Rect) {
    let scale = window.scale_factor().unwrap_or(1.0);
    let tray_pos: PhysicalPosition<f64> = tray_rect.position.to_physical(scale);
    let tray_size: PhysicalSize<f64> = tray_rect.size.to_physical(scale);
    let tray = Rect {
        x: tray_pos.x,
        y: tray_pos.y,
        width: tray_size.width,
        height: tray_size.height,
    };

    let Ok(Some(monitor)) = window.monitor_from_point(tray.x, tray.y) else {
        return;
    };
    let mp = monitor.position();
    let ms = monitor.size();
    let screen = Rect {
        x: f64::from(mp.x),
        y: f64::from(mp.y),
        width: f64::from(ms.width),
        height: f64::from(ms.height),
    };

    let Ok(size) = window.outer_size() else {
        return;
    };
    let popover = Size {
        width: f64::from(size.width),
        height: f64::from(size.height),
    };
    let at = popover_position(tray, popover, screen);
    let _ = window.set_position(PhysicalPosition::new(at.x, at.y));
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn the_blur_is_remembered_only_briefly() {
        let blur = LastBlur::default();
        assert!(!blur.is_recent());
        blur.mark();
        assert!(blur.is_recent());
    }
}
