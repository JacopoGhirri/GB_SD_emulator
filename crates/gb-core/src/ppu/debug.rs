//! Debug views of VRAM for `gb-cli` dumps: the tile atlas and the two background maps.
//! Pure functions over VRAM bytes, no I/O. They show what the game *put* in VRAM, regardless of
//! what the renderer does with it, which is what you want when a picture looks wrong.

use super::SHADES;

/// Tiles are 8x8 pixels, 16 bytes each, 2 bits per pixel (Pan Docs "Tile Data").
const TILE_BYTES: usize = 16;
/// 0x8000..=0x97FF holds 384 tiles.
pub const TILE_COUNT: usize = 384;
pub const ATLAS_TILES_PER_ROW: usize = 16;
pub const ATLAS_WIDTH: usize = ATLAS_TILES_PER_ROW * 8;
pub const ATLAS_HEIGHT: usize = TILE_COUNT / ATLAS_TILES_PER_ROW * 8;
pub const BG_MAP_SIZE: usize = 256;

/// Offsets inside VRAM (address - 0x8000).
const MAP0_OFFSET: usize = 0x1800;
const MAP1_OFFSET: usize = 0x1C00;
/// Tile id 0 of the "0x8800 method" sits at 0x9000.
const SIGNED_BASE_OFFSET: usize = 0x1000;

/// Colour index (0..=3) of pixel `x` (0 = leftmost) on row `y` of the tile starting at `tile`.
/// A row is two bytes: the low bit of each pixel is in the first, the high bit in the second,
/// with bit 7 being the leftmost pixel.
pub fn tile_pixel(vram: &[u8], tile: usize, x: usize, y: usize) -> u8 {
    let lo = vram[tile + y * 2];
    let hi = vram[tile + y * 2 + 1];
    let bit = 7 - x;
    (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1)
}

/// Map a colour index through a DMG palette register (2 bits per index) to a screen colour.
pub fn shade(palette: u8, index: u8) -> u32 {
    SHADES[usize::from((palette >> (index * 2)) & 3)]
}

/// All 384 tiles laid out 16 per row, `ATLAS_WIDTH` x `ATLAS_HEIGHT` pixels, through `bgp`.
pub fn tile_atlas(vram: &[u8], bgp: u8) -> Vec<u32> {
    let mut out = vec![0; ATLAS_WIDTH * ATLAS_HEIGHT];
    for t in 0..TILE_COUNT {
        let (tx, ty) = (t % ATLAS_TILES_PER_ROW, t / ATLAS_TILES_PER_ROW);
        for y in 0..8 {
            for x in 0..8 {
                let c = tile_pixel(vram, t * TILE_BYTES, x, y);
                out[(ty * 8 + y) * ATLAS_WIDTH + tx * 8 + x] = shade(bgp, c);
            }
        }
    }
    out
}

/// Start of a BG/window tile in VRAM for map entry `id`. With LCDC bit 4 set tiles are indexed
/// unsigned from 0x8000; otherwise signed from 0x9000 (the "0x8800 method"), Pan Docs
/// "LCDC.4". Returns an offset into VRAM.
pub fn bg_tile_offset(id: u8, unsigned_addressing: bool) -> usize {
    if unsigned_addressing {
        usize::from(id) * TILE_BYTES
    } else {
        // `id as i8` reinterprets the byte as signed: 0x80..=0xFF map to tiles 0x8800..0x8FFF.
        (SIGNED_BASE_OFFSET as isize + isize::from(id as i8) * TILE_BYTES as isize) as usize
    }
}

/// One full 256x256 background map. `second_map` selects 0x9C00 instead of 0x9800;
/// `unsigned_addressing` is LCDC bit 4. No scrolling and no window: the raw map.
pub fn bg_map(vram: &[u8], second_map: bool, unsigned_addressing: bool, bgp: u8) -> Vec<u32> {
    let base = if second_map { MAP1_OFFSET } else { MAP0_OFFSET };
    let mut out = vec![0; BG_MAP_SIZE * BG_MAP_SIZE];
    for my in 0..32 {
        for mx in 0..32 {
            let id = vram[base + my * 32 + mx];
            let tile = bg_tile_offset(id, unsigned_addressing);
            for y in 0..8 {
                for x in 0..8 {
                    let c = tile_pixel(vram, tile, x, y);
                    out[(my * 8 + y) * BG_MAP_SIZE + mx * 8 + x] = shade(bgp, c);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vram() -> Vec<u8> {
        vec![0; 0x2000]
    }

    #[test]
    fn pixel_bits_come_from_two_planes_msb_first() {
        let mut v = vram();
        // Row 0: lo = 1000_0001, hi = 1100_0000 -> indices 3,2,0,0,0,0,0,1.
        v[0] = 0b1000_0001;
        v[1] = 0b1100_0000;
        let row: Vec<u8> = (0..8).map(|x| tile_pixel(&v, 0, x, 0)).collect();
        assert_eq!(row, [3, 2, 0, 0, 0, 0, 0, 1]);
    }

    #[test]
    fn palette_maps_indices_to_shades() {
        // BGP 0xE4 is the identity mapping 0,1,2,3.
        assert_eq!(shade(0xE4, 0), SHADES[0]);
        assert_eq!(shade(0xE4, 3), SHADES[3]);
        // 0x1B reverses it.
        assert_eq!(shade(0x1B, 0), SHADES[3]);
        assert_eq!(shade(0x1B, 3), SHADES[0]);
    }

    #[test]
    fn signed_addressing_puts_tile_0_at_9000_and_negative_at_8800() {
        assert_eq!(bg_tile_offset(0, true), 0);
        assert_eq!(bg_tile_offset(1, true), 16);
        assert_eq!(bg_tile_offset(0, false), 0x1000);
        assert_eq!(bg_tile_offset(0x7F, false), 0x1000 + 127 * 16);
        assert_eq!(bg_tile_offset(0x80, false), 0x0800);
        assert_eq!(bg_tile_offset(0xFF, false), 0x1000 - 16);
    }

    #[test]
    fn atlas_places_tile_n_at_row_major_position() {
        let mut v = vram();
        // Tile 17 (second row, second column): first pixel row all index 3.
        v[17 * 16] = 0xFF;
        v[17 * 16 + 1] = 0xFF;
        let atlas = tile_atlas(&v, 0xE4);
        assert_eq!(atlas[8 * ATLAS_WIDTH + 8], SHADES[3]);
        assert_eq!(atlas[8 * ATLAS_WIDTH + 15], SHADES[3]);
        assert_eq!(atlas[9 * ATLAS_WIDTH + 8], SHADES[0]);
    }

    #[test]
    fn bg_map_uses_the_selected_map_and_addressing() {
        let mut v = vram();
        v[0x1800] = 1; // map 0, top-left entry -> tile 1
        v[0x1C00 + 1] = 0xFF; // map 1, entry (1,0) -> signed tile -1 = 0x8FF0
        v[16] = 0xFF;
        v[17] = 0xFF; // tile 1 row 0 solid
        v[0x1000 - 16] = 0xFF;
        v[0x1000 - 15] = 0xFF;
        let m0 = bg_map(&v, false, true, 0xE4);
        assert_eq!(m0[0], SHADES[3]);
        let m1 = bg_map(&v, true, false, 0xE4);
        assert_eq!(m1[8], SHADES[3]);
        assert_eq!(m1[0], SHADES[0]);
    }
}
