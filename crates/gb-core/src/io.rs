//! I/O register addresses (0xFF00..=0xFF7F), their DMG power-up values and read masks.
//!
//! Power-up values: Pan Docs "Power-Up Sequence" (DMG column; no boot ROM is emulated).
//! Read masks: many registers have unused or write-only bits that read as 1 (Pan Docs
//! "Hardware Registers"; verified by Mooneye `acceptance/bits/unused_hwio-GS`).

pub const IO_START: u16 = 0xFF00;
pub const IO_LEN: usize = 0x80;

pub const REG_P1: u16 = 0xFF00;
pub const REG_SB: u16 = 0xFF01;
pub const REG_SC: u16 = 0xFF02;
pub const REG_LCDC: u16 = 0xFF40;
pub const REG_STAT: u16 = 0xFF41;
pub const REG_LY: u16 = 0xFF44;
pub const REG_DMA: u16 = 0xFF46;
/// First and last audio register, and wave RAM (phase 6 gives them behaviour).
pub const REG_NR10: u16 = 0xFF10;
pub const REG_NR52: u16 = 0xFF26;
pub const WAVE_RAM_START: u16 = 0xFF30;
pub const WAVE_RAM_END: u16 = 0xFF3F;

/// Value of a register right after the DMG boot ROM hands over (stored form; reads also apply
/// [`read_mask`]). Registers not listed power up as 0x00 (stored) and read via their mask.
pub fn power_up_value(addr: u16) -> u8 {
    match addr {
        REG_P1 => 0xCF,
        REG_SC => 0x7E,
        0xFF04 => 0xAB, // DIV; the timer owns the real value, kept here for documentation
        0xFF07 => 0xF8, // TAC
        0xFF0F => 0xE1, // IF; owned by `Interrupts`
        0xFF10 => 0x80, // NR10
        0xFF11 => 0xBF,
        0xFF12 => 0xF3,
        0xFF13 => 0xFF,
        0xFF14 => 0xBF,
        0xFF16 => 0x3F, // NR21
        0xFF18 => 0xFF,
        0xFF19 => 0xBF,
        0xFF1A => 0x7F, // NR30
        0xFF1B => 0xFF,
        0xFF1C => 0x9F,
        0xFF1D => 0xFF,
        0xFF1E => 0xBF,
        0xFF20 => 0xFF, // NR41
        0xFF23 => 0xBF,
        0xFF24 => 0x77, // NR50
        0xFF25 => 0xF3, // NR51
        0xFF26 => 0xF1, // NR52
        REG_LCDC => 0x91,
        REG_STAT => 0x85,
        REG_DMA => 0xFF,
        0xFF47 => 0xFC, // BGP
        // OBP0/OBP1 are undefined after boot (Pan Docs); 0xFF is a common choice.
        0xFF48 | 0xFF49 => 0xFF,
        _ => 0x00,
    }
}

/// Bits that always read as 1 (unused, or write-only), OR-ed into the stored value on read.
/// Addresses with no register at all return 0xFF.
pub fn read_mask(addr: u16) -> u8 {
    match addr {
        REG_P1 => 0xC0,
        REG_SB => 0x00,
        REG_SC => 0x7E, // DMG: only bits 7 (start) and 0 (clock) exist
        0xFF04..=0xFF06 => 0x00,
        0xFF07 => 0xF8, // TAC
        0xFF0F => 0xE0, // IF
        // Audio (Pan Docs "Audio Registers"): write-only bits read as 1.
        0xFF10 => 0x80,
        0xFF11 | 0xFF16 => 0x3F,
        0xFF12 | 0xFF17 | 0xFF21 | 0xFF22 | 0xFF24 | 0xFF25 => 0x00,
        0xFF13 | 0xFF15 | 0xFF18 | 0xFF1B | 0xFF1D | 0xFF1F | 0xFF20 => 0xFF,
        0xFF14 | 0xFF19 | 0xFF1E | 0xFF23 => 0xBF,
        0xFF1A => 0x7F,
        0xFF1C => 0x9F,
        REG_NR52 => 0x70,
        0xFF27..=0xFF2F => 0xFF,
        WAVE_RAM_START..=WAVE_RAM_END => 0x00,
        REG_LCDC | 0xFF42..=0xFF43 | REG_LY | 0xFF45 | REG_DMA | 0xFF47..=0xFF4B => 0x00,
        REG_STAT => 0x80,
        // Everything else (0xFF03, 0xFF08..=0xFF0E, 0xFF4C..=0xFF7F): unmapped or CGB-only.
        _ => 0xFF,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmapped_registers_read_ff() {
        for addr in [0xFF03, 0xFF08, 0xFF0E, 0xFF4C, 0xFF50, 0xFF7F] {
            assert_eq!(read_mask(addr), 0xFF, "{addr:#06X}");
        }
    }

    #[test]
    fn power_up_values_respect_their_masks() {
        // A stored power-up value must never contradict "bits that always read 1" being set.
        assert_eq!(power_up_value(REG_P1) | read_mask(REG_P1), 0xCF);
        assert_eq!(power_up_value(REG_SC) | read_mask(REG_SC), 0x7E);
        assert_eq!(power_up_value(REG_STAT) | read_mask(REG_STAT), 0x85);
    }
}
