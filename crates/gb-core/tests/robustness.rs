//! Bad input must give an error (or be played harmlessly), never a panic and never a hang.
//!
//! Everything is deterministic: a fixed-seed pseudo-random generator, and every emulation loop
//! has a hard cap on the number of CPU steps, so "hang" would show up as a test that is too
//! slow, not as an endless run. Covered: random bytes and damaged ROMs (including headers that
//! lie about the ROM size and cartridge RAM), every cartridge-type byte, corrupt battery saves,
//! and corrupt save-state files. No ROM files are needed.

use gb_core::{Buttons, Cartridge, CartridgeError, GameBoy, Model};

/// xorshift64*: tiny, fixed-seed, good enough to scatter bytes.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn byte(&mut self) -> u8 {
        (self.next() >> 32) as u8
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.byte()).collect()
    }
}

/// Every cartridge type the emulator claims to support.
const SUPPORTED: [u8; 15] = [
    0x00, 0x01, 0x02, 0x03, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E,
];
/// Types with a battery (their save files are read at start-up).
const BATTERY: [u8; 8] = [0x03, 0x0F, 0x10, 0x13, 0x1B, 0x1E, 0x02, 0x12];

/// A ROM of `len` bytes whose header claims the given type and size codes. The header may lie
/// about the real length on purpose. Code at 0x100 jumps to a tiny loop that touches the
/// mapper and RAM, so even a clean ROM exercises the banking code.
fn rom(cart_type: u8, rom_code: u8, ram_code: u8, cgb_flag: u8, len: usize) -> Vec<u8> {
    let mut r = vec![0u8; len.max(0x150 + 32)];
    // 0x100: jp 0x0150
    r[0x100..0x103].copy_from_slice(&[0xC3, 0x50, 0x01]);
    r[0x143] = cgb_flag;
    r[0x147] = cart_type;
    r[0x148] = rom_code;
    r[0x149] = ram_code;
    // 0x150: enable RAM, switch banks, write and read RAM, loop forever.
    let code = [
        0x3E, 0x0A, 0xEA, 0x00, 0x00, // ld a,$0A ; ld ($0000),a   RAM enable
        0x3E, 0x03, 0xEA, 0x00, 0x20, // ld a,3 ; ld ($2000),a     ROM bank
        0x3E, 0x01, 0xEA, 0x00, 0x40, // ld a,1 ; ld ($4000),a     RAM bank
        0x3E, 0x5A, 0xEA, 0x00, 0xA0, // ld a,$5A ; ld ($A000),a
        0xFA, 0x00, 0xA0, // ld a,($A000)
        0x18, 0xE5, // jr back to the RAM enable
    ];
    r[0x150..0x150 + code.len()].copy_from_slice(&code);
    r
}

/// Load `bytes` and, if the cartridge is accepted, play it for a bounded number of steps with
/// random buttons and random mapper accesses. Returns whether it was accepted.
fn exercise(bytes: Vec<u8>, model: Model, steps: usize, rng: &mut Rng) -> bool {
    let Ok(cart) = Cartridge::from_bytes(bytes) else {
        return false;
    };
    let mut gb = GameBoy::new(cart, model);
    for i in 0..steps {
        gb.step();
        if i % 4_000 == 0 {
            gb.set_buttons(Buttons(rng.byte()));
            // Direct mapper traffic over the whole ROM-control and RAM windows.
            let c = gb.cartridge_mut();
            for _ in 0..4 {
                c.write_rom(rng.below(0x8000) as u16, rng.byte());
                let _ = c.read_rom(rng.below(0x8000) as u16);
                c.write_ram(0xA000 + rng.below(0x2000) as u16, rng.byte());
                let _ = c.read_ram(0xA000 + rng.below(0x2000) as u16);
            }
        }
    }
    // Saving and restoring must also be safe in whatever state the run ended up in.
    let save = gb.save_ram();
    gb.load_save_ram(&save);
    let state = gb.save_state();
    gb.load_state(&state).expect("a state we just made loads");
    true
}

#[test]
fn rom_files_too_short_for_a_header_are_rejected() {
    for len in 0..0x150 {
        match Cartridge::from_bytes(vec![0u8; len]) {
            Err(CartridgeError::TooSmall { .. }) => {}
            other => panic!("{len} bytes: expected TooSmall, got {other:?}"),
        }
    }
}

#[test]
fn every_cartridge_type_byte_is_either_played_or_refused() {
    let mut rng = Rng(1);
    for t in 0..=255u8 {
        let ok = exercise(
            rom(t, 0x00, 0x00, 0x00, 0x8000),
            Model::Dmg,
            5_000,
            &mut rng,
        );
        assert_eq!(
            ok,
            SUPPORTED.contains(&t),
            "type {t:#04x}: accepted = {ok}, expected {}",
            SUPPORTED.contains(&t)
        );
    }
}

#[test]
fn random_bytes_as_a_rom_never_panic() {
    let mut rng = Rng(0xDEAD_BEEF);
    let (mut accepted, mut rejected) = (0, 0);
    for i in 0..400 {
        let len = rng.below(70_000);
        let bytes = rng.bytes(len);
        let model = if i % 2 == 0 { Model::Dmg } else { Model::Cgb };
        if exercise(bytes, model, 30_000, &mut rng) {
            accepted += 1;
        } else {
            rejected += 1;
        }
    }
    // Pure noise almost never has a valid header; that is the point, they are refused.
    assert_eq!(accepted + rejected, 400);
    assert!(
        rejected > 300,
        "only {rejected} of 400 noise ROMs were refused"
    );
}

