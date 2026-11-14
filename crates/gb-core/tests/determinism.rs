//! Determinism smoke test (phase 4): the same ROM and inputs must give the same framebuffer.
//! Runs 600 frames of freely distributable test ROMs twice, with button presses mixed in, and
//! compares an FNV-1a hash of every 60th frame plus the last.

mod common;

use gb_core::{Buttons, Cartridge, GameBoy, Model};

fn fnv1a(frame: &[u32]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for px in frame {
        for b in px.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h
}

fn run(rom: Vec<u8>) -> Vec<u64> {
    let cart = Cartridge::from_bytes(rom).expect("test ROM loads");
    let mut gb = GameBoy::new(cart, Model::Dmg);
    let mut hashes = Vec::new();
    for frame in 0..600u32 {
        // Scripted input: tap A and Right now and then, so input handling is covered too.
        let held = match frame % 90 {
            10..=19 => Buttons::A,
            40..=59 => Buttons::RIGHT,
            _ => 0,
        };
        gb.set_buttons(Buttons(held));
        gb.run_frame();
        if frame % 60 == 59 {
            hashes.push(fnv1a(gb.framebuffer()));
        }
    }
    hashes
}

fn check(relative: &str) {
    let path = common::rom_path(relative);
    let Ok(rom) = std::fs::read(&path) else {
        println!(
            "SKIP: {} not found. Run scripts/fetch_test_roms.sh to download it.",
            path.display()
        );
        return;
    };
    let first = run(rom.clone());
    let second = run(rom);
    assert_eq!(first, second, "{relative}: two runs differ");
    println!("{relative}: 600 frames x2 identical, hashes {first:016x?}");
}

#[test]
fn dmg_acid2_is_deterministic() {
    check("dmg-acid2/dmg-acid2.gb");
}

#[test]
fn scribbltests_fairylake_is_deterministic() {
    // Animated, so successive frame hashes differ: a stronger check than a static screen.
    check("scribbltests/fairylake/fairylake.gb");
}
