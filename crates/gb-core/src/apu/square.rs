//! Square channels 1 (with frequency sweep) and 2 (Pan Docs "Audio Details").

use super::common::{envelope_dac_on, Envelope, LengthCounter};

/// Duty waveforms in time order, MSB first (Pan Docs NRx1): 12.5 %, 25 %, 50 %, 75 %.
const DUTY: [u8; 4] = [0b0000_0001, 0b0000_0011, 0b0000_1111, 0b1111_1100];

#[derive(Debug, Clone)]
pub struct Square {
    has_sweep: bool,
    pub on: bool,
    dac: bool,
    duty: u8,
    pos: u8,
    /// T-cycles until the duty position advances.
    timer: u32,
    freq: u16,
    pub len: LengthCounter,
    env: Envelope,
    sweep_period: u8,
    sweep_negate: bool,
    sweep_shift: u8,
    sweep_timer: u8,
    shadow: u16,
    sweep_enabled: bool,
    /// A sweep calculation was done in negate mode since the last trigger.
    negate_used: bool,
}

impl Square {
    pub fn new(has_sweep: bool) -> Square {
        Square {
            has_sweep,
            on: false,
            dac: false,
            duty: 0,
            pos: 0,
            timer: 4,
            freq: 0,
            len: LengthCounter::default(),
            env: Envelope::default(),
            sweep_period: 0,
            sweep_negate: false,
            sweep_shift: 0,
            sweep_timer: 0,
            shadow: 0,
            sweep_enabled: false,
            negate_used: false,
        }
    }

    /// Channel 1 as the boot ROM leaves it: NR11 = 0xBF, NR12 = 0xF3, NR13/14 = 0xFF/0xBF, with
    /// the start-up beep already over (volume 0) but the channel still flagged on in NR52.
    pub fn post_boot_ch1() -> Square {
        let mut ch = Square::new(true);
        ch.write(0, 0x80, false);
        ch.write(1, 0xBF, false);
        ch.write(2, 0xF3, false);
        ch.write(3, 0xFF, false);
        ch.write(4, 0xBF, false);
        ch.on = true;
        ch.env.volume = 0;
        ch.len.enabled = false;
        ch
    }

    /// Power off: everything except the length counter is cleared (the DMG keeps it).
    pub fn power_off(&mut self) {
        let counter = self.len.counter;
        *self = Square::new(self.has_sweep);
        self.len.counter = counter;
    }

    /// Powering on resets the duty position (the frame sequencer reset is done by the APU).
    pub fn power_on(&mut self) {
        self.pos = 0;
    }

    /// Write to a length register while the APU is off (DMG: only the length bits work).
    pub fn write_length_only(&mut self, value: u8) {
        self.len.counter = 64 - u16::from(value & 0x3F);
    }

    fn period(&self) -> u32 {
        (2048 - u32::from(self.freq)) * 4
    }

    /// Register write. `reg` is 0..=4 for NRx0..NRx4.
    pub fn write(&mut self, reg: u8, value: u8, next_step_odd: bool) {
        match reg {
            0 => {
                let negate = value & 0x08 != 0;
                // Leaving negate mode after a negate-mode calculation disables the channel.
                if self.negate_used && !negate {
                    self.on = false;
                }
                self.sweep_period = (value >> 4) & 7;
                self.sweep_negate = negate;
                self.sweep_shift = value & 7;
            }
            1 => {
                self.duty = value >> 6;
                self.len.counter = 64 - u16::from(value & 0x3F);
            }
            2 => {
                self.env.write(value);
                self.dac = envelope_dac_on(value);
                if !self.dac {
                    self.on = false;
                }
            }
            3 => self.freq = (self.freq & 0x700) | u16::from(value),
            _ => {
                self.freq = (self.freq & 0x0FF) | (u16::from(value & 7) << 8);
                if self.len.set_enabled(value & 0x40 != 0, next_step_odd) {
                    self.on = false;
                }
                if value & 0x80 != 0 {
                    self.trigger(next_step_odd);
                }
            }
        }
    }

