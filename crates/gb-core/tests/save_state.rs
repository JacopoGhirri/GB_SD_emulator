//! Save-state tests (phase 8, task B4).
//!
//! The core idea: save, run N frames, load, run N frames => identical output. A field that the
//! serializer forgets shows up as a *subtle* divergence much later, so the tests save at the
//! awkward moments where hidden state is live: mid-frame, in the middle of an OAM DMA, in the
//! middle of an HBlank VRAM DMA, in double speed, with the MBC3 clock running, and with sound
//! playing on all channels. The programs are small hand-assembled ROMs that exercise those
//! units continuously; no third-party ROM is needed (so these tests always run).
//!
//! A long-run check on the owner's Pokemon Red recording runs only when the ROM is present.

use gb_core::input_script::InputScript;
use gb_core::state::checksum;
use gb_core::{Buttons, Cartridge, GameBoy, Model, StateError};
use std::path::{Path, PathBuf};

// ------------------------------------------------------------------ test programs

/// Tiny assembler: raw bytes plus a position, enough for loops and jumps.
struct Asm {
    code: Vec<u8>,
}

impl Asm {
    const BASE: u16 = 0x0150;

    fn new() -> Asm {
        Asm { code: Vec::new() }
    }
    fn b(&mut self, bytes: &[u8]) -> &mut Asm {
        self.code.extend_from_slice(bytes);
        self
    }
    /// `LD A,v ; LDH (reg),A`
    fn ldh(&mut self, reg: u8, v: u8) -> &mut Asm {
        self.b(&[0x3E, v, 0xE0, reg])
    }
    /// `LD A,v ; LD (addr),A`
    fn st(&mut self, addr: u16, v: u8) -> &mut Asm {
        self.b(&[0x3E, v, 0xEA, addr as u8, (addr >> 8) as u8])
    }
    fn here(&self) -> u16 {
        Asm::BASE + self.code.len() as u16
    }
    fn jp(&mut self, target: u16) -> &mut Asm {
        self.b(&[0xC3, target as u8, (target >> 8) as u8])
    }
    fn jr_back(&mut self, op: u8, target: u16) -> &mut Asm {
        let rel = i32::from(target) - i32::from(self.here() + 2);
        self.b(&[op, rel as i8 as u8])
    }
}

/// Build a 32 KiB ROM: header fields plus `JP 0x0150` at the entry point.
fn rom_with(cart_type: u8, cgb_flag: u8, ram_code: u8, code: &[u8]) -> Vec<u8> {
    let mut rom = vec![0u8; 0x8000];
    rom[0x100..0x103].copy_from_slice(&[0xC3, 0x50, 0x01]);
    rom[0x134..0x13A].copy_from_slice(b"SAVEST");
    rom[0x143] = cgb_flag;
    rom[0x147] = cart_type;
    rom[0x148] = 0; // 32 KiB
    rom[0x149] = ram_code;
    rom[0x150..0x150 + code.len()].copy_from_slice(code);
    // Interrupt handlers, one per vector (8 bytes each): VBlank 0x40, STAT 0x48, Timer 0x50,
    // Serial 0x58. Each is `PUSH AF ; LDH A,(n) ; INC A ; LDH (n),A ; POP AF ; RETI`, counting
    // in HRAM 0xFF90.. so that interrupt dispatch, EI/RETI and register saving are live state.
    for (i, vector) in [0x40usize, 0x48, 0x50, 0x58].into_iter().enumerate() {
        let n = 0x90 + i as u8;
        rom[vector..vector + 8].copy_from_slice(&[0xF5, 0xF0, n, 0x3C, 0xE0, n, 0xF1, 0xD9]);
    }
    // Make the ROM area that HDMA copies from look like noise rather than all zeros.
    let mut x = 0x1234_5678u32;
    for b in rom[0x400..0x2000].iter_mut() {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *b = x as u8;
    }
    rom
}

/// Interrupts, STAT sources, serial and joypad selection: everything that makes the
/// one-M-cycle edge states (EI delay, STAT line, serial transfer) live.
fn enable_interrupts(a: &mut Asm) {
    a.ldh(0x41, 0x48); // STAT: HBlank and LYC=LY interrupts
    a.ldh(0x45, 100); // LYC
    a.ldh(0x00, 0x20); // joypad: select the button group
    a.ldh(0xFF, 0x0F); // IE: VBlank, STAT, timer, serial
    a.b(&[0xFB]); // EI
}

