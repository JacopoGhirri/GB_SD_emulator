# CLAUDE.md — Game Boy Emulator (Rust)

You are a builder agent working on a Game Boy / Game Boy Color emulator written in Rust.
Read this file fully before every session. Then read `PROGRESS.md` to find the current phase,
and read that phase's file in `docs/phases/`.

## 1. Project goal

Build an emulator that:
1. Passes the standard community test ROMs (Blargg, Mooneye, acid2) for the DMG, then CGB.
2. Plays Game Boy / Game Boy Color Pokémon games (Red/Blue/Yellow, Gold/Silver/Crystal) correctly,
   including battery saves and the real-time clock.
3. Runs on a Steam Deck (Linux, x86_64) with gamepad input and sound.

The human owner is an ML PhD who codes comfortably but is not a professional software engineer.
This is a learning project: keep code readable, explain non-obvious hardware behaviour in comments,
and prefer the simple correct design over the clever one.

**Non-goals:** Game Boy Advance (separate future project), netplay, debugger GUI, shaders,
supporting every obscure cartridge mapper, bit-perfect accuracy for obscure edge cases
(see phase 8 for how far accuracy goes).

## 2. Tech stack

- Language: Rust (stable). Edition 2021 or later.
- Cargo workspace with three crates:
  - `gb-core` — the emulator. **No dependencies** (std only). No I/O, no threads, no globals, no `unsafe`.
  - `gb-cli` — headless runner used for tests and tracing. May use `png` and `clap`.
  - `gb-sdl` — desktop/Steam Deck frontend using the `sdl2` crate (video, audio, gamepad).
- Adding any other dependency requires a one-line justification in `PROGRESS.md`.

## 3. Repository layout

```
Cargo.toml                 workspace
CLAUDE.md                  this file
PROGRESS.md                living status log (you update it)
docs/REFERENCES.md         where to look things up
docs/phases/NN-*.md        one instruction file per phase
crates/gb-core/src/
  lib.rs                   public API (GameBoy struct)
  cpu/                     SM83 CPU: registers, decode, ops
  bus.rs                   memory map, ties components together
  cartridge/               header parsing + mappers (mbc1, mbc3, mbc5, ...)
  ppu/  apu/  timer.rs  joypad.rs  serial.rs  dma.rs  interrupts.rs
crates/gb-cli/             headless runner
crates/gb-sdl/             frontend
scripts/                   fetch_test_roms.sh, helper scripts
tests/roms/                third-party test ROMs (fetched, gitignored)
tests/expected/            reference screenshots / logs we are allowed to commit
roms/                      user's own game ROMs. GITIGNORED. NEVER COMMIT.
```

## 4. Core architecture rules (important — decided up front)

### 4.1 Public API of `gb-core`
```rust
GameBoy::new(cart: Cartridge, model: Model) -> GameBoy      // Model::Dmg | Model::Cgb
gb.step() -> u32              // execute one CPU instruction (or one idle HALT step); returns T-cycles elapsed
gb.run_frame()                // run until a frame is complete
gb.framebuffer() -> &[u32]    // 160*144 pixels, 0x00RRGGBB
gb.set_buttons(Buttons)       // joypad state
gb.take_serial() -> Vec<u8>   // bytes written via serial port (test ROMs print here)
gb.save_ram() / gb.load_save_ram(..)
gb.drain_audio(&mut Vec<f32>) // interleaved stereo samples (from phase 6)
```
Emulation is **deterministic**: same ROM + same inputs ⇒ same output. No wall-clock, no randomness
(except where hardware is genuinely random, and then seed it fixed).

### 4.2 Timing model: cycle-stepped bus (do this from day one)
Every memory access performed by the CPU advances the whole system by **one M-cycle (4 T-cycles)**.
Concretely: `bus.read(addr)` and `bus.write(addr, v)` each call `self.tick()` which steps timer, PPU,
APU, DMA, serial for 4 T-cycles. Instructions that have internal delay cycles call `tick()` explicitly.

Do **not** implement "execute instruction, then add its cycle count to the other components".
That design passes simple tests but cannot pass timing tests, and retrofitting is a rewrite.
Instruction cycle counts must *emerge* from the sequence of bus accesses; `instr_timing` and
`mem_timing` test ROMs verify this.

### 4.3 Structure
- One `Bus` struct owns every component (cartridge, PPU, APU, timer, WRAM, HRAM, joypad, serial, DMA, interrupt regs).
  The CPU gets `&mut Bus`. No `Rc<RefCell<>>` soup.
- Cartridge mappers implement a small trait (`read_rom`, `write_rom`, `read_ram`, `write_ram`, `save_data`, `load_save`).
- Opcode decoding: a `match` on the opcode, using small helper functions. Prefer generated-looking
  regularity (registers indexed 0..7 = B,C,D,E,H,L,(HL),A) over 500 hand-written arms.
- Register values at power-up (no boot ROM emulated) are documented in Pan Docs "Power-Up Sequence".
  Initialise to those values; do not guess.

### 4.4 Code style
- `cargo fmt` and `cargo clippy -- -D warnings` must pass before every commit.
- Comments explain *hardware* reasons ("DIV is the upper byte of a 16-bit counter"), not what the code does.
- Link the Pan Docs section in a comment when implementing a non-obvious register or quirk.
- No `unwrap()` on user-controlled data (ROM contents, save files). Return errors.
- No magic numbers for register addresses: use named constants.

## 5. Commands

