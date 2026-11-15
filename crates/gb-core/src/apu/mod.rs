//! Audio processing unit (Pan Docs "Audio Overview", "Audio Registers", "Audio Details").
//!
//! Layout: four channels ([`square::Square`] x2, [`wave::Wave`], [`noise::Noise`]), a frame
//! sequencer clocked from the timer's system counter, and a mixer that turns the channel outputs
//! into stereo samples at a configurable rate. Everything advances from [`Apu::tick`], one
//! M-cycle at a time, so the output is a pure function of the emulated machine (deterministic).

mod common;
mod noise;
mod square;
mod wave;

use crate::io::{read_mask, WAVE_RAM_END, WAVE_RAM_START};
use noise::Noise;
use square::Square;
use wave::Wave;

pub const REG_NR10: u16 = 0xFF10;
pub const REG_NR50: u16 = 0xFF24;
pub const REG_NR51: u16 = 0xFF25;
pub const REG_NR52: u16 = 0xFF26;

/// Number of registers from NR10 (0xFF10) up to and including NR51 (0xFF25).
const REG_COUNT: usize = 0x16;

/// T-cycles per second.
const CPU_HZ: u32 = 4_194_304;
pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;
/// Stop queueing when this many seconds of audio are waiting; keeps memory bounded if a
/// frontend never drains.
const MAX_BUFFERED_SECONDS: usize = 1;

/// Typical DMG wave RAM contents after the boot ROM. Real hardware powers up with
/// effectively random data; this is what several accurate emulators use.
const POST_BOOT_WAVE_RAM: [u8; 16] = [
    0x84, 0x40, 0x43, 0xAA, 0x2D, 0x78, 0x92, 0x3C, 0x60, 0x59, 0x59, 0xB0, 0x34, 0xB8, 0x2E, 0xDA,
];

#[derive(Debug, Clone)]
pub struct Apu {
    /// Raw written values of NR10..=NR51. Reads OR in the unused/write-only bits ([`read_mask`]).
    regs: [u8; REG_COUNT],
    /// NR52 bit 7. While false the registers are zeroed and read-only.
    powered: bool,
    ch1: Square,
    ch2: Square,
    ch3: Wave,
    ch4: Noise,

    /// Next frame-sequencer step to run (0..=7). Length is clocked on even steps, sweep on 2 and
    /// 6, envelopes on 7 (Pan Docs "Audio Details": frame sequencer).
    fs_step: u8,
    /// Bit 12 of the timer's system counter (bit 4 of DIV) at the last tick; the frame sequencer
    /// advances on its falling edge, 512 times a second.
    last_div_bit: bool,

