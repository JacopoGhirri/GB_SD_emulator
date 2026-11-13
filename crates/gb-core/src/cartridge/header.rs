//! Header fields at 0x0100..0x0150. Addresses from Pan Docs "The Cartridge Header".

use super::{CartridgeError, HEADER_END};

/// Title region: 0x0134..=0x0143 on DMG. On CGB cartridges the last bytes are repurposed.
const TITLE_START: usize = 0x0134;
const TITLE_END: usize = 0x0143;
/// CGB flag: 0x80 = CGB-enhanced (also runs on DMG), 0xC0 = CGB-only.
const CGB_FLAG: usize = 0x0143;
const CGB_FLAG_ENHANCED: u8 = 0x80;
const CGB_FLAG_ONLY: u8 = 0xC0;
/// Cartridge type (mapper + features).
const CART_TYPE: usize = 0x0147;
/// ROM size: size = 32 KiB << code, for codes 0x00..=0x08.
const ROM_SIZE: usize = 0x0148;
/// RAM size code.
const RAM_SIZE: usize = 0x0149;
/// Header checksum. Boot ROM refuses to run the cartridge if this is wrong.
const HEADER_CHECKSUM: usize = 0x014D;
/// The checksum covers 0x0134..=0x014C.
const CHECKSUM_START: usize = 0x0134;
const CHECKSUM_END: usize = 0x014C;

/// Cartridge type byte with a human-readable name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CartridgeType(pub u8);

impl CartridgeType {
    /// Name from Pan Docs "Cartridge Type". Unlisted codes return `"unknown"`.
    pub fn name(self) -> &'static str {
        match self.0 {
            0x00 => "ROM ONLY",
            0x01 => "MBC1",
            0x02 => "MBC1+RAM",
            0x03 => "MBC1+RAM+BATTERY",
            0x0F => "MBC3+TIMER+BATTERY",
            0x10 => "MBC3+TIMER+RAM+BATTERY",
            0x11 => "MBC3",
            0x12 => "MBC3+RAM",
            0x13 => "MBC3+RAM+BATTERY",
            0x19 => "MBC5",
            0x1A => "MBC5+RAM",
            0x1B => "MBC5+RAM+BATTERY",
            0x1C => "MBC5+RUMBLE",
            0x1D => "MBC5+RUMBLE+RAM",
            0x1E => "MBC5+RUMBLE+RAM+BATTERY",
            _ => "unknown",
        }
    }
}

/// Parsed header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Title as ASCII, trailing zero bytes removed.
    pub title: String,
    /// Raw CGB flag byte at 0x143.
    pub cgb_flag: u8,
    pub cart_type: CartridgeType,
    /// ROM size in bytes, as declared by the header.
    pub rom_size: usize,
    /// External RAM size in bytes, as declared by the header.
    pub ram_size: usize,
    /// Whether the header checksum at 0x14D matches the computed value.
    pub checksum_ok: bool,
    /// The checksum byte stored at 0x14D.
    pub checksum_stored: u8,
    /// The checksum we computed from 0x134..=0x14C.
    pub checksum_computed: u8,
}

impl Header {
    pub fn parse(rom: &[u8]) -> Result<Header, CartridgeError> {
        if rom.len() < HEADER_END {
            return Err(CartridgeError::TooSmall { len: rom.len() });
        }

        let title_bytes = &rom[TITLE_START..=TITLE_END];
        let title_len = title_bytes
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(title_bytes.len());
        // Non-ASCII bytes are shown as '?' rather than failing the whole parse.
        let title: String = title_bytes[..title_len]
            .iter()
            .map(|&b| {
                if b.is_ascii_graphic() || b == b' ' {
                    b as char
                } else {
                    '?'
                }
            })
            .collect();

        let stored = rom[HEADER_CHECKSUM];
        let computed = header_checksum(&rom[CHECKSUM_START..=CHECKSUM_END]);

        Ok(Header {
            title,
            cgb_flag: rom[CGB_FLAG],
            cart_type: CartridgeType(rom[CART_TYPE]),
            rom_size: rom_size_bytes(rom[ROM_SIZE])?,
            ram_size: ram_size_bytes(rom[RAM_SIZE])?,
            checksum_ok: stored == computed,
            checksum_stored: stored,
            checksum_computed: computed,
        })
    }

    /// True if the cartridge sets the CGB-compatible or CGB-only bit.
    pub fn supports_cgb(&self) -> bool {
        self.cgb_flag == CGB_FLAG_ENHANCED || self.cgb_flag == CGB_FLAG_ONLY
    }

    /// True if the cartridge is CGB-only (will not run on a DMG).
    pub fn cgb_only(&self) -> bool {
        self.cgb_flag == CGB_FLAG_ONLY
    }
}

/// Header checksum: x = 0; for each byte b: x = x - b - 1 (wrapping).
/// Pan Docs gives this exact loop; the boot ROM computes it the same way.
pub fn header_checksum(bytes: &[u8]) -> u8 {
    bytes
        .iter()
        .fold(0u8, |x, &b| x.wrapping_sub(b).wrapping_sub(1))
}

/// ROM size code to bytes. Codes 0x52..=0x54 are unofficial sizes (1.1, 1.2, 1.5 MiB).
fn rom_size_bytes(code: u8) -> Result<usize, CartridgeError> {
    const BASE: usize = 32 * 1024;
    match code {
        0x00..=0x08 => Ok(BASE << code),
        0x52 => Ok(72 * 16 * 1024),
        0x53 => Ok(80 * 16 * 1024),
        0x54 => Ok(96 * 16 * 1024),
        other => Err(CartridgeError::UnknownRomSize(other)),
    }
}

