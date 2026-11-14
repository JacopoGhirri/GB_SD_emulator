//! Serial port (Pan Docs "Serial Data Transfer").
//!
//! SB (0xFF01) holds the byte; SC (0xFF02) bit 7 starts a transfer and bit 0 selects the internal
//! clock. No link partner exists, so:
//! * every byte whose transfer is started with the internal clock is captured in an output
//!   buffer: test ROMs (Blargg) print their verdict this way;
//! * with the internal clock the transfer completes after 8 bits at 8192 Hz (4096 T-cycles):
//!   SB reads 0xFF (nothing was shifted in), bit 7 of SC clears and the serial interrupt fires;
//! * with the external clock nothing ever arrives, so the transfer never completes.

pub const REG_SB: u16 = 0xFF01;
pub const REG_SC: u16 = 0xFF02;

const SC_TRANSFER_START: u8 = 0x80;
const SC_INTERNAL_CLOCK: u8 = 0x01;
/// Only bits 7 and 0 exist on DMG; the rest read as 1.
const SC_UNUSED_BITS: u8 = 0x7E;
/// 8 bits at 8192 Hz = 8 * 512 T-cycles = 1024 M-cycles.
const TRANSFER_M_CYCLES: u32 = 1024;

#[derive(Debug, Clone)]
pub struct Serial {
    sb: u8,
    sc: u8,
    /// M-cycles left in the running transfer; 0 when idle.
    remaining: u32,
    out: Vec<u8>,
}

impl Serial {
    pub fn new() -> Serial {
        Serial {
            sb: 0x00,
            sc: 0x00, // reads 0x7E post-boot via the unused-bit mask
            remaining: 0,
            out: Vec::new(),
        }
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            REG_SB => self.sb,
            _ => self.sc | SC_UNUSED_BITS,
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            REG_SB => self.sb = value,
            _ => {
                self.sc = value & (SC_TRANSFER_START | SC_INTERNAL_CLOCK);
                if value & (SC_TRANSFER_START | SC_INTERNAL_CLOCK)
                    == (SC_TRANSFER_START | SC_INTERNAL_CLOCK)
                {
                    self.out.push(self.sb);
                    self.remaining = TRANSFER_M_CYCLES;
                } else {
                    self.remaining = 0;
                }
            }
        }
    }

    /// Advance one M-cycle. Returns true when a transfer completes (raise the serial interrupt).
    pub fn tick(&mut self) -> bool {
        if self.remaining == 0 {
            return false;
        }
        self.remaining -= 1;
        if self.remaining > 0 {
            return false;
        }
        self.sb = 0xFF;
        self.sc &= !SC_TRANSFER_START;
        true
    }

    /// Bytes sent since the last call.
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.out)
    }
}

impl Default for Serial {
    fn default() -> Serial {
        Serial::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_clock_start_captures_sb() {
        let mut s = Serial::new();
        s.write(REG_SB, b'H');
        s.write(REG_SC, 0x81);
        s.write(REG_SB, b'i');
        s.write(REG_SC, 0x80); // external clock: not captured
        s.write(REG_SC, 0x81);
        assert_eq!(s.take_output(), b"Hi");
        assert!(s.take_output().is_empty(), "buffer is drained");
    }

    #[test]
    fn internal_transfer_completes_after_4096_t_cycles_reading_ff() {
        let mut s = Serial::new();
        s.write(REG_SB, 0x42);
        s.write(REG_SC, 0x81);
        for _ in 0..TRANSFER_M_CYCLES - 1 {
            assert!(!s.tick());
        }
        assert_eq!(s.read(REG_SC) & 0x80, 0x80, "still running");
        assert!(s.tick(), "interrupt on the last cycle");
        assert_eq!(s.read(REG_SB), 0xFF);
        assert_eq!(s.read(REG_SC) & 0x80, 0, "start bit cleared");
        assert!(!s.tick(), "fires once");
    }

    #[test]
    fn external_clock_never_completes() {
        let mut s = Serial::new();
        s.write(REG_SC, 0x80);
        for _ in 0..5000 {
            assert!(!s.tick());
        }
        assert_eq!(s.read(REG_SC) & 0x80, 0x80);
    }

    #[test]
    fn unused_sc_bits_read_as_one() {
        assert_eq!(Serial::new().read(REG_SC), 0x7E);
    }
}
