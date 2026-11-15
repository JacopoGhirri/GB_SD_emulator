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

/// Run one Mooneye ROM (path relative to [`ROM_ROOT`]) to its `LD B,B` verdict.
/// `None` (after a SKIP line) if the ROM is absent.
pub fn run_mooneye_rom(
    relative: &str,
    max_instructions: u64,
) -> Option<(gb_core::mooneye::MooneyeVerdict, u64)> {
    let path = rom_path(relative);
    let bytes = fs::read(&path).ok()?;
    let cart = Cartridge::from_bytes(bytes)
        .unwrap_or_else(|e| panic!("{} is not a valid ROM: {e}", path.display()));
    let mut gb = GameBoy::new(cart, Model::Dmg);
    Some(gb_core::mooneye::run_mooneye(&mut gb, max_instructions))
}

/// Run every DMG-compatible `.gb` ROM directly inside `dir` (relative to [`ROM_ROOT`]) and
/// return `(name, verdict text)` per ROM; ROMs for other hardware are listed as "skipped".
/// Returns `None` with a SKIP line if the directory is absent.
pub fn mooneye_sweep(dir: &str, max_instructions: u64) -> Option<Vec<(String, bool, String)>> {
    let root = rom_path(dir);
    let Ok(entries) = fs::read_dir(&root) else {
        println!(
            "SKIP: {} not found. Run scripts/fetch_test_roms.sh.",
            root.display()
        );
        return None;
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.ends_with(".gb"))
        .collect();
    names.sort();
    let mut out = Vec::new();
    for name in names {
        if !gb_core::mooneye::dmg_compatible(&name) {
            println!("{dir}/{name}: skipped (other hardware)");
            continue;
        }
        let (verdict, n) = run_mooneye_rom(&format!("{dir}/{name}"), max_instructions)?;
        let pass = verdict == gb_core::mooneye::MooneyeVerdict::Pass;
        let text = format!("{verdict:?} after {n} instructions");
        println!(
            "{dir}/{name}: {}  ({text})",
            if pass { "PASS" } else { "FAIL" }
        );
        out.push((name, pass, text));
    }
    Some(out)
}

/// Newer Blargg ROMs (e.g. `mem_timing-2`) report through cartridge RAM instead of the serial
/// port: 0xA001..=0xA003 hold the signature DE B0 61, 0xA000 holds the status (0x80 = still
/// running, 0x00 = passed, anything else = failure code) and 0xA004.. a NUL-terminated text.
/// Runs until a finished status appears; returns `(status, text, instructions)`.
/// `None` (after a SKIP line) if the ROM is absent. `status == 0x80` means it never finished.
pub fn run_blargg_ram(relative: &str, max_instructions: u64) -> Option<(u8, String, u64)> {
    let path = rom_path(relative);
    let Ok(bytes) = fs::read(&path) else {
        println!(
            "SKIP: {} not found. Run scripts/fetch_test_roms.sh.",
            path.display()
        );
        return None;
    };
    let cart = Cartridge::from_bytes(bytes)
        .unwrap_or_else(|e| panic!("{} is not a valid ROM: {e}", path.display()));
    let mut gb = GameBoy::new(cart, Model::Dmg);
    let mut n = 0u64;
    let mut status = 0x80;
    while n < max_instructions {
        gb.step();
        n += 1;
        if n.is_multiple_of(4096) {
            let b = gb.bus();
            if [0xDE, 0xB0, 0x61] == [b.peek(0xA001), b.peek(0xA002), b.peek(0xA003)] {
                status = b.peek(0xA000);
                if status != 0x80 {
                    break;
                }
            }
        }
    }
    let b = gb.bus();
    let text: Vec<u8> = (0xA004u16..0xA404)
        .map(|a| b.peek(a))
        .take_while(|&c| c != 0)
        .collect();
    Some((status, String::from_utf8_lossy(&text).into_owned(), n))
}
