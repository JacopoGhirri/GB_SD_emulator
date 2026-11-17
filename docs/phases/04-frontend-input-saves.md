# Phase 4 — Frontend, Input, Saves, Steam Deck

## Goal
A playable desktop emulator: window, keyboard and gamepad input, persistent battery saves,
correct frame pacing. Runs on the Steam Deck.

## Prerequisites
Phase 3 gate met. SDL2 available (`libsdl2-dev` on Debian/Ubuntu; on SteamOS/Arch it is already present
at runtime, and the `sdl2` crate's `bundled` feature can build it if needed).

## Tasks
1. `gb-sdl` binary: `gb-sdl <rom.gb> [--scale N] [--fullscreen]`.
2. **Video**: SDL window, streaming texture 160×144 (RGBA), integer scaling by default with letterboxing
   (important for the Deck's 1280×800 screen: integer scale 5 → 800×720). Nearest-neighbour filtering.
3. **Timing**: one emulated frame = 70224 dots ≈ 16.74 ms (59.7275 Hz). For now pace using vsync plus a
   frame limiter. Phase 6 will switch to audio-driven pacing; keep the pacing in one function so it is easy to replace.
4. **Keyboard**: arrows = D-pad, Z = B, X = A, Enter = Start, Backspace/RShift = Select, Esc = quit,
   F11 = fullscreen, Tab (hold) = fast-forward (uncapped). Make the mapping a table at the top of the file.
5. **Gamepad** via SDL `GameController` API (maps Steam Deck controls and most pads automatically):
   D-pad and left stick → D-pad, **Deck B (east) → GB B and Deck A (south) → GB A** (matches by
   *label*, not by the Nintendo physical layout; `--swap-ab` gives the physical layout), Start/Select, **Guide/Steam button combos left to Steam**.
   Hot-plug support.
6. **Save files**: battery RAM (+RTC for MBC3) written to `<rom>.sav` beside the ROM
   (or an XDG data dir; pick one and document it). Load on start if present. Flush on exit, on
   focus loss, and every ~30 s if changed. Write atomically (temp file + rename) so a crash cannot corrupt a save.
7. **RTC wall-clock mode**: for MBC3 carts in the real frontend, advance the RTC by real elapsed time
   between sessions (store a timestamp in the save). Keep the deterministic cycle-based RTC for tests.
8. **Error handling**: bad ROM, unsupported mapper, corrupt save → clear message, no panic.
9. **Steam Deck run** (human step): build on the Deck in Desktop Mode (install Rust via `rustup` into the home directory,
   no root filesystem changes needed) or cross-build and copy the binary. Add `gb-sdl` as a
   Non-Steam Game, set launch options to include the ROM path, and use a gamepad controller template.
   Write `docs/STEAMDECK.md` with exactly the steps that worked.

## Verification
- Use a free homebrew game or a test ROM with visible output (human checks). If the human owns a Game Boy
  cartridge ROM they have dumped, they may try it, but nothing in the repo depends on it.
- Automated: a headless smoke test that runs 600 frames of a homebrew ROM and checks the framebuffer hash is stable
  across two runs (determinism).

## Known pitfalls
- Run emulation on one thread; the frontend loop calls `run_frame()` once per displayed frame.
- Don't lose input events during frame execution: poll SDL events once per frame, and set buttons before `run_frame()`.
- Opposing D-pad directions pressed simultaneously are impossible on real hardware; consider filtering them
  (some games glitch). Document the choice.
- `.sav` must be flushed even on `SIGTERM`/window close, not only on a clean Esc.

## Gate (definition of done)

> **Status: done.** Gate evidence, owner waivers and measured results are recorded in `PROGRESS.md` (section "Phase 4"). The boxes below are the original checklist and were not ticked individually.
- [ ] `gb-sdl` runs a homebrew/test game at the correct speed, controls work with keyboard **and** a gamepad.
- [ ] Battery save round-trip works (write → quit → relaunch → data intact), tested automatically for the mapper level and manually in the frontend.
- [ ] Human confirmation that it runs on the Steam Deck in Game Mode with built-in controls (record in `PROGRESS.md`).
- [ ] `docs/STEAMDECK.md` written.

## Out of scope
Audio, shaders, save states, config UI.
