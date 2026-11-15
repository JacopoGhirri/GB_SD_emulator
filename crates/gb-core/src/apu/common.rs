//! Pieces shared by the sound channels: the length counter and the volume envelope.

/// Length counter (Pan Docs "Audio Details": "Length timer").
///
/// A channel is switched off when its counter, clocked at 256 Hz by the frame sequencer, reaches
/// zero (only while the length-enable bit in NRx4 is set).
#[derive(Debug, Clone, Default)]
pub struct LengthCounter {
    pub counter: u16,
    pub enabled: bool,
}

impl LengthCounter {
    /// One 256 Hz clock. Returns true if the channel must now be switched off.
    pub fn clock(&mut self) -> bool {
        if self.enabled && self.counter > 0 {
            self.counter -= 1;
            return self.counter == 0;
        }
        false
    }

    /// NRx4 bit 6 written. Returns true if the channel must be switched off.
    ///
    /// Hardware quirk (Pan Docs "Obscure behaviour"; Blargg dmg_sound 02/03): enabling the
    /// length counter while the frame sequencer's *next* step is one that does not clock length
    /// (an odd step) gives the counter an extra clock right away.
    pub fn set_enabled(&mut self, enable: bool, next_step_odd: bool) -> bool {
        let was = self.enabled;
        self.enabled = enable;
        !was && enable && next_step_odd && self.clock()
    }

    /// Trigger: a counter at zero is reloaded to its maximum. If the length is enabled and the
    /// next frame-sequencer step does not clock length, that reload is clocked once straight away.
    pub fn trigger(&mut self, max: u16, next_step_odd: bool) {
        if self.counter == 0 {
            self.counter = max;
            if self.enabled && next_step_odd {
                self.counter -= 1;
            }
        }
    }
}

/// Volume envelope of the square and noise channels (NRx2).
#[derive(Debug, Clone, Default)]
pub struct Envelope {
    initial: u8,
    add: bool,
    period: u8,
    timer: u8,
    pub volume: u8,
}

impl Envelope {
    pub fn write(&mut self, value: u8) {
        self.initial = value >> 4;
        self.add = value & 0x08 != 0;
        self.period = value & 0x07;
    }

    pub fn trigger(&mut self) {
        self.volume = self.initial;
        self.timer = if self.period == 0 { 8 } else { self.period };
    }

    /// One 64 Hz clock. A period of 0 means the envelope does not change the volume.
    pub fn clock(&mut self) {
        if self.period == 0 {
            return;
        }
        self.timer = self.timer.saturating_sub(1);
        if self.timer == 0 {
            self.timer = self.period;
            if self.add && self.volume < 15 {
                self.volume += 1;
            } else if !self.add && self.volume > 0 {
                self.volume -= 1;
            }
        }
    }
}

/// The DAC of a square/noise channel is on iff the top five bits of NRx2 are not all zero.
pub fn envelope_dac_on(nrx2: u8) -> bool {
    nrx2 & 0xF8 != 0
}