/// Per-iteration extras: serial transfer start, a DI/HALT/EI dance (HALT with IME off and an
/// interrupt pending triggers the HALT bug), and an LCD off/on toggle every 256 iterations.
fn loop_extras(a: &mut Asm) {
    a.ldh(0x01, 0x55).ldh(0x02, 0x81); // serial: send 0x55 with the internal clock
    a.b(&[0xF3, 0x76, 0x00, 0xFB, 0x00]); // DI ; HALT ; NOP ; EI ; NOP
                                          // LDH A,(94) ; INC A ; LDH (94),A ; JR NZ,+8 ; LCD off ; LCD on
    a.b(&[0xF0, 0x94, 0x3C, 0xE0, 0x94, 0x20, 0x08]);
    a.b(&[0x3E, 0x13, 0xE0, 0x40, 0x3E, 0xB3, 0xE0, 0x40]);
}

/// DMG, MBC3+TIMER+RAM+BATTERY. Loop: OAM DMA from HRAM, cart RAM and RTC reads/latches,
/// timer running, VRAM tile and palette writes, scrolling, three sound channels playing.
fn program_dmg_mbc3() -> Vec<u8> {
    let mut a = Asm::new();
    a.b(&[0x31, 0xFE, 0xFF]); // LD SP,0xFFFE
    a.st(0x0000, 0x0A); // enable cart RAM and RTC
                        // OAM DMA routine into HRAM at 0xFF80: LDH (46),A ; LD B,40 ; DEC B ; JR NZ,-3 ; RET
    for (i, byte) in [0xE0, 0x46, 0x06, 0x28, 0x05, 0x20, 0xFD, 0xC9]
        .iter()
        .enumerate()
    {
        a.ldh(0x80 + i as u8, *byte);
    }
    a.ldh(0x40, 0xB3); // LCD on, BG on, sprites on, window on
    a.ldh(0x4A, 40).ldh(0x4B, 60); // window at WY=40, WX=60
    a.ldh(0x26, 0x80).ldh(0x24, 0x77).ldh(0x25, 0xFF); // sound on, full volume, all channels
    a.ldh(0x11, 0x80).ldh(0x12, 0xF3).ldh(0x14, 0x87); // channel 1
    a.ldh(0x1A, 0x80).ldh(0x1C, 0x20).ldh(0x1E, 0x87); // channel 3 (wave)
    a.ldh(0x21, 0xF1).ldh(0x22, 0x55).ldh(0x23, 0x80); // channel 4 (noise)
                                                       // Timer on at 262144 Hz and overflowing every 8 increments (32 M-cycles), so the one-cycle
                                                       // "overflowed, reload pending" window is common.
    a.ldh(0x06, 0xF8).ldh(0x05, 0xF8).ldh(0x07, 0x05);
    a.ldh(0x10, 0x1F); // channel 1 sweep: period 1, negate, shift 7
    enable_interrupts(&mut a);
    let top = a.here();
    a.b(&[0xF0, 0x04]); // LDH A,(DIV)
    a.b(&[0xEA, 0x00, 0xA0]); // LD (A000),A   cart RAM
    a.b(&[0xEA, 0x00, 0xC0]); // LD (C000),A   becomes OAM data
    a.b(&[0xEA, 0x00, 0x80]); // LD (8000),A   tile 0 row 0
    a.b(&[0xEA, 0x01, 0x80]); // LD (8001),A
    a.b(&[0xE0, 0x43]); // SCX
    a.b(&[0xE0, 0x47]); // BGP
    a.b(&[0xE0, 0x13]); // NR13
    a.b(&[0x3E, 0xC0, 0xCD, 0x80, 0xFF]); // LD A,C0 ; CALL FF80 (OAM DMA)
    a.b(&[0xF0, 0x05, 0xEA, 0x00, 0xC1]); // TIMA -> (C100)
    a.st(0x6000, 0x00).st(0x6000, 0x01); // latch the RTC
    a.st(0x4000, 0x08); // select RTC seconds
    a.b(&[0xFA, 0x00, 0xA0, 0xE0, 0x42]); // LD A,(A000) ; SCY = RTC seconds
    a.st(0x4000, 0x00); // back to RAM bank 0
    a.ldh(0x14, 0x87); // retrigger channel 1 (restarts the sweep)
    loop_extras(&mut a);
    a.jp(top);
    rom_with(0x10, 0x00, 2, &a.code)
}

