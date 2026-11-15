//! Audio processing unit (Pan Docs "Audio Overview", "Audio Registers").
//!
//! Step 1 (this file for now): the register file, the master power switch (NR52 bit 7) and wave
//! RAM. Channels, the frame sequencer and mixing are added in later steps.

use crate::io::{read_mask, WAVE_RAM_END, WAVE_RAM_START};

pub const REG_NR10: u16 = 0xFF10;
pub const REG_NR11: u16 = 0xFF11;
pub const REG_NR21: u16 = 0xFF16;
pub const REG_NR30: u16 = 0xFF1A;
pub const REG_NR31: u16 = 0xFF1B;
pub const REG_NR41: u16 = 0xFF20;
pub const REG_NR50: u16 = 0xFF24;
pub const REG_NR51: u16 = 0xFF25;
pub const REG_NR52: u16 = 0xFF26;

/// Number of registers from NR10 (0xFF10) up to and including NR51 (0xFF25).
const REG_COUNT: usize = 0x16;

/// Typical DMG wave RAM contents after the boot ROM. Real hardware powers up with
/// effectively random data; this is what several accurate emulators use.
const POST_BOOT_WAVE_RAM: [u8; 16] = [
    0x84, 0x40, 0x43, 0xAA, 0x2D, 0x78, 0x92, 0x3C, 0x60, 0x59, 0x59, 0xB0, 0x34, 0xB8, 0x2E, 0xDA,
];

#[derive(Debug, Clone)]
pub struct Apu {
    /// Raw written values of NR10..=NR51. Reads OR in the unused/write-only bits ([`read_mask`]).
    regs: [u8; REG_COUNT],
    wave_ram: [u8; 16],
    /// NR52 bit 7. While false the registers are zeroed and read-only.
    powered: bool,
    /// Channel-enabled flags, shown in NR52 bits 0-3.
    channel_on: [bool; 4],
}

impl Default for Apu {
    fn default() -> Apu {
        Apu::new()
    }
}

impl Apu {
    /// State right after the DMG boot ROM (Pan Docs "Power-Up Sequence").
    pub fn new() -> Apu {
        let mut regs = [0u8; REG_COUNT];
        for (i, r) in regs.iter_mut().enumerate() {
            *r = crate::io::power_up_value(REG_NR10 + i as u16);
        }
        Apu {
            regs,
            wave_ram: POST_BOOT_WAVE_RAM,
            powered: true,
            // The boot ROM's start-up beep leaves channel 1 flagged on (NR52 = 0xF1).
            channel_on: [true, false, false, false],
        }
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            REG_NR52 => {
                let status = self
                    .channel_on
                    .iter()
                    .enumerate()
                    .fold(0, |acc, (i, &on)| acc | (u8::from(on) << i));
                0x70 | (u8::from(self.powered) << 7) | status
            }
            REG_NR10..=REG_NR51 => self.regs[(addr - REG_NR10) as usize] | read_mask(addr),
            WAVE_RAM_START..=WAVE_RAM_END => self.wave_ram[(addr - WAVE_RAM_START) as usize],
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            REG_NR52 => self.write_power(value & 0x80 != 0),
            REG_NR10..=REG_NR51 if self.powered => self.regs[(addr - REG_NR10) as usize] = value,
            WAVE_RAM_START..=WAVE_RAM_END => {
                self.wave_ram[(addr - WAVE_RAM_START) as usize] = value
            }
            _ => {}
        }
    }

    fn write_power(&mut self, on: bool) {
        if on == self.powered {
            return;
        }
        self.powered = on;
        if !on {
            // Powering off clears every register (NR10..=NR51) and silences all channels.
            // Wave RAM is untouched.
            self.regs = [0; REG_COUNT];
            self.channel_on = [false; 4];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_boot_registers_read_back_documented_values() {
        let apu = Apu::new();
        assert_eq!(apu.read(REG_NR52), 0xF1);
        assert_eq!(apu.read(REG_NR50), 0x77);
        assert_eq!(apu.read(REG_NR51), 0xF3);
        assert_eq!(apu.read(REG_NR11), 0xBF);
    }

    #[test]
    fn write_only_bits_read_as_one() {
        let mut apu = Apu::new();
        apu.write(0xFF13, 0x00); // NR13 is write-only
        assert_eq!(apu.read(0xFF13), 0xFF);
        apu.write(0xFF1A, 0x00);
        assert_eq!(apu.read(0xFF1A), 0x7F);
        apu.write(REG_NR50, 0x12);
        assert_eq!(apu.read(REG_NR50), 0x12);
    }

    #[test]
    fn power_off_clears_registers_and_blocks_writes_but_not_wave_ram() {
        let mut apu = Apu::new();
        apu.write(REG_NR50, 0x77);
        apu.write(REG_NR52, 0x00);
        assert_eq!(apu.read(REG_NR52), 0x70);
        assert_eq!(apu.read(REG_NR50), 0x00);
        apu.write(REG_NR50, 0x55);
        assert_eq!(
            apu.read(REG_NR50),
            0x00,
            "registers are read-only while off"
        );
        apu.write(0xFF30, 0xAB);
        assert_eq!(apu.read(0xFF30), 0xAB, "wave RAM stays writable");
        apu.write(REG_NR52, 0x80);
        assert_eq!(apu.read(REG_NR52), 0xF0);
        assert_eq!(apu.read(0xFF30), 0xAB, "wave RAM survives power cycling");
    }
}
