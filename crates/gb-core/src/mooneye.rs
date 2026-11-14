//! Pass/fail protocol of the Mooneye test suite, shared by `gb-cli --mooneye` and the ROM tests.
//!
//! A Mooneye ROM finishes by loading B,C,D,E,H,L and executing `LD B,B` (opcode 0x40), which
//! acts as a debug breakpoint. Passing is the Fibonacci numbers 3,5,8,13,21,34; failing is
//! 0x42 in all six registers. See `mooneye-test-suite/README.markdown`, "Pass/fail reporting".
//! No I/O here: callers supply the ROM and read the verdict.

use crate::GameBoy;

/// Opcode of `LD B,B`.
const OPCODE_LD_B_B: u8 = 0x40;
/// Registers B,C,D,E,H,L of a passing test.
const PASS_SIGNATURE: [u8; 6] = [3, 5, 8, 13, 21, 34];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MooneyeVerdict {
    Pass,
    /// `LD B,B` was reached with some other register values (0x42 x6 is the canonical failure).
    Fail([u8; 6]),
    /// No `LD B,B` within the instruction budget.
    Timeout,
}

/// Run until the first `LD B,B` executes, or `max_instructions` is exhausted.
/// Returns the verdict and the number of instructions executed.
pub fn run_mooneye(gb: &mut GameBoy, max_instructions: u64) -> (MooneyeVerdict, u64) {
    for n in 0..max_instructions {
        let is_breakpoint = gb.bus().peek(gb.pc()) == OPCODE_LD_B_B;
        gb.step();
        if is_breakpoint {
            let r = &gb.cpu().regs;
            let regs = [r.b, r.c, r.d, r.e, r.h, r.l];
            let verdict = if regs == PASS_SIGNATURE {
                MooneyeVerdict::Pass
            } else {
                MooneyeVerdict::Fail(regs)
            };
            return (verdict, n + 1);
        }
    }
    (MooneyeVerdict::Timeout, max_instructions)
}

/// Whether a Mooneye ROM is expected to pass on a DMG (revision ABC, our power-up state).
///
/// Mooneye encodes the target hardware in the file name (README, "Test naming"): a suffix of
/// model groups `G` (dmg+mgb), `S` (sgb), `C` (cgb/agb/ags), `A` (agb), or explicit models
/// `dmg`, `dmg0`, `dmgABC`, `mgb`, `sgb`, `sgb2`, `cgb`, `agb`... A ROM with no suffix runs
/// everywhere. We run it iff it applies to a DMG revision A/B/C. `dmg0` (first silicon, other
/// power-up values), `mgb`, SGB, CGB and AGB-only ROMs are "other hardware", not failures.
pub fn dmg_compatible(file_name: &str) -> bool {
    let stem = file_name.strip_suffix(".gb").unwrap_or(file_name);
    let Some((_, suffix)) = stem.rsplit_once('-') else {
        return true;
    };
    // Model-group letters, e.g. "GS", "C", "A".
    if suffix.chars().all(|c| matches!(c, 'G' | 'S' | 'C' | 'A')) {
        return suffix.contains('G');
    }
    // Explicit model list, e.g. "dmgABC", "dmgABCmgb", "dmg0", "cgb", "sgb2".
    if let Some(revs) = suffix.strip_prefix("dmg") {
        // Revisions A/B/C (or no revision) but not revision 0 alone.
        return revs.is_empty() || revs.starts_with(['A', 'B', 'C']);
    }
    // Not a recognised model suffix (e.g. "rom_8Mb" has none after '-'): treat as no suffix,
    // unless it is a known other-model token.
    !matches!(
        suffix.split(|c: char| !c.is_ascii_alphabetic()).next(),
        Some("mgb" | "sgb" | "cgb" | "agb" | "ags")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffix_rule() {
        for ok in [
            "div_write.gb",
            "sources-GS.gb",
            "unused_hwio-GS.gb",
            "boot_div-dmgABCmgb.gb",
            "boot_regs-dmgABC.gb",
            "halt_ime1_timing2-GS.gb",
            "di_timing-GS.gb",
        ] {
            assert!(dmg_compatible(ok), "{ok} should run on DMG");
        }
        for skip in [
            "boot_div-S.gb",
            "boot_hwio-dmg0.gb",
            "boot_regs-mgb.gb",
            "boot_regs-sgb2.gb",
            "x-C.gb",
            "x-cgb.gb",
            "x-A.gb",
        ] {
            assert!(!dmg_compatible(skip), "{skip} is other hardware");
        }
    }
}