/// CGB, ROM only. Double-speed switch, then a loop of HBlank VRAM DMA (waited for), general
/// DMA, VRAM and WRAM bank switching, palette writes, sound on channels 1 and 3.
fn program_cgb() -> Vec<u8> {
    let mut a = Asm::new();
    a.b(&[0x31, 0xFE, 0xFF]); // LD SP,0xFFFE
    a.ldh(0x4D, 0x01); // arm the speed switch
    a.b(&[0x10, 0x00]); // STOP
    a.ldh(0x26, 0x80).ldh(0x24, 0x77).ldh(0x25, 0xFF);
    a.ldh(0x12, 0xF3).ldh(0x14, 0x87); // channel 1
    a.ldh(0x1A, 0x80).ldh(0x1C, 0x20).ldh(0x1E, 0x87); // channel 3
    a.ldh(0x68, 0x80); // BCPS: index 0, auto-increment
    a.ldh(0x40, 0xB1); // LCD on, BG on, window on
    a.ldh(0x4A, 40).ldh(0x4B, 60);
    a.ldh(0x06, 0xF8).ldh(0x05, 0xF8).ldh(0x07, 0x05);
    enable_interrupts(&mut a);
    let top = a.here();
    a.b(&[0xF0, 0x04, 0xE0, 0x69]); // DIV -> BCPD
    a.b(&[0xE0, 0x43, 0xE0, 0x13]); // SCX, NR13
    a.ldh(0x4F, 0x01).b(&[0xEA, 0x00, 0x80]).ldh(0x4F, 0x00); // write VRAM bank 1
    a.ldh(0x70, 0x02).b(&[0xEA, 0x00, 0xD0]).ldh(0x70, 0x01); // write WRAM bank 2
    a.ldh(0x51, 0x01).ldh(0x52, 0x50); // HDMA source 0x0150
    a.ldh(0x53, 0x00).ldh(0x54, 0x00); // destination 0x8000
    a.ldh(0x55, 0x87); // HBlank DMA, 8 blocks
    let wait = a.here();
    a.b(&[0xF0, 0x55, 0xCB, 0x7F]); // LDH A,(55) ; BIT 7,A
    a.jr_back(0x28, wait); // JR Z,wait
    a.ldh(0x55, 0x01); // general DMA, 2 blocks
    loop_extras(&mut a);
    a.jp(top);
    rom_with(0x00, 0x80, 0, &a.code)
}

fn make(rom: &[u8], model: Model) -> GameBoy {
    GameBoy::new(
        Cartridge::from_bytes(rom.to_vec()).expect("test ROM loads"),
        model,
    )
}

// ------------------------------------------------------------------ comparison helpers

/// What one frame produced, as hashes.
#[derive(Debug, PartialEq, Eq)]
struct FrameTrace {
    picture: u64,
    audio: u64,
    pc: u16,
    cycles: u64,
}

fn run_frames(gb: &mut GameBoy, frames: usize) -> Vec<FrameTrace> {
    let mut out = Vec::new();
    let mut audio = Vec::new();
    for _ in 0..frames {
        gb.run_frame();
        audio.clear();
        gb.drain_audio(&mut audio);
        let bytes: Vec<u8> = audio
            .iter()
            .flat_map(|s| s.to_bits().to_le_bytes())
            .collect();
        out.push(FrameTrace {
            picture: gb.framebuffer_hash(),
            audio: checksum(&bytes),
            pc: gb.pc(),
            cycles: gb.bus().cycles(),
        });
    }
    out
}