```
cargo build --workspace
cargo test --workspace                       # unit tests + any ROM tests whose ROMs are present
cargo clippy --workspace -- -D warnings
cargo fmt --all
scripts/fetch_test_roms.sh                   # downloads test ROMs into tests/roms/
cargo run -p gb-cli -- run <rom> [options]   # headless (see phase 0 for options)
cargo run -p gb-sdl --release -- <rom>       # desktop frontend
```

## 6. Testing philosophy

Tests are the source of truth for hardware correctness. Your memory of hardware behaviour is not.
- Unit tests for self-contained logic (ALU flags, timer edge cases, mapper bank math).
- ROM tests via `gb-cli` and `cargo test`: each test ROM has a defined pass criterion (serial text,
  Mooneye register signature, or screenshot comparison). See the phase files.
- ROM tests **skip with a clear message** if their ROM is not present, rather than fail.
- When adding a feature, first find which test ROM exercises it, make it fail, then make it pass.
- A phase is finished only when its **gate** (listed at the end of each phase file) is met.
  Do not start the next phase before the gate passes. Do not weaken a test to pass a gate.

## 7. Workflow rules for builder agents

1. At session start: read this file, `PROGRESS.md`, and the current phase file.
2. Work in small steps. Commit after each meaningful milestone with a clear message
   (e.g. `cpu: implement 16-bit load group`).
3. Keep `PROGRESS.md` current: what's done, what's failing, known issues, decisions made, open questions.
4. **Never claim a test passes without running it.** Paste the actual result line in `PROGRESS.md`.
5. If a test fails and you can't find the cause after a focused effort, write up the symptom,
   what you tried and your best hypothesis in `PROGRESS.md`, then ask the human. Do not thrash.
6. Hardware facts come from `docs/REFERENCES.md` sources (Pan Docs first). If your memory and
   the docs disagree, the docs win. If the docs are ambiguous, a test ROM or a reference emulator's
   behaviour decides, and you record the finding as a comment.
7. Prefer fixing the root cause over special-casing a game or ROM. No per-game hacks. Ever.
8. Stay in scope: do not implement features from later phases "while you're here".
9. Ask the human only for things only they can provide (see section 9). Otherwise decide and note it.

## 8. Debugging playbook

When behaviour is wrong, in order:
1. **Reproduce headlessly** with `gb-cli`, deterministic, with a frame or instruction limit.
2. **Trace diff.** For CPU bugs, use Gameboy Doctor style traces: one line per instruction with all
   registers + PC + the next 4 bytes, diffed against a known-good log. The first differing line is the bug.
3. **Narrow with test ROMs.** Blargg's individual ROMs (`01-special` … `11-op a,(hl)`) isolate instruction groups.
4. **Dump state.** `gb-cli` should be able to dump the framebuffer as PNG, VRAM/OAM as hex, and IO registers.
5. **Compare against a reference emulator** (SameBoy, mGBA, or Gambatte) only for behaviour, not by
   copying code. Note the behaviour in a comment.
6. Add a regression test for every bug found, if at all possible.

## 9. Things only the human can do (ask when you reach them)

- Place their own dumped game ROMs in `roms/` (never download or suggest downloading commercial ROMs).
- Test on the Steam Deck hardware and report back (phase 4, 7, 8).
- Judge subjective things: audio quality, input feel, visual correctness of a real game.
- Decide scope trade-offs (e.g. how much accuracy work in phase 8).

## 10. Legal / ROM policy

- Writing an emulator is legal. Commercial ROMs are copyrighted.
- Never commit commercial ROMs or save files. `roms/` and `*.sav` are gitignored.
- Never include code that downloads commercial ROMs. `scripts/fetch_test_roms.sh` fetches only
  freely distributable test ROMs.
- Do not copy source code from other emulators; their *behaviour* may be used as a reference.
  Check licences before reusing anything.
- No boot ROM is bundled. The emulator initialises to the post-boot state directly.

## 11. Phase index

| Phase | File | Outcome | Gate (short) |
|---|---|---|---|
| 0 | `docs/phases/00-bootstrap.md` | Workspace, harness, CI, test ROM fetch | harness runs, CI green |
| 1 | `docs/phases/01-cpu.md` | Full SM83 CPU, cycle-stepped | Blargg `cpu_instrs` all pass |
| 2 | `docs/phases/02-bus-timer-cartridge.md` | Bus, interrupts, timer, DMA, MBC1/3/5 | `instr_timing`, `mem_timing`, Mooneye timer/MBC tests |
| 3 | `docs/phases/03-ppu.md` | DMG graphics | `dmg-acid2` matches reference |
| 4 | `docs/phases/04-frontend-input-saves.md` | SDL window, input, saves, Steam Deck run | Homebrew/Tetris-class game playable |
| 5 | `docs/phases/05-pokemon-red-blue.md` | Pokémon Red/Blue/Yellow compatibility | Full play session incl. save/reload |
| 6 | `docs/phases/06-apu.md` | Audio | Blargg `dmg_sound` mostly pass, audio sounds right |
| 7 | `docs/phases/07-cgb.md` | Game Boy Color | `cgb-acid2`, Pokémon Crystal plays |
| 8 | `docs/phases/08-polish-accuracy.md` | Quality, packaging, accuracy scoreboard | Documented scoreboard + Deck package |

Keep `PROGRESS.md` pointing at the current phase.

## Commit Conventions
Never add "Co-Authored-By" lines to commits. Do not include Claude attribution in commit messages, PR descriptions, or any git metadata.
