# Running gb-sdl on the Steam Deck

Status legend: **[confirmed]** = the owner did it and reported it worked. **[unconfirmed]** = not
checked by anyone yet. This file records the setup that worked for the owner (2026-11-15); it is
not a general guide to every Deck configuration.

## 1. Build the package in WSL and copy it  [binary approach confirmed; the packaged layout is unconfirmed]
Nothing is built on the Deck, so SteamOS needs no changes (no unlocking the root filesystem, no Rust).

1. In WSL Ubuntu (needs `libsdl2-dev`): `scripts/package_deck.sh`
   (set `CARGO_TARGET_DIR=~/gb-target` as usual). It builds the release binary and produces
   `release/gb-sdl-deck/` and `release/gb-sdl-deck.tar.gz` containing: `gb-sdl`, `run.sh`, `config.ini`
   (every setting commented out, showing the default), `README.txt`, `LICENSE`. It refuses to package
   anything that looks like a ROM or a save, and prints the shared libraries the binary needs.
2. Copy the `.tar.gz` to the Deck and unpack it (`tar xzf gb-sdl-deck.tar.gz`). SteamOS already ships
   `libSDL2` at runtime. Copying only the `gb-sdl` binary (the way it was first done) still works too.

## 2. Folder layout on the Deck  [confirmed]
```
/home/deck/Desktop/gb_emu/
    gb-sdl            the binary (chmod +x gb-sdl)
    run.sh            wrapper script, see section 4 (chmod +x run.sh)
    config.ini        optional settings (section 10); found automatically beside the binary
    Pokemon_Red.gb    your own dumped ROM
```
(The package's `run.sh` has a `ROM=` line to edit, and writes its log to `gb_log.txt` in the same folder
instead of `~/gb_log.txt`.)
Copy the ROM too: the first attempt failed with `cannot read ROM: No such file` only because the ROM
had not been copied yet.

## 3. Try it in Desktop Mode first  [confirmed]
From Konsole, in that folder: `./gb-sdl Pokemon_Red.gb`

- Keyboard: arrows, Z = B, X = A, Enter = Start, Backspace/RShift = Select, F (hold) = fast-forward,
  F11 = fullscreen, Esc = quit. Full list in section 10. (Tab is **no longer** fast-forward.)
- Gamepad: D-pad and left stick = D-pad, Menu = Start, View = Select, R2 (hold) = fast-forward.
  The Steam button is left to Steam.
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
- Nothing important. Confirmed: D-pad, left stick (2026-11-16), View = Select (2026-11-16), Deck A = GB A, Deck B = GB B, Menu (three lines) = Start.
- `--fullscreen` itself in Game Mode (not needed: the game already opens full screen in Game Mode without it).
- Cause of the original instant close (section 4).

## 9. Sound (phase 6; owner-confirmed fine on the Deck 2026-11-16 with Pokémon Gold, 16 underruns in ~9.6 min, not audible)
- `gb-sdl` opens the default SDL audio device (48 kHz stereo) and paces the game by the audio clock. If the Deck has no usable audio device it prints a warning and falls back to timer pacing.
- Keys (Desktop Mode): `1`-`4` mute channels 1-4, `+`/`-` volume. `--no-audio` turns sound off. On exit it prints one line of queue statistics (underruns should be 0).
- Crackle or lag in Game Mode: none reported. If it appears, send the "audio:" line from `~/gb_log.txt`.

## 10. Controls, config file, save states, rewind (phase 8)  [implemented and unit-tested; not yet tried on the Deck]
Everything here can be changed in `config.ini` (`key.NAME = ...` for the keyboard, `pad.NAME = ...` for
the gamepad; a line replaces that control's defaults). Command-line flags override the file. The file is
looked up beside the binary, then in `~/.config/gb-sdl/config.ini`; `--config FILE` names one explicitly.
A mistake in it stops the program with a message naming the line (see `gb_log.txt`).

| Action | Keyboard | Gamepad (default) |
|---|---|---|
| D-pad / A / B / Start / Select | arrows / X / Z / Enter / Backspace or RShift | D-pad or left stick / Deck A / Deck B / Menu / View |
| Fast-forward (hold) | F | R2 |
| Rewind (hold; needs `rewind = true`) | R | L2 |
| Slow motion (hold) | S | none |
| Pause | P | none |
| Reset (battery save is kept) | F10 | none |
| Quick save / quick load / next slot (1-9) | F5 / F8 / F6 | none |
| Next DMG colour scheme | F9 | none |
| Screenshot (PNG beside the ROM) | F12 | none |
| Fullscreen / quit | F11 / Esc | none (Steam button, Exit game) |

Deliberately only fast-forward and rewind are on the pad by default: nothing destructive (load state,
reset) sits on a button you can hit by accident. `pad.save_state = x` and so on in `config.ini` add more.

### Reaching the rest without a keyboard: Steam Input back buttons  [unconfirmed]
In Game Mode, open the shortcut's controller settings and bind the back grips to **keyboard keys**, e.g.
L4 = `F5` (quick save), R4 = `F8` (quick load), L5 = `P` (pause), R5 = `F6` (next slot). To quit, use
Steam button, Exit game, or bind a grip to `Esc`. The emulator only sees keys, so it does not care how
they got there. (SDL sees Steam's virtual gamepad, which has no back-grip buttons unless Steam Input maps
them.) The trigger bindings (R2/L2) need no setup: the "Gamepad With Joystick Trackpad" layout passes the
triggers through as analogue axes and they count as pressed past about half travel.

### Save states and rewind: what to know
- State files are `<rom>.state1` ... `.state9` beside the ROM (about 270 KB each) and are tied to that
  exact ROM and console model: loading one into another game is refused with a message, not a crash.
- A state includes the cartridge RAM, so **loading a state (or rewinding) across an in-game save puts the
  save file back to what it was then** at the next flush. State files are not a substitute for the game's
  own save; keep playing normally and treat quick save as a convenience.
- Rewind is off by default. `rewind = true` keeps `rewind_seconds` (default 10) of history in memory,
  one state every `rewind_interval_frames` (default 4): about 40 MB with the defaults. More than 1000
  states is refused at start-up. Holding L2 (or R on the keyboard) steps backwards at 4x speed; there is no undo.
- Reset, save state, load state and rewind are disabled while `--record` is active.
- Messages ("saved state 1", "paused", ...) show in the window title for 3 seconds and in `gb_log.txt`;
  there is no on-screen text, so in Game Mode you will mostly notice them by the effect.
