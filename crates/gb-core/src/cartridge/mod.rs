//! Cartridge: ROM image, header parsing (Pan Docs "The Cartridge Header") and mapper dispatch.
//!
//! Supported cartridge types: ROM only, MBC1, MBC3 (with RTC), MBC5. Anything else is rejected
//! with [`CartridgeError::UnsupportedType`] rather than being run with wrong banking.

mod header;
mod mbc1;
mod mbc3;
mod mbc5;

pub use header::{CartridgeType, Header};

use std::fmt;

/// Size of the header region. Everything the header needs lives below 0x0150.
pub const HEADER_END: usize = 0x0150;

pub(crate) const ROM_BANK_SIZE: usize = 0x4000;
pub(crate) const RAM_BANK_SIZE: usize = 0x2000;
const EXT_RAM_START: u16 = 0xA000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CartridgeError {
    /// The file is shorter than the header, so it cannot be a Game Boy ROM.
    TooSmall { len: usize },
    /// The ROM-size byte at 0x148 names a size we do not know.
    UnknownRomSize(u8),
    /// The RAM-size byte at 0x149 names a size we do not know.
    UnknownRamSize(u8),
    /// The cartridge-type byte at 0x147 names a mapper this emulator does not implement.
    UnsupportedType(u8),
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
            CartridgeError::UnsupportedType(code) => write!(
                f,
                "unsupported cartridge type 0x{code:02X} ({}) at 0x0147; supported: ROM only, MBC1, MBC3, MBC5",
                CartridgeType(*code).name()
            ),
        }
    }
}

impl std::error::Error for CartridgeError {}

/// What a cartridge's memory-bank controller (MBC) must do (CLAUDE.md §4.3).
/// The ROM bytes stay in [`Cartridge`]; the mapper decides *which* byte an address means and
/// owns the cartridge RAM and any clock.
pub trait Mapper: fmt::Debug {
    /// Read from 0x0000..=0x7FFF.
    fn read_rom(&self, rom: &[u8], addr: u16) -> u8;
    /// A write to 0x0000..=0x7FFF does not change ROM; it programs the mapper's registers.
    fn write_rom(&mut self, addr: u16, value: u8);
    /// Read from 0xA000..=0xBFFF. Returns 0xFF when RAM is absent or disabled.
    fn read_ram(&self, addr: u16) -> u8;
    fn write_ram(&mut self, addr: u16, value: u8);
    /// Advance one M-cycle. Only the MBC3 real-time clock needs time.
    fn tick(&mut self) {}
    /// Battery-backed data to persist (empty if none).
    fn save_data(&self) -> Vec<u8>;
    fn load_save(&mut self, data: &[u8]);
    /// Wall-clock support for frontends (MBC3+TIMER only; no-ops elsewhere). The core never
    /// reads the system clock itself, so emulation stays deterministic.
    fn set_save_timestamp(&mut self, _unix_secs: u64) {}
    /// Unix time stored in the loaded save, `None` if the cartridge has no clock.
    fn save_timestamp(&self) -> Option<u64> {
        None
    }
    fn advance_rtc_seconds(&mut self, _secs: u64) {}
}

/// Cartridge RAM. An absent RAM reads 0xFF; offsets wrap inside the actual size, which is how
/// 2 KiB RAMs (mirrored in the 8 KiB window) and out-of-range banks behave.
#[derive(Debug, Clone)]
pub(crate) struct Ram {
    data: Vec<u8>,
}

impl Ram {
    pub(crate) fn new(size: usize) -> Ram {
        Ram {
            data: vec![0; size],
        }
    }

    pub(crate) fn read(&self, offset: usize) -> u8 {
        if self.data.is_empty() {
            0xFF
        } else {
            self.data[offset % self.data.len()]
        }
    }

    pub(crate) fn write(&mut self, offset: usize, value: u8) {
        if !self.data.is_empty() {
            let len = self.data.len();
            self.data[offset % len] = value;
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.data.len()
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.data
    }

    /// Copy as much of `saved` as fits; a short or long save file never panics.
    pub(crate) fn load(&mut self, saved: &[u8]) {
        let n = saved.len().min(self.data.len());
        self.data[..n].copy_from_slice(&saved[..n]);
    }
}

/// Offset into RAM for an address in 0xA000..=0xBFFF with a RAM bank selected.
pub(crate) fn ram_offset(bank: usize, addr: u16) -> usize {
    bank * RAM_BANK_SIZE + (addr - EXT_RAM_START) as usize
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

/// Features implied by the cartridge type byte (Pan Docs "Cartridge Type").
fn has_battery(code: u8) -> bool {
    matches!(code, 0x03 | 0x0F | 0x10 | 0x13 | 0x1B | 0x1E)
}

fn has_rtc(code: u8) -> bool {
    matches!(code, 0x0F | 0x10)
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
        let code = header.cart_type.0;
        let rom_banks = rom.len().div_ceil(ROM_BANK_SIZE).max(1);
        let ram = Ram::new(header.ram_size);
        let battery = has_battery(code);
        let mapper: Box<dyn Mapper> = match code {
            0x00 => Box::new(RomOnly),
            0x01..=0x03 => Box::new(mbc1::Mbc1::new(
                rom_banks,
                ram,
                battery,
                mbc1::is_multicart(&rom),
            )),
            0x0F..=0x13 => Box::new(mbc3::Mbc3::new(rom_banks, ram, battery, has_rtc(code))),
            0x19..=0x1E => Box::new(mbc5::Mbc5::new(rom_banks, ram, battery)),
            other => return Err(CartridgeError::UnsupportedType(other)),
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
    /// Advance one M-cycle (drives the MBC3 clock).
    pub fn tick(&mut self) {
        self.mapper.tick();
    }
    /// Battery-backed data: cartridge RAM, plus the RTC footer on MBC3+TIMER cartridges.
    pub fn save_data(&self) -> Vec<u8> {
        self.mapper.save_data()
    }
    pub fn load_save(&mut self, data: &[u8]) {
        self.mapper.load_save(data);
    }
    /// True for cartridges with a battery-backed real-time clock (MBC3+TIMER).
    pub fn has_rtc(&self) -> bool {
        self.mapper.save_timestamp().is_some()
    }
    /// Stamp the next `save_data()` with this Unix time (RTC cartridges only).
    pub fn set_save_timestamp(&mut self, unix_secs: u64) {
        self.mapper.set_save_timestamp(unix_secs);
    }
    /// Unix time stored in the loaded save (0 = unknown), `None` without an RTC.
    pub fn save_timestamp(&self) -> Option<u64> {
        self.mapper.save_timestamp()
    }
    /// Advance the RTC by real elapsed seconds. Frontend use only.
    pub fn advance_rtc_seconds(&mut self, secs: u64) {
        self.mapper.advance_rtc_seconds(secs);
    }
}
