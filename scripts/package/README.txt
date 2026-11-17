gb-sdl for the Steam Deck (Game Boy / Game Boy Color emulator)
================================================================

Contents
  gb-sdl       the emulator (Linux x86_64; needs SDL2, which SteamOS already has)
  run.sh       launcher for Game Mode (see below)
  config.ini   settings; every line is commented out and shows the default
  LICENSE      MIT licence

No game ROMs are included. Copy your own dumped ROM into this folder.

Quick start
  1. Unpack this folder somewhere on the Deck, e.g. /home/deck/Desktop/gb_emu/
     (tar xzf gb-sdl-deck.tar.gz). Make sure gb-sdl and run.sh are executable:
     chmod +x gb-sdl run.sh
  2. Copy your ROM here and put its file name in run.sh (the ROM= line).
  3. Desktop Mode test:   ./gb-sdl yourgame.gb
  4. Game Mode: Steam, Games, Add a Non-Steam Game, pick run.sh ("All Files" in the picker).
     Properties: Target = the full path of run.sh, Start In = this folder, Launch Options empty.
     Controller layout: "Gamepad With Joystick Trackpad" (not the Team Fortress 2 one).

Controls (defaults; change them in config.ini)
  Gamepad: D-pad / left stick = D-pad, Deck A = Game Boy A, Deck B = Game Boy B,
           Menu = Start, View = Select, R2 (hold) = fast-forward, L2 (hold) = rewind
           (rewind is off until you set rewind = true in config.ini).
  Keyboard: arrows, X = A, Z = B, Enter = Start, Backspace / Right Shift = Select,
            F = fast-forward (hold), S = slow motion (hold), R = rewind (hold),
            P = pause, F10 = reset, F5 = quick save, F8 = quick load, F6 = next save slot (1-9),
            F9 = next DMG colour scheme, F12 = screenshot, F11 = fullscreen, Esc = quit,
            1-4 = mute a sound channel, + / - = volume.
  Back buttons / combos: map them in Steam Input to the keyboard keys above
  (for example L4 = F5, R4 = F8, L5 = P, R5 = Esc). See docs/STEAMDECK.md in the source tree.
  Quit from Game Mode: Steam button, Exit game.

Files it writes next to the ROM
  <rom>.sav            battery save (flushed on quit, on focus loss and every 30 s)
  <rom>.state1..9      quick-save slots
  <rom>-shot-NNN.png   screenshots

If something goes wrong, read gb_log.txt in this folder (it is overwritten at each launch).
