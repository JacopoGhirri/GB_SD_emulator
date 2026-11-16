//! MBC5 (Pan Docs "MBC5").
//!
//! * 0000-1FFF: RAM enable. 0x0A enables, 0x00 disables (other values: unspecified; we disable).
//! * 2000-2FFF: low 8 bits of the ROM bank; 3000-3FFF: bit 8.
//! * 4000-5FFF: RAM bank, 4 bits (rumble cartridges use bit 3 for the motor, which we ignore).
//!   Unlike MBC1, bank 0 can be selected for 4000-7FFF, and 0000-3FFF is always bank 0.

use super::{ram_offset, Mapper, Ram, ROM_BANK_SIZE};
use crate::state::{StateError, StateReader, StateWriter};

#[derive(Debug)]
pub struct Mbc5 {
    rom_banks: usize,
    ram: Ram,
    battery: bool,
    ram_enabled: bool,
    rom_bank: u16,
    ram_bank: u8,
}

impl Mbc5 {
    pub fn new(rom_banks: usize, ram: Ram, battery: bool) -> Mbc5 {
        Mbc5 {
            rom_banks,
            ram,
            battery,
            ram_enabled: false,
            rom_bank: 1,
            ram_bank: 0,
        }
    }
}

impl Mapper for Mbc5 {
    fn read_rom(&self, rom: &[u8], addr: u16) -> u8 {
        let bank = if addr < 0x4000 {
            0
        } else {
            self.rom_bank as usize % self.rom_banks
        };
        rom.get(bank * ROM_BANK_SIZE + (addr as usize & 0x3FFF))
            .copied()
            .unwrap_or(0xFF)
    }

    fn write_rom(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram_enabled = value == 0x0A,
            0x2000..=0x2FFF => self.rom_bank = (self.rom_bank & 0x100) | u16::from(value),
            0x3000..=0x3FFF => {
                self.rom_bank = (self.rom_bank & 0x0FF) | (u16::from(value & 1) << 8)
            }
            0x4000..=0x5FFF => self.ram_bank = value & 0x0F,
            _ => {}
        }
    }

    fn read_ram(&self, addr: u16) -> u8 {
        if self.ram_enabled {
            self.ram.read(ram_offset(self.ram_bank as usize, addr))
        } else {
            0xFF
        }
    }

    fn write_ram(&mut self, addr: u16, value: u8) {
        if self.ram_enabled {
            self.ram
                .write(ram_offset(self.ram_bank as usize, addr), value);
        }
    }

    fn save_state(&self, w: &mut StateWriter) {
        self.ram.save_state(w);
        w.bool(self.ram_enabled);
        w.u16(self.rom_bank);
        w.u8(self.ram_bank);
    }

    fn load_state(&mut self, r: &mut StateReader) -> Result<(), StateError> {
        self.ram.load_state(r)?;
        self.ram_enabled = r.bool()?;
        self.rom_bank = r.u16()? & 0x1FF;
        self.ram_bank = r.u8()?;
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
            rom[b * ROM_BANK_SIZE + 1] = (b >> 8) as u8;
        }
        rom
    }

    #[test]
    fn nine_bit_rom_bank_and_bank_zero_is_selectable() {
        let rom = rom_with_bank_ids(512);
        let mut m = Mbc5::new(512, Ram::new(0), false);
        m.write_rom(0x2000, 0x34);
        m.write_rom(0x3000, 1);
        assert_eq!(m.read_rom(&rom, 0x4000), 0x34);
        assert_eq!(m.read_rom(&rom, 0x4001), 0x01);
        m.write_rom(0x2000, 0);
        m.write_rom(0x3000, 0);
        assert_eq!(
            m.read_rom(&rom, 0x4000),
            0,
            "bank 0 shows in the switchable area"
        );
    }

    #[test]
    fn banks_wrap_to_rom_size() {
        let rom = rom_with_bank_ids(4);
        let mut m = Mbc5::new(4, Ram::new(0), false);
        m.write_rom(0x2000, 7);
        assert_eq!(m.read_rom(&rom, 0x4000), 3);
    }

    #[test]
    fn ram_enable_and_banking() {
        let mut m = Mbc5::new(4, Ram::new(32 * 1024), true);
        m.write_ram(0xA000, 1);
        assert_eq!(m.read_ram(0xA000), 0xFF);
        m.write_rom(0x0000, 0x0A);
        m.write_rom(0x4000, 2);
        m.write_ram(0xA000, 0x77);
        m.write_rom(0x4000, 0);
        assert_eq!(m.read_ram(0xA000), 0x00);
        m.write_rom(0x4000, 2);
        assert_eq!(m.read_ram(0xA000), 0x77);
        assert_eq!(m.save_data().len(), 32 * 1024);
    }
}
