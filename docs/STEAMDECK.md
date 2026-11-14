# Running gb-sdl on the Steam Deck

> **UNVERIFIED DRAFT.** Written from the phase 4 plan before anyone ran it on a Deck. The owner
> must try these steps and this file must then be corrected to exactly what worked
> (phase 4 gate). Items marked (?) are guesses.

## 1. Build in Desktop Mode
SteamOS keeps the root filesystem read-only; nothing below changes it.

1. Switch to Desktop Mode (Steam button, Power, Switch to Desktop).
2. Open Konsole and install Rust into your home directory:
   `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh` (accept the defaults).
   Then `source ~/.cargo/env`.
3. SDL2 is already present on SteamOS at runtime. Building needs the SDL2 *development*
   files and a C toolchain (?). If `cargo build` fails to find SDL2 or a linker, report the
   exact error; do not unlock the root filesystem without deciding to.
4. Get the source (clone or copy this repository to e.g. `~/gb`), then:
   `cd ~/gb && cargo build -p gb-sdl --release`
   The binary is `target/release/gb-sdl`.

Alternative: build on another Linux x86_64 machine and copy the `gb-sdl` binary (it needs
`libSDL2` at runtime).

## 2. Put a ROM somewhere
Use a ROM you dumped yourself, or a free homebrew/test ROM. The battery save is written next to
the ROM as `<rom>.sav` (for `pokemon.gb`, `pokemon.gb.sav`), so keep ROMs in a writable folder
such as `~/roms/`.

## 3. Try it in Desktop Mode first
`~/gb/target/release/gb-sdl ~/roms/game.gb --scale 5`
- D-pad / left stick = D-pad, east button (B) = Game Boy B, south button (A) = Game Boy A.
  Add `--swap-ab` to exchange them.
- Menu (Start) = Start, View (Select/Back) = Select. The Steam button is left to Steam.
- Keyboard: arrows, Z = B, X = A, Enter = Start, Backspace/RShift = Select, Tab (hold) =
  fast-forward, F11 = fullscreen, Esc = quit.

## 4. Add to Steam (Game Mode)
1. In Steam (Desktop Mode): Games, Add a Non-Steam Game to My Library, Browse, pick `gb-sdl`.
2. Open its Properties. Set Launch Options to
   `%command% /home/deck/roms/game.gb --fullscreen`
   (one non-Steam shortcut per game, or a wrapper script (?)).
3. Controller settings: use the "Gamepad" template (not keyboard/mouse), so Steam passes the
   buttons through as a controller.
4. Switch back to Game Mode and launch it from the library.

## 5. What to report back
Record in `PROGRESS.md`: did it start, is the speed right, do the buttons feel right, does a
save survive quitting from Game Mode (Steam button, Exit game), and anything that failed.
