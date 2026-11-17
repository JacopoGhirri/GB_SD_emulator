//! Files the frontend writes beside the ROM: save-state slots and screenshots.
//! (Battery saves live in `save.rs`.) All of them are gitignored.

use std::path::{Path, PathBuf};

use gb_core::GameBoy;

use crate::save::write_atomic;

/// Number of quick-save slots, numbered from 1.
pub const SLOTS: u8 = 9;

/// `<rom>.state<N>`, e.g. `tetris.gb.state1`.
pub fn state_path(rom: &Path, slot: u8) -> PathBuf {
    let mut s = rom.as_os_str().to_owned();
    s.push(format!(".state{slot}"));
    PathBuf::from(s)
}

pub fn next_slot(slot: u8) -> u8 {
    slot % SLOTS + 1
}

/// Returns the size written.
pub fn save_state(gb: &GameBoy, rom: &Path, slot: u8) -> Result<usize, String> {
    let data = gb.save_state();
    let path = state_path(rom, slot);
    write_atomic(&path, &data).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(data.len())
}

/// A failed load leaves the machine unchanged (`GameBoy::load_state` restores a backup).
pub fn load_state(gb: &mut GameBoy, rom: &Path, slot: u8) -> Result<(), String> {
    let path = state_path(rom, slot);
    let data = std::fs::read(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!("slot {slot} is empty")
        } else {
            format!("cannot read {}: {e}", path.display())
        }
    })?;
    gb.load_state(&data)
        .map_err(|e| format!("slot {slot} cannot be loaded: {e}"))
}

/// First unused `<rom>-shot-NNN.png`.
pub fn next_screenshot_path(rom: &Path) -> PathBuf {
    (1..)
        .map(|n| {
            let mut s = rom.as_os_str().to_owned();
            s.push(format!("-shot-{n:03}.png"));
            PathBuf::from(s)
        })
        .find(|p| !p.exists())
        .expect("an unused screenshot number exists")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_path_appends_the_slot() {
        assert_eq!(
            state_path(Path::new("roms/a.gb"), 3),
            PathBuf::from("roms/a.gb.state3")
        );
    }

    #[test]
    fn slots_cycle_from_the_last_back_to_one() {
        assert_eq!(next_slot(1), 2);
        assert_eq!(next_slot(SLOTS), 1);
    }
}
