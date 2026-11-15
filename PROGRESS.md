# PROGRESS

**Current phase:** 7 CGB is next. Phase 6 APU is done except the Steam Deck audio check, which the owner consciously deferred on 2026-11-15 (phase 7's prerequisites allow this); it is bundled with phase 7's Deck test (see "Phase 6")
**Last updated:** 2026-11-15

## Status by phase
- [x] 0 Bootstrap
- [x] 1 CPU  (gate met 2026-11-13, see "Phase 1 — CPU: gate status")
- [x] 2 Bus, timer, cartridge  (gate met 2026-11-14, see "Phase 2" below)
- [x] 3 PPU  (gate met 2026-11-14, owner visual review done, see "Phase 3" below)
- [x] 4 Frontend, input, saves  (gate met; minor untested items: left stick, View/Select, see "Phase 4" below)
- [x] 5 Pokémon Red/Blue  (gate met with owner waivers: first gym not reached, Blue not separately tested; Yellow dropped by owner decision 2026-11-15: owner has no Yellow ROM and won't use it)
- [~] 6 APU  (gate met except the Steam Deck audio check, consciously deferred by the owner 2026-11-15; dmg_sound 10 is a documented known failure, see "Phase 6")
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
- **Test-only LY stand-in (owner decision 2026-11-14):** Mooneye's startup code busy-waits for LY = 0x8F then 0x90 (twice), so a constant LY hangs. `Bus::set_ly_stub` makes LY free-run as `(cycles / 456) % 154`; no modes/STAT/interrupts. Enabled only by `run_mooneye`. **Done in phase 3: the real PPU replaced it and `ly_stub` is deleted.**
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

## Phase 3 — PPU (DMG graphics): gate status (2026-11-14)
Environment as before (WSL Ubuntu, rustc 1.100.0).

- [x] `dmg-acid2` matches the reference exactly via `gb-cli --compare` (first attempt after sprites landed).
- [x] Mooneye `acceptance/ppu`: recorded below, 6 of 12 pass; no regression elsewhere.
- [x] Blargg `instr_timing`, `mem_timing`, `mem_timing-2`, cpu_instrs, Doctor diffs and all phase-2 Mooneye groups still pass.
- [x] **Owner review of screenshots** (human gate item, confirmed by owner 2026-11-14): see "Screenshots to review".
- [x] `cargo test` includes acid2 as an automated screenshot test (`crates/gb-cli/tests/acid2.rs`, drives the real `gb-cli` binary).

```
$ cargo test --workspace   (WSL)
test result: ok. 7 passed    (gb-cli unit)
test result: ok. 3 passed    (gb-cli tests/acid2.rs: dmg-acid2, wrong-frame-must-fail, 4 scribbltests)
test result: ok. 123 passed  (gb-core unit: ppu, ppu::render, ppu::debug, bus, ...)
test result: ok. 3 passed    (tests/blargg_timing.rs)
test result: ok. 12 passed   (tests/cpu_instrs.rs)
test result: ok. 11 passed   (tests/doctor_trace.rs, 37 s)
test result: ok. 2 passed    (tests/harness_smoke.rs)
test result: ok. 7 passed    (tests/mooneye.rs: 6 phase-2 groups + ppu recorded)
$ cargo clippy --workspace --all-targets -- -D warnings   -> clean;  cargo fmt --all -- --check -> fmt-ok

$ gb-cli run dmg-acid2.gb --frames 10 --compare dmg-acid2-dmg.png
compare passed: final frame matches .../dmg-acid2/dmg-acid2-dmg.png
$ gb-cli run dmg-acid2.gb --frames 1 --compare dmg-acid2-dmg.png     (control: must fail)
compare FAILED: 10191 pixels differ from .../dmg-acid2-dmg.png

$ scribbltests (120 frames, --compare against shipped DMG-valid references)
lycscx, lycscy, scxly, palettely: compare passed (all four)
```

### Mooneye `acceptance/ppu` scoreboard (recorded, not gated; 6 pinned as regression guards in `tests/mooneye.rs`)
| ROM | Result |
|---|---|
| intr_1_2_timing-GS | PASS |
| intr_2_0_timing | PASS |
| intr_2_mode0_timing | PASS |
| intr_2_mode3_timing | PASS |
| intr_2_oam_ok_timing | PASS |
| stat_irq_blocking | PASS |
| hblank_ly_scx_timing-GS | FAIL (needs SCX-dependent mode 3 length) |
| intr_2_mode0_timing_sprites | FAIL (needs sprite-dependent mode 3 length) |
| lcdon_timing-GS | FAIL (LCD-on first-line timing is approximate) |
| lcdon_write_timing-GS | FAIL (same) |
| stat_lyc_onoff | FAIL (LYC/LCD-off edge cases not modelled) |
| vblank_stat_intr-GS | FAIL (VBlank/mode-2 STAT quirk at line 144 not modelled) |
The failures are exactly the cycle-exact items the phase file puts out of scope; causes above are my reading of the ROM names, not yet investigated.

### Screenshots to review (owner)
Generated with `gb-cli run <rom> --frames 120 --screenshot out.png`. I viewed these and they look right: dmg-acid2 (the face, matches reference), Blargg cpu_instrs text, scribbltests `winpos` (WX/WY labels and tiled window), `fairylake` (parallax-scrolled sunrise). Please run `winpos`, `fairylake` and any ROM you like yourself; there are no reference images for those two.

### Phase 3 decisions
- **Layout:** `ppu/mod.rs` (registers, state machine, STAT), `ppu/render.rs` (`render_line`, the seam for a future pixel FIFO), `ppu/debug.rs` (tile atlas, BG maps). The Bus owns the `Ppu`, which owns VRAM and OAM. `peek` and DMA bypass access blocking; CPU `read`/`write` do not.
- **Palette:** plain greyscale `FF, AA, 55, 00` (dmg-acid2 README requires these values).
- **Timing:** mode 2 = 80 dots, mode 3 fixed at 172, then mode 0. The line is drawn at the end of mode 3, so register writes in mode 3 are not seen on that line.
- **STAT:** one shared line (OR of enabled sources), interrupt on rising edge. DMG write quirk implemented: writing STAT while in HBlank/VBlank or with LY==LYC, with the line low, requests an interrupt. OAM-scan is not part of the quirk. Not yet verified against a ROM (`stat_lyc_onoff` fails for other reasons).
- **LCD on:** restarts at line 0; line 0 has no OAM scan (mode 0 for its first 80 dots). **LCD off:** LY=0, mode 0, screen cleared, a frame boundary every 70224 dots so pacing continues.
- **Frame handling:** `Ppu::back` is drawn into and copied to the visible framebuffer when line 144 starts, so a half-drawn frame is never exposed. `GameBoy::run_frame` steps until that frame-ready flag.
- **Window:** internal line counter advances only on lines where the window was drawn, reset each frame. Drawn when LCDC.5, LY >= WY and WX <= 166.
- **Post-boot position:** the PPU starts at line 0, dot 0 (the real post-boot dot is not documented in Pan Docs). Mooneye ppu tests that pass suggest it is close enough.
- **Test placement:** the acid2 test lives in `gb-cli` (it already depends on `png`), not `gb-core`, which must stay dependency-free.
- **Debug dumps (`gb-cli`):** `--dump-tiles`, `--dump-bgmap-9800`, `--dump-bgmap-9c00`; `--compare` now also reads greyscale PNGs.

### Phase 3 known issues
- Line 153 LY quirk (LY reads 0 early on the last line), OAM bug, and sprite-dependent mode 3 length are not modelled.
- The 13 non-gated Mooneye `acceptance/` root ROMs from phase 2 were not re-run after the PPU change (their CPU-timing causes are unrelated).
- A commit (`867eae9`) was made with one failing PPU unit test; it was fixed in the next commit (`926aeda`). The intermediate commit does not pass `cargo test`.

## Phase 4 — Frontend, input, saves: status (2026-11-14)
Environment as before (WSL Ubuntu, rustc 1.100.0, system libsdl2 2.32.10). **Automated work is done; the gate's human items are open.**

- [x] `gb-sdl <rom> [--scale N] [--fullscreen] [--swap-ab] [--frames N]`: SDL window, 160x144 streaming texture, integer scale + letterbox, F11 fullscreen.
- [x] Pacing (`pacing.rs`, one function): monotonic 16.742 ms deadline, Tab = uncapped.
- [x] Keyboard and gamepad (GameController API, hot-plug, left stick, `--swap-ab`), mapping tables at the top of `input.rs`.
- [x] Saves: `<rom>.sav` beside the ROM, atomic temp+rename, flushed on quit/SIGINT/SIGTERM (SDL_QUIT), focus loss and every 30 s if changed; wrong-size save moved to `.sav.bad`.
- [x] RTC wall-clock mode (frontend only): save footer timestamp, `advance_rtc_seconds` on load. Core default stays deterministic.
- [x] Errors: missing/garbage ROM, unsupported mapper, bad save give a message and exit 1 (2 for bad arguments), no panics.

Human gate items (owner-reported 2026-11-15; I did not verify these myself):
- [x] WSL (WSLg window), Pokémon Red: runs smoothly, speed correct, keyboard works, no glitches. **Owner-confirmed.**
- [x] Manual battery save round trip in the frontend (WSL): saved in game, quit, relaunched, Continue restored it. **Owner-confirmed.**
- [x] Steam Deck, Desktop Mode (Konsole): binary built in WSL and copied, runs. **Owner-confirmed.**
- [x] Steam Deck, Game Mode with the `run.sh` setup: owner reports it "works perfectly". **Owner-confirmed, no detail.**
- [x] `docs/STEAMDECK.md` written from the working setup (steps marked confirmed/unconfirmed inside).
- [ ] Gamepad in WSL: not testable (no USB passthrough). Gamepad on the Deck is covered only by the "works perfectly" report.
- [x] Save/Continue round trip on the Deck in Game Mode: opens, saves, reopens fine. **Owner-confirmed 2026-11-15.**
- [x] Deck A = GB A, Deck B = GB B (label matching) in Game Mode. **Owner-confirmed 2026-11-15.**
- [x] Deck Menu (three-line button) = GB Start: opened the in-game menu, used to save. **Owner-confirmed 2026-11-15.**
- [x] Deck D-pad moves around the game and menus. **Owner-confirmed 2026-11-15.**
- [ ] **Open (minor):** left stick (not tested) and View/Select (not reported).
- [x] Game Mode shows the game full screen without `--fullscreen`. **Owner-confirmed 2026-11-15.** (`--fullscreen` itself is still untested in Game Mode and is not needed.)
Gate line "human confirmation it runs on the Deck in Game Mode with built-in controls": met. Remaining open items are minor: left stick and View/Select.

```
$ cargo test --workspace   (WSL)
test result: ok. 7 passed    (gb-cli unit)       test result: ok. 3 passed    (gb-cli tests/acid2.rs)
test result: ok. 124 passed  (gb-core unit)      test result: ok. 3 passed    (blargg_timing)
test result: ok. 12 passed   (cpu_instrs)        test result: ok. 2 passed    (determinism: dmg-acid2, fairylake, 600 frames x2)
test result: ok. 11 passed   (doctor_trace)      test result: ok. 2 passed    (harness_smoke)
test result: ok. 7 passed    (mooneye)           test result: ok. 5 passed    (save_roundtrip: MBC1/3/5 RAM, RTC footer+timestamp)
test result: ok. 14 passed   (gb-sdl: input, pacing, save, video)
$ cargo clippy --workspace --all-targets -- -D warnings -> clean;  cargo fmt --all -- --check -> fmt-ok

$ SDL_VIDEODRIVER=dummy gb-sdl savetest.gb --frames 300     (synthetic MBC1+RAM+battery ROM that writes 0x42 to RAM)
real 0m5.012s            (300 frames at 59.7275 Hz = 5.023 s)
$ xxd savetest.gb.sav | head -1  ->  00000000: 4200 0000 ...   (8192-byte save written on exit)
$ gb-sdl /nonexistent.gb -> "error: cannot read ROM ...", exit 1;  100-byte ROM -> "too small to contain a header", exit 1
$ gb-sdl odd.gb (123-byte .sav, expected ~8192) -> warning, moved to odd.gb.sav.bad, exit 0
```
Headless checks used SDL's dummy video driver, so they prove the loop, pacing and save flush, **not** that a window, keyboard or gamepad work; the owner's real-window results are in the checklist above.

Re-run 2026-11-15 after a clean rebuild of the workspace crates (WSL, ROMs present, only the deliberate missing-ROM smoke test prints SKIP):
```
test result: ok. 7 passed (gb-cli unit)   3 passed (acid2, 0.29s)   124 passed (gb-core unit)   3 passed (blargg_timing)
test result: ok. 12 passed (cpu_instrs, 2.38s)   2 passed (determinism)   11 passed (doctor_trace, 55.13s)
test result: ok. 2 passed (harness_smoke)   7 passed (mooneye)   5 passed (save_roundtrip)   14 passed (gb-sdl)
clippy --all-targets -D warnings: clean;  cargo fmt --all -- --check: fmt-ok
```

### Phase 4 decisions
- **Gamepad face buttons match by label** (2026-11-15): Deck A = GB A, Deck B = GB B. This is *not* the Nintendo physical layout; `--swap-ab` gives that. Earlier plan text claiming it matched the physical layout was wrong and is fixed (phase file, `input.rs` comment).
- **Game Mode launch uses a `run.sh` wrapper** (`cd` to the folder, run `./gb-sdl <rom>` with output to `~/gb_log.txt`); Steam Target = `run.sh`, Start In = the folder, Launch Options empty. Controller layout "Gamepad With Joystick Trackpad"; avoid the TF2 (keyboard/mouse) layout. Details in `docs/STEAMDECK.md`.
- **Build env:** WSL with system `libsdl2-dev` (owner choice); CI installs `libsdl2-dev`. No `bundled` feature.
- **Dependency:** `sdl2` 0.37 for `gb-sdl` (window, texture, keyboard, GameController). No clap in `gb-sdl`: hand-rolled argument parsing.
- **Save location:** `<rom>.sav` beside the ROM (not an XDG dir).
- **Opposite D-pad directions:** both held = neither reported.
- **Pacing:** timer deadline, no vsync, so it works the same on any refresh rate; phase 6 replaces `Pacer` with audio-driven pacing.
- **RTC:** the core never reads the clock; the frontend stamps the footer (`set_save_timestamp`) and advances on load. A halted RTC does not advance; a backwards clock adds 0.
- **Window events:** `--frames N` is still paced (that is how the 5.012 s figure was measured).

### Phase 4 known issues
- **Game Mode instant close, cause UNCONFIRMED.** Launching the binary directly from Steam closed immediately; the wrapper fixed it. Suspects: working directory, relative ROM path, or `--fullscreen` under Game Mode. Nobody isolated which one.
- **WSL test cache gotcha (2026-11-15):** the cached test binaries in `~/gb-target` had a stale baked-in `CARGO_MANIFEST_DIR` (`.../Desktop/Desktop/Desktop/...`), so ROM tests printed SKIP and passed in 0.00 s. `cargo clean -p gb-core -p gb-cli -p gb-sdl` fixed it. If a ROM suite finishes instantly, check for SKIP with `-- --nocapture`.
- Focus-loss flush and hot-plug code paths are untested without real devices.
- A crash (panic) does not flush the save; only quit, signals and the 30 s timer do.
- Save write failures print an error and are retried at the next flush; they do not stop the game.

## Phase 5 — Pokémon Red/Blue: status (2026-11-15)
Yellow dropped from scope by owner decision (no ROM, won't use it). `roms/` holds Red and Blue.

- [x] `gb_core::input_script`: text format `<frame> <buttons>` (`120 A+START`, `130 -`, `# end N`), parser + `Recorder`, unit-tested (round trip of every button mask). Frame n = n-th `run_frame` since boot; buttons are set before it runs.
- [x] `gb-cli run --input script [--hash-at N,N] [--screenshot-at N,N --screenshot-dir DIR]`; without `--frames` it runs to the script's `# end`.
- [x] `gb-sdl --record FILE`: logs your button changes by frame. **Recording ignores the .sav file (neither read nor written)** so it starts from a fresh boot and the headless replay is identical; your real save is never touched (checked: .sav mtime unchanged).
- [x] `tests/pokemon_local.rs`: for each `tests/expected/local/<name>.hashes` + `<name>.input` + `roms/<name>.gb`, replays and checks hashes; SKIPs if anything is absent. No hash files exist yet (they need owner-approved screenshots).
- [x] Boot: Red and Blue headless, 900 frames: reach the intro cutscene (Gengar vs Nidorino / Jigglypuff), which is correct boot behaviour. Screenshots viewed by me only.
- Determinism check: same script replayed twice on Red gave identical hashes (`frame 200: 6b70ab86acc6fafa`, `frame 400: 4d2cb65c748170f7`).

Next (needs the owner): record a new-game playthrough with `gb-sdl --record`.

### Phase 5 — Red recording #1 (2026-11-15)
- Owner played ~13.6 min from a fresh boot with `gb-sdl --record` (48,789 frames): new game, Oak intro, name entry, rival fight (won), Rattata fight (won), fled from wild Pidgey and Rattata. Owner reports no glitches; sprites appear a moment after entering a map (very likely normal Gen 1 behaviour, not confirmed against another emulator).
- Replay is deterministic (twice: final hash `54ccc208a5b8d74f`). I viewed screenshots at frames 3000 (Oak: "So your name is RED!"), 10000 (Pallet Town), 25000 (rival fight, Squirtle 4/22, "RED defeat…"), 48789 (grass area with sign and ledge). Owner confirmed these match the session.
- Approved hashes (frames 3000/10000/25000/48789) are in `tests/expected/local/pokemon_red.hashes`, script in `pokemon_red.input`.
```
$ cargo test -p gb-core --test pokemon_local -- --nocapture
pokemon_red: 4 hashes match after 48789 frames
test result: ok. 2 passed; 0 failed   (24.17 s)
```
- **Soak (partial):** that 13.6-minute replay ran to the end with no panic or hang, so the "10+ minutes" requirement is met for Red. Not yet done for Blue.
- **Per-game hack grep:** searched `crates/` for "pokemon" and for `title ==`/`.title` uses. Only hits: header parsing, one unit-test fixture titled "POKEMON RED", and printing the title in the CLI/frontend. No per-game code.
- **Bugs found so far:** none.

### Phase 5 — Red recording #2 and gate (2026-11-15)
- Owner played ~22.4 min (80,740 frames) from a fresh boot: rival in Oak's lab, starter Squirtle, levelling, a wild Pidgey fight, walking on routes. Owner reports no glitches. Replay deterministic twice (final hash `c28d93c11a5d06d5`). I viewed frames 15000, 30000, 45000, 60000, 75000, 80740; owner approved. Hashes in `tests/expected/local/pokemon_red.run2.hashes` (harness now accepts `<rom>.<label>.hashes`).
```
$ cargo test --workspace   (WSL, ROMs present)
gb-cli 7 + 3 passed; gb-core unit 128; blargg_timing 3; cpu_instrs 12; determinism 2; doctor_trace 11 (51 s);
harness_smoke 2; mooneye 7; pokemon_local 2 (65 s); save_roundtrip 5; gb-sdl 16 -> all "test result: ok", 0 failed
$ cargo clippy --workspace --all-targets -- -D warnings -> clean;  cargo fmt -> applied
pokemon_red: 4 hashes match after 48789 frames
pokemon_red.run2: 6 hashes match after 80740 frames
```
Total recorded play replayed in tests: ~36 min emulated, no panic or hang.

### Phase 5 gate
- [~] Red plays from new game with no visible glitches: owner played ~36 min over two recordings (intro, name entry, rival fights, wild battles, menus, routes), no glitches. **Owner waived reaching the first gym (2026-11-15, "no need to reach the gym")**: the gate wording asked for it; this is an owner scope decision, not a test I ran.
- [x] Blue: owner states it runs on the Deck and that if Red does both do; **not separately recorded or tested by me** (owner decision: no Blue run needed).
- [x] Save → quit → relaunch → Continue on desktop and Deck: **owner-reported** working on both (Red confirmed in phase 4; owner says the same holds for Blue). Not verifiable by replay since recordings ignore saves.
- [x] No panics/hangs over a long scripted soak (replays of 48,789 and 80,740 frames).
- [x] Bugs fixed: none were found, so nothing to log.
- [x] No per-game special cases (grep, see above).

## Phase 6 — APU: status (2026-11-15)
Environment as before (WSL Ubuntu, rustc 1.100.0). **Automated gate items are done; the human items are open.**

- [~] Blargg `dmg_sound` individual ROMs: **11 of 12 pass**. `10-wave trigger while on` fails (documented below, listed in `KNOWN_FAILING` in `tests/dmg_sound.rs`, which fails if it ever starts passing).
- [ ] WAV dumps look/sound right: peak/RMS and pitch are checked by tests; **owner must listen** to `C:/Users/jacop/Desktop/red_title_2400frames.wav` (40 s of Pokémon Red from boot: intro and title music; made with `gb-cli run roms/pokemon_red.gb --frames 2400 --wav ...`).
- [~] Owner confirms Pokémon audio on desktop and Steam Deck. **Desktop (WSLg), owner-reported 2026-11-15: music sounds right; one audible hiccup on the "POKEMON RED" title screen, otherwise fine.** Printed stats of that ~69 s session: `audio: 4119 frames paced, queue fill min/avg/max 751/2139/2400 stereo frames (target 2400), 1 underruns, 0 overflows` (the one underrun matches the one hiccup; cause unknown, possibly a WSLg audio-bridge or scheduling stall, not investigated). **Steam Deck: not tested, owner cannot test now; deferred (not waived).**
- [~] Pacing is audio-driven; the headless soak below shows a stable queue over 10 minutes. Real-device drift not verified.

```
$ cargo test -p gb-core --test dmg_sound -- --nocapture
01-registers: PASS   02-len ctr: PASS   03-trigger: PASS   04-sweep: PASS   05-sweep details: PASS
06-overflow on trigger: PASS   07-len sweep period sync: PASS   08-len ctr during power: PASS
09-wave read while on: PASS   10-wave trigger while on: FAIL (known)   11-regs after power: PASS
12-wave write while on: PASS          test result: ok. 1 passed; 0 failed

$ cargo test --workspace   (WSL, ROMs present)  -> every "test result" line ok, 0 failed:
gb-cli 9 + 3 (acid2) + 1 (wav); gb-core unit 135; audio_determinism 1 ("290122 samples identical");
blargg_timing 3; cpu_instrs 12; determinism 2; dmg_sound 1; doctor_trace 11; harness_smoke 2; mooneye 7;
pokemon_local 2 ("pokemon_red: 4 hashes match after 48789 frames", "pokemon_red.run2: 6 hashes match after 80740 frames");
save_roundtrip 5; gb-sdl 20.   clippy --all-targets -D warnings: clean;  cargo fmt --check: fmt-ok

$ SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy gb-sdl roms/pokemon_blue.gb --frames 35836 --record /tmp/x   (10 min of emulated time)
audio: 35835 frames paced, queue fill min/avg/max 1889/2144/2400 stereo frames (target 2400), 0 underruns, 0 overflows
real 9m31.247s
(60 s run: min/avg/max 1889/2144/2400, 0 underruns, real 57.5 s for 60.27 s nominal)
```
Wall time is 4.8 % *shorter* than nominal in both runs, with identical queue statistics: a constant offset, not drift. My explanation (not verified) is that SDL's dummy audio driver ticks slightly fast; a real device is the owner's check.

### Phase 6 decisions
- **Layout:** `apu/mod.rs` (registers, power, frame sequencer, mixer, sampling), `common.rs` (length counter, envelope), `square.rs` (with sweep), `wave.rs`, `noise.rs`. The bus ticks the APU once per M-cycle and passes the timer's 16-bit counter (`Timer::counter()`); the frame sequencer steps on the falling edge of counter bit 12, also right after a DIV write.
- **Stepping:** channels advance per T-cycle (4 steps per M-cycle).
- **Mixing:** DAC-on channels map digital 0..15 to -1..1 (DAC off = 0), summed with NR51 panning and NR50 volume, divided by 8 (6 dB headroom: a first version with /4 peaked at 1.2 and would clip), then a high-pass filter (0.999958 per T-cycle). Each output sample is the average of the mix over its T-cycles (box filter). 48 kHz default via `set_sample_rate`; the internal buffer is capped at 1 s if never drained.
- **DMG only:** length counters survive power-off and are writable while off; wave RAM access while the channel plays follows the DMG rule (only at the fetch cycle).
- **Frontend pacing:** SDL audio queue (f32, 48 kHz stereo, 50 ms target). After each frame the loop sleeps until the queue drains to the target, so the sound card's clock sets the speed; there is no separate rate-adjust factor, the game simply follows the card. A queue over 250 ms is dropped; waits are capped at 100 ms. Tab (fast-forward) discards audio. No device or `--no-audio` falls back to the timer `Pacer`.
- **Keys:** 1-4 mute channels, +/- volume (default 100 %). **No new dependencies** (the WAV header is written by hand).
- **Process note:** commit `a0d9fa9` went in with a clippy error in a new test (fixed in the next commit); the later commit `189c5a1` has a misleading message: it only contains the Steam Deck doc note, PROGRESS.md follows in the next commit.

### Phase 6 known issues
- **dmg_sound 10 (wave retrigger corruption) fails.** Tunables: `TRIGGER_DELAY_T` and `ACCESS_WINDOW_T` in `wave.rs`. A first-sample delay of 5 or 6 passes 09 and 12 but not 10; a delay of 4 passes 10 but breaks 09 and 12. Varying the corruption window and a separate retrigger delay never fixed it. I stopped after a focused grid search; hypothesis: real hardware timing is finer than my 4-T bus granularity, so one delay cannot satisfy both groups. Games should not depend on this.
- Not implemented (obscure): envelope "zombie mode", exact first-sample delays for squares/noise, CGB sound differences (phase 7/8).
- `gb-cli --wav` assumes 48 kHz (the core default).
- Test suite time grew: `pokemon_local` now takes ~175-195 s (was ~65 s) because the APU runs every T-cycle.

### Phase 6: what I need from the owner
1. Listen to `C:/Users/jacop/Desktop/red_title_2400frames.wav` (I put it there; delete it when done). Does the title theme sound right (pitch, tempo, no harshness)?
2. Run `gb-sdl roms/pokemon_red.gb` on desktop (WSLg): is the music right in the intro/title and in a battle, any crackle? Try keys 1-4 and +/-. Quit normally and send me the printed `audio:` line.
3. Same on the Steam Deck (Game Mode: the line lands in `~/gb_log.txt`).
4. Decide whether you want more effort on dmg_sound 10 (I recommend not).
