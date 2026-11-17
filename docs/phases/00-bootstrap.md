# Phase 0 — Bootstrap

## Goal
A working workspace with build, lint, CI and a headless test harness skeleton, so every later
phase has a place to plug tests in. No emulation yet.

## Prerequisites
Rust toolchain installed (`rustup`), git repo initialised.

## Tasks
1. Create the Cargo workspace: `gb-core` (lib), `gb-cli` (bin), `gb-sdl` (bin, may be an empty `main` for now).
2. `.gitignore`: `target/`, `roms/`, `tests/roms/`, `*.sav`, `*.ppm`, `*.png` outputs (except `tests/expected/`).
3. `rustfmt.toml` (defaults are fine) and clippy configured to deny warnings in CI.
4. `scripts/fetch_test_roms.sh`: downloads free test ROMs into `tests/roms/<suite>/`.
   - Idempotent (skips files already present), verifies downloads are non-empty.
   - Sources are listed in `docs/REFERENCES.md`. Check each URL and licence before scripting it.
   - If the network is unavailable, print instructions and exit 0. Tests must skip, not fail.
5. `gb-core` skeleton: `Cartridge::from_bytes` (header parsing + checksum validation only),
   a `GameBoy` struct with the public API from `CLAUDE.md` §4.1 stubbed with `todo!()`-free placeholders
   (return zeros / empty), and a `Model` enum.
6. `gb-cli` with subcommand `run <rom>` and these options (implement as far as the core allows; stub the rest with clear errors):
   - `--frames N` / `--max-instructions N`: stop condition
   - `--serial`: print serial output to stdout
   - `--expect-serial "<text>"`: exit code 0 only if the text appeared in serial output
   - `--mooneye`: run until `ld b,b` executes, then check Fibonacci signature in B,C,D,E,H,L
   - `--screenshot out.png`: dump the framebuffer at stop
   - `--compare ref.png`: exit non-zero if the final frame differs from the reference (report pixel diff count)
   - `--trace out.log`: write a Gameboy Doctor style trace line per instruction
     (`A:.. F:.. B:.. C:.. D:.. E:.. H:.. L:.. SP:.... PC:.... PCMEM:xx,xx,xx,xx`)
7. A Rust integration test helper in `crates/gb-core/tests/common/` that runs a ROM headlessly and
   returns serial output / final framebuffer, and that **skips with a printed message** if the ROM file is absent.
8. GitHub Actions workflow: build, `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`,
   plus a step running `scripts/fetch_test_roms.sh` (allowed to no-op offline).
9. `README.md`: what the project is, how to build, how to run, ROM legal note.

## Design notes
- Keep `gb-core` free of file and terminal I/O. The CLI does all the I/O.
- The harness is the most valuable thing you build in this phase. Make its failure output readable:
  show the last serial output, the PC, and the instruction count at stop.

## Gate (definition of done)

> **Status: done.** Gate evidence, owner waivers and measured results are recorded in `PROGRESS.md` (section "Phase 0"). The boxes below are the original checklist and were not ticked individually.
- [ ] `cargo build --workspace`, `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace` all pass.
- [ ] `scripts/fetch_test_roms.sh` has been run and the test ROM directories exist (or the human confirmed offline mode).
- [ ] `gb-cli run <rom>` loads a ROM, prints the parsed header (title, cart type, sizes, checksum OK/bad) and exits cleanly.
- [ ] CI configuration committed.
- [ ] `PROGRESS.md` updated and phase 1 marked as current.

## Out of scope
Any CPU, PPU, or audio implementation.