    // Mixer and sample output.
    sample_rate: u32,
    /// Bit i set = channel i+1 muted (debug aid for the frontend).
    muted: u8,
    sample_phase: u32,
    acc_left: f32,
    acc_right: f32,
    acc_count: u32,
    /// High-pass filter state (the DC-blocking capacitor of the real hardware).
    cap_left: f32,
    cap_right: f32,
    highpass_charge: f32,
    out: Vec<f32>,
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
        let mut apu = Apu {
            regs,
            powered: true,
            ch1: Square::post_boot_ch1(),
            ch2: Square::new(false),
            ch3: Wave::new(POST_BOOT_WAVE_RAM),
            ch4: Noise::new(),
            fs_step: 0,
            last_div_bit: false,
            sample_rate: DEFAULT_SAMPLE_RATE,
            muted: 0,
            sample_phase: 0,
            acc_left: 0.0,
            acc_right: 0.0,
            acc_count: 0,
            cap_left: 0.0,
            cap_right: 0.0,
            highpass_charge: 0.0,
            out: Vec::new(),
        };
        apu.set_sample_rate(DEFAULT_SAMPLE_RATE);
        apu
    }

    pub fn set_sample_rate(&mut self, rate: u32) {
        self.sample_rate = rate.clamp(8_000, 192_000);
        // The hardware capacitor loses 0.999958 of its charge per T-cycle... per *sample* that is
        // 0.999958^(4194304/rate) (Pan Docs "Audio Details": high-pass filter).
        self.highpass_charge = 0.999_958f32.powf(CPU_HZ as f32 / self.sample_rate as f32);
        self.sample_phase = 0;
    }

    pub fn set_muted_channels(&mut self, mask: u8) {
        self.muted = mask & 0x0F;
    }

    /// Append the interleaved stereo samples produced since the last call.
    pub fn drain_audio(&mut self, out: &mut Vec<f32>) {
        out.append(&mut self.out);
    }

    // ---------------------------------------------------------------- registers

    /// True if the next frame-sequencer step does not clock length counters.
    fn next_step_odd(&self) -> bool {
        self.fs_step & 1 == 1
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            REG_NR52 => {
                let status = u8::from(self.ch1.on)
                    | (u8::from(self.ch2.on) << 1)
                    | (u8::from(self.ch3.on) << 2)
                    | (u8::from(self.ch4.on) << 3);
                0x70 | (u8::from(self.powered) << 7) | status
            }
            REG_NR10..=REG_NR51 => self.regs[(addr - REG_NR10) as usize] | read_mask(addr),
            WAVE_RAM_START..=WAVE_RAM_END => self.ch3.read_ram((addr - WAVE_RAM_START) as usize),
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        let odd = self.next_step_odd();
        match addr {
            REG_NR52 => self.write_power(value & 0x80 != 0),
            WAVE_RAM_START..=WAVE_RAM_END => {
                self.ch3.write_ram((addr - WAVE_RAM_START) as usize, value)
            }
            REG_NR10..=REG_NR51 if !self.powered => {
                // Powered off: registers are read-only, except that on the DMG the length
                // counters can still be loaded (Blargg dmg_sound 08).
                match addr {
                    0xFF11 => self.ch1.write_length_only(value),
                    0xFF16 => self.ch2.write_length_only(value),
                    0xFF1B => self.ch3.write_length_only(value),
                    0xFF20 => self.ch4.write_length_only(value),
                    _ => {}
                }
            }
            REG_NR10..=REG_NR51 => {
                self.regs[(addr - REG_NR10) as usize] = value;
                let reg = (addr & 0xF) as u8 % 5;
                match addr {
                    0xFF10..=0xFF14 => self.ch1.write(reg, value, odd),
                    0xFF16..=0xFF19 => self.ch2.write(reg, value, odd),
                    0xFF1A..=0xFF1E => self.ch3.write(reg, value, odd),
                    0xFF20..=0xFF23 => self.ch4.write(reg + 1, value, odd),
                    _ => {} // NR50, NR51 are read by the mixer; 0xFF15 and 0xFF1F do not exist
                }
            }
            _ => {}
        }
    }

    fn write_power(&mut self, on: bool) {
        if on == self.powered {
            return;
        }
        self.powered = on;
        if on {
            // The frame sequencer restarts from step 0 and the square duty phases reset.
            self.fs_step = 0;
            self.ch1.power_on();
            self.ch2.power_on();
        } else {
            // Powering off clears every register (NR10..=NR51) and silences all channels.
            // Wave RAM and (on the DMG) the length counters are untouched.
            self.regs = [0; REG_COUNT];
            self.ch1.power_off();
            self.ch2.power_off();
            self.ch3.power_off();
            self.ch4.power_off();
        }
    }

    // ---------------------------------------------------------------- time

    /// Advance one M-cycle. `div_counter` is the timer's 16-bit system counter after this cycle.
    pub fn tick(&mut self, div_counter: u16) {
        self.clock_div(div_counter);
        for _ in 0..4 {
            self.ch1.step();
            self.ch2.step();
            self.ch3.step();
            self.ch4.step();
            self.sample_t();
        }
    }

    /// Feed the system counter to the frame sequencer. Also called right after a DIV write,
    /// because resetting the counter can itself produce a falling edge.
    pub fn clock_div(&mut self, div_counter: u16) {
        let bit = div_counter & 0x1000 != 0;
        if self.last_div_bit && !bit && self.powered {
            self.frame_step();
        }
        self.last_div_bit = bit;
    }

    fn frame_step(&mut self) {
        let step = self.fs_step;
        if step & 1 == 0 {
            self.ch1.clock_length();
            self.ch2.clock_length();
            self.ch3.clock_length();
            self.ch4.clock_length();
        }
        if step == 2 || step == 6 {
            self.ch1.clock_sweep();
        }
        if step == 7 {
            self.ch1.clock_envelope();
            self.ch2.clock_envelope();
            self.ch4.clock_envelope();
        }
        self.fs_step = (step + 1) & 7;
    }

    // ---------------------------------------------------------------- mixer

    /// Analog level of one channel in -1.0..=1.0. A channel whose DAC is off contributes 0.
    fn dac(on: bool, digital: u8, muted: bool) -> f32 {
        if !on {
            0.0
        } else if muted {
            -1.0 // a muted channel with its DAC on still sits at the DAC's idle level
        } else {
            f32::from(digital) / 7.5 - 1.0
        }
    }

    /// Current stereo mix (NR51 panning, NR50 master volume), each side in about -1.0..=1.0.
    fn mix(&self) -> (f32, f32) {
        let m = |i: u8| self.muted & (1 << i) != 0;
        let ch = [
            Self::dac(self.ch1.dac_on(), self.ch1.output(), m(0)),
            Self::dac(self.ch2.dac_on(), self.ch2.output(), m(1)),
            Self::dac(self.ch3.dac_on(), self.ch3.output(), m(2)),
            Self::dac(self.ch4.dac_on(), self.ch4.output(), m(3)),
        ];
        let nr50 = self.regs[0x14];
        let nr51 = self.regs[0x15];
        let (mut left, mut right) = (0.0, 0.0);
        for (i, level) in ch.iter().enumerate() {
            if nr51 & (0x10 << i) != 0 {
                left += level;
            }
            if nr51 & (1 << i) != 0 {
                right += level;
            }
        }
        let left_vol = f32::from(((nr50 >> 4) & 7) + 1) / 8.0;
        let right_vol = f32::from((nr50 & 7) + 1) / 8.0;
        // Four channels at full swing sum to +-4; /8 leaves 6 dB of headroom, because the
        // high-pass filter overshoots when a loud DC step passes through it (a first version
        // with /4 measured peaks of 1.2, i.e. clipping, on Pokemon Red's title music).
        (left / 8.0 * left_vol, right / 8.0 * right_vol)
    }

    /// One T-cycle of sample generation: box-filter (average) the mix over each output sample.
    fn sample_t(&mut self) {
        let (l, r) = self.mix();
        self.acc_left += l;
        self.acc_right += r;
        self.acc_count += 1;
        self.sample_phase += self.sample_rate;
        if self.sample_phase >= CPU_HZ {
            self.sample_phase -= CPU_HZ;
            let n = self.acc_count as f32;
            let (l, r) = (self.acc_left / n, self.acc_right / n);
            self.acc_left = 0.0;
            self.acc_right = 0.0;
            self.acc_count = 0;
            // DC-blocking high-pass: out = in - capacitor, capacitor tracks the input slowly.
            let out_l = l - self.cap_left;
            self.cap_left = l - out_l * self.highpass_charge;
            let out_r = r - self.cap_right;
            self.cap_right = r - out_r * self.highpass_charge;
            if self.out.len() < 2 * self.sample_rate as usize * MAX_BUFFERED_SECONDS {
                self.out.push(out_l);
                self.out.push(out_r);
            }
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
        assert_eq!(apu.read(0xFF11), 0xBF);
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

    /// Run `m_cycles` M-cycles with a free-running counter, like the bus does.
    fn run(apu: &mut Apu, counter: &mut u16, m_cycles: u32) {
        for _ in 0..m_cycles {
            *counter = counter.wrapping_add(4);
            apu.tick(*counter);
        }
    }

    #[test]
    fn square_channel_plays_and_length_counter_stops_it() {
        let mut apu = Apu::new();
        let mut counter = 0u16;
        apu.write(0xFF12, 0xF0); // volume 15, no envelope sweep
        apu.write(0xFF11, 0x80 | 62); // duty 50 %, length 2 -> stops after 2 length clocks
        apu.write(0xFF13, 0x00);
        apu.write(0xFF14, 0xC7); // trigger, length enabled, freq 0x700
        assert_eq!(apu.read(REG_NR52) & 1, 1);
        let mut seen_high = false;
        let mut seen_low = false;
        for _ in 0..2000 {
            run(&mut apu, &mut counter, 1);
            let o = apu.ch1.output();
            seen_high |= o == 15;
            seen_low |= o == 0;
        }
        assert!(seen_high && seen_low, "50 % duty square must toggle");
        run(&mut apu, &mut counter, 4_194_304 / 4 / 64 * 3); // > 2 length clocks (256 Hz)
        assert_eq!(apu.read(REG_NR52) & 1, 0, "length ran out");
    }

    /// Pitch check: a square channel with frequency register 0x700 plays 131072 / (2048 - 0x700)
    /// = 512 Hz (Pan Docs NR13/NR14), so one emulated second of output has ~512 rising edges.
    #[test]
    fn square_pitch_matches_the_frequency_formula() {
        let mut apu = Apu::new();
        let mut counter = 0u16;
        apu.write(0xFF12, 0xF0);
        apu.write(0xFF13, 0x00);
        apu.write(0xFF14, 0x87); // trigger, freq high bits = 7 -> 0x700
        run(&mut apu, &mut counter, 4_194_304 / 4);
        let mut out = Vec::new();
        apu.drain_audio(&mut out);
        let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
        let mid = (left.iter().cloned().fold(f32::MIN, f32::max)
            + left.iter().cloned().fold(f32::MAX, f32::min))
            / 2.0;
        let edges = left
            .windows(2)
            .filter(|w| w[0] < mid && w[1] >= mid)
            .count();
        assert!(
            (510..=514).contains(&edges),
            "{edges} rising edges per second"
        );
    }

    #[test]
    fn dac_off_channel_cannot_be_triggered() {
        let mut apu = Apu::new();
        apu.write(0xFF12, 0x00); // DAC off
        apu.write(0xFF14, 0x80);
        assert_eq!(apu.read(REG_NR52) & 1, 0);
    }

    #[test]
    fn sample_rate_is_respected_and_buffer_is_bounded() {
        let mut apu = Apu::new();
        let mut counter = 0u16;
        run(&mut apu, &mut counter, 4_194_304 / 4); // one emulated second
        let mut out = Vec::new();
        apu.drain_audio(&mut out);
        assert_eq!(out.len() / 2, 48_000, "one second = 48000 stereo frames");
        run(&mut apu, &mut counter, 4_194_304 / 4 * 3); // never drained: capped at 1 s
        assert_eq!(apu.out.len(), 2 * 48_000);
    }
}
