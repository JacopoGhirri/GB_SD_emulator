//! Blargg timing suites (phase 2 gate). Pass criterion: "Passed" on the serial port.
//! Skips if the ROM bundle is absent.

mod common;

use common::{run_rom_until, Limits};

const BUDGET: u64 = 100_000_000;

fn expect_passed(rom: &str) {
    let limits = Limits {
        max_instructions: BUDGET,
    };
    let Some(outcome) = run_rom_until(rom, limits, &["Passed", "Failed"]) else {
        return;
    };
    println!("{rom}: {}", outcome.describe());
    assert!(
        outcome.serial_text().contains("Passed"),
        "{rom}: {}",
        outcome.describe()
    );
}

#[test]
fn instr_timing() {
    expect_passed("blargg/instr_timing/instr_timing.gb");
}
#[test]
fn mem_timing() {
    expect_passed("blargg/mem_timing/mem_timing.gb");
}
#[test]
fn mem_timing_2() {
    let Some((status, text, n)) =
        common::run_blargg_ram("blargg/mem_timing-2/mem_timing.gb", BUDGET)
    else {
        return;
    };
    println!(
        "mem_timing-2: status {status:#04X} after {n} instructions
{text}"
    );
    assert_eq!(
        status, 0,
        "mem_timing-2 failed:
{text}"
    );
}
