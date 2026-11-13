//! Checks that the ROM-test harness itself works. These tests do not check hardware
//! behaviour; that starts in phase 1 with the Blargg CPU tests.

mod common;

use common::{run_rom, Limits};

#[test]
fn missing_rom_skips_instead_of_failing() {
    let result = run_rom(
        "this-rom-does-not-exist.gb",
        Limits {
            max_instructions: 10,
        },
    );
    assert!(result.is_none(), "a missing ROM must skip, not run or fail");
}

#[test]
fn harness_runs_blargg_cpu_instrs_for_a_fixed_budget() {
    let limits = Limits {
        max_instructions: 1000,
    };
    let Some(outcome) = run_rom("blargg/cpu_instrs/cpu_instrs.gb", limits) else {
        return; // SKIP line already printed by the harness
    };
    assert_eq!(outcome.instructions, 1000, "{}", outcome.describe());
    assert_eq!(
        outcome.framebuffer.len(),
        gb_core::SCREEN_WIDTH * gb_core::SCREEN_HEIGHT,
        "{}",
        outcome.describe()
    );
}