/// Core of every scenario: `a` is saved at its current point, both continue for `frames`
/// frames and must agree on picture, audio, PC, cycle count and the full final state.
fn assert_round_trip(rom: &[u8], model: Model, mut a: GameBoy, frames: usize, what: &str) {
    let saved = a.save_state();
    // Samples queued before the save point are output, not state: a loaded console starts with
    // an empty queue, so compare only what is produced *after* the save.
    a.drain_audio(&mut Vec::new());

    // `b` runs somewhere else first, so that loading really has to overwrite everything.
    let mut b = make(rom, model);
    b.set_buttons(Buttons(Buttons::A));
    run_frames(&mut b, 7);
    b.set_buttons(Buttons(0));
    b.load_state(&saved)
        .unwrap_or_else(|e| panic!("{what}: load failed: {e}"));

    // 1. Loading reproduces the saved bytes exactly (catches fields loaded wrongly or masked).
    assert_eq!(
        b.save_state(),
        saved,
        "{what}: state changed by a save/load round trip"
    );
    // 2. Loading must not replay stale audio.
    let mut stale = Vec::new();
    b.drain_audio(&mut stale);
    assert!(stale.is_empty(), "{what}: audio queued right after load");

    let trace_a = run_frames(&mut a, frames);
    let trace_b = run_frames(&mut b, frames);
    if let Some(i) = trace_a.iter().zip(&trace_b).position(|(x, y)| x != y) {
        panic!(
            "{what}: outputs diverge {i} frames after load
 original: {:?}
 loaded:   {:?}",
            trace_a[i], trace_b[i]
        );
    }
    assert_eq!(
        a.save_state(),
        b.save_state(),
        "{what}: final states differ"
    );
    let distinct: std::collections::HashSet<u64> = trace_a.iter().map(|t| t.picture).collect();
    assert!(
        distinct.len() > 3 && trace_a.iter().any(|t| t.audio != trace_a[0].audio),
        "{what}: the program produced a static picture or constant audio, so the comparison proves little"
    );
}

/// Step until `cond` holds (after a warm-up), failing loudly if it never does.
fn step_until(gb: &mut GameBoy, what: &str, cond: impl Fn(&GameBoy) -> bool) {
    for _ in 0..2_000_000 {
        gb.step();
        if cond(gb) {
            return;
        }
    }
    panic!("never reached: {what}");
}

fn warmed_up(rom: &[u8], model: Model) -> GameBoy {
    let mut gb = make(rom, model);
    // Several frames: sound, timer, DMA and (CGB) the speed switch are running by now.
    for _ in 0..4 {
        gb.run_frame();
    }
    gb
}

// ------------------------------------------------------------------ scenarios

const FRAMES: usize = 150; // 2.5 s of emulated time: the RTC ticks and the sound sequencer cycles

#[test]
fn dmg_mbc3_at_a_frame_boundary() {
    let rom = program_dmg_mbc3();
    let a = warmed_up(&rom, Model::Dmg);
    assert_round_trip(&rom, Model::Dmg, a, FRAMES, "DMG/MBC3 frame boundary");
}

#[test]
fn dmg_mbc3_mid_frame_at_arbitrary_instruction_counts() {
    let rom = program_dmg_mbc3();
    for steps in [1, 17, 1234, 9999] {
        let mut a = warmed_up(&rom, Model::Dmg);
        for _ in 0..steps {
            a.step();
        }
        assert_round_trip(
            &rom,
            Model::Dmg,
            a,
            FRAMES,
            &format!("DMG/MBC3 mid-frame +{steps} instructions"),
        );
    }
}

#[test]
fn dmg_mbc3_during_oam_dma() {
    let rom = program_dmg_mbc3();
    let mut a = warmed_up(&rom, Model::Dmg);
    step_until(&mut a, "OAM DMA active", |g| g.bus().oam_dma_active());
    assert!(a.bus().oam_dma_active());
    assert_round_trip(&rom, Model::Dmg, a, FRAMES, "DMG/MBC3 during OAM DMA");
}

#[test]
fn dmg_mbc3_with_rtc_state_carried_over() {
    // The state must carry the RTC (live registers, latched copy, sub-second counter).
    let rom = program_dmg_mbc3();
    let mut a = warmed_up(&rom, Model::Dmg);
    for _ in 0..130 {
        a.run_frame(); // > 2 s: the RTC has counted
    }
    assert_round_trip(&rom, Model::Dmg, a, FRAMES, "DMG/MBC3 after 2 s of RTC");
}

