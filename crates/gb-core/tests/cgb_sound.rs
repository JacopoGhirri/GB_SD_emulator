//! Blargg `cgb_sound` individual ROMs on `Model::Cgb`: recorded, not gated (phase 7). The audio
//! unit is still the DMG model from phase 6 (CGB differences such as length counters that do not
//! survive power-off are not implemented), so some ROMs are expected to fail. Each ROM reports
//! through cartridge RAM like `dmg_sound`. ROMs in `MUST_PASS` guard against regressions.
//! Skips if the ROM bundle is absent.

mod common;

const BUDGET: u64 = 60_000_000;

const ROMS: [&str; 12] = [
    "01-registers",
    "02-len ctr",
    "03-trigger",
    "04-sweep",
    "05-sweep details",
    "06-overflow on trigger",
    "07-len sweep period sync",
    "08-len ctr during power",
    "09-wave read while on",
    "10-wave trigger while on",
    "11-regs after power",
    "12-wave",
];

/// ROMs that passed in the first recorded run (see PROGRESS.md).
const MUST_PASS: [&str; 7] = [
    "01-registers",
    "02-len ctr",
    "03-trigger",
    "04-sweep",
    "05-sweep details",
    "06-overflow on trigger",
    "07-len sweep period sync",
];

#[test]
fn cgb_sound_individual_roms_recorded() {
    let mut regressed = Vec::new();
    for name in ROMS {
        let rel = format!("blargg/cgb_sound/rom_singles/{name}.gb");
        let Some((status, text, n)) = common::run_blargg_ram_on(&rel, gb_core::Model::Cgb, BUDGET)
        else {
            return;
        };
        let pass = status == 0;
        println!(
            "CGB-SOUND {name}: {} (status {status:#04X}, {n} instructions)",
            if pass { "PASS" } else { "FAIL" }
        );
        if !pass {
            println!("{}", text.trim_end());
            if MUST_PASS.contains(&name) {
                regressed.push(name);
            }
        }
    }
    assert!(
        regressed.is_empty(),
        "passed before, fail now: {regressed:?}"
    );
}
