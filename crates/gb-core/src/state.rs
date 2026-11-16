//! Save-state plumbing: a tiny little-endian byte writer/reader and the error type.
//!
//! Every component writes its own fields (`save_state`) and reads them back in the same order
//! (`load_state`). There is no serde: the core has no dependencies, and an explicit field list
//! makes it obvious in review when a new field was forgotten (the round-trip tests in
//! `tests/save_state.rs` catch the rest).
//!
//! What a state contains: everything that changes the future behaviour of the emulated machine,
//! including both framebuffers (a state saved mid-frame must show the same picture when loaded).
//! What it does *not* contain: the ROM, the audio output queue (loading must not replay stale
//! samples), the serial-test output log, and frontend settings (sample rate, muted channels).

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateError {
    /// Not a save state (wrong magic bytes).
    BadMagic,
    /// Written by a different version of the format.
    UnsupportedVersion(u32),
    /// The data ends early, or its stored length/checksum does not match: damaged file.
    Truncated,
    Checksum,
    /// The state was made for another console model or another cartridge.
    WrongMachine(&'static str),
    /// Well-formed bytes that describe an impossible machine (never produced by `save_state`).
    Corrupt(&'static str),
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateError::BadMagic => write!(f, "not a save state"),
            StateError::UnsupportedVersion(v) => write!(f, "unsupported save-state version {v}"),
            StateError::Truncated => write!(f, "save state is truncated"),
            StateError::Checksum => write!(f, "save state is damaged (checksum mismatch)"),
            StateError::WrongMachine(what) => {
                write!(f, "save state is for a different {what}")
            }
            StateError::Corrupt(what) => write!(f, "save state is corrupt: {what}"),
        }
    }
}

impl std::error::Error for StateError {}

/// Append-only little-endian writer.
#[derive(Default)]
pub struct StateWriter {
    buf: Vec<u8>,
}

impl StateWriter {
    pub fn new() -> StateWriter {
        StateWriter::default()
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn bool(&mut self, v: bool) {
        self.buf.push(u8::from(v));
    }

    pub fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    /// Stored as the raw IEEE bits so the round trip is exact (and NaN-safe).
    pub fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }

    /// Raw bytes with no length prefix: the reader must know the size (arrays).
    pub fn bytes(&mut self, v: &[u8]) {
        self.buf.extend_from_slice(v);
    }

    pub fn u32s(&mut self, v: &[u32]) {
        for x in v {
            self.u32(*x);
        }
    }
}

/// Bounds-checked reader: any read past the end is `StateError::Truncated`, never a panic.
pub struct StateReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> StateReader<'a> {
    pub fn new(data: &'a [u8]) -> StateReader<'a> {
        StateReader { data, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], StateError> {
        if self.remaining() < n {
            return Err(StateError::Truncated);
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub fn u8(&mut self) -> Result<u8, StateError> {
        Ok(self.take(1)?[0])
    }

    /// Only 0 and 1 are valid; anything else means the data is not ours.
    pub fn bool(&mut self) -> Result<bool, StateError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(StateError::Corrupt("boolean out of range")),
        }
    }

    pub fn u16(&mut self) -> Result<u16, StateError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn u32(&mut self) -> Result<u32, StateError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn u64(&mut self) -> Result<u64, StateError> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(u64::from_le_bytes(a))
    }

    pub fn f32(&mut self) -> Result<f32, StateError> {
        Ok(f32::from_bits(self.u32()?))
    }

    /// Fill `out` (fixed size, no length prefix).
    pub fn bytes(&mut self, out: &mut [u8]) -> Result<(), StateError> {
        out.copy_from_slice(self.take(out.len())?);
        Ok(())
    }

    pub fn u32s(&mut self, out: &mut [u32]) -> Result<(), StateError> {
        for x in out.iter_mut() {
            *x = self.u32()?;
        }
        Ok(())
    }
}

/// FNV-1a, 64 bit: cheap, and enough to catch truncation and accidental damage (this is not a
/// security boundary).
pub fn checksum(data: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in data {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_truncation() {
        let mut w = StateWriter::new();
        w.u8(7);
        w.bool(true);
        w.u16(0xBEEF);
        w.u32(0xDEAD_BEEF);
        w.u64(u64::MAX - 1);
        w.f32(-1.5);
        w.bytes(&[1, 2, 3]);
        let bytes = w.into_bytes();
        let mut r = StateReader::new(&bytes);
        assert_eq!(r.u8(), Ok(7));
        assert_eq!(r.bool(), Ok(true));
        assert_eq!(r.u16(), Ok(0xBEEF));
        assert_eq!(r.u32(), Ok(0xDEAD_BEEF));
        assert_eq!(r.u64(), Ok(u64::MAX - 1));
        assert_eq!(r.f32(), Ok(-1.5));
        let mut b = [0u8; 3];
        assert_eq!(r.bytes(&mut b), Ok(()));
        assert_eq!(b, [1, 2, 3]);
        assert_eq!(r.u8(), Err(StateError::Truncated));
    }

    #[test]
    fn bool_rejects_other_values() {
        let mut r = StateReader::new(&[2]);
        assert!(matches!(r.bool(), Err(StateError::Corrupt(_))));
    }
}
