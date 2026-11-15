# Phase 5 — Pokémon Red / Blue compatibility

## Goal
Pokémon Red and Blue boot, play correctly for an extended session, and save/load reliably.
This is a **compatibility and debugging** phase: most code already exists; the work is finding and fixing
what real games need beyond test ROMs. No per-game hacks (see `CLAUDE.md` §7).

## Prerequisites
Phases 0–4 gates met. **Human action required:** the human places their own legitimately dumped ROM(s) in `roms/`
(e.g. `roms/pokemon_red.gb`). Do not search for, download, or suggest sources for commercial ROMs.
If no ROM is present, do only the preparation tasks (1–2) and ask the human.

## Facts to confirm from the header (do not hardcode)
- Red/Blue: cartridge type 0x13 (MBC3 + RAM + BATTERY), 1 MiB ROM, 32 KiB RAM.
- Neither uses the RTC. Both run on the DMG.
- (Yellow, MBC5, was dropped from scope by the owner on 2026-11-15: no ROM available.)

## Tasks
1. **Local-only ROM test harness**: `cargo test` tests that look for `roms/*.gb`, and **skip if absent**.
   Each runs N frames headless, feeds a scripted input sequence (from a text file: frame number + buttons), and
   checks a framebuffer hash at chosen frames. Hashes are recorded by the human after visually verifying
   screenshots, stored in `tests/expected/local/` (hash text files only, never images of the game if the repo is public).
2. **Input script support** in `gb-cli`: `--input script.txt`, deterministic, same format as the test harness.
3. **Boot sequence**: verify the game reaches the title screen. If not, debug with trace + screenshots (see `CLAUDE.md` §8).
4. **Gameplay checklist** (human confirms; agent supports with screenshot dumps and debugging):
   - Title screen, intro, name entry, Oak's speech.
   - Walking, scrolling maps, doors/transitions, NPC sprites.
   - Menus and text rendering, text speed settings.
   - Battle entry transition animation, battle menus, attack animations, HP bar drain.
   - Start menu → Save → quit emulator → relaunch → Continue works (save integrity).
   - Pokédex, party screen, bag, PC.
   - Link cable features are expected **not** to work (serial with no partner must not hang the game).
5. **Debugging playbook for typical Pokémon issues**:
   - Blank/garbage screen at boot → CPU or MBC bank bug. Trace and compare against a reference emulator's behaviour.
   - Freeze on the "game freak" screen or title → interrupt/timer/HALT issue.
   - Flickering/missing sprites in battle → OAM DMA timing, sprite priority, or per-line sprite limits.
   - Graphical glitches on screen transitions → PPU mode timing, STAT interrupts, mid-frame register writes.
   - Save corrupt or lost → RAM enable gating, bank selection, flush logic.
   - Hang when using link cable menu → serial with no partner must complete or time out, as on hardware.
6. Fix root causes in the relevant subsystem and add a **unit or Mooneye-style regression test** for each fix when feasible.
7. Fast-forward through a long play session using scripted input (e.g. 10+ minutes of emulated time) to catch rare bugs
   (crashes, desyncs, save problems).

## Gate (definition of done)
- [ ] Human confirms Red (or Blue) plays from new game through the first gym with no visible glitches. (Yellow dropped by owner decision, 2026-11-15.)
- [ ] Save → quit → relaunch → Continue verified by the human on desktop **and** Steam Deck.
- [ ] No panics, no hangs over a long scripted soak run.
- [ ] Every bug fixed has a note in `PROGRESS.md` (symptom → cause → fix → regression test if any).
- [ ] No per-game special cases anywhere in the code (agent greps for ROM titles/checksums to confirm).

## Out of scope
Sound (phase 6), colour (phase 7), link cable, Pokémon Gold/Silver/Crystal.
