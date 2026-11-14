//! PNG I/O for screenshots and `--compare`. Pixels are the core's 0x00RRGGBB values.

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use gb_core::{SCREEN_HEIGHT, SCREEN_WIDTH};

/// Convert 0x00RRGGBB pixels to packed RGB bytes.
pub fn to_rgb_bytes(pixels: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels.len() * 3);
    for &p in pixels {
        out.push((p >> 16) as u8);
        out.push((p >> 8) as u8);
        out.push(p as u8);
    }
    out
}

/// Write the 160x144 screen.
pub fn write_png(path: &Path, pixels: &[u32]) -> Result<(), String> {
    write_png_sized(path, pixels, SCREEN_WIDTH, SCREEN_HEIGHT)
}

/// Write any `width` x `height` image of 0x00RRGGBB pixels (used by the VRAM dumps).
pub fn write_png_sized(
    path: &Path,
    pixels: &[u32],
    width: usize,
    height: usize,
) -> Result<(), String> {
    let file = File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer
        .write_image_data(&to_rgb_bytes(pixels))
        .map_err(|e| e.to_string())
}

/// Count pixels that differ between `pixels` and the reference PNG at `path`.
/// The reference may be RGB, RGBA, greyscale or greyscale+alpha (dmg-acid2's is 2-bit greyscale);
/// low bit depths are expanded to 8 bits (2-bit: 0, 85, 170, 255 = $00, $55, $AA, $FF).
/// Returns an error for other formats or sizes.
pub fn compare_with_png(path: &Path, pixels: &[u32]) -> Result<usize, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut decoder = png::Decoder::new(BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;

    if info.width as usize != SCREEN_WIDTH || info.height as usize != SCREEN_HEIGHT {
        return Err(format!(
            "reference is {}x{}, expected {SCREEN_WIDTH}x{SCREEN_HEIGHT}",
            info.width, info.height
        ));
    }
    if info.bit_depth != png::BitDepth::Eight {
        return Err(format!("unsupported bit depth {:?}", info.bit_depth));
    }
    let data = &buf[..info.buffer_size()];
    match info.color_type {
        png::ColorType::Rgb => Ok(count_differences(pixels, data, 3)),
        png::ColorType::Rgba => Ok(count_differences(pixels, data, 4)),
        png::ColorType::Grayscale | png::ColorType::GrayscaleAlpha => {
            let step = if info.color_type == png::ColorType::Grayscale {
                1
            } else {
                2
            };
            let rgb: Vec<u8> = data
                .chunks(step)
                .flat_map(|px| [px[0], px[0], px[0]])
                .collect();
            Ok(count_differences(pixels, &rgb, 3))
        }
        other => Err(format!("unsupported reference colour type {other:?}")),
    }
}

/// Compare 0x00RRGGBB pixels with interleaved RGB/RGBA bytes. Alpha is ignored.
pub fn count_differences(pixels: &[u32], reference: &[u8], channels: usize) -> usize {
    pixels
        .iter()
        .enumerate()
        .filter(|(i, &p)| {
            let r = &reference[i * channels..i * channels + 3];
            let ours = [(p >> 16) as u8, (p >> 8) as u8, p as u8];
            ours != *r
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_bytes_are_big_endian_per_pixel() {
        assert_eq!(to_rgb_bytes(&[0x00AABBCC]), vec![0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn counts_only_differing_pixels_and_ignores_alpha() {
        let pixels = [0x00FF0000, 0x0000FF00, 0x000000FF];
        // RGBA reference: pixel 0 matches, pixel 1 differs in green, pixel 2 matches (alpha 0).
        let reference = [255, 0, 0, 255, 0, 1, 0, 255, 0, 0, 255, 0];
        assert_eq!(count_differences(&pixels, &reference, 4), 1);
    }

    #[test]
    fn compares_against_a_2_bit_greyscale_png() {
        // Round trip: write a 2-bit greyscale PNG with shades 0..3 and compare.
        let dir = std::env::temp_dir().join(format!("gb-cli-grey-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("g.png");
        let mut enc = png::Encoder::new(
            BufWriter::new(File::create(&path).unwrap()),
            SCREEN_WIDTH as u32,
            SCREEN_HEIGHT as u32,
        );
        enc.set_color(png::ColorType::Grayscale);
        enc.set_depth(png::BitDepth::Two);
        let mut w = enc.write_header().unwrap();
        // 160 px * 2 bits = 40 bytes per row; every pixel is shade index 1 (0b01).
        w.write_image_data(&vec![0b0101_0101; 40 * SCREEN_HEIGHT])
            .unwrap();
        drop(w);
        let same = vec![0x0055_5555; SCREEN_WIDTH * SCREEN_HEIGHT];
        assert_eq!(compare_with_png(&path, &same).unwrap(), 0);
        let mut other = same.clone();
        other[0] = 0x00AA_AAAA;
        assert_eq!(compare_with_png(&path, &other).unwrap(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn identical_frames_have_zero_differences() {
        let pixels = vec![0x00123456; 4];
        let reference = to_rgb_bytes(&pixels);
        assert_eq!(count_differences(&pixels, &reference, 3), 0);
    }
}
