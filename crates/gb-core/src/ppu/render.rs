//! Per-scanline renderer. `render_line` draws the whole visible line `ly` into the back buffer
//! at the end of mode 3. This is the seam a pixel-FIFO renderer would replace: it only needs
//! the PPU registers, VRAM and OAM, and writes one finished line.
//!
//! Limitation (documented, out of scope for phase 3): register changes *during* mode 3 are not
//! seen, because the line is drawn in one go.

use super::debug::{bg_tile_offset, shade, tile_pixel};
use super::{Ppu, SCREEN_WIDTH};

// LCDC bits (Pan Docs "LCD Control").
const LCDC_BG_WINDOW_ENABLE: u8 = 0x01;
const LCDC_BG_MAP_SELECT: u8 = 0x08;
const LCDC_TILE_DATA_UNSIGNED: u8 = 0x10;
const LCDC_WINDOW_ENABLE: u8 = 0x20;
const LCDC_WINDOW_MAP_SELECT: u8 = 0x40;

/// Offsets inside VRAM of the two 32x32 tile maps (0x9800 and 0x9C00).
const MAP0_OFFSET: usize = 0x1800;
const MAP1_OFFSET: usize = 0x1C00;

/// The window's top-left corner is at screen x = WX - 7 (Pan Docs "Window").
const WINDOW_X_OFFSET: i32 = 7;
/// WX values of 167 and above put the window fully off the right edge.
const WINDOW_MAX_WX: u8 = 166;

impl Ppu {
    /// Draw line `self.ly` into the back buffer, and update the window line counter.
    pub(super) fn render_line(&mut self) {
        let y = usize::from(self.ly);
        // Colour index (0..=3, before palette) of each BG/window pixel, kept for sprite priority.
        let mut bg_index = [0u8; SCREEN_WIDTH];

        if self.lcdc & LCDC_BG_WINDOW_ENABLE != 0 {
            let window_on_line = self.window_visible_on_line();
            let unsigned = self.lcdc & LCDC_TILE_DATA_UNSIGNED != 0;
            let bg_map = if self.lcdc & LCDC_BG_MAP_SELECT != 0 {
                MAP1_OFFSET
            } else {
                MAP0_OFFSET
            };
            let win_map = if self.lcdc & LCDC_WINDOW_MAP_SELECT != 0 {
                MAP1_OFFSET
            } else {
                MAP0_OFFSET
            };
            let window_x = i32::from(self.wx) - WINDOW_X_OFFSET;
            let bg_y = (y + usize::from(self.scy)) & 0xFF;

            for (x, slot) in bg_index.iter_mut().enumerate() {
                let (map, px, py) = if window_on_line && x as i32 >= window_x {
                    (
                        win_map,
                        (x as i32 - window_x) as usize,
                        usize::from(self.window_line),
                    )
                } else {
                    (bg_map, (x + usize::from(self.scx)) & 0xFF, bg_y)
                };
                let id = self.vram[map + (py / 8) * 32 + px / 8];
                let tile = bg_tile_offset(id, unsigned);
                *slot = tile_pixel(&self.vram, tile, px % 8, py % 8);
            }
            if window_on_line {
                // The counter only advances on lines where the window was actually drawn
                // (Pan Docs "Window": a window hidden by WX or turned off mid-frame resumes
                // where it left off).
                self.window_line = self.window_line.wrapping_add(1);
            }
        }

        let row = &mut self.back[y * SCREEN_WIDTH..(y + 1) * SCREEN_WIDTH];
        for (px, &idx) in row.iter_mut().zip(bg_index.iter()) {
            *px = shade(self.bgp, idx);
        }
    }

