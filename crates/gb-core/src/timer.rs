//! Timer and divider (Pan Docs "Timer and Divider Registers").
//!
//! Hardware model:
//! * A 16-bit system counter increments every T-cycle. `DIV` (0xFF04) is its **upper byte**.
//!   Any write to DIV resets the whole counter to 0.
//! * `TAC` (0xFF07) bit 2 enables the timer; bits 0-1 select which counter bit clocks `TIMA`:
//!   bit 9 (4096 Hz), 3 (262144 Hz), 5 (65536 Hz), 7 (16384 Hz).
//! * `TIMA` increments on the **falling edge** of `(selected counter bit AND enable)`. So
//!   resetting DIV, or writing TAC so that the signal drops, also increments TIMA.
//! * When TIMA overflows it reads 0x00 for one M-cycle; in the next M-cycle it is reloaded from
//!   TMA and the interrupt is requested. Writing TIMA during the first cycle cancels the reload
//!   and interrupt; writing TIMA during the reload cycle is ignored (TMA wins); writing TMA
//!   during the reload cycle also changes the value loaded.
//!
//! Time advances one M-cycle per [`Timer::tick`], so the counter moves in steps of 4. The
//! selected bits are all >= bit 3, so no edge can be missed at that granularity.

pub const REG_DIV: u16 = 0xFF04;
pub const REG_TIMA: u16 = 0xFF05;
pub const REG_TMA: u16 = 0xFF06;
pub const REG_TAC: u16 = 0xFF07;

const TAC_ENABLE: u8 = 0x04;
const TAC_UNUSED_BITS: u8 = 0xF8;

/// System counter right after the DMG boot ROM (DIV reads 0xAB; Pan Docs "Power-Up Sequence").
/// The low byte is the phase of the boot sequence's last instructions and matters to Mooneye's
/// timer tests. 0xABCC is the value other accurate emulators use; the tests decide if it is right.
const POST_BOOT_COUNTER: u16 = 0xABCC;

#[derive(Debug, Clone)]
pub struct Timer {
    counter: u16,
    tima: u8,
    tma: u8,
    tac: u8,
    /// TIMA overflowed in the previous M-cycle: it reads 0 now and reloads at the next tick.
    reload_pending: bool,
    /// The reload happened in the current M-cycle (TIMA writes ignored, TMA writes propagate).
    reloading: bool,
}

impl Timer {
    pub fn new() -> Timer {
        Timer {
            counter: POST_BOOT_COUNTER,
            tima: 0,
            tma: 0,
            tac: 0,
            reload_pending: false,
            reloading: false,
        }
    }

    /// Counter bit that clocks TIMA for the current TAC clock select.
    fn selected_bit(&self) -> u16 {
        match self.tac & 3 {
            0 => 1 << 9,
            1 => 1 << 3,
            2 => 1 << 5,
            _ => 1 << 7,
        }
    }

    /// The signal whose falling edge increments TIMA.
    fn signal(&self) -> bool {
        self.tac & TAC_ENABLE != 0 && self.counter & self.selected_bit() != 0
    }

    fn increment_tima(&mut self) {
        let (next, overflowed) = self.tima.overflowing_add(1);
        self.tima = next; // wraps to 0x00 on overflow: that is what software reads this cycle
        if overflowed {
            self.reload_pending = true;
        }
    }

    /// Advance one M-cycle (4 T-cycles). Returns true if the timer interrupt is requested.
    pub fn tick(&mut self) -> bool {
        let mut interrupt = false;
        self.reloading = false;
        if self.reload_pending {
            self.reload_pending = false;
            self.reloading = true;
            self.tima = self.tma;
            interrupt = true;
        }
        let before = self.signal();
        self.counter = self.counter.wrapping_add(4);
        if before && !self.signal() {
            self.increment_tima();
        }
        interrupt
    }

    /// The 16-bit system counter (DIV is its upper byte). The APU frame sequencer watches bit 12.
    pub fn counter(&self) -> u16 {
        self.counter
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            REG_DIV => (self.counter >> 8) as u8,
            REG_TIMA => self.tima,
            REG_TMA => self.tma,
            _ => self.tac | TAC_UNUSED_BITS,
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            REG_DIV => {
                let before = self.signal();
                self.counter = 0;
                if before && !self.signal() {
                    self.increment_tima();
                }
            }
            REG_TIMA => {
                // Cancels a pending reload; ignored while the reload is happening.
                if !self.reloading {
                    self.tima = value;
                    self.reload_pending = false;
                }
            }
            REG_TMA => {
                self.tma = value;
                if self.reloading {
                    self.tima = value;
                }
            }
            _ => {
                let before = self.signal();
                self.tac = value & !TAC_UNUSED_BITS;
                if before && !self.signal() {
                    self.increment_tima();
                }
            }
        }
    }
}