#[test]
fn cgb_at_a_frame_boundary_in_double_speed() {
    let rom = program_cgb();
    let a = warmed_up(&rom, Model::Cgb);
    assert!(
        a.bus().double_speed(),
        "the program switches to double speed"
    );
    assert_round_trip(&rom, Model::Cgb, a, FRAMES, "CGB frame boundary");
}

#[test]
fn cgb_mid_frame() {
    let rom = program_cgb();
    for steps in [3, 555, 7777] {
        let mut a = warmed_up(&rom, Model::Cgb);
        for _ in 0..steps {
            a.step();
        }
        assert_round_trip(
            &rom,
            Model::Cgb,
            a,
            FRAMES,
            &format!("CGB mid-frame +{steps}"),
        );
    }
}

#[test]
fn cgb_during_hblank_vram_dma() {
    let rom = program_cgb();
    // Save at several different moments of the transfer (it moves one 16-byte block per line).
    for skip_blocks in [0, 1, 3] {
        let mut a = warmed_up(&rom, Model::Cgb);
        step_until(&mut a, "HBlank DMA active", |g| g.bus().vram_dma_active());
        // Let `skip_blocks` more blocks go by: wait for FF55 to read back fewer blocks.
        for _ in 0..skip_blocks {
            let left = a.bus().peek(0xFF55) & 0x7F;
            step_until(&mut a, "next HDMA block", |g| {
                g.bus().peek(0xFF55) & 0x7F != left && g.bus().vram_dma_active()
            });
        }
        assert!(a.bus().vram_dma_active());
        assert_eq!(
            a.bus().peek(0xFF55) & 0x80,
            0,
            "FF55 bit 7 clear = transfer running"
        );
        assert_round_trip(
            &rom,
            Model::Cgb,
            a,
            FRAMES,
            &format!("CGB during HBlank DMA (after {skip_blocks} more blocks)"),
        );
    }
}

#[test]
fn dmg_mbc3_right_after_the_oam_dma_start_write() {
    // The DMA routine in HRAM starts with `LDH (46),A` at 0xFF80; right after it the transfer is
    // requested but has not started yet (hardware delay), which is its own piece of state.
    let rom = program_dmg_mbc3();
    let mut a = warmed_up(&rom, Model::Dmg);
    step_until(&mut a, "pc after LDH (46),A", |g| g.pc() == 0xFF82);
    assert!(
        !a.bus().oam_dma_active(),
        "the transfer has not started yet"
    );
    assert_round_trip(
        &rom,
        Model::Dmg,
        a,
        FRAMES,
        "DMG/MBC3 right after the DMA write",
    );
}

#[test]
fn dmg_mbc3_right_after_the_lcd_is_switched_on() {
    // After LCD off -> on the PPU restarts at line 0 with a special first line (own state).
    let rom = program_dmg_mbc3();
    let mut a = warmed_up(&rom, Model::Dmg);
    step_until(&mut a, "LCD off", |g| g.bus().peek(0xFF40) & 0x80 == 0);
    step_until(&mut a, "LCD on again", |g| g.bus().peek(0xFF40) & 0x80 != 0);
    assert_round_trip(&rom, Model::Dmg, a, FRAMES, "DMG/MBC3 right after LCD on");
}

#[test]
fn cgb_with_the_speed_switch_armed() {
    // Between `LDH (4D),A` and the STOP that performs the switch, KEY1's arm bit is state.
    let rom = program_cgb();
    let mut a = make(&rom, Model::Cgb);
    step_until(&mut a, "KEY1 armed", |g| g.bus().speed_switch_armed());
    assert!(!a.bus().double_speed());
    assert_round_trip(
        &rom,
        Model::Cgb,
        a,
        FRAMES,
        "CGB with the speed switch armed",
    );
}

