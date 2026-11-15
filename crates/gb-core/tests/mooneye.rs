//! Mooneye test-suite sweeps (phase 2 gate). Each test runs every DMG-compatible ROM in one
//! directory (see `gb_core::mooneye::dmg_compatible`) and fails listing the ROMs that did not pass.
//! Skips if the ROM bundle is absent. Run with `-- --nocapture` for the per-ROM table.

mod common;

/// Mooneye ROMs finish in well under a million instructions; this only bounds a hang.
const BUDGET: u64 = 20_000_000;

fn sweep(dir: &str) {
    let Some(results) = common::mooneye_sweep(dir, BUDGET) else {
        return;
    };
    let failed: Vec<_> = results.iter().filter(|(_, pass, _)| !pass).collect();
    let passed = results.len() - failed.len();
    println!("{dir}: {passed} passed, {} failed", failed.len());
    assert!(failed.is_empty(), "{dir}: failing ROMs: {failed:#?}");
}

const MOONEYE: &str = "mooneye-test-suite";

#[test]
fn acceptance_timer() {
    sweep(&format!("{MOONEYE}/acceptance/timer"));
}
#[test]
fn acceptance_oam_dma() {
    sweep(&format!("{MOONEYE}/acceptance/oam_dma"));
}
#[test]
fn acceptance_interrupts() {
    sweep(&format!("{MOONEYE}/acceptance/interrupts"));
}
#[test]
fn acceptance_bits() {
    sweep(&format!("{MOONEYE}/acceptance/bits"));
}
#[test]
fn emulator_only_mbc1() {
    sweep(&format!("{MOONEYE}/emulator-only/mbc1"));
}
#[test]
fn emulator_only_mbc5() {
    sweep(&format!("{MOONEYE}/emulator-only/mbc5"));
}

/// Phase 3: `acceptance/ppu` is recorded, not gated (cycle-exact mode 3 is out of scope).
/// The ROMs in `PPU_MUST_PASS` pass today and must keep passing; the rest are printed only.
const PPU_MUST_PASS: &[&str] = &[
    "intr_1_2_timing-GS.gb",
    "intr_2_0_timing.gb",
    "intr_2_mode0_timing.gb",
    "intr_2_mode3_timing.gb",
    "intr_2_oam_ok_timing.gb",
    "stat_irq_blocking.gb",
];

#[test]
fn acceptance_ppu_recorded() {
    let Some(results) = common::mooneye_sweep(&format!("{MOONEYE}/acceptance/ppu"), BUDGET) else {
        return;
    };
    for (name, pass, text) in &results {
        println!(
            "PPU-RESULT {name}: {} ({text})",
            if *pass { "PASS" } else { "FAIL" }
        );
    }
    let regressed: Vec<_> = PPU_MUST_PASS
        .iter()
        .filter(|n| results.iter().any(|(name, pass, _)| name == *n && !pass))
        .collect();
    assert!(
        regressed.is_empty(),
        "PPU ROMs that used to pass now fail: {regressed:?}"
    );
}

/// CGB boot-state ROMs (phase 7, task 2/7). They are DMG-flagged, so they run in DMG compatibility
/// mode on `Model::Cgb`. `boot_div-cgb0` targets the first CGB silicon (different power-up timing)
/// and is deliberately not run, like `dmg0`.
#[test]
fn misc_cgb_boot_state() {
    for name in ["boot_regs-cgb", "boot_div-cgbABCDE", "boot_hwio-C"] {
        let rom = format!("{MOONEYE}/misc/{name}.gb");
        let Some((verdict, n)) = common::run_mooneye_rom_on(&rom, gb_core::Model::Cgb, BUDGET)
        else {
            println!("SKIP: {rom} not found. Run scripts/fetch_test_roms.sh.");
            return;
        };
        println!("{rom}: {verdict:?} after {n} instructions");
        assert_eq!(
            verdict,
            gb_core::mooneye::MooneyeVerdict::Pass,
            "{rom} on CGB"
        );
    }
}

/// Directories swept on CGB hardware (every directory that holds `.gb` ROMs directly).
const CGB_DIRS: &[&str] = &[
    "acceptance",
    "acceptance/bits",
    "acceptance/instr",
    "acceptance/interrupts",
    "acceptance/oam_dma",
    "acceptance/ppu",
    "acceptance/serial",
    "acceptance/timer",
    "emulator-only/mbc1",
    "emulator-only/mbc5",
    "misc",
    "misc/bits",
    "misc/ppu",
];

