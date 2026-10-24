//! Où poser le popover : centré sous l'icône de la barre de menus, mais
//! toujours entièrement visible, même quand l'icône est au bord de l'écran ou
//! sur un second écran. Tout en pixels physiques, sans rien demander à Tauri :
//! c'est une fonction pure, donc testable.

/// Marge gardée entre le popover et le bord de l'écran.
pub const EDGE_MARGIN: f64 = 8.0;
/// Espace entre l'icône et le haut du popover.
pub const GAP_BELOW_TRAY: f64 = 4.0;
/// Le popover ne dépasse jamais la hauteur de l'écran moins cette marge (pixels logiques) :
/// au-delà, son contenu défile.
pub const SCREEN_HEIGHT_MARGIN: f64 = 80.0;
/// Hauteur minimale du popover (pixels logiques), même si l'interface demande moins.
pub const MIN_POPOVER_HEIGHT: f64 = 120.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Coin haut-gauche du popover : centré sous `tray`, ramené dans `screen`.
pub fn popover_position(tray: Rect, popover: Size, screen: Rect) -> Point {
    let centered = tray.x + tray.width / 2.0 - popover.width / 2.0;
    let min_x = screen.x + EDGE_MARGIN;
    let max_x = (screen.x + screen.width - popover.width - EDGE_MARGIN).max(min_x);
    let x = centered.clamp(min_x, max_x);

    let below = tray.y + tray.height + GAP_BELOW_TRAY;
    let max_y = (screen.y + screen.height - popover.height - EDGE_MARGIN).max(screen.y);
    let y = below.clamp(screen.y, max_y);

    Point { x, y }
}

/// La hauteur du popover pour un contenu de `requested` pixels, sur un écran de
/// `screen_height` pixels (logiques tous les deux).
pub fn popover_height(requested: f64, screen_height: f64) -> f64 {
    let max = (screen_height - SCREEN_HEIGHT_MARGIN).max(MIN_POPOVER_HEIGHT);
    if !requested.is_finite() {
        return MIN_POPOVER_HEIGHT;
    }
    requested.clamp(MIN_POPOVER_HEIGHT, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 1440.0,
        height: 900.0,
    };
    const POPOVER: Size = Size {
        width: 340.0,
        height: 420.0,
    };

    fn tray_at(x: f64) -> Rect {
        Rect {
            x,
            y: 0.0,
            width: 24.0,
            height: 24.0,
        }
    }

    #[test]
    fn centered_under_the_icon_when_everything_fits() {
        let p = popover_position(tray_at(700.0), POPOVER, SCREEN);
        assert_eq!(p.x, 700.0 + 12.0 - 170.0);
        assert_eq!(p.y, 24.0 + GAP_BELOW_TRAY);
    }

    #[test]
    fn pulled_back_left_when_the_icon_is_at_the_right_edge() {
        let p = popover_position(tray_at(1430.0), POPOVER, SCREEN);
        assert_eq!(p.x, 1440.0 - 340.0 - EDGE_MARGIN);
    }

    #[test]
    fn pulled_back_right_when_the_icon_is_at_the_left_edge() {
        let p = popover_position(tray_at(0.0), POPOVER, SCREEN);
        assert_eq!(p.x, EDGE_MARGIN);
    }

    #[test]
    fn stays_inside_a_second_screen_placed_to_the_right() {
        let second = Rect {
            x: 1440.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        };
        let p = popover_position(
            Rect {
                x: 1445.0,
                y: 0.0,
                width: 24.0,
                height: 24.0,
            },
            POPOVER,
            second,
        );
        assert_eq!(p.x, 1440.0 + EDGE_MARGIN);
        let p = popover_position(
            Rect {
                x: 3350.0,
                y: 0.0,
                width: 24.0,
                height: 24.0,
            },
            POPOVER,
            second,
        );
        assert_eq!(p.x, 1440.0 + 1920.0 - 340.0 - EDGE_MARGIN);
    }

    #[test]
    fn never_above_the_top_edge_and_never_below_the_bottom() {
        let above = Rect {
            x: 700.0,
            y: -50.0,
            width: 24.0,
            height: 24.0,
        };
        assert_eq!(popover_position(above, POPOVER, SCREEN).y, 0.0);
        let low = Rect {
            x: 700.0,
            y: 880.0,
            width: 24.0,
            height: 24.0,
        };
        assert_eq!(
            popover_position(low, POPOVER, SCREEN).y,
            900.0 - 420.0 - EDGE_MARGIN
        );
    }

    #[test]
    fn a_popover_wider_than_the_screen_sticks_to_the_left_margin() {
        let tiny = Rect {
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 900.0,
        };
        assert_eq!(
            popover_position(tray_at(100.0), POPOVER, tiny).x,
            EDGE_MARGIN
        );
    }

    #[test]
    fn the_height_follows_the_content_up_to_the_screen_minus_80() {
        assert_eq!(popover_height(420.0, 900.0), 420.0);
        assert_eq!(popover_height(1200.0, 900.0), 820.0);
        assert_eq!(popover_height(10.0, 900.0), MIN_POPOVER_HEIGHT);
        assert_eq!(popover_height(f64::NAN, 900.0), MIN_POPOVER_HEIGHT);
        assert_eq!(popover_height(500.0, 150.0), MIN_POPOVER_HEIGHT);
    }
}
