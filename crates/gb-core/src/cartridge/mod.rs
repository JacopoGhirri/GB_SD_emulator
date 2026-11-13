//! Cartridge: ROM image, header parsing (Pan Docs "The Cartridge Header") and mapper dispatch.
//!
//! Phase 1 scope: ROM-only and a minimal MBC1 (ROM banking only, no cartridge RAM) so that the
//! combined Blargg `cpu_instrs.gb` boots. Full MBC1/3/5 with RAM and RTC arrive in phase 2.

mod header;
mod mbc1;

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

/// What a cartridge's memory-bank controller (MBC) must do (CLAUDE.md §4.3).
/// The ROM bytes stay in [`Cartridge`]; the mapper only decides *which* byte an address means.
pub trait Mapper: fmt::Debug {
    /// Read from 0x0000..=0x7FFF.
    fn read_rom(&self, rom: &[u8], addr: u16) -> u8;
    /// A write to 0x0000..=0x7FFF does not change ROM; it programs the mapper's registers.
    fn write_rom(&mut self, addr: u16, value: u8);
    /// Read from 0xA000..=0xBFFF.
    fn read_ram(&self, addr: u16) -> u8;
    fn write_ram(&mut self, addr: u16, value: u8);
    /// Battery-backed data to persist (empty if none).
    fn save_data(&self) -> Vec<u8>;
    fn load_save(&mut self, data: &[u8]);
}

/// No mapper: 32 KiB of ROM at 0x0000..=0x7FFF, no RAM.
#[derive(Debug)]
struct RomOnly;

impl Mapper for RomOnly {
    fn read_rom(&self, rom: &[u8], addr: u16) -> u8 {
        rom.get(addr as usize).copied().unwrap_or(0xFF)
    }
    fn write_rom(&mut self, _addr: u16, _value: u8) {}
    fn read_ram(&self, _addr: u16) -> u8 {
        0xFF
    }
    fn write_ram(&mut self, _addr: u16, _value: u8) {}
    fn save_data(&self) -> Vec<u8> {
        Vec::new()
    }
    fn load_save(&mut self, _data: &[u8]) {}
}

/// A loaded ROM image with its parsed header and mapper.
#[derive(Debug)]
pub struct Cartridge {
    rom: Vec<u8>,
    header: Header,
    mapper: Box<dyn Mapper>,
}

impl Cartridge {
    /// Parse a ROM image. Does not panic on any input: bad data becomes an error.
    /// A wrong header checksum is *not* an error; check [`Header::checksum_ok`].
    pub fn from_bytes(rom: Vec<u8>) -> Result<Cartridge, CartridgeError> {
        let header = Header::parse(&rom)?;
        let mapper: Box<dyn Mapper> = match header.cart_type.0 {
            0x01..=0x03 => Box::new(mbc1::Mbc1::new(rom.len())),
            // Anything else is treated as ROM-only until phase 2 adds more mappers.
            _ => Box::new(RomOnly),
        };
        Ok(Cartridge {
            rom,
            header,
            mapper,
        })
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn rom(&self) -> &[u8] {
        &self.rom
    }

    pub fn read_rom(&self, addr: u16) -> u8 {
        self.mapper.read_rom(&self.rom, addr)
    }
    pub fn write_rom(&mut self, addr: u16, value: u8) {
        self.mapper.write_rom(addr, value);
    }
    pub fn read_ram(&self, addr: u16) -> u8 {
        self.mapper.read_ram(addr)
    }
    pub fn write_ram(&mut self, addr: u16, value: u8) {
        self.mapper.write_ram(addr, value);
    }
    pub fn save_data(&self) -> Vec<u8> {
        self.mapper.save_data()
    }
    pub fn load_save(&mut self, data: &[u8]) {
        self.mapper.load_save(data);
    }
}
