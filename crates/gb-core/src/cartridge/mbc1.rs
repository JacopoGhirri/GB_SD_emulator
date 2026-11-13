//! Minimal MBC1: ROM banking only (Pan Docs "MBC1"). No cartridge RAM, no advanced banking mode.
//! Phase 2 replaces this with the complete mapper.

use super::Mapper;

const ROM_BANK_SIZE: usize = 0x4000;

#[derive(Debug)]
pub struct Mbc1 {
    /// 5-bit register written at 0x2000..=0x3FFF. A value of 0 selects bank 1,
    /// because bank 0 is already visible at 0x0000..=0x3FFF.
    bank_low: u8,
    bank_count: usize,
}

impl Mbc1 {
    pub fn new(rom_len: usize) -> Mbc1 {
        Mbc1 {
            bank_low: 1,
            bank_count: (rom_len / ROM_BANK_SIZE).max(1),
        }
    }
}

impl Mapper for Mbc1 {
    fn read_rom(&self, rom: &[u8], addr: u16) -> u8 {
        let offset = if addr < 0x4000 {
            addr as usize
        } else {
            // Banks wrap around when the register exceeds the ROM size.
            let bank = self.bank_low as usize % self.bank_count;
            bank * ROM_BANK_SIZE + (addr as usize - 0x4000)
        };
        rom.get(offset).copied().unwrap_or(0xFF)
    }

    fn write_rom(&mut self, addr: u16, value: u8) {
        if (0x2000..=0x3FFF).contains(&addr) {
            let v = value & 0x1F;
            self.bank_low = if v == 0 { 1 } else { v };
        }
        // RAM enable (0x0000..), upper bank bits and mode select (0x4000..) are phase 2.
    }

    fn read_ram(&self, _addr: u16) -> u8 {
        0xFF
    }
    fn write_ram(&mut self, _addr: u16, _value: u8) {}
    fn save_data(&self) -> Vec<u8> {
        Vec::new()
    }
    fn load_save(&mut self, _data: &[u8]) {}
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

    #[test]
    fn bank_zero_write_selects_bank_one() {
        let rom = rom_with_bank_ids(4);
        let mut m = Mbc1::new(rom.len());
        m.write_rom(0x2000, 0);
        assert_eq!(m.read_rom(&rom, 0x4000), 1);
    }

    #[test]
    fn selects_and_wraps_banks() {
        let rom = rom_with_bank_ids(4);
        let mut m = Mbc1::new(rom.len());
        m.write_rom(0x2000, 3);
        assert_eq!(m.read_rom(&rom, 0x4000), 3);
        m.write_rom(0x2000, 6); // 6 % 4 = 2
        assert_eq!(m.read_rom(&rom, 0x4000), 2);
        assert_eq!(m.read_rom(&rom, 0x0000), 0);
    }
}
