//! Screenshot test: dmg-acid2 must match its reference image pixel for pixel.
//!
//! Runs the real `gb-cli` binary (`run <rom> --frames N --compare <png>`), so the whole path
//! ROM -> core -> framebuffer -> PNG comparison is exercised. Skips with a message when the test
//! ROMs have not been fetched (CLAUDE.md section 6). Use `-- --nocapture` to see SKIP lines.

use std::path::PathBuf;
use std::process::Command;

fn rom_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/roms/game-boy-test-roms-v7.0/dmg-acid2")
}

#[test]
fn dmg_acid2_matches_reference() {
    let dir = rom_dir();
    let rom = dir.join("dmg-acid2.gb");
    let reference = dir.join("dmg-acid2-dmg.png");
    if !rom.exists() || !reference.exists() {
        println!("SKIP: dmg-acid2 not found. Run scripts/fetch_test_roms.sh to download it.");
        return;
    }
    let out = Command::new(env!("CARGO_BIN_EXE_gb-cli"))
        .arg("run")
        .arg(&rom)
        .args(["--frames", "10", "--compare"])
        .arg(&reference)
        .output()
        .expect("failed to run gb-cli");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    println!("{stdout}{stderr}");
    assert!(
        out.status.success(),
        "dmg-acid2 differs from reference:\n{stdout}{stderr}"
    );
    assert!(stdout.contains("compare passed"));
}

/// The comparison itself must be able to fail: after one frame the screen is still the startup
/// state, so it must NOT match. Guards against a `--compare` that always passes.
#[test]
fn dmg_acid2_comparison_detects_a_wrong_frame() {
    let dir = rom_dir();
    let rom = dir.join("dmg-acid2.gb");
    let reference = dir.join("dmg-acid2-dmg.png");
    if !rom.exists() || !reference.exists() {
        println!("SKIP: dmg-acid2 not found. Run scripts/fetch_test_roms.sh to download it.");
        return;
    }
    let out = Command::new(env!("CARGO_BIN_EXE_gb-cli"))
        .arg("run")
        .arg(&rom)
        .args(["--frames", "1", "--compare"])
        .arg(&reference)
        .output()
        .expect("failed to run gb-cli");
    assert!(
        !out.status.success(),
        "a 1-frame screenshot must not match the reference"
    );
}
