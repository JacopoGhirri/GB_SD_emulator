//! Blargg `cpu_instrs`: the phase 1 gate. Each ROM prints "Passed" over the serial port.
//! Skips (prints SKIP, passes) if the ROM bundle is missing. Run with `-- --nocapture` to see it.

mod common;

use common::{run_rom_until, Limits};

/// Generous: the slowest ROM needs about 10 million instructions.
const BUDGET: u64 = 100_000_000;

fn expect_passed(rom: &str) {
    let limits = Limits {
        max_instructions: BUDGET,
    };
    let Some(outcome) = run_rom_until(rom, limits, &["Passed", "Failed"]) else {
        return; // SKIP line already printed
    };
    assert!(
        outcome.serial_text().contains("Passed"),
        "{rom}: {}",
        outcome.describe()
    );
    println!("{rom}: Passed after {} instructions", outcome.instructions);
}

macro_rules! individual {
    ($name:ident, $file:expr) => {
        #[test]
        fn $name() {
            expect_passed(concat!("blargg/cpu_instrs/individual/", $file));
        }
    };
}

individual!(special_01, "01-special.gb");
individual!(interrupts_02, "02-interrupts.gb");
individual!(op_sp_hl_03, "03-op sp,hl.gb");
individual!(op_r_imm_04, "04-op r,imm.gb");
individual!(op_rp_05, "05-op rp.gb");
individual!(ld_r_r_06, "06-ld r,r.gb");
individual!(jr_jp_call_ret_rst_07, "07-jr,jp,call,ret,rst.gb");
individual!(misc_instrs_08, "08-misc instrs.gb");
individual!(op_r_r_09, "09-op r,r.gb");
individual!(bit_ops_10, "10-bit ops.gb");
individual!(op_a_hl_11, "11-op a,(hl).gb");

/// The combined ROM needs the MBC1 ROM-bank switching.
#[test]
fn combined_cpu_instrs() {
    expect_passed("blargg/cpu_instrs/cpu_instrs.gb");
}
