# Phase 8 — Polish, Accuracy Scoreboard, Packaging

## Goal
Turn a working emulator into a finished project: measure accuracy honestly, fix the highest-value gaps,
add quality-of-life features, package for the Steam Deck, and document everything.

## Prerequisites
Phase 7 gate met. The human decides how deep to go on accuracy (**ask before starting the accuracy work**:
"how much time do you want to spend on accuracy versus features?").

## Tasks

### A. Accuracy scoreboard
1. Script `scripts/run_all_tests.sh` that runs every test ROM suite available and prints a table (suite, pass/fail counts).
   Commit the latest table in `docs/ACCURACY.md`.
2. Triage failures by value: failures that affect real games first (e.g. mid-scanline effects, STAT/timer edge cases),
   obscure hardware-bug tests last.
3. Optional deep work, only if the human opts in (each independently valuable, each with its own test gate):
   - Pixel-FIFO PPU replacing the scanline renderer (variable mode-3 length, mid-line register changes).
   - OAM bug and other DMG quirks.
   - Correct DMG compatibility palettes for monochrome games on CGB.
   - Sub-M-cycle timing refinements for the timer/PPU interaction.

### B. Quality-of-life
4. **Save states** (versioned binary format; deterministic core makes this straightforward): quick save/load keys, multiple slots.
5. **Config file** (TOML or similar) for key mapping, scale, palette, volume, speed multiplier. Sensible defaults without a file.
6. Pause, reset, screenshot (F12), fast-forward toggle, slow-motion.
7. Selectable DMG palettes (grey, classic green, a few others).
8. Optional: rewind (ring buffer of save states), simple frame-blending to reduce flicker in games that flicker sprites.

### C. Steam Deck packaging
9. A release build script producing a self-contained directory (binary + README + default config).
   Test launching from Game Mode via a Non-Steam shortcut, with and without keyboard attached.
10. Steam Input considerations: provide a recommended controller layout in `docs/STEAMDECK.md`, including a way to
    reach save-state/fast-forward/quit via back buttons or a combo.
11. Performance check: confirm emulation uses a small fraction of one core at 1× (profile with `perf` or a flame graph if not);
    low CPU use matters for battery life. Fix hot spots only if numbers justify it.

### D. Documentation and hygiene
12. `README.md` with screenshots (of test ROMs or homebrew only, not commercial games), features, controls, build instructions.
13. `docs/ARCHITECTURE.md`: components, the cycle-stepped bus design, and how to add a mapper or test suite.
14. CI: run all tests that need only freely available ROMs; keep clippy/fmt gates.
15. Code review pass: remove dead code, check comments reference Pan Docs sections for non-obvious logic, audit for `unsafe` (should be none) and panics on bad input.
16. Fuzz-ish robustness test: random/corrupt ROM and save inputs must never panic or hang (return errors).

## Gate (definition of done)
- [ ] `docs/ACCURACY.md` has a current, honest scoreboard produced by the script.
- [ ] Save states work (round-trip determinism test: save, run N frames, load, run N frames ⇒ identical framebuffer hash).
- [ ] Steam Deck package launches from Game Mode and the human confirms controls, audio, saves, and exit all work.
- [ ] README, ARCHITECTURE, STEAMDECK docs complete and accurate.
- [ ] All CI gates green; robustness test passes.

## What's next (for the human, not the agent)
- Game Boy Advance support is now **phase 9** (`docs/phases/09-gba.md`), added 2026-11-15. When doing the packaging and config
  tasks here, keep them core-agnostic (the frontend will pick a core by file extension) so phase 9 does not have to redo them.
- Or: use the deterministic core for ML experiments (RL agent playing Pokémon through the headless API).
