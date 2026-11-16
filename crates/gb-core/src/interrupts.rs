//! Interrupt registers IE (0xFFFF) and IF (0xFF0F) (Pan Docs "Interrupts").
//!
//! Components never touch IF directly; they call [`Interrupts::request`]. The CPU asks which
//! interrupts are both enabled and requested, and clears the bit it is about to service.

pub const REG_IF: u16 = 0xFF0F;
pub const REG_IE: u16 = 0xFFFF;

/// The five interrupt sources occupy bits 0..=4 of IE and IF.
pub const INTERRUPT_MASK: u8 = 0x1F;

/// Interrupt sources. The discriminant is the bit index in IE/IF and also the priority
/// (lower bit = higher priority) and vector index (0x40 + 8 * bit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interrupt {
    VBlank = 0,
    LcdStat = 1,
    Timer = 2,
    Serial = 3,
    Joypad = 4,
}

#[derive(Debug, Clone)]
pub struct Interrupts {
    enable: u8,
    flag: u8,
}

impl Interrupts {
    /// Post-boot state: IE = 0, IF = 0x01 (VBlank pending), per Pan Docs "Power-Up Sequence"
    /// (DMG values: IF reads 0xE1 including the unused bits).
    pub fn new() -> Interrupts {
        Interrupts {
            enable: 0x00,
            flag: 0x01,
        }
    }

    /// IE keeps all 8 bits as written (games use the upper bits as scratch storage).
    pub fn read_enable(&self) -> u8 {
        self.enable
    }

    pub fn write_enable(&mut self, value: u8) {
        self.enable = value;
    }

    /// The upper three bits of IF are unused and read as 1.
    pub fn read_flag(&self) -> u8 {
        self.flag | !INTERRUPT_MASK
    }

    /// IF as stored, without the always-1 upper bits (Gameboy Doctor reference behaviour).
    pub fn flag_bits(&self) -> u8 {
        self.flag
    }

    pub fn write_flag(&mut self, value: u8) {
        self.flag = value & INTERRUPT_MASK;
    }

    pub fn request(&mut self, which: Interrupt) {
        self.flag |= 1 << which as u8;
    }

    /// Clear one request (bit index 0..=4) when the CPU starts servicing it.
    pub fn clear(&mut self, bit: u8) {
        self.flag &= !(1 << bit);
    }

    /// Interrupts that are both enabled in IE and requested in IF.
    pub fn pending(&self) -> u8 {
        self.enable & self.flag & INTERRUPT_MASK
    }
}

impl Default for Interrupts {
    fn default() -> Interrupts {
        Interrupts::new()
    }
}

impl Interrupts {
    /// Save-state support (see `crate::state`).
    pub fn save_state(&self, w: &mut crate::state::StateWriter) {
        w.u8(self.enable);
        w.u8(self.flag);
    }

    pub fn load_state(
        &mut self,
        r: &mut crate::state::StateReader,
    ) -> Result<(), crate::state::StateError> {
        self.enable = r.u8()?;
        self.flag = r.u8()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unused_if_bits_read_as_one() {
        let mut i = Interrupts::new();
        i.write_flag(0xFF);
        assert_eq!(i.read_flag(), 0xFF);
        i.write_flag(0x00);
        assert_eq!(i.read_flag(), 0xE0);
    }

    #[test]
    fn pending_needs_both_enable_and_request() {
        let mut i = Interrupts::new();
        i.write_flag(0);
        i.request(Interrupt::Timer);
        assert_eq!(i.pending(), 0, "not enabled");
        i.write_enable(1 << Interrupt::Timer as u8);
        assert_eq!(i.pending(), 0x04);
        i.clear(Interrupt::Timer as u8);
        assert_eq!(i.pending(), 0);
    }

    #[test]
    fn ie_upper_bits_are_kept() {
        let mut i = Interrupts::new();
        i.write_enable(0xE0);
        assert_eq!(i.read_enable(), 0xE0);
        assert_eq!(i.pending(), 0, "bits 5-7 never request anything");
    }
}
