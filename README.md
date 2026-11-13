# Game Boy / Game Boy Color emulator (Rust)

A learning project: a Game Boy (DMG) and Game Boy Color (CGB) emulator written in Rust,
with the goal of passing the standard community test ROMs and running Pokémon Red/Blue/Yellow
and Gold/Silver/Crystal correctly, on a Steam Deck with gamepad and sound.

The project is in **phase 0 (bootstrap)**. The emulator core does not run any code yet: it can
parse cartridge headers, and the command-line runner and test harness are in place. See
[PROGRESS.md](PROGRESS.md) for the current status and [CLAUDE.md](CLAUDE.md) for the design rules.

## Layout

| Crate | Purpose |
|---|---|
| `crates/gb-core` | The emulator. Standard library only, no I/O. |
| `crates/gb-cli` | Headless runner for tests and tracing. |
| `crates/gb-sdl` | Desktop / Steam Deck frontend (empty until phase 4). |

## Requirements

- Rust stable (install with [rustup](https://rustup.rs/)).
- `bash`, `curl`, `unzip` and `sha256sum` for the test-ROM script. On Windows, use Git Bash.

## Build and test

```sh
cargo build --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
bash scripts/fetch_test_roms.sh      # downloads the pinned test-ROM bundle into tests/roms/
cargo test --workspace               # ROM tests skip with a SKIP message if their ROM is absent
cargo test --workspace -- --nocapture  # same, but shows SKIP lines
```

## Run the headless runner

```sh
# Print the cartridge header of a ROM
cargo run -p gb-cli -- run path/to/game.gb

# Run for 600 frames, then save the picture
cargo run -p gb-cli -- run path/to/game.gb --frames 600 --screenshot out.png

# Compare the final frame with a reference image (non-zero exit on any difference)
cargo run -p gb-cli -- run path/to/game.gb --frames 600 --compare expected.png
```

Run `cargo run -p gb-cli -- run --help` for the full option list.

## Test ROMs

`scripts/fetch_test_roms.sh` downloads a pinned release of
[c-sp/game-boy-test-roms](https://github.com/c-sp/game-boy-test-roms) (MIT licence for the
collection; each suite keeps its own licence) and checks its SHA-256 before extracting it.
Sources and the pinned hash are in [docs/REFERENCES.md](docs/REFERENCES.md).

## Legal note

This repository contains **no commercial game ROMs and no BIOS**. The emulator starts in the
post-boot state directly.

Place your own dumped game ROMs in `roms/`, which is git-ignored. Do not commit them, and do not
share them. Only use ROMs you own.

Writing an emulator is legal. Commercial ROMs are copyrighted.
