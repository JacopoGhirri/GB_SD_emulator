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

## 2. Folder layout on the Deck  [old single-ROM layout confirmed; the games/ layout is unconfirmed]
```
/home/deck/Desktop/gb_emu/
    gb-sdl            the binary (chmod +x gb-sdl)
    run.sh            wrapper script, see section 4 (chmod +x run.sh)
    config.ini        optional settings (section 10); found automatically beside the binary
    games/            your own dumped ROMs (.gb / .gbc), sub-folders allowed
    saves/            created on first save: <game>.sav and <game>.state1..9
    screenshots/      created on first F12
```
`saves/` and `screenshots/` do not exist until something is written to them, and a normal session
creates no log file (see section 7). The older layout (binary, one ROM and its `.sav` side by side,
started as `./gb-sdl Pokemon_Red.gb`) still works exactly as before; it is "direct mode" below.
Copy the ROMs into `games/`. (An early failure with `cannot read ROM: No such file` was only a ROM
that had not been copied yet.)

## 3. Try it in Desktop Mode first  [confirmed]
From Konsole, in that folder: `./gb-sdl` opens the game list (section 11), or `./gb-sdl games/Pokemon_Red.gb`
plays one game directly (its save then goes beside the ROM instead of into `saves/`).

- Keyboard: arrows, Z = B, X = A, Enter = Start, Backspace/RShift = Select, F (hold) = fast-forward,
  F11 = fullscreen, Esc = quit. Full list in section 10. (Tab is **no longer** fast-forward.)
- Gamepad: D-pad and left stick = D-pad, Menu = Start, View = Select, R2 (hold) = fast-forward.
  The Steam button is left to Steam.
- **A/B mapping is by label (default):** Deck **A** = Game Boy **A**, Deck **B** = Game Boy **B**.
  That is not the Nintendo *physical* layout: on a Game Boy, A is the right-hand button and B the
  left-hand one, whereas on the Deck A is the lower button and B the right-hand one. Pass `--swap-ab`
  to get the Nintendo physical layout (Deck B/east = GB A, Deck A/south = GB B).

## 4. Game Mode: use a wrapper script  [confirmed: a wrapper works; the new one is unconfirmed]
Launching `gb-sdl` directly as the Steam target closed instantly in Game Mode. A wrapper script fixed it.
The packaged `run.sh`:
```bash
cd "$(dirname "$(readlink -f "$0")")" || exit 1
LOG="$(mktemp "${TMPDIR:-/tmp}/gb-sdl.XXXXXX")"
./gb-sdl "$@" > "$LOG" 2>&1
status=$?
if [ "$status" -ne 0 ]; then cp "$LOG" last_error.txt; else rm -f last_error.txt; fi
rm -f "$LOG"
exit "$status"
```
Why it helps: **the exact cause of the instant close is unconfirmed.** Suspects are the working
directory (Steam may not start the program in its own folder), a relative ROM path, or `--fullscreen`
under Game Mode. The script fixes the first by changing directory first. Messages go to a temporary
file, and only a failed run leaves `last_error.txt` in the folder. The working setup does **not** use
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

One shortcut is enough: it opens the game list and you choose the game there (section 11). To launch one
fixed game from its own shortcut instead, put the path in the shortcut's Launch Options, e.g. `games/Tetris.gb`.

## 6. Saves  [direct mode confirmed on the Deck; the saves/ folder is unconfirmed]
- **Game list (folder mode):** `saves/<game file name>.sav`, e.g. `saves/Pokemon_Red.gb.sav`. A save that
  still sits beside the ROM (from the old layout, or copied there by hand) is moved into `saves/`
  the first time that game is started; an existing `saves/` file is never overwritten.
- **Direct mode (`./gb-sdl game.gb`):** `<rom>.sav` beside the ROM, as before.
- Saves are flushed on quit, window close, focus loss, when going back to the list, and every 30 s if
  changed. Quit from Game Mode with Steam button, Exit game (or Esc with a keyboard).
- Back up the `saves/` folder to keep your progress.

- Save, quit, relaunch, Continue works in WSL (WSLg window).  [confirmed]
- The same round trip on the Deck in Game Mode (direct mode): opens, saves, reopens fine.  [confirmed]

## 7. When something goes wrong
If the emulator fails to start or crashes, the launcher leaves `last_error.txt` in the program folder
with the messages (for example `cannot read ROM ...`). A normal session leaves no log. To see the
messages of a working session, run `./gb-sdl` from Konsole in Desktop Mode.

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
| Fullscreen / leave the game | F11 / Esc (list in folder mode, quit in direct mode) | Start+Select held 1.5 s = back to the list (folder mode) |

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
- Reset, save state, load state and rewind are disabled while `--record` is active. (`--record` needs a ROM path, not the game list.)
- Messages ("saved state 1", "paused", ...) show in the window title for 3 seconds and in `gb_log.txt`;
  there is no on-screen text, so in Game Mode you will mostly notice them by the effect.

## 11. The game list (folder mode)  [implemented, unit-tested and smoke-run headless; not yet tried on the Deck]
Started without a ROM (`./gb-sdl`, or the packaged `run.sh`), the program lists every `.gb` / `.gbc` file
under `games/` (up to 3 folder levels down; hidden files skipped), sorted by name, and you pick one.
`./gb-sdl <folder>` does the same for another folder that has a `games/` inside. The list is drawn
in the game window with a built-in font, so there is nothing to install.

| | Keyboard | Gamepad |
|---|---|---|
| Move | Up / Down (hold to scroll) | D-pad or left stick |
| Page | Left / Right, PgUp / PgDn | D-pad left/right, L1 / R1 |
| Play | Enter, Space | A or Start |
| Quit | Esc | B or View |

- A game that cannot be loaded (damaged file, unsupported cartridge) is reported in the bottom line of the
  list and the list stays open. Files added while you play appear when you come back to the list.
- **Back to the list from a game:** Esc on a keyboard, or hold **Start + Select for 1.5 seconds** on the
  gamepad (a message appears after half a second). The game's save is written first. Closing the window,
  or Steam button, Exit game, quits the whole program. In direct mode Esc quits, as before.
- Two games with the same file name in different sub-folders would share a save name; keep names unique.
- Not done: remembering the last game between launches (the cursor does stay on the same game while the
  program runs); doing so would need one more file.
