# PROGRESS

**Current phase:** 2 — gate met on 2026-11-14 (awaiting owner sign-off before phase 3)
**Last updated:** 2026-11-14

## Status by phase
- [x] 0 Bootstrap
- [x] 1 CPU  (gate met 2026-11-13, see "Phase 1 — CPU: gate status")
- [x] 2 Bus, timer, cartridge  (gate met 2026-11-14, see "Phase 2" below)
- [ ] 3 PPU
- [ ] 4 Frontend, input, saves
- [ ] 5 Pokémon Red/Blue
- [ ] 6 APU
- [ ] 7 CGB
- [ ] 8 Polish / accuracy

## Phase 0 gate
- [x] `cargo build --workspace` — passes (see scoreboard).
- [x] `cargo fmt --all -- --check` — passes.
- [x] `cargo clippy --workspace --all-targets -- -D warnings` — passes.
- [x] `cargo test --workspace` — passes; ROM smoke test ran for real (ROM present).
- [x] `scripts/fetch_test_roms.sh` run, twice (second run is a no-op). `tests/roms/game-boy-test-roms-v7.0/` exists.
- [x] `gb-cli run <rom>` prints the parsed header and exits 0.
- [x] CI configuration committed (`.github/workflows/ci.yml`). **Not yet executed on GitHub**; see Known issues.
- [x] `PROGRESS.md` updated; phase 1 is current.

## Test scoreboard
(Paste real command output lines here. Never write "passes" without running it.)

Commands run on 2026-11-13 (Windows 11, rustc 1.100.0):

```
$ cargo fmt --all -- --check
fmt exit 0

$ cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.19s
clippy exit 0

$ cargo test --workspace
test result: ok. 6 passed; 0 failed   (gb-cli)
test result: ok. 8 passed; 0 failed   (gb-core cartridge::header)
test result: ok. 2 passed; 0 failed   (gb-core tests/harness_smoke.rs)
test exit 0

$ bash scripts/fetch_test_roms.sh          (first run)
checksum OK (b9a9d7a1075aa35a3d07c07c34974048672d8520dca9e07a50178f5860c3832c)
extracted 4510 ROM files into .../tests/roms/game-boy-test-roms-v7.0
exit 0
$ bash scripts/fetch_test_roms.sh          (second run)
test ROMs already present and verified: .../tests/roms/game-boy-test-roms-v7.0
exit 0

$ cargo run -p gb-cli -- run tests/roms/.../blargg/cpu_instrs/cpu_instrs.gb
title:        CPU_INSTRS
cgb flag:     0x80 (CGB enhanced, also runs on DMG)
cart type:    0x01 MBC1
rom size:     65536 bytes
ram size:     0 bytes
header sum:   0x3B stored, 0x3B computed -> OK
exit 0

Expected failures, checked on purpose:
$ scripts/fetch_test_roms.sh with a wrong SHA-256 (scratch copy)  -> "error: SHA-256 mismatch", download deleted, exit 1
$ scripts/fetch_test_roms.sh with an unreachable URL (scratch copy) -> instructions printed, exit 0
$ gb-cli run cpu_instrs.gb --max-instructions 1000 --expect-serial Passed
serial check FAILED: "Passed" not found. stopped after 1000 instructions, PC=0x0100   (exit 1, expected: no CPU yet)
$ gb-cli run cpu_instrs.gb --frames 1 --compare cpu_instrs-dmg-cgb.png
compare FAILED: 21465 pixels differ from ...                                          (exit 1, expected: no PPU yet)
```

| Suite | ROM | Result | Date |
|---|---|---|---|
| Harness + unit | (none) | 16 tests pass (6 cli, 8 header, 2 harness) | 2026-11-13 |
| Blargg cpu_instrs | `blargg/cpu_instrs/cpu_instrs.gb` | not run for correctness: CPU not implemented (phase 1) | 2026-11-13 |

