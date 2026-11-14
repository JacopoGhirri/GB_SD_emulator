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
const LCDC_SPRITES_ENABLE: u8 = 0x02;
const LCDC_SPRITE_SIZE_16: u8 = 0x04;
const LCDC_BG_MAP_SELECT: u8 = 0x08;
const LCDC_TILE_DATA_UNSIGNED: u8 = 0x10;
const LCDC_WINDOW_ENABLE: u8 = 0x20;
const LCDC_WINDOW_MAP_SELECT: u8 = 0x40;

/// Offsets inside VRAM of the two 32x32 tile maps (0x9800 and 0x9C00).
const MAP0_OFFSET: usize = 0x1800;
const MAP1_OFFSET: usize = 0x1C00;

// OAM entry: 4 bytes per sprite, 40 sprites (Pan Docs "Object Attribute Memory").
const OAM_ENTRY_BYTES: usize = 4;
const SPRITE_COUNT: usize = 40;
/// Hardware draws at most 10 sprites per scanline: the first 10 found in OAM order.
const MAX_SPRITES_PER_LINE: usize = 10;
/// OAM Y is the sprite's top edge + 16, OAM X its left edge + 8 (so 0 hides it off-screen).
const SPRITE_Y_OFFSET: i32 = 16;
const SPRITE_X_OFFSET: i32 = 8;
// Sprite attribute bits.
const ATTR_BG_OVER_OBJ: u8 = 0x80;
const ATTR_Y_FLIP: u8 = 0x40;
const ATTR_X_FLIP: u8 = 0x20;
const ATTR_PALETTE_OBP1: u8 = 0x10;

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

        let mut shades = [0u32; SCREEN_WIDTH];
        for (px, &idx) in shades.iter_mut().zip(bg_index.iter()) {
            *px = shade(self.bgp, idx);
        }
        if self.lcdc & LCDC_SPRITES_ENABLE != 0 {
            self.draw_sprites(y as i32, &bg_index, &mut shades);
        }
        self.back[y * SCREEN_WIDTH..(y + 1) * SCREEN_WIDTH].copy_from_slice(&shades);
    }

    /// Overlay the sprites that touch line `y` onto `shades`.
    ///
    /// OAM scan: the first 10 sprites (in OAM order) whose vertical range covers the line are
    /// selected, whether or not they are visible horizontally. Priority on DMG (Pan Docs
    /// "OBJ Priority"): the sprite with the smaller X wins; equal X: the earlier OAM entry wins.
    /// Colour index 0 is transparent. The sprite's BG-over-OBJ flag lets BG colours 1-3 cover it.
    fn draw_sprites(
        &self,
        y: i32,
        bg_index: &[u8; SCREEN_WIDTH],
        shades: &mut [u32; SCREEN_WIDTH],
    ) {
        let height = if self.lcdc & LCDC_SPRITE_SIZE_16 != 0 {
            16
        } else {
            8
        };
        // (x, oam index, top y, tile, attributes)
        let mut selected: Vec<(i32, usize, i32, u8, u8)> = Vec::with_capacity(MAX_SPRITES_PER_LINE);
        for i in 0..SPRITE_COUNT {
            let e = &self.oam[i * OAM_ENTRY_BYTES..(i + 1) * OAM_ENTRY_BYTES];
            let top = i32::from(e[0]) - SPRITE_Y_OFFSET;
            if y >= top && y < top + height {
                selected.push((i32::from(e[1]) - SPRITE_X_OFFSET, i, top, e[2], e[3]));
                if selected.len() == MAX_SPRITES_PER_LINE {
                    break;
                }
            }
        }
        // Stable order by X then OAM index; the first opaque sprite pixel found wins.
        selected.sort_by_key(|&(x, i, ..)| (x, i));

        for (sx, slot) in shades.iter_mut().enumerate() {
            for &(x, _, top, tile, attr) in &selected {
                let col = sx as i32 - x;
                if !(0..8).contains(&col) {
                    continue;
                }
                let mut row = (y - top) as usize;
                if attr & ATTR_Y_FLIP != 0 {
                    row = height as usize - 1 - row;
                }
                let col = if attr & ATTR_X_FLIP != 0 {
                    7 - col
                } else {
                    col
                } as usize;
                // In 8x16 mode bit 0 of the tile number is ignored: the sprite is tile N&0xFE
                // on top and N|1 below, and row 8..15 simply continues into the next tile.
                let tile = if height == 16 { tile & 0xFE } else { tile };
                let idx = tile_pixel(&self.vram, usize::from(tile) * 16, col, row);
                if idx == 0 {
                    continue; // transparent: look at the next sprite
                }
                if attr & ATTR_BG_OVER_OBJ == 0 || bg_index[sx] == 0 {
                    let palette = if attr & ATTR_PALETTE_OBP1 != 0 {
                        self.obp1
                    } else {
                        self.obp0
                    };
                    *slot = shade(palette, idx);
                }
                break; // this opaque pixel decided the winner, hidden or not
            }
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

    // --- sprites ---

    /// Put sprite `n` (OAM index) at screen (x, y) top-left; (x, y) may be off-screen.
    fn set_sprite(p: &mut Ppu, n: usize, x: i32, y: i32, tile: u8, attr: u8) {
        p.oam[n * 4] = (y + 16) as u8;
        p.oam[n * 4 + 1] = (x + 8) as u8;
        p.oam[n * 4 + 2] = tile;
        p.oam[n * 4 + 3] = attr;
    }

    fn sprite_ppu() -> Ppu {
        let mut p = ppu();
        p.lcdc |= LCDC_SPRITES_ENABLE;
        p.obp0 = 0xE4;
        p.obp1 = 0xE4;
        set_tile(&mut p, 1, [0, 1, 2, 3, 3, 2, 1, 0]);
        p
    }

    #[test]
    fn sprite_draws_with_transparent_colour_0() {
        let mut p = sprite_ppu();
        set_sprite(&mut p, 0, 10, 0, 1, 0);
        let l = line(&mut p, 0);
        assert_eq!(l[10], SHADES[0], "index 0 is transparent: BG shows");
        assert_eq!(l[11], SHADES[1]);
        assert_eq!(l[13], SHADES[3]);
        assert_eq!(l[18], SHADES[0]);
    }

    #[test]
    fn sprite_flips_and_palette() {
        let mut p = sprite_ppu();
        p.obp1 = 0b00_01_10_11; // reverse mapping
        set_sprite(&mut p, 0, 0, 0, 1, ATTR_X_FLIP | ATTR_PALETTE_OBP1);
        let l = line(&mut p, 0);
        // X-flipped row: 0,1,2,3,3,2,1,0 reversed is the same (symmetric) so use pixel 1:
        assert_eq!(l[1], shade(p.obp1, 1));
        // Y flip: tile row 0 solid, other rows 0.
        let mut p = sprite_ppu();
        p.vram[32] = 0xFF;
        p.vram[33] = 0xFF;
        set_sprite(&mut p, 0, 0, 0, 2, ATTR_Y_FLIP);
        assert_eq!(line(&mut p, 0)[0], SHADES[0], "row 0 shows tile row 7");
        assert_eq!(line(&mut p, 7)[0], SHADES[3], "row 7 shows tile row 0");
    }

    #[test]
    fn sprite_partially_off_screen_left_and_hidden_at_x_0() {
        let mut p = sprite_ppu();
        set_sprite(&mut p, 0, -4, 0, 1, 0); // OAM X = 4: columns 4..7 visible at x 0..3
        let l = line(&mut p, 0);
        assert_eq!(l[0], SHADES[3]);
        assert_eq!(l[2], SHADES[1]);
        let mut p = sprite_ppu();
        set_sprite(&mut p, 0, -8, 0, 1, 0); // OAM X = 0: fully hidden
        assert!(line(&mut p, 0).iter().all(|&c| c == SHADES[0]));
    }

    #[test]
    fn bg_over_obj_only_hides_sprite_behind_nonzero_bg() {
        let mut p = sprite_ppu();
        set_tile(&mut p, 2, [0, 3, 0, 3, 0, 3, 0, 3]);
        p.vram[0x1800] = 2;
        set_sprite(&mut p, 0, 0, 0, 1, ATTR_BG_OVER_OBJ);
        let l = line(&mut p, 0);
        assert_eq!(l[1], SHADES[3], "BG index 3 covers the sprite");
        assert_eq!(l[2], SHADES[2], "BG index 0: sprite shows (tile index 2)");
    }

    #[test]
    fn lower_x_wins_then_lower_oam_index() {
        let mut p = sprite_ppu();
        set_tile(&mut p, 2, [3; 8]);
        // OAM 0 at x=4 shows tile 1 (index 1 at col 4..). OAM 1 at x=2 is solid index 3.
        set_sprite(&mut p, 0, 4, 0, 1, 0);
        set_sprite(&mut p, 1, 2, 0, 2, 0);
        assert_eq!(
            line(&mut p, 0)[5],
            SHADES[3],
            "smaller X wins even with larger OAM index"
        );
        // Equal X: earlier OAM entry wins.
        let mut p = sprite_ppu();
        set_tile(&mut p, 2, [2; 8]);
        set_sprite(&mut p, 0, 0, 0, 2, 0);
        set_sprite(&mut p, 1, 0, 0, 1, 0);
        assert_eq!(line(&mut p, 0)[3], SHADES[2]);
    }

    #[test]
    fn transparent_pixel_of_the_winner_lets_the_next_sprite_show() {
        let mut p = sprite_ppu();
        set_tile(&mut p, 2, [2; 8]);
        set_sprite(&mut p, 0, 0, 0, 1, 0); // pixel 0 is transparent
        set_sprite(&mut p, 1, 0, 0, 2, 0);
        assert_eq!(line(&mut p, 0)[0], SHADES[2]);
    }

    #[test]
    fn only_first_10_sprites_in_oam_order_are_drawn_per_line() {
        let mut p = sprite_ppu();
        set_tile(&mut p, 2, [3; 8]);
        for n in 0..11 {
            set_sprite(&mut p, n, (n * 9) as i32, 0, 2, 0);
        }
        let l = line(&mut p, 0);
        assert_eq!(l[9 * 9], SHADES[3], "the 10th sprite is drawn");
        assert_eq!(l[10 * 9], SHADES[0], "the 11th is not");
        // The 11th is on the next line only if it covers it; sprites on other lines don't count.
        let mut p = sprite_ppu();
        set_tile(&mut p, 2, [3; 8]);
        for n in 0..10 {
            set_sprite(&mut p, n, (n * 9) as i32, 20, 2, 0);
        }
        set_sprite(&mut p, 10, 100, 0, 2, 0);
        assert_eq!(line(&mut p, 0)[100], SHADES[3]);
    }

    #[test]
    fn eight_by_sixteen_ignores_tile_bit_0_and_spans_two_tiles() {
        let mut p = sprite_ppu();
        p.lcdc |= LCDC_SPRITE_SIZE_16;
        set_tile(&mut p, 4, [1; 8]); // top
        set_tile(&mut p, 5, [2; 8]); // bottom
        set_sprite(&mut p, 0, 0, 0, 5, 0); // tile 5 behaves like 4
        assert_eq!(line(&mut p, 0)[0], SHADES[1]);
        assert_eq!(line(&mut p, 7)[0], SHADES[1]);
        assert_eq!(line(&mut p, 8)[0], SHADES[2]);
        assert_eq!(line(&mut p, 15)[0], SHADES[2]);
        let mut p2 = p;
        set_sprite(&mut p2, 0, 0, 0, 4, ATTR_Y_FLIP);
        assert_eq!(
            line(&mut p2, 0)[0],
            SHADES[2],
            "Y flip swaps the two halves"
        );
    }

    #[test]
    fn sprites_disabled_by_lcdc_bit_1() {
        let mut p = sprite_ppu();
        p.lcdc &= !LCDC_SPRITES_ENABLE;
        set_sprite(&mut p, 0, 0, 0, 1, 0);
        assert!(line(&mut p, 0).iter().all(|&c| c == SHADES[0]));
    }
}
