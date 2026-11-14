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
