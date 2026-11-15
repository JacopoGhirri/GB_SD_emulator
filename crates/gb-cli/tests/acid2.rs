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

/// scribbltests that ship a DMG-valid reference screenshot (PPU register timing per line).
/// (rom directory, rom file, reference png) relative to `scribbltests/`.
const SCRIBBL: &[(&str, &str, &str)] = &[
    ("lycscx", "lycscx.gb", "lycscx-cgb-dmg.png"),
    ("lycscy", "lycscy.gb", "lycscy-cgb-dmg.png"),
    ("scxly", "scxly.gb", "scxly-dmg.png"),
    ("palettely", "palettely.gb", "palettely-dmg.png"),
];

#[test]
fn scribbltests_match_their_references() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/roms/game-boy-test-roms-v7.0/scribbltests");
    for (dir, rom, png) in SCRIBBL {
        let (rom, png) = (root.join(dir).join(rom), root.join(dir).join(png));
        if !rom.exists() || !png.exists() {
            println!("SKIP: scribbltests/{dir} not found. Run scripts/fetch_test_roms.sh.");
            continue;
        }
        let out = Command::new(env!("CARGO_BIN_EXE_gb-cli"))
            .arg("run")
            .arg(&rom)
            .args(["--frames", "120", "--compare"])
            .arg(&png)
            .output()
            .expect("failed to run gb-cli");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        println!("scribbltests/{dir}: {}", text.lines().last().unwrap_or(""));
        assert!(out.status.success(), "scribbltests/{dir} differs:\n{text}");
    }
}

fn run_cgb_acid2(frames: &str) -> Option<std::process::Output> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/roms/game-boy-test-roms-v7.0/cgb-acid2");
    let (rom, reference) = (dir.join("cgb-acid2.gbc"), dir.join("cgb-acid2.png"));
    if !rom.exists() || !reference.exists() {
        println!("SKIP: cgb-acid2 not found. Run scripts/fetch_test_roms.sh to download it.");
        return None;
    }
    // `--model cgb` is explicit even though the header (0xC0) would pick it.
    Some(
        Command::new(env!("CARGO_BIN_EXE_gb-cli"))
            .arg("run")
            .arg(&rom)
            .args(["--model", "cgb", "--frames", frames, "--compare"])
            .arg(&reference)
            .output()
            .expect("failed to run gb-cli"),
    )
}

#[test]
fn cgb_acid2_matches_reference() {
    let Some(out) = run_cgb_acid2("30") else {
        return;
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    println!("{stdout}{stderr}");
    assert!(
        out.status.success(),
        "cgb-acid2 differs from reference:\n{stdout}{stderr}"
    );
    assert!(stdout.contains("compare passed"));
}

/// Control: a 1-frame screenshot must NOT match, so the comparison can fail.
#[test]
fn cgb_acid2_comparison_detects_a_wrong_frame() {
    let Some(out) = run_cgb_acid2("1") else {
        return;
    };
    assert!(!out.status.success());
}
