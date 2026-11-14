//! MINIMAL timer (Pan Docs "Timer and Divider Registers").
//!
//! **Phase-1 stand-in, to be REPLACED in phase 2.** Blargg's `02-interrupts` needs a working
//! timer interrupt, so this has just enough behaviour: DIV, TIMA, TMA, TAC and the overflow
//! interrupt. It counts cycles with a simple accumulator. It is NOT accurate to the hardware
//! quirks (falling-edge detection on a bit of the internal counter, the delayed TMA reload,
//! TAC-write glitches). Phase 2 replaces it with the falling-edge implementation.

pub const REG_DIV: u16 = 0xFF04;
pub const REG_TIMA: u16 = 0xFF05;
pub const REG_TMA: u16 = 0xFF06;
pub const REG_TAC: u16 = 0xFF07;

const TAC_ENABLE: u8 = 0x04;

#[derive(Debug, Clone, Default)]
pub struct Timer {
    /// DIV is the upper byte of a free-running 16-bit counter that ticks every T-cycle.
    counter: u16,
    /// T-cycles accumulated towards the next TIMA increment.
    accumulator: u32,
    tima: u8,
    tma: u8,
    tac: u8,
}

impl Timer {
    pub fn new() -> Timer {
        Timer::default()
    }

    /// T-cycles per TIMA increment for the TAC clock-select bits.
    fn period(&self) -> u32 {
        match self.tac & 3 {
            0 => 1024, // 4096 Hz
            1 => 16,   // 262144 Hz
            2 => 64,   // 65536 Hz
            _ => 256,  // 16384 Hz
        }
    }

    /// Advance by `t_cycles`. Returns true if TIMA overflowed (the caller raises the interrupt).
    pub fn tick(&mut self, t_cycles: u32) -> bool {
        self.counter = self.counter.wrapping_add(t_cycles as u16);
        if self.tac & TAC_ENABLE == 0 {
            return false;
        }
        let mut overflowed = false;
        self.accumulator += t_cycles;
        let period = self.period();
        while self.accumulator >= period {
            self.accumulator -= period;
            let (next, wrapped) = self.tima.overflowing_add(1);
            if wrapped {
                self.tima = self.tma;
                overflowed = true;
            } else {
                self.tima = next;
            }
        }
        overflowed
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            REG_DIV => (self.counter >> 8) as u8,
            REG_TIMA => self.tima,
            REG_TMA => self.tma,
            // The unused upper bits of TAC read as 1.
            _ => self.tac | 0xF8,
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            // Any write to DIV resets the whole internal counter.
            REG_DIV => self.counter = 0,
            REG_TIMA => self.tima = value,
            REG_TMA => self.tma = value,
            _ => self.tac = value & 0x07,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn div_is_upper_byte_and_write_resets() {
        let mut t = Timer::new();
        t.tick(255);
        assert_eq!(t.read(REG_DIV), 0);
        t.tick(1);
        assert_eq!(t.read(REG_DIV), 1);
        t.write(REG_DIV, 0x77);
        assert_eq!(t.read(REG_DIV), 0);
    }

    #[test]
    fn tima_rates_follow_tac() {
        for (tac, period) in [(0b100, 1024u32), (0b101, 16), (0b110, 64), (0b111, 256)] {
            let mut t = Timer::new();
            t.write(REG_TAC, tac);
            t.tick(period - 4);
            assert_eq!(t.read(REG_TIMA), 0, "tac {tac:#b}");
            t.tick(4);
            assert_eq!(t.read(REG_TIMA), 1, "tac {tac:#b}");
        }
    }

    #[test]
    fn disabled_timer_does_not_count() {
        let mut t = Timer::new();
        t.write(REG_TAC, 0b001);
        t.tick(1000);
        assert_eq!(t.read(REG_TIMA), 0);
    }

    #[test]
    fn overflow_reloads_tma_and_reports_interrupt() {
        let mut t = Timer::new();
        t.write(REG_TAC, 0b101);
        t.write(REG_TMA, 0xAB);
        t.write(REG_TIMA, 0xFF);
        assert!(!t.tick(12));
        assert!(t.tick(4));
        assert_eq!(t.read(REG_TIMA), 0xAB);
    }
}