/// Records (not gates) how the CGB-applicable Mooneye ROMs do on `Model::Cgb`, one line per ROM
/// (`CGB-RESULT ...`), and guards the ROMs listed in `CGB_MUST_PASS` against regressions.
#[test]
fn cgb_recorded() {
    let mut all = Vec::new();
    for dir in CGB_DIRS {
        let path = format!("{MOONEYE}/{dir}");
        let Some(results) = common::mooneye_sweep_on(
            &path,
            gb_core::Model::Cgb,
            gb_core::mooneye::cgb_compatible,
            BUDGET,
        ) else {
            return;
        };
        for (name, pass, text) in results {
            println!(
                "CGB-RESULT {dir}/{name}: {} ({text})",
                if pass { "PASS" } else { "FAIL" }
            );
            all.push((format!("{dir}/{name}"), pass));
        }
    }
    let passed = all.iter().filter(|(_, p)| *p).count();
    println!(
        "CGB-SUMMARY: {passed} passed, {} failed",
        all.len() - passed
    );
    let regressed: Vec<_> = CGB_MUST_PASS
        .iter()
        .filter(|n| all.iter().any(|(name, pass)| name == *n && !pass))
        .collect();
    assert!(
        regressed.is_empty(),
        "CGB ROMs that passed now fail: {regressed:?}"
    );
}

/// Every ROM that passed in the first recorded run (2026-11-15, see PROGRESS.md).
const CGB_MUST_PASS: &[&str] = &[
    "acceptance/call_cc_timing2.gb",
    "acceptance/call_timing2.gb",
    "acceptance/div_timing.gb",
    "acceptance/ei_sequence.gb",
    "acceptance/ei_timing.gb",
    "acceptance/halt_ime0_ei.gb",
    "acceptance/halt_ime0_nointr_timing.gb",
    "acceptance/halt_ime1_timing.gb",
    "acceptance/if_ie_registers.gb",
    "acceptance/intr_timing.gb",
    "acceptance/oam_dma_restart.gb",
    "acceptance/oam_dma_start.gb",
    "acceptance/oam_dma_timing.gb",
    "acceptance/pop_timing.gb",
    "acceptance/push_timing.gb",
    "acceptance/rapid_di_ei.gb",
    "acceptance/reti_intr_timing.gb",
    "acceptance/rst_timing.gb",
    "acceptance/bits/mem_oam.gb",
    "acceptance/bits/reg_f.gb",
    "acceptance/instr/daa.gb",
    "acceptance/interrupts/ie_push.gb",
    "acceptance/oam_dma/basic.gb",
    "acceptance/oam_dma/reg_read.gb",
    "acceptance/ppu/intr_2_0_timing.gb",
    "acceptance/ppu/intr_2_mode0_timing.gb",
    "acceptance/ppu/intr_2_mode3_timing.gb",
    "acceptance/ppu/intr_2_oam_ok_timing.gb",
    "acceptance/ppu/stat_irq_blocking.gb",
    "acceptance/timer/div_write.gb",
    "acceptance/timer/rapid_toggle.gb",
    "acceptance/timer/tim00.gb",
    "acceptance/timer/tim00_div_trigger.gb",
    "acceptance/timer/tim01.gb",
    "acceptance/timer/tim01_div_trigger.gb",
    "acceptance/timer/tim10.gb",
    "acceptance/timer/tim10_div_trigger.gb",
    "acceptance/timer/tim11.gb",
    "acceptance/timer/tim11_div_trigger.gb",
    "acceptance/timer/tima_reload.gb",
    "acceptance/timer/tima_write_reloading.gb",
    "acceptance/timer/tma_write_reloading.gb",
    "emulator-only/mbc1/bits_bank1.gb",
    "emulator-only/mbc1/bits_bank2.gb",
    "emulator-only/mbc1/bits_mode.gb",
    "emulator-only/mbc1/bits_ramg.gb",
    "emulator-only/mbc1/multicart_rom_8Mb.gb",
    "emulator-only/mbc1/ram_256kb.gb",
    "emulator-only/mbc1/ram_64kb.gb",
    "emulator-only/mbc1/rom_16Mb.gb",
    "emulator-only/mbc1/rom_1Mb.gb",
    "emulator-only/mbc1/rom_2Mb.gb",
    "emulator-only/mbc1/rom_4Mb.gb",
    "emulator-only/mbc1/rom_512kb.gb",
    "emulator-only/mbc1/rom_8Mb.gb",
    "emulator-only/mbc5/rom_16Mb.gb",
    "emulator-only/mbc5/rom_1Mb.gb",
    "emulator-only/mbc5/rom_2Mb.gb",
    "emulator-only/mbc5/rom_32Mb.gb",
    "emulator-only/mbc5/rom_4Mb.gb",
    "emulator-only/mbc5/rom_512kb.gb",
    "emulator-only/mbc5/rom_64Mb.gb",
    "emulator-only/mbc5/rom_8Mb.gb",
    "misc/boot_div-cgbABCDE.gb",
    "misc/boot_hwio-C.gb",
    "misc/boot_regs-cgb.gb",
];
