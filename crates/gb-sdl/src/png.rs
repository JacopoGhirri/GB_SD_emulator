//! Minimal PNG writer for screenshots: 8-bit RGB, no compression.
//!
//! `png` is not an allowed dependency of this crate (CLAUDE.md section 2), and a 160x144
//! picture does not need compression, so the zlib stream uses "stored" deflate blocks.
//! The format needs three checksums/containers, all small:
//!   * each chunk ends with a CRC-32 of its type + data,
//!   * the zlib stream ends with an Adler-32 of the raw data,
//!   * stored blocks hold at most 65535 bytes each.
//!
//! This file is deliberately self-contained (no `crate::` paths): `gb-cli` includes it with
//! `#[path]` in a test and checks the output against the real `png` decoder.

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// CRC-32 (IEEE, reflected, polynomial 0xEDB88320) as PNG chunks use it.
pub fn crc32(parts: &[&[u8]]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for part in parts {
        for &byte in *part {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
    }
    !crc
}

/// Adler-32 as used at the end of a zlib stream.
pub fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % MOD;
        b = (b + a) % MOD;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32(&[kind, data]).to_be_bytes());
}

/// zlib stream made only of stored (uncompressed) deflate blocks.
fn zlib_stored(raw: &[u8]) -> Vec<u8> {
    // 0x78 0x01: deflate, 32 KiB window, no preset dictionary, "fastest" level; and
    // 0x7801 is a multiple of 31 as the header check requires.
    let mut z = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65_535).peekable();
    if blocks.peek().is_none() {
        // Empty input still needs one (final, empty) block.
        z.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
    }
    while let Some(block) = blocks.next() {
        let last = blocks.peek().is_none();
        let len = block.len() as u16;
        z.push(u8::from(last)); // BFINAL bit, BTYPE = 00 (stored)
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(block);
    }
    z.extend_from_slice(&adler32(raw).to_be_bytes());
    z
}

/// Encode `width * height` pixels (`0x00RRGGBB`, the core's framebuffer format) as a PNG.
/// `pixels` must hold exactly `width * height` values.
pub fn encode_rgb(width: u32, height: u32, pixels: &[u32]) -> Vec<u8> {
    assert_eq!(pixels.len(), width as usize * height as usize);
    // Every scanline starts with a filter-type byte; 0 = no filter.
    let mut raw = Vec::with_capacity((width as usize * 3 + 1) * height as usize);
    for row in pixels.chunks(width.max(1) as usize) {
        raw.push(0);
        for &p in row {
            raw.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
        }
    }
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8 bits, RGB, deflate, adaptive filtering, no interlace

    let mut out = SIGNATURE.to_vec();
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_the_standard_check_value() {
        // The CRC-32 "check" value for the ASCII string 123456789.
        assert_eq!(crc32(&[b"123456789"]), 0xCBF4_3926);
        assert_eq!(
            crc32(&[b"1234", b"56789"]),
            0xCBF4_3926,
            "parts are concatenated"
        );
    }

    #[test]
    fn adler32_matches_known_values() {
        assert_eq!(adler32(b""), 1);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn output_has_the_png_structure() {
        let png = encode_rgb(2, 1, &[0x00FF_0000, 0x0000_00FF]);
        assert_eq!(&png[..8], &SIGNATURE);
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    }

    #[test]
    fn large_images_split_into_several_stored_blocks() {
        // 160x144 RGB is 69264 raw bytes: more than one 65535-byte block.
        let z = zlib_stored(&vec![7u8; 69_264]);
        assert_eq!(z[2], 0, "first block is not final");
        assert_eq!(z.len(), 2 + 5 + 65_535 + 5 + (69_264 - 65_535) + 4);
    }
}
