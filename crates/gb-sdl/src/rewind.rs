//! Rewind: a bounded history of save states.
//!
//! The frontend pushes a state every `interval` frames while playing. While the rewind control
//! is held it takes one state back per displayed frame (so with the default interval of 4 the
//! picture runs backwards at four times normal speed). There is no "undo rewind": states newer
//! than the one you stop at are gone.
//!
//! Memory is bounded: a state is about 270 KB (more on CGB), and the number of states is capped
//! (`MAX_STATES`), which `capacity_for` enforces when the config is read.

use std::collections::VecDeque;

/// Upper bound on stored states: about 270 MB at 270 KB each.
pub const MAX_STATES: usize = 1000;
/// Frames per second of the Game Boy (59.7275), rounded up for sizing.
const FPS: f64 = 60.0;
/// Rough size of one state, only used to explain the limit in an error message.
const APPROX_STATE_KB: usize = 270;

/// Number of states needed to cover `seconds` of play with one state every `interval` frames.
pub fn capacity_for(seconds: f64, interval: u32) -> Result<usize, String> {
    let n = (seconds * FPS / f64::from(interval)).ceil() as usize;
    if n == 0 || n > MAX_STATES {
        return Err(format!(
            "rewind_seconds = {seconds} with rewind_interval_frames = {interval} needs {n} saved \
             states (about {} MB); the limit is {MAX_STATES} (about {} MB). Lower rewind_seconds \
             or raise rewind_interval_frames",
            n * APPROX_STATE_KB / 1000,
            MAX_STATES * APPROX_STATE_KB / 1000,
        ));
    }
    Ok(n)
}

pub struct RewindBuffer {
    states: VecDeque<Vec<u8>>,
    capacity: usize,
}

impl RewindBuffer {
    pub fn new(capacity: usize) -> RewindBuffer {
        let capacity = capacity.clamp(1, MAX_STATES);
        RewindBuffer {
            states: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Store a state; the oldest one is dropped when the buffer is full.
    pub fn push(&mut self, state: Vec<u8>) {
        if self.states.len() == self.capacity {
            self.states.pop_front();
        }
        self.states.push_back(state);
    }

    /// Step one state back in time. The oldest state is never removed (it is returned again on
    /// every call), so holding rewind at the start of the history just stays there.
    pub fn step_back(&mut self) -> Option<Vec<u8>> {
        if self.states.len() > 1 {
            self.states.pop_back()
        } else {
            self.states.front().cloned()
        }
    }

    /// Forget the history (after a reset or a quick load, the old timeline is gone).
    pub fn clear(&mut self) {
        self.states.clear();
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.states.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(n: u8) -> Vec<u8> {
        vec![n]
    }

    #[test]
    fn steps_back_newest_first() {
        let mut b = RewindBuffer::new(4);
        for n in 1..=3 {
            b.push(s(n));
        }
        assert_eq!(b.step_back(), Some(s(3)));
        assert_eq!(b.step_back(), Some(s(2)));
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn oldest_state_is_kept_and_repeated() {
        let mut b = RewindBuffer::new(4);
        b.push(s(1));
        b.push(s(2));
        assert_eq!(b.step_back(), Some(s(2)));
        assert_eq!(b.step_back(), Some(s(1)));
        assert_eq!(b.step_back(), Some(s(1)), "stays at the start of history");
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn empty_buffer_has_nothing_to_return() {
        let mut b = RewindBuffer::new(2);
        assert_eq!(b.step_back(), None);
    }

    #[test]
    fn full_buffer_drops_the_oldest() {
        let mut b = RewindBuffer::new(3);
        for n in 1..=5 {
            b.push(s(n));
        }
        assert_eq!(b.len(), 3);
        assert_eq!(b.step_back(), Some(s(5)));
        assert_eq!(b.step_back(), Some(s(4)));
        assert_eq!(b.step_back(), Some(s(3)));
        assert_eq!(b.step_back(), Some(s(3)), "1 and 2 were dropped");
    }

    #[test]
    fn pushing_after_stepping_back_replaces_the_future() {
        let mut b = RewindBuffer::new(4);
        for n in 1..=3 {
            b.push(s(n));
        }
        b.step_back(); // back at 2
        b.push(s(9)); // new timeline from 2
        assert_eq!(b.step_back(), Some(s(9)));
        assert_eq!(b.step_back(), Some(s(2)));
    }

    #[test]
    fn clear_empties_the_history() {
        let mut b = RewindBuffer::new(2);
        b.push(s(1));
        b.clear();
        assert_eq!((b.len(), b.step_back()), (0, None));
    }

    #[test]
    fn capacity_never_exceeds_the_cap() {
        assert_eq!(RewindBuffer::new(1_000_000).capacity, MAX_STATES);
        assert_eq!(RewindBuffer::new(0).capacity, 1);
    }

    #[test]
    fn capacity_for_the_defaults_and_the_limit() {
        // 10 s at 60 fps, one state per 4 frames.
        assert_eq!(capacity_for(10.0, 4), Ok(150));
        assert_eq!(capacity_for(1.0, 30), Ok(2));
        assert!(
            capacity_for(120.0, 4).is_err(),
            "1800 states is over the cap"
        );
        let msg = capacity_for(120.0, 1).unwrap_err();
        assert!(msg.contains("limit is 1000"), "{msg}");
        assert!(capacity_for(0.0, 4).is_err());
    }
}
