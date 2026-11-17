#!/usr/bin/env bash
# Build the Steam Deck package: release/gb-sdl-deck/ and release/gb-sdl-deck.tar.gz.
#
# Run it in WSL Ubuntu / Linux x86_64 (needs libsdl2-dev; nothing is built on the Deck).
# The binary links against this machine's glibc and the system SDL2; the Deck already
# runs a binary built this way (docs/STEAMDECK.md). No ROM, save or BIOS is ever included.
#
# usage: scripts/package_deck.sh [output-dir]      (default: release/gb-sdl-deck)
set -euo pipefail
cd "$(dirname "$0")/.."

OUT="${1:-release/gb-sdl-deck}"
TARGET_DIR="${CARGO_TARGET_DIR:-target}"

cargo build --release -p gb-sdl
BIN="$TARGET_DIR/release/gb-sdl"
[ -x "$BIN" ] || { echo "error: $BIN was not built" >&2; exit 1; }

rm -rf "$OUT"
mkdir -p "$OUT"
cp "$BIN" "$OUT/gb-sdl"
strip "$OUT/gb-sdl" 2>/dev/null || true
cp scripts/package/run.sh "$OUT/run.sh"
cp scripts/package/README.txt "$OUT/README.txt"
cp crates/gb-sdl/config.example.ini "$OUT/config.ini"
cp LICENSE "$OUT/LICENSE"
chmod +x "$OUT/gb-sdl" "$OUT/run.sh"

# Safety net for CLAUDE.md section 10: refuse to ship anything that looks like a game or a save.
if find "$OUT" -type f \( -iname '*.gb' -o -iname '*.gbc' -o -iname '*.gba' -o -iname '*.sav' \
        -o -iname '*.state*' -o -iname '*.bin' \) | grep -q .; then
    echo "error: the package contains a ROM, save or BIOS-looking file" >&2
    exit 1
fi

# Show what the binary needs at run time (SDL2 is expected; the rest is the base system).
echo "shared libraries needed by gb-sdl:"
ldd "$OUT/gb-sdl" | sed 's/^/  /'

# tar keeps the executable bits (zip from Windows would not).
tar -C "$(dirname "$OUT")" -czf "$OUT.tar.gz" "$(basename "$OUT")"
echo
echo "package:  $OUT/"
ls -l "$OUT" | sed 's/^/  /'
echo "archive:  $OUT.tar.gz ($(du -h "$OUT.tar.gz" | cut -f1))"
