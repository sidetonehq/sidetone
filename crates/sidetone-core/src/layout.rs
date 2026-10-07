//! Window placement in X-Plane's global boxel space (y grows upwards; `top > bottom`).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Bounds {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.top - self.bottom
    }
}

/// Default spot for the minimal panel: top-left of the screen, inset by `margin`.
/// The top margin is larger so the panel clears X-Plane's menu bar.
pub fn panel_default(screen: Bounds, width: i32, height: i32, margin: i32) -> Bounds {
    let top = screen.top - margin - MENU_BAR_HEIGHT;
    let left = screen.left + margin;
    Bounds { left, top, right: left + width, bottom: top - height }
}

/// Places a window of the given size with its top-left at `(left, top)`, kept fully on screen.
pub fn place_clamped(screen: Bounds, left: i32, top: i32, width: i32, height: i32) -> Bounds {
    let width = width.min(screen.width());
    let height = height.min(screen.height());
    let left = left.clamp(screen.left, screen.right - width);
    let top = top.clamp(screen.bottom + height, screen.top);
    Bounds { left, top, right: left + width, bottom: top - height }
}

/// The main window attached under the panel: same left edge and width, `gap` below it (room
/// for the window's title bar), as tall as asked but never past the bottom of the screen.
pub fn attached_below(panel: Bounds, height: i32, gap: i32, screen: Bounds) -> Bounds {
    let top = panel.bottom - gap;
    let room = top - screen.bottom - gap;
    let height = height.min(room).max(MIN_ATTACHED_HEIGHT.min(room.max(0)));
    Bounds { left: panel.left, top, right: panel.right, bottom: top - height }
}

/// Shortest the attached window gets when the panel sits low on the screen.
const MIN_ATTACHED_HEIGHT: i32 = 200;

/// Centres a window on the screen.
pub fn centered(screen: Bounds, width: i32, height: i32) -> Bounds {
    let left = screen.left + (screen.width() - width) / 2;
    let top = screen.top - (screen.height() - height) / 2;
    place_clamped(screen, left, top, width, height)
}

const MENU_BAR_HEIGHT: i32 = 28;

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Bounds = Bounds { left: 0, top: 1080, right: 1920, bottom: 0 };

    #[test]
    fn panel_sits_top_left() {
        let b = panel_default(SCREEN, 320, 64, 12);
        assert_eq!(b.left, 12);
        assert_eq!(b.width(), 320);
        assert_eq!(b.height(), 64);
        assert_eq!(b.top, 1080 - 12 - MENU_BAR_HEIGHT);
    }

    #[test]
    fn clamps_offscreen_positions() {
        let b = place_clamped(SCREEN, 1800, 2000, 320, 64);
        assert_eq!(b, Bounds { left: 1600, top: 1080, right: 1920, bottom: 1016 });
        let b = place_clamped(SCREEN, -50, 10, 320, 64);
        assert_eq!(b.left, 0);
        assert_eq!(b.bottom, 0);
    }

    #[test]
    fn attaches_under_the_panel() {
        let panel = Bounds { left: 12, top: 1040, right: 452, bottom: 956 };
        let b = attached_below(panel, 600, 30, SCREEN);
        assert_eq!(b, Bounds { left: 12, top: 926, right: 452, bottom: 326 });
        // Not past the bottom of the screen.
        let low = attached_below(panel, 5000, 30, SCREEN);
        assert_eq!(low.bottom, 30);
    }

    #[test]
    fn centres() {
        let b = centered(SCREEN, 600, 400);
        assert_eq!(b, Bounds { left: 660, top: 740, right: 1260, bottom: 340 });
    }
}