## Decisions log
- **Branch:** `master` is the only branch and the target for PRs, per the owner's decision (2026-11-13). Phase 0 is committed on `master`, tracking `origin/master`.
- **Line endings:** `.gitattributes` forces LF (`* text=auto eol=lf`) and marks ROM/image files as binary. Added first so shell scripts stay LF on Windows checkouts and on the Linux CI runner.
- **Cargo.lock is committed** (not ignored): the workspace ships binaries.
- **Dependencies added** (CLAUDE.md §2 requires a one-line justification each):
  - `clap` 4 (derive): argument parsing for `gb-cli`.
  - `png` 0.17: writing screenshots and reading reference images for `--compare`.
- **gb-sdl** is an empty placeholder with no dependencies, per user choice. It will depend on `sdl2` in phase 4.
- **Test ROMs are pinned:** `c-sp/game-boy-test-roms` v7.0, verified by SHA-256 `B9A9D7A1…C3832C`. GitHub publishes no digest for this asset, so I computed the hash from the first download. This is trust-on-first-use: it protects against later changes to the release, not against a compromised first download. Source and hash are in `docs/REFERENCES.md`.
- **`--mooneye` and `--trace` fail with exit 2** until the CPU exists. I chose not to emit fake register values, which would look like real output.
- **Placeholder `GameBoy::pc()` returns 0x0100** (the post-boot value from Pan Docs), so failure messages show a plausible PC. Phase 1 replaces it.
- **Commit trailers:** commits in this repo carry no `Co-Authored-By` lines, per the user's instruction.
- **Commit granularity:** the workspace, the `gb-core` header parser and the `gb-cli` runner were committed together (`45d8004`), because each crate needs the others to compile.

## Known issues
- **CI:** the owner confirmed on 2026-11-14 that CI runs OK on GitHub (includes the Doctor-log fetch step).
- **`cargo test` output hides SKIP lines** unless run with `-- --nocapture`. The harness prints them to stdout, which the test runner captures for passing tests.
- The "stopped after N instructions" line also appears for frame-only runs, where it reads 0 instructions. Cosmetic only.
- The SKIP path shows a mixed `\` and `/` path on Windows. Cosmetic only.

## Open questions for the human
- None outstanding. Resolved 2026-11-13: commit `cb635f9` ("remove co authoring", three lines in `CLAUDE.md`) is intended and is left as is; PRs target `master`.

## Phase 1 — CPU: gate status (2026-11-13)
Environment: Windows Smart App Control blocks freshly built test executables, so all builds and tests below were run in **WSL Ubuntu** (rustc/cargo 1.100.0, build output in `~/gb-target`, sources read from the Windows tree). No workaround for the block was attempted.

- [x] Unit tests for ALU/flag logic: `alu.rs` checks ADD/ADC/SUB/SBC against a slow reference for all 256x256x2 operands.
- [x] Each Blargg `cpu_instrs/individual/01..11` prints `Passed`.
- [x] Combined `cpu_instrs.gb` passes, using the minimal MBC1 (ROM banking only).
- [x] Doctor trace diff clean for 01..11 (`crates/gb-core/tests/doctor_trace.rs`; line counts below).
- [x] Automated `cargo test` runs the individual ROMs and the combined ROM (`tests/cpu_instrs.rs`), skipping if absent.
- [x] Scoreboard has real result lines (below).

### Phase 1 scoreboard (real output)
```
$ cargo test --workspace            (WSL)
test result: ok. 6 passed; 0 failed    (gb-cli)
test result: ok. 42 passed; 0 failed   (gb-core unit: registers, alu, bus, timer, mbc1, cpu, header)
test result: ok. 12 passed; 0 failed   (gb-core tests/cpu_instrs.rs)
test result: ok. 11 passed; 0 failed   (gb-core tests/doctor_trace.rs, 49 s)
test result: ok. 2 passed; 0 failed    (gb-core tests/harness_smoke.rs)
$ cargo clippy --workspace --all-targets -- -D warnings     -> no output (clean)
$ cargo fmt --all -- --check                                 -> fmt-ok

