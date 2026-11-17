# Phase 9 — Cleanup: dead code and documentation

## Goal
Leave the finished Game Boy / Game Boy Color emulator tidy: no code that nothing uses, and documentation
that is true, consistent and easy to follow. **No behaviour change**: every test result, picture hash and
audio output stays exactly as it is.

(This phase replaced a planned Game Boy Advance phase. A GBA emulator is a different console with a different
CPU, so it does not belong in this project; it was removed from scope on 2026-11-17.)

## Prerequisites
Phase 8 tasks done (scoreboard, save states, config, controls, palettes, packaging, performance check,
README and ARCHITECTURE, CI, robustness test).

## Tasks

### A. Dead code
1. **Find candidates mechanically.** `cargo clippy --workspace --all-targets -- -D warnings` already denies unused
   private items. For `pub` items, list every `pub fn` / `pub const` / `pub struct` / `pub enum` / trait method and
   count its references across the whole workspace (including tests). An item referenced only at its definition
   is a candidate. Also check: `Cargo.toml` dependencies that nothing imports, scripts nothing calls, files nothing
   includes, config settings or actions that no code reads, `#[allow(...)]` attributes that are no longer needed.
2. **Decide each candidate**: delete it, or keep it with a reason. Keep: part of the documented public API
   (CLAUDE.md section 4.1), used only by an integration test or by `gb-cli` (then it is not dead), or a
   documented extension point. Delete: anything left over from a removed feature (rewind, fast-forward, the
   analogue-trigger path), a placeholder, a helper that was replaced, an unused test utility.
3. **Narrow visibility** where it is free: `pub` items used only inside their crate become `pub(crate)`.
4. Delete in small commits (one area per commit). After each: `cargo fmt`, `cargo clippy ... -D warnings`,
   `cargo test --workspace`.

### B. Documentation
5. **Module and item doc comments tell the truth.** Remove stale statements (for example "placeholder: nothing is
   executed yet", "the DMG PPU" where CGB is also handled). Non-obvious hardware behaviour keeps its Pan Docs
   reference; do not add comments that only restate the code.
6. **README, ARCHITECTURE, STEAMDECK, ACCURACY, REFERENCES agree with the code and with each other.** Check every
   command, path, key binding and number against reality; every relative link resolves
   (`scripts/check_docs.sh`).
7. **CLAUDE.md matches the finished project**: phase index, repository layout (new frontend modules, `docs/images/`),
   commands, non-goals.
8. **PROGRESS.md is usable**: a short current-status summary at the top (what is done, what is deliberately not
   supported, how to run the checks), the phase list ticked correctly, the gate checklists of finished phases
   ticked or annotated with the reason where a gate was waived. History stays, but the top tells a new reader
   what to do.
9. **Phase files**: mark finished phases as done and fix gate checklists that were met but never ticked.

## Gate (definition of done)
- [ ] `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] `scripts/run_all_tests.sh`: 0 failed, and the totals equal the scoreboard before the cleanup (same suites;
      only the count of unit tests may drop if a test existed solely for deleted code).
- [ ] Local game checks (`pokemon_local`) still match their approved hashes (skips if the ROMs are absent: run
      them where the ROMs are).
- [ ] The dead-code search is recorded in PROGRESS.md: what was deleted, and what was kept and why.
- [ ] `scripts/check_docs.sh` passes (all relative links in the markdown files resolve).
- [ ] The documentation read-through is done and the stale statements found are listed in PROGRESS.md.

## What's next (for the human, not the agent)
The Game Boy / Game Boy Color emulator is finished after this phase. Possible side projects that need no changes
to the core: use the deterministic core for ML experiments (an agent playing Pokémon through the headless API),
or build more tools around the input-script format.
