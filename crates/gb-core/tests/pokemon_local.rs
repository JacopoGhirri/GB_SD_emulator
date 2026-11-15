//! Local-only game replay tests (phase 5). Never runs in CI: the ROMs are the owner's own.
//!
//! For every `tests/expected/local/<name>.hashes` it looks for `roms/<name>.gb` and
//! `tests/expected/local/<name>.input` (an input script recorded with `gb-sdl --record`), replays
//! the script from a fresh boot and compares the framebuffer hash at each listed frame count.
//! Each line of a `.hashes` file is `<frames> <hex hash>`. Hashes are added only after the owner
//! has looked at the screenshots and approved them. Anything missing => SKIP, not failure.

use std::path::{Path, PathBuf};

use gb_core::input_script::InputScript;
use gb_core::{Cartridge, GameBoy, Model};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `<frames> <hash>` lines; `#` comments allowed.
fn parse_hashes(text: &str) -> Result<Vec<(u64, u64)>, String> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bad = || format!("line {}: expected \"<frames> <hex hash>\"", i + 1);
        let mut f = line.split_whitespace();
        let frames = f.next().and_then(|s| s.parse().ok()).ok_or_else(bad)?;
        let hash = f
            .next()
            .and_then(|s| u64::from_str_radix(s, 16).ok())
            .ok_or_else(bad)?;
        out.push((frames, hash));
    }
    Ok(out)
}

#[test]
fn hash_file_parser() {
    let v = parse_hashes("# c\n300 00ff\n\n600 deadbeef\n").unwrap();
    assert_eq!(v, vec![(300, 0xff), (600, 0xdead_beef)]);
    assert!(parse_hashes("x y").is_err());
}

#[test]
fn approved_replays_match() {
    let root = workspace_root();
    let dir = root.join("tests/expected/local");
    let mut ran = 0;
    let entries = std::fs::read_dir(&dir).map(|d| d.flatten().collect::<Vec<_>>());
    for entry in entries.unwrap_or_default() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("hashes") {
            continue;
        }
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let rom_path = root.join("roms").join(format!("{name}.gb"));
        let input_path = dir.join(format!("{name}.input"));
        let (Ok(rom), Ok(input)) = (
            std::fs::read(&rom_path),
            std::fs::read_to_string(&input_path),
        ) else {
            println!(
                "SKIP: {name}: need {} and {}",
                rom_path.display(),
                input_path.display()
            );
            continue;
        };
        let script = InputScript::parse(&input).expect("input script parses");
        let expected = parse_hashes(&std::fs::read_to_string(&path).unwrap()).expect("hash file");
        let last = expected.iter().map(|&(f, _)| f).max().unwrap_or(0);
        let mut gb = GameBoy::new(Cartridge::from_bytes(rom).expect("ROM loads"), Model::Dmg);
        for frame in 0..last {
            gb.set_buttons(script.buttons_at(frame));
            gb.run_frame();
            let done = frame + 1;
            for &(f, want) in expected.iter().filter(|&&(f, _)| f == done) {
                assert_eq!(
                    gb.framebuffer_hash(),
                    want,
                    "{name}: frame {f} no longer matches the approved hash"
                );
            }
        }
        println!(
            "{name}: {} hashes match after {last} frames",
            expected.len()
        );
        ran += 1;
    }
    if ran == 0 {
        println!("SKIP: no approved local replays yet (tests/expected/local/*.hashes)");
    }
}
