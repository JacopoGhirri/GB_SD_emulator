# PROGRESS

**Current phase:** 1 — CPU (phase 0 gate met on 2026-11-13)
**Last updated:** 2026-11-13

## Status by phase
- [x] 0 Bootstrap
- [ ] 1 CPU  ← current
- [ ] 2 Bus, timer, cartridge
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
- **Branch:** Phase 0 committed on `master` (the repo's only branch, tracking `origin/master`). The PR base named in the instructions is `main`, which does not exist locally.
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
- A commit `cb635f9 "remove co authoring"` by you appears in the history between my commits. It adds three lines to `CLAUDE.md`. I did not touch it. Please confirm it is intended.
- Should `main` be created as the PR base, or should PRs target `master`?
