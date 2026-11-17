gb-sdl for the Steam Deck (Game Boy / Game Boy Color emulator)
================================================================

Contents
  gb-sdl       the emulator (Linux x86_64; needs SDL2, which SteamOS already has)
  run.sh       launcher for Game Mode (see below)
  config.ini   settings; every line is commented out and shows the default
  games/       put your own dumped ROMs (.gb / .gbc) here; sub-folders are fine
  LICENSE      MIT licence

No game ROMs are included.

Quick start
  1. Unpack this folder somewhere on the Deck, e.g. /home/deck/Desktop/gb_emu/
     (tar xzf gb-sdl-deck.tar.gz). Make sure gb-sdl and run.sh are executable:
     chmod +x gb-sdl run.sh
  2. Copy your ROMs into the games/ folder.
  3. Desktop Mode test:   ./gb-sdl     (opens the game list)
  4. Game Mode: Steam, Games, Add a Non-Steam Game, pick run.sh ("All Files" in the picker).
     Properties: Target = the full path of run.sh, Start In = this folder, Launch Options empty.
     Controller layout: "Gamepad With Joystick Trackpad" (not the Team Fortress 2 one).
     One shortcut is enough for every game.

The game list
  D-pad or left stick = move (hold to scroll), left/right (or L1/R1) = page, A or Start = play,
  B or View = quit. Keyboard: arrows, Enter, Esc.
  Back to the list from a game: Esc on a keyboard, or hold Start+Select for 1.5 seconds.
  Closing the window (or Steam button, Exit game) quits everything.

Controls in a game (defaults; change them in config.ini)
  Gamepad: D-pad / left stick = D-pad, Deck A = Game Boy A, Deck B = Game Boy B,
           Menu = Start, View = Select, R2 (hold) = fast-forward, L2 (hold) = rewind
           (rewind is off until you set rewind = true in config.ini).
  Keyboard: arrows, X = A, Z = B, Enter = Start, Backspace / Right Shift = Select,
            F = fast-forward (hold), S = slow motion (hold), R = rewind (hold),
            P = pause, F10 = reset, F5 = quick save, F8 = quick load, F6 = next save slot (1-9),
            F9 = next DMG colour scheme, F12 = screenshot, F11 = fullscreen,
            1-4 = mute a sound channel, + / - = volume.
  Back buttons / combos: map them in Steam Input to the keyboard keys above
  (for example L4 = F5, R4 = F8, L5 = P, R5 = Esc). See docs/STEAMDECK.md in the source tree.

Files it writes (created only when first needed)
  saves/         <game>.sav battery saves, <game>.state1..9 quick-save slots
  screenshots/   <game>-shot-NNN.png
  last_error.txt only after a failed start or crash; delete it freely

Back up the saves/ folder to keep your progress.
