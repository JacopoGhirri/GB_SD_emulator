//! Cartridge: ROM image plus header parsing (Pan Docs "The Cartridge Header").
//!
//! Phase 0 scope: parse and validate the header only. Mappers (MBC1/3/5) arrive in phase 2.

mod header;

pub use header::{CartridgeType, Header};

use std::fmt;

/// Size of the header region. Everything the header needs lives below 0x0150.
pub const HEADER_END: usize = 0x0150;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CartridgeError {
    /// The file is shorter than the header, so it cannot be a Game Boy ROM.
    TooSmall { len: usize },
    /// The ROM-size byte at 0x148 names a size we do not know.
    UnknownRomSize(u8),
    /// The RAM-size byte at 0x149 names a size we do not know.
    UnknownRamSize(u8),
}

impl fmt::Display for CartridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CartridgeError::TooSmall { len } => write!(
                f,
                "ROM is {len} bytes, too small to contain a header (need at least {HEADER_END})"
            ),
            CartridgeError::UnknownRomSize(code) => {
                write!(f, "unknown ROM size code 0x{code:02X} at 0x0148")
            }
            CartridgeError::UnknownRamSize(code) => {
                write!(f, "unknown RAM size code 0x{code:02X} at 0x0149")
            }
        }
    }
}

impl std::error::Error for CartridgeError {}

/// A loaded ROM image with its parsed header.
#[derive(Debug, Clone)]
pub struct Cartridge {
    rom: Vec<u8>,
    header: Header,
}

impl Cartridge {
    /// Parse a ROM image. Does not panic on any input: bad data becomes an error.
    /// A wrong header checksum is *not* an error; check [`Header::checksum_ok`].
    pub fn from_bytes(rom: Vec<u8>) -> Result<Cartridge, CartridgeError> {
        let header = Header::parse(&rom)?;
        Ok(Cartridge { rom, header })
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn rom(&self) -> &[u8] {
        &self.rom
    }
}
