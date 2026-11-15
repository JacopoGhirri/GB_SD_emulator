# Running gb-sdl on the Steam Deck

Status legend: **[confirmed]** = the owner did it and reported it worked. **[unconfirmed]** = not
checked by anyone yet. This file records the setup that worked for the owner (2026-11-15); it is
not a general guide to every Deck configuration.

## 1. Build in WSL and copy the binary  [confirmed]
Nothing is built on the Deck, so SteamOS needs no changes (no unlocking the root filesystem, no Rust).

1. In WSL Ubuntu (needs `libsdl2-dev`): `cargo build -p gb-sdl --release`
   (the binary lands in the cargo target dir, e.g. `~/gb-target/release/gb-sdl`).
2. Copy the binary to the Deck. SteamOS already ships `libSDL2` at runtime.

## 2. Folder layout on the Deck  [confirmed]
```
/home/deck/Desktop/gb_emu/
    gb-sdl            the binary (chmod +x gb-sdl)
    run.sh            wrapper script, see section 4 (chmod +x run.sh)
    Pokemon_Red.gb    your own dumped ROM
```
Copy the ROM too: the first attempt failed with `cannot read ROM: No such file` only because the ROM
had not been copied yet.

## 3. Try it in Desktop Mode first  [confirmed]
From Konsole, in that folder: `./gb-sdl Pokemon_Red.gb`

- Keyboard: arrows, Z = B, X = A, Enter = Start, Backspace/RShift = Select, Tab (hold) = fast-forward,
  F11 = fullscreen, Esc = quit.
- Gamepad: D-pad and left stick = D-pad, Menu = Start, View = Select. The Steam button is left to Steam.
- **A/B mapping is by label (default):** Deck **A** = Game Boy **A**, Deck **B** = Game Boy **B**.
  That is not the Nintendo *physical* layout: on a Game Boy, A is the right-hand button and B the
  left-hand one, whereas on the Deck A is the lower button and B the right-hand one. Pass `--swap-ab`
  to get the Nintendo physical layout (Deck B/east = GB A, Deck A/south = GB B).

## 4. Game Mode: use a wrapper script  [confirmed: works]
Launching `gb-sdl` directly as the Steam target closed instantly in Game Mode. A wrapper script fixed it.
`/home/deck/Desktop/gb_emu/run.sh`:
```bash
#!/bin/bash
cd /home/deck/Desktop/gb_emu
./gb-sdl Pokemon_Red.gb > /home/deck/gb_log.txt 2>&1
```
Why it helps: **the exact cause of the instant close is unconfirmed.** Suspects are the working
directory (Steam may not start the program in its own folder, so a relative ROM path fails), the
relative ROM path itself, or `--fullscreen` under Game Mode. The script fixes the first two by
changing directory first, and it captures any error text in a log. The working setup does **not** use
`--fullscreen`.

## 5. Steam settings (Desktop Mode, then switch to Game Mode)
1. Games, Add a Non-Steam Game to My Library, Browse, pick `run.sh` (choose "All Files" in the picker).  [confirmed]
2. Properties:  [confirmed]
   - **Target:** `run.sh` (full path `/home/deck/Desktop/gb_emu/run.sh`)
   - **Start In:** `/home/deck/Desktop/gb_emu/`
   - **Launch Options:** empty
3. Controller layout: choose **"Gamepad With Joystick Trackpad"** (Steam Input on, default settings).  [confirmed]
   **Avoid the "Team Fortress 2" layout** that the picker may offer: it maps to keyboard and mouse,
   not to a gamepad.
4. Launch from the library in Game Mode.  [confirmed: owner reports it "works perfectly"]

One shortcut plays one ROM (the ROM name is inside `run.sh`). For another game, copy the script and
shortcut, or edit the ROM name.

## 6. Saves  [confirmed in WSL and on the Deck in Game Mode]
The battery save is written beside the ROM as `<rom>.sav`, so here
`/home/deck/Desktop/gb_emu/Pokemon_Red.gb.sav`. The script's `cd` is what puts it there, since the ROM
path is relative. Saves are flushed on quit, window close, focus loss and every 30 s if changed.
Quit from Game Mode with Steam button, Exit game (or Esc with a keyboard).

- Save, quit, relaunch, Continue works in WSL (WSLg window).  [confirmed]
- The same round trip on the Deck in Game Mode: opens, saves, reopens fine.  [confirmed]

## 7. When something goes wrong
Read `/home/deck/gb_log.txt` (it is overwritten at every launch). Errors such as `cannot read ROM ...`
appear there, because the script redirects stdout and stderr to it.

## 8. Still unconfirmed on the Deck
- Left stick (not tested) and View/Select (not reported). Confirmed: D-pad, Deck A = GB A, Deck B = GB B, Menu (three lines) = Start.
- `--fullscreen` itself in Game Mode (not needed: the game already opens full screen in Game Mode without it).
- Cause of the original instant close (section 4).

## 9. Sound (phase 6, untested on the Deck)
- `gb-sdl` opens the default SDL audio device (48 kHz stereo) and paces the game by the audio clock. If the Deck has no usable audio device it prints a warning and falls back to timer pacing.
- Keys (Desktop Mode): `1`-`4` mute channels 1-4, `+`/`-` volume. `--no-audio` turns sound off. On exit it prints one line of queue statistics (underruns should be 0).
- **Unconfirmed:** crackle or lag in Game Mode. If it crackles, tell me and send the "audio:" line from `~/gb_log.txt`.