#[test]
fn damaged_and_lying_roms_never_panic() {
    let mut rng = Rng(0x00C0_FFEE);
    let mut accepted = 0;
    for i in 0..600 {
        let cart_type = SUPPORTED[rng.below(SUPPORTED.len())];
        // Declared ROM size code (valid ones and junk), RAM size code, real file length.
        let rom_code = [0, 1, 2, 3, 4, 5, 6, 7, 8, 0x52, 0x53, 0x54, 0x09][rng.below(13)];
        let ram_code = rng.below(8) as u8;
        let len = [0x150, 0x4000, 0x8000, 0x8001, 0x20000, 0x80000][rng.below(6)] + rng.below(300);
        let cgb = [0x00, 0x80, 0xC0, rng.byte()][rng.below(4)];
        let mut bytes = rom(cart_type, rom_code, ram_code, cgb, len);
        // Random damage: noise as "code" after the header, and single-byte flips anywhere.
        if rng.below(2) == 0 {
            let start = 0x150 + 40;
            if start < bytes.len() {
                let n = (bytes.len() - start).min(0x4000);
                for b in &mut bytes[start..start + n] {
                    *b = rng.byte();
                }
            }
        }
        for _ in 0..rng.below(8) {
            let at = rng.below(bytes.len());
            bytes[at] = rng.byte();
        }
        let model = if i % 3 == 0 { Model::Cgb } else { Model::Dmg };
        if exercise(bytes, model, 40_000, &mut rng) {
            accepted += 1;
        }
    }
    assert!(
        accepted > 100,
        "only {accepted} damaged ROMs were playable; the test is not reaching the mapper code"
    );
}

#[test]
fn corrupt_battery_saves_never_panic_and_come_back_usable() {
    let mut rng = Rng(0x5A5A);
    for &t in &BATTERY {
        for ram_code in [0u8, 1, 2, 3, 4, 5] {
            let bytes = rom(t, 0x00, ram_code, 0x00, 0x8000);
            let Ok(cart) = Cartridge::from_bytes(bytes.clone()) else {
                continue; // an unknown RAM size code is refused, which is fine
            };
            let expected = cart.save_data().len();
            // Sizes around every boundary the loader cares about (RAM, RTC footer 44/48 bytes).
            let mut lens = vec![0, 1, 7, 47, 100, expected / 2, 70_000, 1 << 20];
            for d in [0usize, 1, 2, 43, 44, 45, 47, 48, 49] {
                lens.push(expected.saturating_sub(d));
                lens.push(expected + d);
            }
            for len in lens {
                let mut gb =
                    GameBoy::new(Cartridge::from_bytes(bytes.clone()).unwrap(), Model::Dmg);
                gb.load_save_ram(&rng.bytes(len));
                for _ in 0..20_000 {
                    gb.step();
                }
                // The result of loading garbage can be saved and loaded again without trouble.
                let again = gb.save_ram();
                gb.load_save_ram(&again);
                assert_eq!(
                    gb.save_ram().len(),
                    again.len(),
                    "type {t:#04x}, loaded {len} bytes"
                );
            }
        }
    }
}

#[test]
fn clock_footers_with_absurd_values_do_not_panic() {
    // MBC3+TIMER: all-0xFF and all-0x00 footers, huge timestamps, then let time pass.
    let bytes = rom(0x10, 0x00, 0x03, 0x00, 0x8000);
    for fill in [0x00u8, 0xFF, 0x7F, 0x80] {
        let mut gb = GameBoy::new(Cartridge::from_bytes(bytes.clone()).unwrap(), Model::Dmg);
        let n = gb.save_ram().len();
        gb.load_save_ram(&vec![fill; n]);
        gb.cartridge_mut().advance_rtc_seconds(u64::MAX);
        gb.cartridge_mut().advance_rtc_seconds(1);
        gb.cartridge_mut().set_save_timestamp(u64::MAX);
        for _ in 0..50_000 {
            gb.step();
        }
        let _ = gb.save_ram();
    }
}

#[test]
fn random_and_truncated_state_files_are_errors_not_panics() {
    let mut rng = Rng(0x57A7E);
    let rom_bytes = rom(0x03, 0x00, 0x02, 0x00, 0x8000);
    let mut gb = GameBoy::new(Cartridge::from_bytes(rom_bytes).unwrap(), Model::Dmg);
    for _ in 0..30_000 {
        gb.step();
    }
    let good = gb.save_state();

    // Pure noise, with and without the right 4-byte magic at the front.
    for i in 0..300 {
        let len = rng.below(1_000);
        let mut data = rng.bytes(len);
        if i % 2 == 0 && data.len() >= 4 {
            data[..4].copy_from_slice(b"GBST");
        }
        assert!(gb.load_state(&data).is_err(), "noise #{i} was accepted");
    }
    // Every prefix of a real state (length 0 up to one byte short) is refused.
    let step = (good.len() / 500).max(1);
    for cut in (0..good.len()).step_by(step) {
        assert!(
            gb.load_state(&good[..cut]).is_err(),
            "prefix of {cut} bytes was accepted"
        );
    }
    // A real state with a few bytes flipped is refused (the checksum catches it) or, at worst,
    // loads; it must never panic. The machine keeps working either way.
    for _ in 0..300 {
        let mut data = good.clone();
        for _ in 0..1 + rng.below(4) {
            let at = rng.below(data.len());
            data[at] = rng.byte();
        }
        let _ = gb.load_state(&data);
        for _ in 0..200 {
            gb.step();
        }
    }
    // The untouched state still loads after all that abuse.
    gb.load_state(&good).expect("the original state loads");
}