impl Default for Timer {
    fn default() -> Timer {
        Timer::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Timer with the counter at zero so tests can count cycles from a known phase.
    fn timer() -> Timer {
        let mut t = Timer::new();
        t.write(REG_DIV, 0);
        t
    }

    fn ticks(t: &mut Timer, n: u32) -> bool {
        let mut irq = false;
        for _ in 0..n {
            irq |= t.tick();
        }
        irq
    }

    #[test]
    fn div_is_upper_byte_and_write_resets() {
        let mut t = timer();
        ticks(&mut t, 63); // 252 T-cycles
        assert_eq!(t.read(REG_DIV), 0);
        ticks(&mut t, 1);
        assert_eq!(t.read(REG_DIV), 1);
        t.write(REG_DIV, 0x77);
        assert_eq!(t.read(REG_DIV), 0);
    }

    #[test]
    fn post_boot_div_is_0xab() {
        assert_eq!(Timer::new().read(REG_DIV), 0xAB);
    }

    #[test]
    fn tima_rates_follow_tac() {
        // M-cycles per TIMA increment for TAC clock select 0..=3.
        for (tac, m_cycles) in [(0b100, 256u32), (0b101, 4), (0b110, 16), (0b111, 64)] {
            let mut t = timer();
            t.write(REG_TAC, tac);
            ticks(&mut t, m_cycles - 1);
            assert_eq!(t.read(REG_TIMA), 0, "tac {tac:#b} too early");
            ticks(&mut t, 1);
            assert_eq!(t.read(REG_TIMA), 1, "tac {tac:#b}");
        }
    }

    #[test]
    fn disabled_timer_does_not_count() {
        let mut t = timer();
        t.write(REG_TAC, 0b001);
        ticks(&mut t, 1000);
        assert_eq!(t.read(REG_TIMA), 0);
    }

    #[test]
    fn div_write_while_selected_bit_high_increments_tima() {
        let mut t = timer();
        t.write(REG_TAC, 0b101); // bit 3
        ticks(&mut t, 2); // counter = 8: bit 3 high
        assert_eq!(t.read(REG_TIMA), 0);
        t.write(REG_DIV, 0); // falling edge
        assert_eq!(t.read(REG_TIMA), 1);
        t.write(REG_DIV, 0); // signal already low: nothing
        assert_eq!(t.read(REG_TIMA), 1);
    }

    #[test]
    fn disabling_timer_while_bit_high_increments_tima() {
        let mut t = timer();
        t.write(REG_TAC, 0b101);
        ticks(&mut t, 2);
        t.write(REG_TAC, 0b001); // enable drops: signal falls
        assert_eq!(t.read(REG_TIMA), 1);
    }

    #[test]
    fn changing_clock_select_can_cause_falling_edge() {
        let mut t = timer();
        t.write(REG_TAC, 0b101); // bit 3
        ticks(&mut t, 2); // counter 8: bit 3 high, bit 5 low
        t.write(REG_TAC, 0b110); // now watching bit 5 (low): signal falls
        assert_eq!(t.read(REG_TIMA), 1);
    }

    #[test]
    fn overflow_reads_zero_then_reloads_with_interrupt() {
        let mut t = timer();
        t.write(REG_TMA, 0xAB);
        t.write(REG_TIMA, 0xFF);
        t.write(REG_TAC, 0b101);
        assert!(!ticks(&mut t, 3));
        assert!(!t.tick(), "overflow cycle: no interrupt yet");
        assert_eq!(t.read(REG_TIMA), 0x00, "reads 0 for one M-cycle");
        assert!(t.tick(), "reload cycle raises the interrupt");
        assert_eq!(t.read(REG_TIMA), 0xAB);
    }

    #[test]
    fn tima_write_in_overflow_cycle_cancels_reload() {
        let mut t = timer();
        t.write(REG_TMA, 0xAB);
        t.write(REG_TIMA, 0xFF);
        t.write(REG_TAC, 0b101);
        ticks(&mut t, 4); // overflow just happened
        t.write(REG_TIMA, 0x42);
        assert!(!t.tick(), "interrupt cancelled");
        assert_eq!(t.read(REG_TIMA), 0x42);
    }

    #[test]
    fn tima_write_in_reload_cycle_is_ignored() {
        let mut t = timer();
        t.write(REG_TMA, 0xAB);
        t.write(REG_TIMA, 0xFF);
        t.write(REG_TAC, 0b101);
        ticks(&mut t, 5); // reload just happened
        t.write(REG_TIMA, 0x42);
        assert_eq!(t.read(REG_TIMA), 0xAB);
    }

    #[test]
    fn tma_write_in_reload_cycle_changes_loaded_value() {
        let mut t = timer();
        t.write(REG_TMA, 0xAB);
        t.write(REG_TIMA, 0xFF);
        t.write(REG_TAC, 0b101);
        ticks(&mut t, 5);
        t.write(REG_TMA, 0x77);
        assert_eq!(t.read(REG_TIMA), 0x77);
    }
}