#[test]
fn the_test_programs_really_exercise_what_they_claim() {
    // If a program silently stopped raising interrupts or moving data, the round-trip tests
    // would still pass and prove much less. Check the evidence in HRAM / VRAM.
    for (rom, model, what) in [
        (program_dmg_mbc3(), Model::Dmg, "DMG"),
        (program_cgb(), Model::Cgb, "CGB"),
    ] {
        let mut gb = warmed_up(&rom, model);
        for _ in 0..30 {
            gb.run_frame();
        }
        for (addr, name) in [
            (0xFF90, "VBlank"),
            (0xFF91, "STAT"),
            (0xFF92, "timer"),
            (0xFF93, "serial"),
        ] {
            assert_ne!(
                gb.bus().peek(addr),
                0,
                "{what}: no {name} interrupt was serviced"
            );
        }
        assert_ne!(gb.bus().peek(0xFF94), 0, "{what}: main loop is not running");
    }
}

/// Save at many consecutive instruction boundaries and compare a console that loaded the state
/// with the original over the next 120 instructions, by full state bytes. The one-M-cycle
/// states (TIMA reload pending, a DMA start delay, HDMA block edges, STAT edges) are only
/// visible between instructions now and then; a dense scan reaches them.
fn dense_scan(rom: &[u8], model: Model, saves: usize, what: &str) {
    let mut a = warmed_up(rom, model);
    let mut c = make(rom, model);
    for i in 0..saves {
        for _ in 0..(1 + i % 5) {
            a.step();
        }
        let saved = a.save_state();
        c.load_state(&saved).unwrap();
        for _ in 0..120 {
            a.step();
            c.step();
        }
        assert!(
            a.save_state() == c.save_state(),
            "{what}: states differ 120 instructions after a load at save point {i}"
        );
    }
}

#[test]
fn dense_scan_dmg_mbc3() {
    dense_scan(&program_dmg_mbc3(), Model::Dmg, 1500, "DMG/MBC3");
}

#[test]
fn dense_scan_cgb() {
    dense_scan(&program_cgb(), Model::Cgb, 1500, "CGB");
}

// ------------------------------------------------------------------ errors and robustness

fn sample_state() -> (Vec<u8>, Vec<u8>, GameBoy) {
    let rom = program_dmg_mbc3();
    let gb = warmed_up(&rom, Model::Dmg);
    let state = gb.save_state();
    (rom, state, gb)
}

#[test]
fn saving_twice_gives_identical_bytes() {
    let (_, state, gb) = sample_state();
    assert_eq!(gb.save_state(), state);
}

#[test]
fn rejects_wrong_magic_version_model_and_cartridge() {
    let (rom, state, _) = sample_state();
    let mut gb = make(&rom, Model::Dmg);
    let before = gb.save_state();

    let mut bad = state.clone();
    bad[0] ^= 0xFF;
    assert_eq!(gb.load_state(&bad), Err(StateError::BadMagic));

    let mut bad = state.clone();
    bad[4] = 99;
    assert_eq!(gb.load_state(&bad), Err(StateError::UnsupportedVersion(99)));

    // A DMG state into a CGB console (same ROM).
    let mut cgb = make(&rom, Model::Cgb);
    assert!(matches!(
        cgb.load_state(&state),
        Err(StateError::WrongMachine(_))
    ));

    // Another cartridge.
    let other = program_cgb();
    let mut gb2 = make(&other, Model::Dmg);
    assert!(matches!(
        gb2.load_state(&state),
        Err(StateError::WrongMachine(_))
    ));

    assert_eq!(
        gb.save_state(),
        before,
        "failed loads must not change the machine"
    );
}

#[test]
fn truncated_and_damaged_states_are_errors_and_leave_the_machine_alone() {
    let (rom, state, _) = sample_state();
    let mut gb = make(&rom, Model::Dmg);
    for _ in 0..3 {
        gb.run_frame();
    }
    let before = gb.save_state();

    // Every short prefix, then a stride through the rest.
    let cuts = (0..64).chain((64..state.len()).step_by(997));
    for cut in cuts {
        assert!(
            gb.load_state(&state[..cut]).is_err(),
            "prefix of {cut} bytes accepted"
        );
    }
    assert!(gb.load_state(&[]).is_err());
    let mut longer = state.clone();
    longer.push(0);
    assert!(gb.load_state(&longer).is_err(), "trailing byte accepted");

    // Single flipped bytes anywhere are caught by the checksum.
    for pos in (0..state.len()).step_by(1013) {
        let mut bad = state.clone();
        bad[pos] ^= 0x5A;
        assert!(gb.load_state(&bad).is_err(), "flip at {pos} accepted");
    }
    assert_eq!(
        gb.save_state(),
        before,
        "failed loads must not change the machine"
    );
}