/// RAM size code to bytes. Code 0x01 (2 KiB) exists but no retail cart used it.
fn ram_size_bytes(code: u8) -> Result<usize, CartridgeError> {
    const KIB: usize = 1024;
    match code {
        0x00 => Ok(0),
        0x01 => Ok(2 * KIB),
        0x02 => Ok(8 * KIB),
        0x03 => Ok(32 * KIB),
        0x04 => Ok(128 * KIB),
        0x05 => Ok(64 * KIB),
        other => Err(CartridgeError::UnknownRamSize(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid header with a correct checksum.
    fn make_rom(title: &[u8]) -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        rom[TITLE_START..TITLE_START + title.len()].copy_from_slice(title);
        rom[CART_TYPE] = 0x13; // MBC3+RAM+BATTERY
        rom[ROM_SIZE] = 0x00; // 32 KiB
        rom[RAM_SIZE] = 0x03; // 32 KiB
        rom[HEADER_CHECKSUM] = header_checksum(&rom[CHECKSUM_START..=CHECKSUM_END]);
        rom
    }

    #[test]
    fn checksum_matches_pan_docs_loop() {
        // Zero bytes: x = 0 - 1 per byte, so 25 bytes gives -25 = 231 (0xE7).
        assert_eq!(header_checksum(&[0u8; 25]), 0xE7);
        assert_eq!(header_checksum(&[]), 0x00);
    }

    #[test]
    fn parses_title_type_and_sizes() {
        let cart = super::super::Cartridge::from_bytes(make_rom(b"POKEMON RED")).unwrap();
        let h = cart.header();
        assert_eq!(h.title, "POKEMON RED");
        assert_eq!(h.cart_type, CartridgeType(0x13));
        assert_eq!(h.cart_type.name(), "MBC3+RAM+BATTERY");
        assert_eq!(h.rom_size, 32 * 1024);
        assert_eq!(h.ram_size, 32 * 1024);
        assert!(h.checksum_ok);
    }

    #[test]
    fn detects_bad_checksum_without_failing() {
        let mut rom = make_rom(b"TEST");
        rom[HEADER_CHECKSUM] ^= 0xFF;
        let cart = super::super::Cartridge::from_bytes(rom).unwrap();
        assert!(!cart.header().checksum_ok);
        assert_ne!(
            cart.header().checksum_stored,
            cart.header().checksum_computed
        );
    }

    #[test]
    fn title_stops_at_zero_and_masks_non_ascii() {
        // Bytes after the first 0x00 are ignored, as the title field is zero-padded.
        let mut rom = make_rom(b"AB");
        rom[TITLE_START + 2] = 0x00;
        rom[TITLE_START + 3] = b'Z'; // after the terminator, so not part of the title
        let cart = super::super::Cartridge::from_bytes(rom.clone()).unwrap();
        assert_eq!(cart.header().title, "AB");

        // Non-ASCII inside the title (before any zero) becomes '?'.
        rom[TITLE_START + 1] = 0xE9;
        rom[TITLE_START + 2] = b'X';
        rom[TITLE_START + 3] = 0x00; // terminate again so the 'Z' from above is not read
        let cart = super::super::Cartridge::from_bytes(rom).unwrap();
        assert_eq!(cart.header().title, "A?X");
    }

    #[test]
    fn rejects_truncated_rom_without_panicking() {
        // 0x150 bytes is the smallest ROM that contains the whole header.
        for len in [0usize, 1, 0x14F] {
            let err = super::super::Cartridge::from_bytes(vec![0u8; len]).unwrap_err();
            assert_eq!(err, CartridgeError::TooSmall { len });
        }
    }

    #[test]
    fn rejects_unknown_size_codes() {
        let mut rom = make_rom(b"X");
        rom[ROM_SIZE] = 0x09;
        rom[HEADER_CHECKSUM] = header_checksum(&rom[CHECKSUM_START..=CHECKSUM_END]);
        assert_eq!(
            super::super::Cartridge::from_bytes(rom.clone()).unwrap_err(),
            CartridgeError::UnknownRomSize(0x09)
        );

        rom[ROM_SIZE] = 0x00;
        rom[RAM_SIZE] = 0x06;
        rom[HEADER_CHECKSUM] = header_checksum(&rom[CHECKSUM_START..=CHECKSUM_END]);
        assert_eq!(
            super::super::Cartridge::from_bytes(rom).unwrap_err(),
            CartridgeError::UnknownRamSize(0x06)
        );
    }

    #[test]
    fn rom_size_codes_scale_by_powers_of_two() {
        assert_eq!(rom_size_bytes(0x05).unwrap(), 1024 * 1024);
        assert_eq!(rom_size_bytes(0x08).unwrap(), 8 * 1024 * 1024);
    }

    #[test]
    fn cgb_flag_bits() {
        let mut rom = make_rom(b"X");
        rom[CGB_FLAG] = 0x80;
        let h = Header::parse(&rom).unwrap();
        assert!(h.supports_cgb());
        assert!(!h.cgb_only());

        rom[CGB_FLAG] = 0xC0;
        let h = Header::parse(&rom).unwrap();
        assert!(h.cgb_only());
    }
}