$ cargo test -p gb-core --test cpu_instrs -- --nocapture
01-special.gb: Passed after 1300335 instructions
02-interrupts.gb: Passed after 203515 instructions
03-op sp,hl.gb: Passed after 1109862 instructions
04-op r,imm.gb: Passed after 1304206 instructions
05-op rp.gb: Passed after 1804828 instructions
06-ld r,r.gb: Passed after 284713 instructions
07-jr,jp,call,ret,rst.gb: Passed after 337852 instructions
08-misc instrs.gb: Passed after 265332 instructions
09-op r,r.gb: Passed after 4461822 instructions
10-bit ops.gb: Passed after 6756163 instructions
11-op a,(hl).gb: Passed after 7471202 instructions
cpu_instrs.gb (combined): Passed after 24914321 instructions

$ gb-cli run cpu_instrs.gb --max-instructions 80000000 --serial --expect-serial Passed
cpu_instrs
01:ok  02:ok  03:ok  04:ok  05:ok  06:ok  07:ok  08:ok  09:ok  10:ok  11:ok
Passed all tests
serial check passed: found "Passed"

$ cargo test -p gb-core --test doctor_trace -- --nocapture    (matching lines per ROM)
01: 1256633   02: 161057   03: 1066160   04: 1260504   05: 1761126   06: 241011
07: 587415    08: 221630   09: 4418120   10: 6712461   11: 7427500    (all: 11 passed; 0 failed)

$ gb-cli run "06-ld r,r.gb" --max-instructions 241011 --trace /tmp/t6.log ; cmp /tmp/t6.log 6.log
241011 /tmp/t6.log
TRACE IDENTICAL to Doctor log 6
```
Note: each Blargg ROM was first run through `gb-cli` and passed on the first attempt; the Doctor test then found three *convention* differences (below), not CPU bugs.

### Phase 1 decisions
- **Timing:** every bus access ticks 4 T-cycles, no cycle tables. A `read`/`write` ticks *before* the access happens. Revisit in phase 2 with `mem_timing` if a ROM disagrees.
- **Minimal timer (`timer.rs`) is a stand-in. Phase 2 MUST REPLACE it with the accurate falling-edge implementation** (DIV/TIMA/TMA/TAC; delayed TMA reload; TAC-write glitches). Added at the owner's request because 02-interrupts needs a timer interrupt; this slightly extends the phase file's "timer out of scope".
- **Minimal MBC1 (`cartridge/mbc1.rs`)** has ROM banking only (5-bit register); phase 2 replaces it with the full MBC1 plus MBC3/5, RAM, RTC.
- **Mapper trait** has `read_rom/write_rom/read_ram/write_ram/save_data/load_save`; `Cartridge` holds a `Box<dyn Mapper>` and no longer derives `Clone`.
- **STOP** is treated as a 2-byte NOP (low-power mode is not needed yet).
- **Interrupt dispatch** picks the vector after the high-byte push (jumps to 0x0000 if IE was cleared by it).
- **Gameboy Doctor conventions** (in `--trace` and `doctor_trace.rs` only; normal emulation follows hardware):
  1. LY reads a constant 0x90 in "doctor mode" (there is no PPU yet).
  2. IF reads **without** its unused upper bits in doctor mode. Real hardware (and normal mode here) reads them as 1; Doctor's reference emulator does not.
  3. Doctor logs have one line per *instruction*: no line for the state right after an interrupt dispatch, none for idle HALT cycles, but one line for the state after HALT itself. `Cpu::is_trace_point` encodes that.
- **Doctor logs** are fetched by a separate `scripts/fetch_doctor_logs.sh` (not by extending `fetch_test_roms.sh`), pinned to `robert/gameboy-doctor` commit `cc742777…` with per-file SHA-256 (trust on first use, computed 2026-11-13). Output in gitignored `tests/roms/gameboy-doctor/`. CI runs it before the tests.
- **`[profile.test] opt-level = 3`** in the workspace `Cargo.toml`: the ROM tests run ~25M instructions and the Doctor diffs ~25M lines.
- **LY stand-in was not needed**: Blargg cpu_instrs does not wait on LY, so no fake LY exists outside doctor mode.

### Phase 1 known issues
- `unzip` is now installed in WSL (owner, 2026-11-14), so the fetch scripts run there too. Earlier fetches were run from Git Bash.
- `--mooneye` is still unimplemented (exit 2); it belongs to phase 2's gate.
- `halt_bug` ROM and the other Blargg suites (`instr_timing`, `mem_timing`, `halt_bug`) have not been run; they are phase 2 gates. Only the HALT-bug unit test exists.

## Phase 2 — Bus, timer, DMA, cartridge: gate status (2026-11-14)
Environment as phase 1 (WSL Ubuntu, rustc 1.100.0).

- [x] All phase 1 tests still pass (cpu_instrs individual + combined, Doctor diffs 01..11).
- [x] Blargg `instr_timing`, `mem_timing`, `mem_timing-2` pass.
- [x] Mooneye `acceptance/timer`, `oam_dma`, `interrupts`, `bits`, `emulator-only/mbc1`, `mbc5`: all DMG-compatible ROMs pass.
- [x] MBC3 unit-tested (bank math, RAM banks, RTC latch, carries, day overflow/halt, save footer).
- [x] Timer unit tests: falling edges (DIV write, TAC enable/clock-select), TIMA overflow window.

```
$ cargo test --workspace   (WSL)
test result: ok. 6 passed   (gb-cli)
test result: ok. 87 passed  (gb-core unit)
test result: ok. 3 passed   (tests/blargg_timing.rs)
test result: ok. 12 passed  (tests/cpu_instrs.rs)
test result: ok. 11 passed  (tests/doctor_trace.rs, 37 s)
test result: ok. 2 passed   (tests/harness_smoke.rs)
test result: ok. 6 passed   (tests/mooneye.rs)
$ cargo clippy --workspace --all-targets -- -D warnings   -> clean;  cargo fmt --all -- --check -> fmt-ok