/// Recompute the payload length/checksum fields so that a mutated payload gets past the
/// integrity check and reaches the field-by-field parser (header: see `STATE_HEADER_LEN`).
fn reseal(state: &mut [u8]) {
    const HEADER: usize = 30;
    let sum = checksum(&state[HEADER..]);
    state[22..30].copy_from_slice(&sum.to_le_bytes());
}

#[test]
fn mutated_but_resealed_states_never_panic() {
    // Deterministic xorshift, so a failure is reproducible.
    let mut x = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    for (rom, model) in [
        (program_dmg_mbc3(), Model::Dmg),
        (program_cgb(), Model::Cgb),
    ] {
        let gb = warmed_up(&rom, model);
        let state = gb.save_state();
        let mut target = make(&rom, model);
        let mut accepted = 0;
        for _ in 0..400 {
            let mut bad = state.clone();
            for _ in 0..(1 + next() % 4) {
                let pos = 30 + (next() as usize) % (bad.len() - 30);
                bad[pos] = next() as u8;
            }
            reseal(&mut bad);
            let before = target.save_state();
            match target.load_state(&bad) {
                Ok(()) => {
                    accepted += 1;
                    // An accepted state must still run, without panicking.
                    target.run_frame();
                }
                Err(_) => assert_eq!(
                    target.save_state(),
                    before,
                    "failed load changed the machine"
                ),
            }
        }
        println!("{model:?}: {accepted} of 400 mutated states accepted and ran a frame");
    }
}

// ------------------------------------------------------------------ real-game long run

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn pokemon_red_save_mid_replay_then_load_into_a_fresh_console_matches_approved_hashes() {
    let root = workspace_root();
    let dir = root.join("tests/expected/local");
    let (Ok(rom), Ok(input), Ok(hashes)) = (
        std::fs::read(root.join("roms/pokemon_red.gb")),
        std::fs::read_to_string(dir.join("pokemon_red.run2.input")),
        std::fs::read_to_string(dir.join("pokemon_red.run2.hashes")),
    ) else {
        println!("SKIP: roms/pokemon_red.gb or the approved run2 replay is missing");
        return;
    };
    let script = InputScript::parse(&input).expect("input script");
    let expected: Vec<(u64, u64)> = hashes
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| {
            let mut f = l.split_whitespace();
            (
                f.next().unwrap().parse().unwrap(),
                u64::from_str_radix(f.next().unwrap(), 16).unwrap(),
            )
        })
        .collect();
    let last = expected.iter().map(|&(f, _)| f).max().unwrap();

    // Play on the original console up to frame 45000, then 5000 instructions into the next
    // frame (mid-frame, in the overworld).
    const SAVE_AT: u64 = 45_000;
    let mut original = make(&rom, Model::Dmg);
    for frame in 0..SAVE_AT {
        original.set_buttons(script.buttons_at(frame));
        original.run_frame();
    }
    original.set_buttons(script.buttons_at(SAVE_AT));
    for _ in 0..5000 {
        original.step();
    }
    let state = original.save_state();
    println!(
        "Pokemon Red state at frame {SAVE_AT}+5000 steps: {} bytes",
        state.len()
    );
    drop(original);

    // A brand-new console loads it and finishes the recording: every approved hash after the
    // save point must still match.
    let mut gb = make(&rom, Model::Dmg);
    gb.load_state(&state).expect("state loads");
    assert_eq!(gb.save_state(), state);
    let mut checked = 0;
    for frame in SAVE_AT..last {
        gb.set_buttons(script.buttons_at(frame));
        gb.run_frame();
        for &(f, want) in expected.iter().filter(|&&(f, _)| f == frame + 1) {
            assert_eq!(
                gb.framebuffer_hash(),
                want,
                "frame {f} differs from the approved hash after loading the state"
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "no approved hash lies after the save point");
    println!("pokemon_red.run2 via state: {checked} approved hashes match after frame {SAVE_AT}");
}
