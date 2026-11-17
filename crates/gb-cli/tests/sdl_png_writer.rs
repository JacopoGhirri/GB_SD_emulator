//! The frontend's hand-written PNG writer (`gb-sdl/src/png.rs`, used for F12 screenshots) is
//! checked here against the real `png` decoder, which gb-sdl may not depend on.
//! The file is self-contained, so it is included by path instead of linking the binary crate.

#[path = "../../gb-sdl/src/png.rs"]
#[allow(dead_code)]
mod sdl_png;

fn decode(bytes: &[u8]) -> (png::OutputInfo, Vec<u8>) {
    let mut reader = png::Decoder::new(bytes)
        .read_info()
        .expect("valid PNG header");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("valid PNG image data");
    buf.truncate(info.buffer_size());
    (info, buf)
}

/// Deterministic pseudo-random picture so every byte is checked, not just flat colours.
fn picture(w: u32, h: u32, seed: u32) -> Vec<u32> {
    let mut x = seed;
    (0..w * h)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            x >> 8 // 0x00RRGGBB
        })
        .collect()
}

fn check(w: u32, h: u32, seed: u32) {
    let pixels = picture(w, h, seed);
    let (info, data) = decode(&sdl_png::encode_rgb(w, h, &pixels));
    assert_eq!((info.width, info.height), (w, h));
    assert_eq!(info.color_type, png::ColorType::Rgb);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    let expected: Vec<u8> = pixels
        .iter()
        .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8])
        .collect();
    assert_eq!(data, expected, "{w}x{h} pixels differ after the round trip");
}

#[test]
fn game_boy_sized_picture_round_trips() {
    // 160x144: 69264 raw bytes, so the zlib stream has two stored blocks.
    check(160, 144, 1);
}

#[test]
fn awkward_sizes_round_trip() {
    for (w, h) in [
        (1, 1),
        (2, 1),
        (1, 2),
        (7, 3),
        (255, 1),
        (256, 3),
        (21845, 1),
        (21846, 1),
    ] {
        check(w, h, w * 31 + h);
    }
}

#[test]
fn exact_block_boundaries_round_trip() {
    // Raw size = (3w + 1) * h. The stored-block limit is 65535 bytes.
    check(1456, 15, 5); // (4368 + 1) * 15 = 65535: exactly one full block
    check(21845, 1, 6); // 65536: one full block plus one byte
    check(1, 16384, 7); // 4 * 16384 = 65536 as a tall image
    check(21844, 1, 8); // 65533: just under the limit
}
