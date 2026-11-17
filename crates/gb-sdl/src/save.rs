//! Battery save files: `<rom>.sav` beside the ROM (decision: simplest to find and back up; the
//! Steam Deck launches with an explicit ROM path so the directory is writable in practice).
//!
//! Writes are atomic: data goes to `<rom>.sav.tmp` and is renamed over the real file, so a
//! crash or power loss mid-write leaves either the old or the new save, never a torn one.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gb_core::GameBoy;

/// How often a changed save is flushed while playing.
pub const FLUSH_INTERVAL: Duration = Duration::from_secs(30);

/// Write `data` to `path` atomically. Missing parent folders (`saves/`) are created here, so
/// they appear only when the first file is written.
pub fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path)
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Seconds to add to a loaded RTC. Zero if the save has no timestamp or the clock went backwards.
pub fn elapsed_since(saved: u64, now: u64) -> u64 {
    if saved == 0 {
        0
    } else {
        now.saturating_sub(saved)
    }
}

/// Owns the save file of the running game.
pub struct SaveFile {
    path: PathBuf,
    enabled: bool,
    has_rtc: bool,
    last_written: Vec<u8>,
    last_flush: Instant,
}

impl SaveFile {
    /// A save file that never reads or writes anything. Used while recording an input script:
    /// the recording must start from a fresh boot so that a headless replay (which loads no
    /// save) reproduces it exactly, and a recording session must not overwrite a real save.
    pub fn disabled(path: &Path) -> SaveFile {
        SaveFile {
            path: path.to_path_buf(),
            enabled: false,
            has_rtc: false,
            last_written: Vec::new(),
            last_flush: Instant::now(),
        }
    }

    /// Load an existing save (if any) into `gb`. Problems are reported on stderr and never
    /// abort: a corrupt save is moved to `<rom>.sav.bad` instead of being overwritten.
    pub fn open(path: &Path, gb: &mut GameBoy) -> SaveFile {
        let path = path.to_path_buf();
        // `save_ram()` is empty exactly when the cartridge has no battery.
        let enabled = !gb.save_ram().is_empty();
        let mut file = SaveFile {
            path,
            enabled,
            has_rtc: gb.cartridge().has_rtc(),
            last_written: Vec::new(),
            last_flush: Instant::now(),
        };
        if !enabled {
            return file;
        }
        let expected = gb.save_ram().len();
        match fs::read(&file.path) {
            Ok(data) => {
                // The RTC footer may be 44 or 48 bytes, so accept a small range above RAM size.
                let plausible =
                    data.len() >= expected.saturating_sub(48) && data.len() <= expected + 48;
                if !plausible {
                    let bad = file.path.with_extension("sav.bad");
                    eprintln!(
                        "warning: {} has {} bytes, expected about {}. Not loading it; moved to {}.",
                        file.path.display(),
                        data.len(),
                        expected,
                        bad.display()
                    );
                    if let Err(e) = fs::rename(&file.path, &bad) {
                        eprintln!("warning: could not move the bad save aside: {e}");
                    }
                } else {
                    gb.load_save_ram(&data);
                    file.last_written = data;
                    if gb.cartridge().has_rtc() {
                        let saved = gb.cartridge().save_timestamp().unwrap_or(0);
                        let secs = elapsed_since(saved, unix_now());
                        gb.cartridge_mut().advance_rtc_seconds(secs);
                        eprintln!("RTC advanced by {secs} s of real time since the last session");
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => eprintln!("warning: cannot read {}: {e}", file.path.display()),
        }
        file
    }

    /// Write the save if it changed since the last write (`force` skips the 30 s timer).
    pub fn flush(&mut self, gb: &mut GameBoy, force: bool) {
        if !self.enabled || (!force && self.last_flush.elapsed() < FLUSH_INTERVAL) {
            return;
        }
        self.last_flush = Instant::now();
        if gb.cartridge().has_rtc() {
            gb.cartridge_mut().set_save_timestamp(unix_now());
        }
        let mut data = gb.save_ram();
        // The timestamp changes on every call; only RAM or clock-register changes count.
        if self.same_as_last(&data) {
            return;
        }
        if let Err(e) = write_atomic(&self.path, &data) {
            eprintln!("error: could not write {}: {e}", self.path.display());
            return;
        }
        std::mem::swap(&mut self.last_written, &mut data);
    }

    /// Compare ignoring the RTC timestamp (last 8 bytes of a 48-byte footer).
    fn same_as_last(&self, data: &[u8]) -> bool {
        let n = data.len();
        if n != self.last_written.len() {
            return false;
        }
        // With an RTC the final 8 bytes are the timestamp. A running RTC changes its seconds
        // anyway, so ignoring the stamp loses nothing.
        let end = if self.has_rtc && n >= 8 { n - 8 } else { n };
        data[..end] == self.last_written[..end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gb-sdl-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn write_creates_missing_folders_only_when_writing() {
        let dir = temp_dir("mkdir");
        assert!(!dir.join("saves").exists());
        write_atomic(&dir.join("saves/g.gb.sav"), b"x").unwrap();
        assert_eq!(fs::read(dir.join("saves/g.gb.sav")).unwrap(), b"x");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn atomic_write_replaces_file_and_leaves_no_tmp() {
        let dir = temp_dir("atomic");
        let p = dir.join("g.gb.sav");
        write_atomic(&p, b"one").unwrap();
        write_atomic(&p, b"two!").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"two!");
        assert!(!dir.join("g.gb.sav.tmp").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn elapsed_time_handles_unknown_and_backwards_clock() {
        assert_eq!(elapsed_since(0, 5000), 0);
        assert_eq!(elapsed_since(1000, 1600), 600);
        assert_eq!(elapsed_since(2000, 1000), 0);
    }
}
