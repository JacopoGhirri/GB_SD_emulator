//! Save-state slots (the paths themselves come from `layout`, which decides where files go).

use std::path::Path;

use gb_core::GameBoy;

use crate::layout::Layout;
use crate::save::write_atomic;

/// Number of quick-save slots, numbered from 1.
pub const SLOTS: u8 = 9;

pub fn next_slot(slot: u8) -> u8 {
    slot % SLOTS + 1
}

/// Returns the size written.
pub fn save_state(gb: &GameBoy, layout: &Layout, rom: &Path, slot: u8) -> Result<usize, String> {
    let data = gb.save_state();
    let path = layout.state_path(rom, slot);
    write_atomic(&path, &data).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(data.len())
}

/// A failed load leaves the machine unchanged (`GameBoy::load_state` restores a backup).
pub fn load_state(gb: &mut GameBoy, layout: &Layout, rom: &Path, slot: u8) -> Result<(), String> {
    let path = layout.state_path(rom, slot);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_cycle_from_the_last_back_to_one() {
        assert_eq!(next_slot(1), 2);
        assert_eq!(next_slot(SLOTS), 1);
    }
}
