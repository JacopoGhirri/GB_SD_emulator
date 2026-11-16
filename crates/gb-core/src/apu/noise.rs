//! Noise channel 4 (Pan Docs "Audio Details": LFSR).

use super::common::{envelope_dac_on, Envelope, LengthCounter};

/// Divisor codes 0..=7 in T-cycles (Pan Docs NR43: 8, 16, 32, 48, ... 112).
const DIVISORS: [u32; 8] = [8, 16, 32, 48, 64, 80, 96, 112];

#[derive(Debug, Clone)]
pub struct Noise {
    pub on: bool,
    dac: bool,
    nr43: u8,
    lfsr: u16,
    timer: u32,
    pub len: LengthCounter,
    env: Envelope,
}

impl Default for Noise {
    fn default() -> Noise {
        Noise::new()
    }
}

impl Noise {
    pub fn new() -> Noise {
        Noise {
            on: false,
            dac: false,
            nr43: 0,
            lfsr: 0x7FFF,
            timer: 8,
            len: LengthCounter::default(),
            env: Envelope::default(),
        }
    }

    pub fn power_off(&mut self) {
        let counter = self.len.counter;
        *self = Noise::new();
        self.len.counter = counter;
    }

    pub fn write_length_only(&mut self, value: u8) {
        self.len.counter = 64 - u16::from(value & 0x3F);
    }

    /// Clock shifts of 14 and 15 stop the LFSR (Pan Docs NR43).
    fn period(&self) -> Option<u32> {
        let shift = self.nr43 >> 4;
        (shift < 14).then(|| DIVISORS[usize::from(self.nr43 & 7)] << shift)
    }

    pub fn write(&mut self, reg: u8, value: u8, next_step_odd: bool) {
        match reg {
            1 => self.len.counter = 64 - u16::from(value & 0x3F),
            2 => {
                self.env.write(value);
                self.dac = envelope_dac_on(value);
                if !self.dac {
                    self.on = false;
                }
            }
            3 => self.nr43 = value,
            4 => {
                if self.len.set_enabled(value & 0x40 != 0, next_step_odd) {
                    self.on = false;
                }
                if value & 0x80 != 0 {
                    self.on = self.dac;
                    self.len.trigger(64, next_step_odd);
                    self.env.trigger();
                    self.lfsr = 0x7FFF;
                    self.timer = self.period().unwrap_or(u32::MAX);
                }
            }
            _ => {}
        }
    }

    pub fn clock_length(&mut self) {
        if self.len.clock() {
            self.on = false;
        }
    }

    pub fn clock_envelope(&mut self) {
        self.env.clock();
    }

    pub fn step(&mut self) {
        if self.timer > 1 {
            self.timer -= 1;
            return;
        }
        let Some(period) = self.period() else {
            self.timer = u32::MAX;
            return;
        };
        self.timer = period;
        let feedback = (self.lfsr ^ (self.lfsr >> 1)) & 1;
        self.lfsr = (self.lfsr >> 1) | (feedback << 14);
        if self.nr43 & 0x08 != 0 {
            // 7-bit mode: the feedback bit is also copied into bit 6.
            self.lfsr = (self.lfsr & !0x40) | (feedback << 6);
        }
    }

    pub fn output(&self) -> u8 {
        if self.on && self.lfsr & 1 == 0 {
            self.env.volume
        } else {
            0
        }
    }

    pub fn dac_on(&self) -> bool {
        self.dac
    }
}

impl Noise {
    /// Save-state support (see `crate::state`).
    pub fn save_state(&self, w: &mut crate::state::StateWriter) {
        w.bool(self.on);
        w.bool(self.dac);
        w.u8(self.nr43);
        w.u16(self.lfsr);
        w.u32(self.timer);
        self.len.save_state(w);
        self.env.save_state(w);
    }

    pub fn load_state(
        &mut self,
        r: &mut crate::state::StateReader,
    ) -> Result<(), crate::state::StateError> {
        self.on = r.bool()?;
        self.dac = r.bool()?;
        self.nr43 = r.u8()?;
        self.lfsr = r.u16()? & 0x7FFF;
        self.timer = r.u32()? & 0x00FF_FFFF;
        self.len.load_state(r)?;
        self.env.load_state(r)?;
        Ok(())
    }
}
