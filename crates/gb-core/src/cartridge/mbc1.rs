//! MBC1 (Pan Docs "MBC1").
//!
//! Registers: RAMG (0000-1FFF, enable when low nibble is 0xA), BANK1 (2000-3FFF, 5 bits, a
//! value of 0 reads as 1), BANK2 (4000-5FFF, 2 bits) and MODE (6000-7FFF, 1 bit).
//! * 4000-7FFF shows bank `BANK2 << 5 | BANK1`.
//! * In mode 1 BANK2 also selects the bank at 0000-3FFF (`BANK2 << 5`) and the RAM bank;
//!   in mode 0 those are fixed to bank 0.
//! * "Multicart" boards (several games on one ROM) wire only 4 bits of BANK1, so BANK2 is
//!   shifted by 4 instead of 5.
//!   Bank numbers wrap to the real ROM size.

use super::{ram_offset, Mapper, Ram, ROM_BANK_SIZE};
use crate::state::{StateError, StateReader, StateWriter};

#[derive(Debug)]
pub struct Mbc1 {
    rom_banks: usize,
    ram: Ram,
    battery: bool,
    ram_enabled: bool,
    bank1: u8,
    bank2: u8,
    mode1: bool,
    multicart: bool,
}

impl Mbc1 {
    pub fn new(rom_banks: usize, ram: Ram, battery: bool, multicart: bool) -> Mbc1 {
        Mbc1 {
            rom_banks,
            ram,
            battery,
            ram_enabled: false,
            bank1: 1,
            bank2: 0,
            mode1: false,
            multicart,
        }
    }

    fn bank2_shift(&self) -> u32 {
        if self.multicart {
            4
        } else {
            5
        }
    }

    fn bank1_mask(&self) -> u8 {
        if self.multicart {
            0x0F
        } else {
            0x1F
        }
    }
}

/// A multicart ROM is exactly 1 MiB and repeats the Nintendo logo at the start of banks
/// 0x10, 0x20 and 0x30 (each game has its own header). Heuristic used by other emulators.
pub fn is_multicart(rom: &[u8]) -> bool {
    const LOGO: std::ops::Range<usize> = 0x104..0x134;
    if rom.len() != 64 * ROM_BANK_SIZE {
        return false;
    }
    [0x10, 0x20, 0x30].iter().all(|&bank| {
        let start = bank * ROM_BANK_SIZE;
        rom[start + LOGO.start..start + LOGO.end] == rom[LOGO]
    })
}

impl Mapper for Mbc1 {
    fn read_rom(&self, rom: &[u8], addr: u16) -> u8 {
        let bank = if addr < 0x4000 {
            if self.mode1 {
                (self.bank2 as usize) << self.bank2_shift()
            } else {
                0
            }
        } else {
            ((self.bank2 as usize) << self.bank2_shift())
                | (self.bank1 & self.bank1_mask()) as usize
        };
        let offset = (bank % self.rom_banks) * ROM_BANK_SIZE + (addr as usize & 0x3FFF);
        rom.get(offset).copied().unwrap_or(0xFF)
    }

