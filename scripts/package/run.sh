#!/bin/bash
# Launcher for Steam Game Mode. Add THIS file as the one Non-Steam game (Target) and set
# "Start In" to this folder. It opens the game list; pick a game there.
# Extra arguments are passed on, e.g.  ./run.sh games/tetris.gb  plays that one game directly.

# Steam may not start the program in its own folder, so go there first.
cd "$(dirname "$(readlink -f "$0")")" || exit 1

# Messages go to a temporary file outside this folder. Only if the emulator fails is a copy
# kept here as last_error.txt, so a normal session leaves nothing behind.
LOG="$(mktemp "${TMPDIR:-/tmp}/gb-sdl.XXXXXX")"
./gb-sdl "$@" > "$LOG" 2>&1
status=$?
if [ "$status" -ne 0 ]; then
    cp "$LOG" last_error.txt
else
    rm -f last_error.txt
fi
rm -f "$LOG"
exit "$status"
