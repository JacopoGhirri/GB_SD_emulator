#!/bin/bash
# Launcher for Steam Game Mode. Add THIS file as the Non-Steam game (Target), and set
# "Start In" to this folder. It plays one ROM: edit ROM below, or copy this script and the
# shortcut for each game. Errors and messages go to gb_log.txt in this folder.
ROM="game.gb"          # file name of your own ROM, in this folder (.gb or .gbc)

# Steam may not start the program in its own folder, so go there first.
cd "$(dirname "$(readlink -f "$0")")" || exit 1
./gb-sdl "$ROM" > gb_log.txt 2>&1
