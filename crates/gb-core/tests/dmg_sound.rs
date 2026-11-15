//! Blargg `dmg_sound` individual ROMs (phase 6 gate). Each ROM reports through cartridge RAM
//! (signature DE B0 61 at 0xA001, status at 0xA000, text at 0xA004), like `mem_timing-2`.
//! Skips if the ROM bundle is absent.
//!
//! ROMs listed in `KNOWN_FAILING` are documented hardware corner cases we do not model; they
//! are run and reported but do not fail the test. A ROM that is listed there and starts to pass
//! fails the test, so the list cannot go stale.

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
    "12-wave write while on",
];

/// (ROM, why it is expected to fail). Empty until the results are known.
const KNOWN_FAILING: [(&str, &str); 1] = [(
    "10-wave trigger while on",
    "DMG wave-RAM corruption on retrigger: needs a different first-sample delay than 09/12 allow",
)];

#[test]
fn dmg_sound_individual_roms() {
    let mut unexpected = Vec::new();
    for name in ROMS {
        let rel = format!("blargg/dmg_sound/rom_singles/{name}.gb");
        let Some((status, text, n)) = common::run_blargg_ram(&rel, BUDGET) else {
            return;
        };
        let known = KNOWN_FAILING.iter().find(|(r, _)| *r == name);
        let pass = status == 0;
        println!(
            "{name}: {} (status {status:#04X}, {n} instructions){}",
            if pass { "PASS" } else { "FAIL" },
            known.map_or(String::new(), |(_, why)| format!("  [known: {why}]"))
        );
        if !pass {
            println!("{}", text.trim_end());
        }
        match (pass, known) {
            (false, None) => unexpected.push(format!("{name} failed:\n{text}")),
            (true, Some(_)) => {
                unexpected.push(format!("{name} is listed as known-failing but passes"))
            }
            _ => {}
        }
    }
    assert!(unexpected.is_empty(), "{}", unexpected.join("\n"));
}