    /// Whether the window is drawn on the current line: enabled, WY reached, WX on screen.
    fn window_visible_on_line(&self) -> bool {
        self.lcdc & LCDC_WINDOW_ENABLE != 0 && self.ly >= self.wy && self.wx <= WINDOW_MAX_WX
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ppu::SHADES;

    /// PPU with BGP identity palette, LCD on, BG on, 0x8000 tile addressing, map 0x9800.
    fn ppu() -> Ppu {
        let mut p = Ppu::new();
        p.lcdc = 0x91;
        p.bgp = 0xE4;
        p
    }

    /// Fill all 8 rows of tile `n` with the 8 colour indices in `row` (given left to right).
    fn set_tile(p: &mut Ppu, n: usize, row: [u8; 8]) {
        for y in 0..8 {
            let (mut lo, mut hi) = (0u8, 0u8);
            for (x, &c) in row.iter().enumerate() {
                lo |= (c & 1) << (7 - x);
                hi |= (c >> 1) << (7 - x);
            }
            p.vram[n * 16 + y * 2] = lo;
            p.vram[n * 16 + y * 2 + 1] = hi;
        }
    }

    fn line(p: &mut Ppu, ly: u8) -> Vec<u32> {
        p.ly = ly;
        p.render_line();
        let y = usize::from(ly);
        p.back[y * SCREEN_WIDTH..(y + 1) * SCREEN_WIDTH].to_vec()
    }

    #[test]
    fn background_tile_and_scx_scroll() {
        let mut p = ppu();
        set_tile(&mut p, 1, [0, 1, 2, 3, 0, 1, 2, 3]);
        p.vram[0x1800] = 1; // map (0,0) -> tile 1
        let l = line(&mut p, 0);
        assert_eq!(&l[0..4], &[SHADES[0], SHADES[1], SHADES[2], SHADES[3]]);
        p.scx = 1;
        let l = line(&mut p, 0);
        assert_eq!(
            l[0], SHADES[1],
            "SCX=1 shifts the picture left by one pixel"
        );
    }

    #[test]
    fn scroll_wraps_around_256() {
        let mut p = ppu();
        set_tile(&mut p, 2, [3; 8]);
        p.vram[0x1800 + 31] = 2; // last column of row 0
        p.scx = 248; // pixel 0 of the screen is map pixel 248 = start of column 31
        let l = line(&mut p, 0);
        assert_eq!(l[0], SHADES[3]);
        assert_eq!(l[8], SHADES[0], "wrapped to column 0 (tile 0, blank)");
    }

    #[test]
    fn scy_selects_the_row() {
        let mut p = ppu();
        // Tile 1: row 0 is index 3, other rows index 0.
        p.vram[16] = 0xFF;
        p.vram[17] = 0xFF;
        p.vram[0x1800 + 32] = 1; // map row 1, column 0
        p.scy = 8;
        assert_eq!(line(&mut p, 0)[0], SHADES[3]);
        assert_eq!(line(&mut p, 1)[0], SHADES[0]);
    }

    #[test]
    fn tile_data_select_bit_uses_signed_ids_from_9000() {
        let mut p = ppu();
        p.lcdc &= !LCDC_TILE_DATA_UNSIGNED;
        set_tile(&mut p, 256 + 128 - 128, [2; 8]); // 0x9000 = tile index 256 in 0x8000 numbering
        p.vram[0x1800] = 0; // id 0 -> 0x9000
        assert_eq!(line(&mut p, 0)[0], SHADES[2]);
        set_tile(&mut p, 128, [1; 8]); // 0x8800 = id 0x80 (-128)
        p.vram[0x1800] = 0x80;
        assert_eq!(line(&mut p, 0)[0], SHADES[1]);
    }

    #[test]
    fn bg_map_select_bit_uses_9c00() {
        let mut p = ppu();
        p.lcdc |= LCDC_BG_MAP_SELECT;
        set_tile(&mut p, 3, [3; 8]);
        p.vram[0x1C00] = 3;
        assert_eq!(line(&mut p, 0)[0], SHADES[3]);
    }

    #[test]
    fn bgp_remaps_colours() {
        let mut p = ppu();
        set_tile(&mut p, 1, [1; 8]);
        p.vram[0x1800] = 1;
        p.bgp = 0b11_10_00_01; // index 1 -> shade 0 (white)
        assert_eq!(line(&mut p, 0)[0], SHADES[0]);
    }

    #[test]
    fn bg_disabled_draws_white() {
        let mut p = ppu();
        set_tile(&mut p, 1, [3; 8]);
        p.vram[0x1800] = 1;
        p.lcdc &= !LCDC_BG_WINDOW_ENABLE;
        assert!(line(&mut p, 0).iter().all(|&c| c == SHADES[0]));
    }

    fn window_ppu() -> Ppu {
        let mut p = ppu();
        p.lcdc |= LCDC_WINDOW_ENABLE | LCDC_WINDOW_MAP_SELECT;
        set_tile(&mut p, 5, [3; 8]);
        p.vram[0x1C00] = 5; // window map (0,0)
        p.wx = 7 + 16; // starts at screen x = 16
        p.wy = 0;
        p
    }

    #[test]
    fn window_starts_at_wx_minus_7() {
        let mut p = window_ppu();
        let l = line(&mut p, 0);
        assert_eq!(l[15], SHADES[0]);
        assert_eq!(l[16], SHADES[3]);
        assert_eq!(l[23], SHADES[3]);
        assert_eq!(l[24], SHADES[0], "window map column 1 is tile 0 (blank)");
    }

    #[test]
    fn window_hidden_when_wx_167_or_wy_below_ly() {
        let mut p = window_ppu();
        p.wx = 167;
        assert!(line(&mut p, 0).iter().all(|&c| c == SHADES[0]));
        let mut p = window_ppu();
        p.wy = 10;
        assert!(line(&mut p, 9).iter().all(|&c| c == SHADES[0]));
        assert_eq!(line(&mut p, 10)[16], SHADES[3]);
    }

    #[test]
    fn window_line_counter_only_advances_on_lines_where_it_was_drawn() {
        let mut p = window_ppu();
        // Window map row 1 (tile row 1) uses tile 6; row 0 uses tile 5.
        set_tile(&mut p, 6, [1; 8]);
        p.vram[0x1C00 + 32] = 6;
        // Draw 8 window lines (rows 0..7 of the window), then hide it for 3 lines, then resume.
        for ly in 0..8 {
            assert_eq!(line(&mut p, ly)[16], SHADES[3]);
        }
        assert_eq!(p.window_line, 8);
        p.lcdc &= !LCDC_WINDOW_ENABLE;
        for ly in 8..11 {
            line(&mut p, ly);
        }
        assert_eq!(p.window_line, 8, "hidden lines do not advance the counter");
        p.lcdc |= LCDC_WINDOW_ENABLE;
        assert_eq!(
            line(&mut p, 11)[16],
            SHADES[1],
            "resumes with window row 8 (tile row 1), not row 11"
        );
    }
}