$ cargo test -p gb-core --test mooneye -- --nocapture --test-threads=1
acceptance/bits: 3 passed, 0 failed          acceptance/interrupts: 1 passed, 0 failed
acceptance/oam_dma: 3 passed, 0 failed       acceptance/timer: 13 passed, 0 failed
emulator-only/mbc1: 13 passed, 0 failed      emulator-only/mbc5: 8 passed, 0 failed
$ cargo test -p gb-core --test blargg_timing -- --nocapture
instr_timing: Passed   mem_timing: "01:ok 02:ok 03:ok Passed"   mem_timing-2: status 0x00 after 1224704 instructions, "Passed"
```
Extra, not gated (cli loop, DMG-compatible Mooneye ROMs in `acceptance/` root, `instr`, `interrupts`, `serial`): 18 pass, 13 fail. Failing: add_sp_e_timing, call_cc_timing, call_timing, di_timing-GS, halt_ime0_ei, halt_ime0_nointr_timing, halt_ime1_timing2-GS, jp_cc_timing, jp_timing, ld_hl_sp_e_timing, ret_cc_timing, ret_timing, reti_timing. Not investigated (out of the gate); the pattern (jp/call/ret/add sp timing) suggests the *order* of internal-delay M-cycles versus bus accesses inside those instructions. Worth a look in phase 8 (accuracy). `oam_dma_start/restart/timing` pass.

### Phase 2 decisions
- **Mooneye runner** (`gb_core::mooneye`, used by `gb-cli --mooneye --max-instructions N` and `tests/mooneye.rs`): pass = `LD B,B` executed with B,C,D,E,H,L = 3,5,8,13,21,34; 0x42 x6 = fail; no `LD B,B` in budget = timeout.
- **DMG filter rule** (README "Test naming", `mooneye::dmg_compatible`): no suffix = run; suffix made of group letters G/S/C/A runs iff it contains `G` (dmg+mgb); `dmg`/`dmgABC`/`dmgABCmgb` run; `dmg0` (first silicon, other power-up values), `mgb`, `sgb*`, `cgb*`, `agb*`, S-only, C-only, A-only are "skipped: other hardware", never counted as failures. Unit-tested.
- **Test-only LY stand-in (owner decision 2026-11-14):** Mooneye's startup code busy-waits for LY = 0x8F then 0x90 (twice), so a constant LY hangs. `Bus::set_ly_stub` makes LY free-run as `(cycles / 456) % 154`; no modes/STAT/interrupts. Enabled only by `run_mooneye`. **Phase 3 replaces it with the real PPU and must delete `ly_stub`.**
- **Bus tick ordering decided by tests (2026-11-14):** the CPU access now happens *before* the M-cycle tick (read: peek then tick; write: poke then tick). Experiment over the four read/write orderings: tick-first for both (phase-1 design) passed 12/13 timer ROMs (rapid_toggle failed); access-first for reads only or writes only failed 5-6 timer ROMs and all of Blargg instr/mem timing; access-first for both passed 13/13 timer ROMs and Blargg. Recorded in a comment on `Bus::read`.
- **Timer:** 16-bit counter stepped 4 per M-cycle, post-boot counter 0xABCC (DIV = 0xAB); TIMA falling edge, one-cycle overflow window, reload next cycle. 0xABCC worked with all 13 Mooneye timer ROMs.
- **OAM DMA:** first byte copied 2 M-cycles after the write cycle; a restart lets the old transfer run through the start-up delay; while active the CPU reads 0xFF / ignores writes for everything below 0xFF00 (HRAM, I/O and IE stay accessible); source pages 0xE0-0xFF mirror 0xC0-0xDF.
- **Interrupt dispatch:** the vector is chosen after the high-byte push and before the low-byte push (needed for `ie_push`); a cancelled dispatch jumps to 0x0000 without clearing IF.
- **Joypad:** `Buttons` bit set = pressed: RIGHT 0, LEFT 1, UP 2, DOWN 3, A 4, B 5, SELECT 6, START 7.
- **Serial:** every internal-clock transfer start captures SB (test output); the transfer completes after 4096 T-cycles with SB = 0xFF, SC bit 7 cleared and the serial interrupt. External clock never completes.
- **I/O registers:** every register has a named constant, DMG post-boot value and read mask (`io.rs`). OBP0/OBP1 power up as 0xFF (undefined in Pan Docs). Unmapped registers read 0xFF.
- **Cartridge:** ROM only, MBC1 (+ multicart detection: 1 MiB with the Nintendo logo at banks 0x10/0x20/0x30), MBC3 (+RTC), MBC5. Other types fail with `CartridgeError::UnsupportedType` (MBC2, HuC1/3, MBC6/7, ROM+RAM types 0x08/0x09 are not supported). RAM wraps inside its real size (2 KiB RAM is mirrored).
- **RTC save format (verified first, per owner):** BGB's documented footer (https://bgb.bircd.org/rtcsave.html) is also used by VBA-M and others: RAM, then 48 bytes = live seconds/minutes/hours/days-low/days-high as five little-endian u32 (value in the low byte), the same five latched, then a u64 Unix timestamp. Old VBA writes 44 bytes (u32 timestamp); we load both and write 48. I could confirm the 44/48 sizes, the little-endian dwords and the offsets of the latched days (32/36) and timestamp (40/44); the exact order of the first ten registers is from memory and matches those offsets. **Not verified against a real file from another emulator**: if you have a save from BGB/VBA-M/mGBA, try loading it. The timestamp is written as 0 and ignored on load (the core has no wall clock; a wall-clock mode comes in phase 4/8).
- **RTC time base:** emulated cycles (1 s = 1,048,576 M-cycles); halt bit stops it; day counter wraps at 512 with the carry flag set.
- **Blargg newer ROMs** (mem_timing-2) report through cartridge RAM (signature DE B0 61 at 0xA001, status at 0xA000, text at 0xA004) rather than serial; `run_blargg_ram` in `tests/common` reads it.

### Phase 2 known issues
- `mbc3-tester` and `rtc3test` ROMs not run: their pass criterion is a screenshot (needs the PPU). Not gated.
- `unused_hwio` and the other register tests only check the registers that exist; APU/PPU registers are plain storage until their phases.
- Writes to ROM/RAM during OAM DMA are dropped for the whole range below 0xFF00, which is simpler than the real per-bus conflict behaviour.
- The 13 non-gated Mooneye ROMs listed above fail.
