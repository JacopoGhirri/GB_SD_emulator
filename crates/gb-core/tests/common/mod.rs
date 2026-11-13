//! Headless ROM-test harness shared by integration tests.
//!
//! Tests call [`run_rom`] with a path relative to `tests/roms/`. If the ROM is absent the
//! test prints a SKIP line and continues, so a missing download never fails `cargo test`
//! (CLAUDE.md §6). Run `cargo test -- --nocapture` to see SKIP lines.

// Each integration test binary compiles this module separately, so not every helper is
// used by every binary.
#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;

use gb_core::{Cartridge, GameBoy, Model};

/// Where `scripts/fetch_test_roms.sh` puts the pinned bundle, relative to this crate.
pub const ROM_ROOT: &str = "../../tests/roms/game-boy-test-roms-v7.0";

/// Limits that stop a run. The first one reached ends the run.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_instructions: u64,
}

/// What a finished run looks like. Enough to explain a failure without rerunning.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// All bytes the ROM sent over the serial port.
    pub serial: Vec<u8>,
    /// Final frame, 160*144 pixels, 0x00RRGGBB.
    pub framebuffer: Vec<u32>,
    /// Program counter when the run stopped.
    pub pc: u16,
    /// Number of CPU instructions executed.
    pub instructions: u64,
}

impl Outcome {
    /// Serial output as text (lossy). Blargg ROMs print their verdict here.
    pub fn serial_text(&self) -> String {
        String::from_utf8_lossy(&self.serial).into_owned()
    }

    /// One-paragraph summary for assertion messages: serial tail, PC and instruction count.
    pub fn describe(&self) -> String {
        const TAIL: usize = 256;
        let start = self.serial.len().saturating_sub(TAIL);
        format!(
            "stopped after {} instructions at PC=0x{:04X}\nlast serial output:\n{}",
            self.instructions,
            self.pc,
            String::from_utf8_lossy(&self.serial[start..])
        )
    }
}

/// Absolute path of a ROM given relative to [`ROM_ROOT`].
pub fn rom_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(ROM_ROOT)
        .join(relative)
}

/// Run a ROM headlessly for up to `limits.max_instructions` instructions.
///
/// Returns `None` (after printing a SKIP line) when the ROM file is not present.
/// Panics only on a bug in the harness or the core, never on a missing or bad file:
/// a file that exists but is not a valid ROM is reported as a test failure.
pub fn run_rom(relative: &str, limits: Limits) -> Option<Outcome> {
    run_rom_until(relative, limits, &[])
}

/// Like [`run_rom`], but also stops as soon as the serial output contains any of `stop_texts`
/// (Blargg ROMs print "Passed" or "Failed" and then loop forever).
pub fn run_rom_until(relative: &str, limits: Limits, stop_texts: &[&str]) -> Option<Outcome> {
    let path = rom_path(relative);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(_) => {
            println!(
                "SKIP: {} not found. Run scripts/fetch_test_roms.sh to download it.",
                path.display()
            );
            return None;
        }
    };
    let cart = Cartridge::from_bytes(bytes)
        .unwrap_or_else(|e| panic!("{} is not a valid ROM: {e}", path.display()));

    let mut gb = GameBoy::new(cart, Model::Dmg);
    let mut serial = Vec::new();
    let mut instructions = 0u64;
    while instructions < limits.max_instructions {
        gb.step();
        instructions += 1;
        let new = gb.take_serial();
        if !new.is_empty() {
            serial.extend(new);
            let text = String::from_utf8_lossy(&serial);
            if stop_texts.iter().any(|s| text.contains(s)) {
                break;
            }
        }
    }

    Some(Outcome {
        serial,
        framebuffer: gb.framebuffer().to_vec(),
        pc: gb.pc(),
        instructions,
    })
}
