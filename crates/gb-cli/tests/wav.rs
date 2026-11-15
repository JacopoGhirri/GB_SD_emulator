//! `--wav` must write a valid, non-silent WAV for a ROM that makes sound (Blargg dmg_sound
//! 03-trigger beeps through the square channels). Skips if the test ROMs are absent.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn wav_dump_is_valid_and_not_silent() {
    let rom = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "../../tests/roms/game-boy-test-roms-v7.0/blargg/dmg_sound/rom_singles/03-trigger.gb",
    );
    if !rom.exists() {
        println!("SKIP: dmg_sound ROMs not found. Run scripts/fetch_test_roms.sh.");
        return;
    }
    let wav_path = std::env::temp_dir().join("gb-cli-wav-test.wav");
    let out = Command::new(env!("CARGO_BIN_EXE_gb-cli"))
        .arg("run")
        .arg(&rom)
        .args(["--frames", "120", "--wav"])
        .arg(&wav_path)
        .output()
        .expect("failed to run gb-cli");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let wav = std::fs::read(&wav_path).expect("wav file written");
    let _ = std::fs::remove_file(&wav_path);
    assert_eq!(&wav[0..4], b"RIFF");
    assert_eq!(&wav[8..12], b"WAVE");
    let samples: Vec<i16> = wav[44..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| i16::from_le_bytes(*b))
        .collect();
    // 120 frames at 59.73 Hz = about 2.0 s of 48 kHz stereo.
    assert!(samples.len() / 2 > 90_000, "{} frames", samples.len() / 2);
    assert!(samples.iter().any(|&s| s.abs() > 500), "audio is silent");
}