    fn trigger(&mut self, next_step_odd: bool) {
        self.on = self.dac;
        self.len.trigger(64, next_step_odd);
        self.timer = self.period();
        self.env.trigger();
        if self.has_sweep {
            self.shadow = self.freq;
            self.sweep_timer = if self.sweep_period == 0 {
                8
            } else {
                self.sweep_period
            };
            self.sweep_enabled = self.sweep_period != 0 || self.sweep_shift != 0;
            self.negate_used = false;
            if self.sweep_shift != 0 {
                self.sweep_calc(); // overflow check only
            }
        }
    }

    /// Next sweep frequency; switches the channel off if it would exceed 2047.
    fn sweep_calc(&mut self) -> u16 {
        let delta = self.shadow >> self.sweep_shift;
        let next = if self.sweep_negate {
            self.negate_used = true;
            self.shadow.wrapping_sub(delta)
        } else {
            self.shadow + delta
        };
        if next > 2047 {
            self.on = false;
        }
        next
    }

    /// 128 Hz sweep clock (frame sequencer steps 2 and 6).
    pub fn clock_sweep(&mut self) {
        if !self.has_sweep || self.sweep_timer == 0 {
            return;
        }
        self.sweep_timer -= 1;
        if self.sweep_timer == 0 {
            // A period of 0 is treated as 8 by the timer (Blargg dmg_sound 05).
            self.sweep_timer = if self.sweep_period == 0 {
                8
            } else {
                self.sweep_period
            };
            if self.sweep_enabled && self.sweep_period != 0 {
                let next = self.sweep_calc();
                if next <= 2047 && self.sweep_shift != 0 {
                    self.shadow = next;
                    self.freq = next;
                    self.sweep_calc(); // second overflow check with the new frequency
                }
            }
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

    /// Advance one T-cycle.
    pub fn step(&mut self) {
        if self.timer > 1 {
            self.timer -= 1;
        } else {
            self.timer = self.period();
            self.pos = (self.pos + 1) & 7;
        }
    }

    /// Digital output 0..=15.
    pub fn output(&self) -> u8 {
        if self.on {
            ((DUTY[self.duty as usize] >> (7 - self.pos)) & 1) * self.env.volume
        } else {
            0
        }
    }

    pub fn dac_on(&self) -> bool {
        self.dac
    }
}

impl Square {
    /// Save-state support (see `crate::state`). `has_sweep` is structural (channel 1 vs 2).
    pub fn save_state(&self, w: &mut crate::state::StateWriter) {
        w.bool(self.on);
        w.bool(self.dac);
        w.u8(self.duty);
        w.u8(self.pos);
        w.u32(self.timer);
        w.u16(self.freq);
        self.len.save_state(w);
        self.env.save_state(w);
        w.u8(self.sweep_period);
        w.bool(self.sweep_negate);
        w.u8(self.sweep_shift);
        w.u8(self.sweep_timer);
        w.u16(self.shadow);
        w.bool(self.sweep_enabled);
        w.bool(self.negate_used);
    }

    pub fn load_state(
        &mut self,
        r: &mut crate::state::StateReader,
    ) -> Result<(), crate::state::StateError> {
        self.on = r.bool()?;
        self.dac = r.bool()?;
        self.duty = r.u8()? & 3;
        self.pos = r.u8()? & 7;
        self.timer = r.u32()? & 0x3FFF;
        self.freq = r.u16()? & 0x7FF;
        self.len.load_state(r)?;
        self.env.load_state(r)?;
        self.sweep_period = r.u8()? & 7;
        self.sweep_negate = r.bool()?;
        self.sweep_shift = r.u8()? & 7;
        self.sweep_timer = r.u8()? & 0x0F;
        self.shadow = r.u16()? & 0x7FF;
        self.sweep_enabled = r.bool()?;
        self.negate_used = r.bool()?;
        Ok(())
    }
}
