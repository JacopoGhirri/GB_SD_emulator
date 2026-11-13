//! Diffs our CPU trace against Gameboy Doctor's known-good logs for Blargg cpu_instrs 01..11
//! (CLAUDE.md §8: "the first differing line is the bug").
//!
//! Needs `scripts/fetch_doctor_logs.sh`. Skips (prints SKIP, passes) if the logs or ROMs are
//! missing. Run with `-- --nocapture` to see SKIP lines.

mod common;

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use common::rom_path;
use gb_core::{Cartridge, GameBoy, Model};

const INDIVIDUAL: [&str; 11] = [
    "01-special.gb",
    "02-interrupts.gb",
    "03-op sp,hl.gb",
    "04-op r,imm.gb",
    "05-op rp.gb",
    "06-ld r,r.gb",
    "07-jr,jp,call,ret,rst.gb",
    "08-misc instrs.gb",
    "09-op r,r.gb",
    "10-bit ops.gb",
    "11-op a,(hl).gb",
];

fn log_path(n: usize) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(common::ROM_ROOT)
        .join("../gameboy-doctor/cpu_instrs")
        .join(format!("{n}.log"))
}

/// Returns the number of matching lines, or skips (None).
fn check(n: usize) -> Option<u64> {
    let rom_rel = format!("blargg/cpu_instrs/individual/{}", INDIVIDUAL[n - 1]);
    let rom = std::fs::read(rom_path(&rom_rel)).ok();
    let log = File::open(log_path(n)).ok();
    let (Some(rom), Some(log)) = (rom, log) else {
        println!("SKIP: doctor log or ROM for test {n} missing (run scripts/fetch_*.sh)");
        return None;
    };
    let mut gb = GameBoy::new(Cartridge::from_bytes(rom).unwrap(), Model::Dmg);
    gb.set_doctor_mode(true);

    let mut matched = 0u64;

    for expected in BufReader::new(log).lines() {
        // Advance past states Doctor has no line for (interrupt dispatch, idle HALT steps).
        while !gb.is_trace_point() {
            gb.step();
        }
        let expected = expected.unwrap();
        let ours = gb.trace_line();
        assert_eq!(
            ours,
            expected,
            "test {n}: first difference at log line {} (after {matched} matching lines)",
            matched + 1
        );
        gb.step();
        matched += 1;
    }
    Some(matched)
}

macro_rules! doctor_test {
    ($name:ident, $n:expr) => {
        #[test]
        fn $name() {
            if let Some(lines) = check($n) {
                println!("doctor trace {}: {lines} lines match", $n);
            }
        }
    };
}

doctor_test!(doctor_01_special, 1);
doctor_test!(doctor_02_interrupts, 2);
doctor_test!(doctor_03_op_sp_hl, 3);
doctor_test!(doctor_04_op_r_imm, 4);
doctor_test!(doctor_05_op_rp, 5);
doctor_test!(doctor_06_ld_r_r, 6);
doctor_test!(doctor_07_jumps, 7);
doctor_test!(doctor_08_misc, 8);
doctor_test!(doctor_09_op_r_r, 9);
doctor_test!(doctor_10_bit_ops, 10);
doctor_test!(doctor_11_op_a_hl, 11);