    fn write_rom(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram_enabled = value & 0x0F == 0x0A,
            0x2000..=0x3FFF => {
                // The zero check happens on all 5 bits, even on multicart boards.
                let v = value & 0x1F;
                self.bank1 = if v == 0 { 1 } else { v };
            }
            0x4000..=0x5FFF => self.bank2 = value & 0x03,
            _ => self.mode1 = value & 1 != 0,
        }
    }

    fn read_ram(&self, addr: u16) -> u8 {
        if !self.ram_enabled {
            return 0xFF;
        }
        let bank = if self.mode1 { self.bank2 as usize } else { 0 };
        self.ram.read(ram_offset(bank, addr))
    }

    fn write_ram(&mut self, addr: u16, value: u8) {
        if self.ram_enabled {
            let bank = if self.mode1 { self.bank2 as usize } else { 0 };
            self.ram.write(ram_offset(bank, addr), value);
        }
    }

    fn save_state(&self, w: &mut StateWriter) {
        self.ram.save_state(w);
        w.bool(self.ram_enabled);
        w.u8(self.bank1);
        w.u8(self.bank2);
        w.bool(self.mode1);
    }

    fn load_state(&mut self, r: &mut StateReader) -> Result<(), StateError> {
        self.ram.load_state(r)?;
        self.ram_enabled = r.bool()?;
        // Same widths the register writes keep (BANK1 5 bits, BANK2 2 bits).
        self.bank1 = r.u8()? & 0x1F;
        self.bank2 = r.u8()? & 0x03;
        self.mode1 = r.bool()?;
        Ok(())
    }

    fn save_data(&self) -> Vec<u8> {
        if self.battery {
            self.ram.bytes().to_vec()
        } else {
            Vec::new()
        }
    }

    fn load_save(&mut self, data: &[u8]) {
        if self.battery {
            self.ram.load(data);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rom_with_bank_ids(banks: usize) -> Vec<u8> {
        let mut rom = vec![0u8; banks * ROM_BANK_SIZE];
        for b in 0..banks {
            rom[b * ROM_BANK_SIZE] = b as u8;
        }
        rom
    }

    fn mbc(banks: usize, ram: usize) -> Mbc1 {
        Mbc1::new(banks, Ram::new(ram), true, false)
    }

    #[test]
    fn bank_zero_write_selects_bank_one() {
        let rom = rom_with_bank_ids(4);
        let mut m = mbc(4, 0);
        m.write_rom(0x2000, 0);
        assert_eq!(m.read_rom(&rom, 0x4000), 1);
    }

    #[test]
    fn selects_and_wraps_banks() {
        let rom = rom_with_bank_ids(4);
        let mut m = mbc(4, 0);
        m.write_rom(0x2000, 3);
        assert_eq!(m.read_rom(&rom, 0x4000), 3);
        m.write_rom(0x2000, 6); // 6 % 4 = 2
        assert_eq!(m.read_rom(&rom, 0x4000), 2);
        assert_eq!(m.read_rom(&rom, 0x0000), 0);
    }

    #[test]
    fn bank2_extends_bank_number_and_mode_moves_bank_zero_area() {
        let rom = rom_with_bank_ids(64);
        let mut m = mbc(64, 0);
        m.write_rom(0x2000, 2);
        m.write_rom(0x4000, 1); // bank 0x22
        assert_eq!(m.read_rom(&rom, 0x4000), 0x22);
        assert_eq!(m.read_rom(&rom, 0x0000), 0, "mode 0: bank 0 area is fixed");
        m.write_rom(0x6000, 1);
        assert_eq!(m.read_rom(&rom, 0x0000), 0x20, "mode 1: BANK2 << 5");
    }

    #[test]
    fn quirk_bank_0x20_reads_as_0x21() {
        let rom = rom_with_bank_ids(64);
        let mut m = mbc(64, 0);
        m.write_rom(0x2000, 0); // BANK1 = 1
        m.write_rom(0x4000, 1);
        assert_eq!(m.read_rom(&rom, 0x4000), 0x21);
    }

    #[test]
    fn ram_is_gated_by_ramg_and_banked_only_in_mode_1() {
        let mut m = mbc(4, 32 * 1024);
        m.write_ram(0xA000, 0x11);
        assert_eq!(m.read_ram(0xA000), 0xFF, "disabled RAM reads 0xFF");
        m.write_rom(0x0000, 0x0A);
        m.write_ram(0xA000, 0x11);
        assert_eq!(m.read_ram(0xA000), 0x11);
        m.write_rom(0x4000, 2);
        assert_eq!(m.read_ram(0xA000), 0x11, "mode 0 ignores BANK2 for RAM");
        m.write_rom(0x6000, 1);
        assert_eq!(m.read_ram(0xA000), 0x00, "mode 1: bank 2 is another bank");
        m.write_rom(0x0000, 0x00);
        assert_eq!(m.read_ram(0xA000), 0xFF);
    }

    #[test]
    fn small_ram_is_mirrored() {
        let mut m = mbc(4, 2048);
        m.write_rom(0x0000, 0x0A);
        m.write_ram(0xA000, 0x5A);
        assert_eq!(m.read_ram(0xA800), 0x5A);
    }

    #[test]
    fn multicart_uses_four_bits_of_bank1() {
        let rom = rom_with_bank_ids(64);
        let mut m = Mbc1::new(64, Ram::new(0), false, true);
        m.write_rom(0x2000, 0x1F);
        m.write_rom(0x4000, 1);
        assert_eq!(m.read_rom(&rom, 0x4000), 0x1F, "BANK2 << 4 | (BANK1 & 0xF)");
    }

    #[test]
    fn save_round_trip_and_short_save_is_tolerated() {
        let mut m = mbc(4, 8192);
        m.load_save(&[1, 2, 3]);
        let saved = m.save_data();
        assert_eq!(saved.len(), 8192);
        assert_eq!(&saved[..4], &[1, 2, 3, 0]);
    }
}
