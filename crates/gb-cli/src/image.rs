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

pub fn write_png(path: &Path, pixels: &[u32]) -> Result<(), String> {
    let file = File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(
        BufWriter::new(file),
        SCREEN_WIDTH as u32,
        SCREEN_HEIGHT as u32,
    );
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer
        .write_image_data(&to_rgb_bytes(pixels))
        .map_err(|e| e.to_string())
}

/// Count pixels that differ between `pixels` and the reference PNG at `path`.
/// Returns an error if the reference is not a 160x144 RGB or RGBA image.
pub fn compare_with_png(path: &Path, pixels: &[u32]) -> Result<usize, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let decoder = png::Decoder::new(BufReader::new(file));
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;

    if info.width as usize != SCREEN_WIDTH || info.height as usize != SCREEN_HEIGHT {
        return Err(format!(
            "reference is {}x{}, expected {SCREEN_WIDTH}x{SCREEN_HEIGHT}",
            info.width, info.height
        ));
    }
    let channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        other => return Err(format!("unsupported reference colour type {other:?}")),
    };
    Ok(count_differences(
        pixels,
        &buf[..info.buffer_size()],
        channels,
    ))
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
    fn identical_frames_have_zero_differences() {
        let pixels = vec![0x00123456; 4];
        let reference = to_rgb_bytes(&pixels);
        assert_eq!(count_differences(&pixels, &reference, 3), 0);
    }
}
