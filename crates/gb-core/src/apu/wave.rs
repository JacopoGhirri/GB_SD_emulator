//! Wave channel 3 (Pan Docs "Audio Details", "Wave RAM").

use super::common::LengthCounter;

/// How many T-cycles after the channel fetches a sample the CPU can still read/write the
/// byte it fetched (DMG only). Tuned against Blargg dmg_sound 09 and 12.
const ACCESS_WINDOW_T: u8 = 2;
/// Extra T-cycles before the first sample after a trigger (DMG). 5 and 6 both pass dmg_sound 09
/// and 12; 4 passes 10 but breaks 09 and 12 (see PROGRESS.md, phase 6 known issues).
const TRIGGER_DELAY_T: u32 = 6;

#[derive(Debug, Clone)]
pub struct Wave {
    pub on: bool,
    dac: bool,
    volume_code: u8,
    freq: u16,
    timer: u32,
    /// Index of the 4-bit sample being played, 0..=31.
    pos: u8,
    sample: u8,
    /// T-cycles since the channel last fetched a sample (saturating).
    since_fetch: u8,
    pub len: LengthCounter,
    ram: [u8; 16],
}

impl Wave {
    pub fn new(ram: [u8; 16]) -> Wave {
        Wave {
            on: false,
            dac: false,
            volume_code: 0,
            freq: 0,
            timer: 2,
            pos: 0,
            sample: 0,
            since_fetch: u8::MAX,
            len: LengthCounter::default(),
            ram,
        }
    }

    pub fn power_off(&mut self) {
        let (counter, ram) = (self.len.counter, self.ram);
        *self = Wave::new(ram);
        self.len.counter = counter;
    }

    pub fn write_length_only(&mut self, value: u8) {
        self.len.counter = 256 - u16::from(value);
    }

    fn period(&self) -> u32 {
        (2048 - u32::from(self.freq)) * 2
    }

    /// True while the CPU "sees" the byte the channel is reading (DMG wave RAM quirk).
    fn access_open(&self) -> bool {
        self.since_fetch < ACCESS_WINDOW_T
    }

    pub fn read_ram(&self, index: usize) -> u8 {
        if !self.on {
            self.ram[index]
        } else if self.access_open() {
            self.ram[usize::from(self.pos >> 1)]
        } else {
            0xFF
        }
    }

    pub fn write_ram(&mut self, index: usize, value: u8) {
        if !self.on {
            self.ram[index] = value;
        } else if self.access_open() {
            self.ram[usize::from(self.pos >> 1)] = value;
        }
    }

    pub fn write(&mut self, reg: u8, value: u8, next_step_odd: bool) {
        match reg {
            0 => {
                self.dac = value & 0x80 != 0;
                if !self.dac {
                    self.on = false;
                }
            }
            1 => self.len.counter = 256 - u16::from(value),
            2 => self.volume_code = (value >> 5) & 3,
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
        // DMG bug: retriggering while the channel is fetching a sample corrupts wave RAM.
        if self.on && self.access_open() {
            let byte = usize::from(self.pos >> 1);
            if byte < 4 {
                self.ram[0] = self.ram[byte];
            } else {
                let block = byte & !3;
                for i in 0..4 {
                    self.ram[i] = self.ram[block + i];
                }
            }
        }
        self.on = self.dac;
        self.len.trigger(256, next_step_odd);
        self.timer = self.period() + TRIGGER_DELAY_T;
        self.pos = 0;
    }

    pub fn clock_length(&mut self) {
        if self.len.clock() {
            self.on = false;
        }
    }

    pub fn step(&mut self) {
        self.since_fetch = self.since_fetch.saturating_add(1);
        if self.timer > 1 {
            self.timer -= 1;
        } else {
            self.timer = self.period();
            self.pos = (self.pos + 1) & 31;
            let byte = self.ram[usize::from(self.pos >> 1)];
            // The high nibble plays first.
            self.sample = if self.pos & 1 == 0 {
                byte >> 4
            } else {
                byte & 0xF
            };
            self.since_fetch = 0;
        }
    }

    pub fn output(&self) -> u8 {
        if !self.on {
            return 0;
        }
        match self.volume_code {
            0 => 0,
            1 => self.sample,
            2 => self.sample >> 1,
            _ => self.sample >> 2,
        }
    }

    pub fn dac_on(&self) -> bool {
        self.dac
    }
}

impl Wave {
    /// Save-state support (see `crate::state`). Includes wave RAM.
    pub fn save_state(&self, w: &mut crate::state::StateWriter) {
        w.bool(self.on);
        w.bool(self.dac);
        w.u8(self.volume_code);
        w.u16(self.freq);
        w.u32(self.timer);
        w.u8(self.pos);
        w.u8(self.sample);
        w.u8(self.since_fetch);
        self.len.save_state(w);
        w.bytes(&self.ram);
    }

    pub fn load_state(
        &mut self,
        r: &mut crate::state::StateReader,
    ) -> Result<(), crate::state::StateError> {
        self.on = r.bool()?;
        self.dac = r.bool()?;
        self.volume_code = r.u8()? & 3;
        self.freq = r.u16()? & 0x7FF;
        self.timer = r.u32()? & 0x3FFF;
        self.pos = r.u8()? & 31;
        self.sample = r.u8()? & 0x0F;
        self.since_fetch = r.u8()?;
        self.len.load_state(r)?;
        r.bytes(&mut self.ram)?;
        Ok(())
    }
}
