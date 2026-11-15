//! Audio must be deterministic (CLAUDE.md section 4.1): the same ROM run twice produces
//! bit-identical samples. Uses Blargg dmg_sound 03-trigger, which makes real sound.
//! Skips if the ROM is absent.

mod common;

use gb_core::{Cartridge, GameBoy, Model};

fn render(bytes: &[u8]) -> Vec<f32> {
    let cart = Cartridge::from_bytes(bytes.to_vec()).expect("valid ROM");
    let mut gb = GameBoy::new(cart, Model::Dmg);
    let mut out = Vec::new();
    for _ in 0..180 {
        gb.run_frame();
        gb.drain_audio(&mut out);
    }
    out
}

#[test]
fn same_rom_gives_identical_audio() {
    let path = common::rom_path("blargg/dmg_sound/rom_singles/03-trigger.gb");
    let Ok(bytes) = std::fs::read(&path) else {
        println!("SKIP: {} not found.", path.display());
        return;
    };
    let (a, b) = (render(&bytes), render(&bytes));
    assert!(a.iter().any(|s| s.abs() > 0.01), "expected audible output");
    assert_eq!(a.len(), b.len());
    assert!(
        a.iter().zip(&b).all(|(x, y)| x.to_bits() == y.to_bits()),
        "audio differs between two identical runs"
    );
    println!("audio determinism: {} samples identical", a.len());
}
