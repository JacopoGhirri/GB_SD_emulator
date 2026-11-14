//! Frame pacing, kept in one function so phase 6 can replace it with audio-driven pacing.

use std::time::{Duration, Instant};

/// One frame is 70224 dots at 4194304 dots/s = 16.7427 ms (59.7275 Hz).
pub const FRAME_DOTS: u64 = 70_224;
pub const DOTS_PER_SECOND: u64 = 4_194_304;

pub fn frame_duration() -> Duration {
    Duration::from_nanos(FRAME_DOTS * 1_000_000_000 / DOTS_PER_SECOND)
}

/// Tracks the deadline of the next frame so rounding errors do not accumulate.
pub struct Pacer {
    next: Instant,
}

impl Pacer {
    pub fn new() -> Pacer {
        Pacer {
            next: Instant::now(),
        }
    }

    /// Next deadline after `deadline`. If we fell more than 4 frames behind (window drag,
    /// suspend) re-sync to `now` instead of running a burst of catch-up frames.
    pub fn advance(deadline: Instant, now: Instant, frame: Duration) -> Instant {
        let next = deadline + frame;
        if now > next + frame * 4 {
            now + frame
        } else {
            next
        }
    }

    /// Block until the current frame's deadline, then schedule the next one.
    /// `fast_forward` skips the wait (uncapped) and re-syncs the clock.
    pub fn wait_for_next_frame(&mut self, fast_forward: bool) {
        let now = Instant::now();
        if fast_forward {
            self.next = now + frame_duration();
            return;
        }
        // Sleep most of the wait, spin the last 1 ms: sleep granularity is coarse.
        let spin = Duration::from_millis(1);
        if let Some(left) = self.next.checked_duration_since(now) {
            if left > spin {
                std::thread::sleep(left - spin);
            }
            while Instant::now() < self.next {
                std::hint::spin_loop();
            }
        }
        self.next = Self::advance(self.next, Instant::now(), frame_duration());
    }
}

impl Default for Pacer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_is_about_16_74_ms() {
        assert_eq!(frame_duration().as_micros(), 16_742);
    }

    #[test]
    fn deadline_advances_by_one_frame_when_on_time() {
        let t0 = Instant::now();
        let f = frame_duration();
        assert_eq!(Pacer::advance(t0, t0, f), t0 + f);
    }

    #[test]
    fn deadline_resyncs_after_a_long_stall() {
        let t0 = Instant::now();
        let f = frame_duration();
        let now = t0 + Duration::from_secs(5);
        assert_eq!(Pacer::advance(t0, now, f), now + f);
    }
}
