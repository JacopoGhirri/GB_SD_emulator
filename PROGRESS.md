# PROGRESS

**Current phase:** 1 — CPU gate met on 2026-11-13 (awaiting owner sign-off before phase 2)
**Last updated:** 2026-11-13

## Status by phase
- [x] 0 Bootstrap
- [x] 1 CPU  (gate met 2026-11-13, see "Phase 1 — CPU: gate status")
- [ ] 2 Bus, timer, cartridge  ← next (MUST replace the phase-1 minimal timer and minimal MBC1)
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
- **CI has not run.** `.github/workflows/ci.yml` is written but not executed. No YAML parser is installed on this machine (no Python, Ruby or Perl YAML module), so the file is only checked by reading it. The first push to GitHub is the real test. The steps mirror the local commands above.
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
- `unzip` is not installed in this WSL distro, so `scripts/fetch_*.sh` were run from Git Bash on Windows (the tree is shared). CI (ubuntu) has `unzip`.
- `--mooneye` is still unimplemented (exit 2); it belongs to phase 2's gate.
- `halt_bug` ROM and the other Blargg suites (`instr_timing`, `mem_timing`, `halt_bug`) have not been run; they are phase 2 gates. Only the HALT-bug unit test exists.
- CI (`.github/workflows/ci.yml`) has still never run on GitHub.

## Phase 2 reminders
- [ ] **Replace the phase-1 minimal timer with the accurate falling-edge implementation.**
- [ ] **Replace the phase-1 minimal MBC1 with the full mapper (and MBC3/MBC5).**
