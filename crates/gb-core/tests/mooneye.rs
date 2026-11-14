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
#[ignore = "phase 2 in progress: un-ignore when this area is implemented"]
fn acceptance_oam_dma() {
    sweep(&format!("{MOONEYE}/acceptance/oam_dma"));
}
#[test]
#[ignore = "phase 2 in progress: un-ignore when this area is implemented"]
fn acceptance_interrupts() {
    sweep(&format!("{MOONEYE}/acceptance/interrupts"));
}
#[test]
fn acceptance_bits() {
    sweep(&format!("{MOONEYE}/acceptance/bits"));
}
#[test]
#[ignore = "phase 2 in progress: un-ignore when this area is implemented"]
fn emulator_only_mbc1() {
    sweep(&format!("{MOONEYE}/emulator-only/mbc1"));
}
#[test]
#[ignore = "phase 2 in progress: un-ignore when this area is implemented"]
fn emulator_only_mbc5() {
    sweep(&format!("{MOONEYE}/emulator-only/mbc5"));
}
