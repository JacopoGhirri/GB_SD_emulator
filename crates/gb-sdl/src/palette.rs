//! DMG colour schemes. The core always renders DMG pictures in four greys (`ppu::SHADES`,
//! which the acid2 reference images need), so a scheme is applied here, in the frontend, by
//! swapping each grey for another colour. It is skipped whenever the console is in CGB mode:
//! a real Game Boy Color game can legitimately use pure white or grey, and must not be recoloured.

use gb_core::ppu::SHADES;

/// Name and the four colours (`0x00RRGGBB`) for shade 0 (lightest) to shade 3 (darkest).
pub const PALETTES: &[(&str, [u32; 4])] = &[
    // The core's own greys: no change.
    ("grey", SHADES),
    // Original DMG LCD, the olive green most people remember.
    (
        "green",
        [0x009B_BC0F, 0x008B_AC0F, 0x0030_6230, 0x000F_380F],
    ),
    // Game Boy Pocket: greener-grey, less yellow.
    (
        "pocket",
        [0x00C4_CFA1, 0x008B_956D, 0x004D_533C, 0x001F_1F1F],
    ),
    // The light mint scheme used by several popular emulators.
    ("mint", [0x00E0_F8D0, 0x0088_C070, 0x0034_6856, 0x0008_1820]),
];

pub fn by_name(name: &str) -> Option<usize> {
    PALETTES
        .iter()
        .position(|(n, _)| n.eq_ignore_ascii_case(name))
}

pub fn next(index: usize) -> usize {
    (index + 1) % PALETTES.len()
}

pub fn name(index: usize) -> &'static str {
    PALETTES[index % PALETTES.len()].0
}

/// Copy `src` to `dst`, recolouring the four DMG greys. Any other pixel passes through.
/// `dst` must have the same length as `src`.
pub fn apply(index: usize, src: &[u32], dst: &mut [u32]) {
    let colours = PALETTES[index % PALETTES.len()].1;
    for (d, &s) in dst.iter_mut().zip(src) {
        *d = SHADES
            .iter()
            .position(|&g| g == s)
            .map_or(s, |shade| colours[shade]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grey_is_the_cores_own_shades_and_is_first() {
        assert_eq!(PALETTES[0], ("grey", SHADES));
        assert_eq!(by_name("GREY"), Some(0));
        assert_eq!(by_name("nope"), None);
    }

    #[test]
    fn names_are_unique_and_colours_distinct() {
        for (i, (n, c)) in PALETTES.iter().enumerate() {
            assert_eq!(by_name(n), Some(i), "duplicate name {n}");
            for a in 0..4 {
                for b in a + 1..4 {
                    assert_ne!(c[a], c[b], "{n}: shades {a} and {b} look the same");
                }
            }
        }
    }

    #[test]
    fn apply_recolours_shades_and_leaves_other_pixels() {
        let green = by_name("green").unwrap();
        let src = [SHADES[0], SHADES[1], SHADES[2], SHADES[3], 0x0012_3456];
        let mut dst = [0; 5];
        apply(green, &src, &mut dst);
        assert_eq!(dst[..4], PALETTES[green].1);
        assert_eq!(
            dst[4], 0x0012_3456,
            "a colour that is not a DMG grey is kept"
        );
        apply(0, &src, &mut dst);
        assert_eq!(dst, src, "grey changes nothing");
    }

    #[test]
    fn next_wraps_around() {
        assert_eq!(next(PALETTES.len() - 1), 0);
        assert_eq!(name(next(0)), PALETTES[1].0);
    }
}
