//! Window geometry helpers.

use gb_core::{SCREEN_HEIGHT, SCREEN_WIDTH};

/// Largest integer scale that fits the window (at least 1).
/// Deck 1280x800 -> 5 (800x720); integer scaling keeps pixels crisp.
pub fn integer_scale(win_w: u32, win_h: u32) -> u32 {
    (win_w / SCREEN_WIDTH as u32)
        .min(win_h / SCREEN_HEIGHT as u32)
        .max(1)
}

/// Destination rectangle (x, y, w, h) centred in the window; the rest is letterbox.
pub fn dest_rect(win_w: u32, win_h: u32) -> (i32, i32, u32, u32) {
    let s = integer_scale(win_w, win_h);
    let w = SCREEN_WIDTH as u32 * s;
    let h = SCREEN_HEIGHT as u32 * s;
    (
        (win_w as i32 - w as i32) / 2,
        (win_h as i32 - h as i32) / 2,
        w,
        h,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steam_deck_screen_scales_by_five() {
        assert_eq!(integer_scale(1280, 800), 5);
        assert_eq!(dest_rect(1280, 800), (240, 40, 800, 720));
    }

    #[test]
    fn tiny_window_still_scales_by_one() {
        assert_eq!(integer_scale(100, 100), 1);
    }

    #[test]
    fn height_can_be_the_limit() {
        assert_eq!(integer_scale(1920, 1080), 7);
    }
}
