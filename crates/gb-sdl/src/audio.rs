//! SDL audio output and audio-driven pacing (phase 6).
//!
//! The audio device is the master clock. Each frame the core's samples are appended to an SDL
//! audio queue, then the main loop sleeps until the queue has drained back to `TARGET_MS` of
//! audio. Emulation therefore runs exactly as fast as the sound card consumes samples, so video
//! and sound cannot drift apart. A rate adjustment is implicit: if the card's clock is 0.01 %
//! fast or slow, the game follows it. `TARGET_MS` is also the cushion against underruns (a
//! frame that takes longer than that to emulate would make the sound crackle).

use std::time::{Duration, Instant};

use gb_core::GameBoy;
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::AudioSubsystem;

/// Audio kept queued ahead of the speakers. About three frames.
pub const TARGET_MS: u32 = 50;
/// If the queue ever holds more than this, the device stalled: drop the backlog instead of
/// letting the sound lag behind the picture.
const MAX_MS: u32 = 250;
/// Give up waiting for the queue to drain after this long (a stuck device must not hang us).
const MAX_WAIT: Duration = Duration::from_millis(100);

/// Stereo frames (one left + one right sample) in `ms` milliseconds.
pub fn frames_for_ms(ms: u32, rate: u32) -> usize {
    (u64::from(ms) * u64::from(rate) / 1000) as usize
}

/// How long to sleep before the queue (`queued` stereo frames) reaches `target` frames, or
/// `None` if it already has. Pure, so the pacing rule is unit-testable.
pub fn sleep_until_target(queued: usize, target: usize, rate: u32) -> Option<Duration> {
    let excess = queued.checked_sub(target).filter(|&e| e > 0)?;
    Some(Duration::from_micros(
        excess as u64 * 1_000_000 / u64::from(rate),
    ))
}

/// Volume steps for the +/- keys.
pub fn step_volume(volume: f32, up: bool) -> f32 {
    let v = if up { volume + 0.1 } else { volume - 0.1 };
    (v.clamp(0.0, 1.0) * 10.0).round() / 10.0
}

#[derive(Default, Debug)]
pub struct Stats {
    pub frames: u64,
    pub underruns: u64,
    pub overflows: u64,
    pub min_queued: usize,
    pub max_queued: usize,
    pub sum_queued: u64,
}

pub struct Audio {
    queue: AudioQueue<f32>,
    rate: u32,
    target: usize,
    pub volume: f32,
    pub stats: Stats,
}

impl Audio {
    pub fn open(subsystem: &AudioSubsystem) -> Result<Audio, String> {
        let desired = AudioSpecDesired {
            freq: Some(48_000),
            channels: Some(2),
            samples: Some(512),
        };
        let queue = subsystem.open_queue::<f32, _>(None, &desired)?;
        let rate = queue.spec().freq as u32;
        let target = frames_for_ms(TARGET_MS, rate);
        // Pre-fill with silence so the first frames do not underrun, then start playback.
        queue.queue_audio(&vec![0.0; target * 2])?;
        queue.resume();
        Ok(Audio {
            queue,
            rate,
            target,
            volume: 1.0,
            stats: Stats {
                min_queued: usize::MAX,
                ..Stats::default()
            },
        })
    }

    /// Start a game with an empty queue plus the usual cushion of silence (the device kept
    /// draining while the game list was on screen).
    pub fn prime(&mut self) {
        self.queue.clear();
        if let Err(e) = self.queue.queue_audio(&vec![0.0; self.target * 2]) {
            eprintln!("warning: audio queue error: {e}");
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    fn queued_frames(&self) -> usize {
        self.queue.size() as usize / (2 * std::mem::size_of::<f32>())
    }

    /// Hand one emulated frame of audio to the device, then block until the queue is back at
    /// the target fill level. This is the frame pacing.
    pub fn push_and_wait(&mut self, gb: &mut GameBoy, scratch: &mut Vec<f32>) {
        scratch.clear();
        gb.drain_audio(scratch);
        for s in scratch.iter_mut() {
            *s *= self.volume;
        }
        let before = self.queued_frames();
        if before == 0 {
            self.stats.underruns += 1;
        }
        if before > frames_for_ms(MAX_MS, self.rate) {
            self.queue.clear();
            self.stats.overflows += 1;
        }
        if let Err(e) = self.queue.queue_audio(scratch) {
            eprintln!("warning: audio queue error: {e}");
        }
        let start = Instant::now();
        while let Some(d) = sleep_until_target(self.queued_frames(), self.target, self.rate) {
            if start.elapsed() > MAX_WAIT {
                break;
            }
            std::thread::sleep(d.min(Duration::from_millis(2)));
        }
        let q = self.queued_frames();
        self.stats.frames += 1;
        self.stats.min_queued = self.stats.min_queued.min(q);
        self.stats.max_queued = self.stats.max_queued.max(q);
        self.stats.sum_queued += q as u64;
    }

    /// Slow motion or pause: throw the audio away and empty the queue so it does not play late.
    pub fn discard(&mut self, gb: &mut GameBoy, scratch: &mut Vec<f32>) {
        scratch.clear();
        gb.drain_audio(scratch);
        self.queue.clear();
    }

    pub fn report(&self) {
        let s = &self.stats;
        if s.frames == 0 {
            return;
        }
        eprintln!(
            "audio: {} frames paced, queue fill min/avg/max {}/{}/{} stereo frames (target {}), {} underruns, {} overflows",
            s.frames,
            s.min_queued,
            s.sum_queued / s.frames,
            s.max_queued,
            self.target,
            s.underruns,
            s.overflows
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_is_50_ms() {
        assert_eq!(frames_for_ms(50, 48_000), 2400);
    }

    #[test]
    fn no_sleep_at_or_below_target() {
        assert_eq!(sleep_until_target(2400, 2400, 48_000), None);
        assert_eq!(sleep_until_target(0, 2400, 48_000), None);
    }

    #[test]
    fn sleeps_for_the_excess_audio() {
        // 480 frames over target at 48 kHz = 10 ms.
        assert_eq!(
            sleep_until_target(2880, 2400, 48_000),
            Some(Duration::from_millis(10))
        );
    }

    #[test]
    fn volume_steps_stay_in_range() {
        assert_eq!(step_volume(0.5, true), 0.6);
        assert_eq!(step_volume(1.0, true), 1.0);
        assert_eq!(step_volume(0.0, false), 0.0);
    }
}
