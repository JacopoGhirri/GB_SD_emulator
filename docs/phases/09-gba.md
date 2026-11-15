# Phase 9 — Game Boy Advance

## Goal
Run commercial Game Boy Advance games from the owner's own cartridge dumps, with video, sound, input and battery
saves, on desktop and on the Steam Deck. This is a **second emulator core** in the same workspace, built with the same
rules as the Game Boy core (deterministic, cycle-stepped bus, test-ROM driven, no per-game hacks).

This phase was added on 2026-11-15 at the owner's request; GBA was previously a non-goal (see CLAUDE.md section 1).
It is large: expect it to be as much work as phases 0-8 combined for the Game Boy. It is split into milestones, each with
its own gate, so progress is measurable and the work can stop at any milestone boundary.

## Prerequisites
- Phase 8 gate met, or consciously deferred in `PROGRESS.md`. The packaging and config work of phase 8 should be written so
  a second core can plug in; if it was not, task 1 below fixes that first.
- Read: GBATEK (the main reference, see `docs/REFERENCES.md`), the ARM7TDMI sections for ARM and Thumb instruction sets,
  Tonc for the video hardware. Pan Docs does **not** apply to GBA hardware.
- **Ask the owner at the start of the phase** (things only they can provide):
  1. Which GBA games they own and want working (these define the human gate; Pokémon FireRed/LeafGreen/Ruby/Sapphire/Emerald
     are the likely ones and need Flash saves, and Ruby/Sapphire/Emerald need the real-time clock).
  2. Whether they can dump a GBA BIOS from their own console and want an optional "use my BIOS" mode (see BIOS policy).
  3. Whether to keep the plan below (separate `gba-core` crate, shared frontend) or use a separate frontend binary.

## Architecture decisions (defaults; confirm with the owner at phase start)
- New crates: `gba-core` (std only, no I/O, no threads, no globals, no `unsafe`) and `gba-cli` (headless runner, may use
  `png`/`clap`). `gb-core` is **not** modified to know about GBA, and `gba-core` does not depend on it.
  Code that is genuinely identical (WAV writer, input script format, PNG compare) may move to a small shared crate with a
  one-line justification in `PROGRESS.md`; do not create a shared "emulator framework" abstraction up front.
- Frontend: extend `gb-sdl` so one binary picks the core from the file extension (`.gb`/`.gbc` vs `.gba`) through a small
  `System` trait (run one frame, framebuffer size, drain audio, set buttons, save RAM). The video size, audio rate and the L/R
  buttons are the only differences the frontend sees. Keep saves, pacing and the Deck wrapper unchanged.
- Public API mirrors the GB core where it makes sense: `Gba::new(rom, bios: Option<..>)`, `run_frame()`,
  `framebuffer() -> &[u32]` (240x160, `0x00RRGGBB`), `set_buttons(..)`, `drain_audio(..)`, `save_ram()` / `load_save_ram(..)`.
  Same determinism rule: same ROM + same inputs => same output; no wall clock, no randomness.
- Timing: same philosophy as CLAUDE.md section 4.2. Every bus access advances the system by the **cycles it costs**
  (including ROM/EWRAM wait states and the sequential/non-sequential distinction), so instruction timing emerges from the
  accesses. The master clock is 16.777216 MHz; one frame is 280896 cycles (59.7275 Hz). Do not add instruction cycle tables.
- Performance matters more than for the GB core (16.8 MHz CPU, 4 DMA channels, 4 timers). Write the straightforward design
  first, measure in release mode, optimise only measured hot spots (task 20).
- **BIOS policy (legal, same spirit as CLAUDE.md section 10):** the GBA BIOS is copyrighted. Never bundle or download it.
  Default is **no BIOS**: start from the documented post-boot state and implement the BIOS system calls (SWI) in Rust
  (high-level emulation, "HLE"). An optional `--bios <file>` lets the owner use a dump from their own console, which is
  both more accurate and a useful cross-check of the HLE. The BIOS file is gitignored like ROMs and saves.

## Milestones and tasks

### M1 — ARM7TDMI CPU and memory map
1. Registers and modes: R0-R15 with banked registers for User/FIQ/IRQ/Supervisor/Abort/Undefined, CPSR/SPSR, ARM and Thumb states.
2. ARM instruction set: data processing (with shifter carry-out), multiply/long multiply, single and block data transfer (including
   the odd cases: unaligned rotate, LDM/STM with empty list, base in list, writeback), branch/BX/BL, SWP, PSR transfer, SWI,
   undefined instruction, conditional execution.
3. Thumb instruction set (all 19 formats), including the two-instruction BL.
4. Pipeline behaviour visible to software: PC reads as address + 8 (ARM) or + 4 (Thumb); flush on branch.
5. Exceptions: reset, IRQ, FIQ, SWI, data/prefetch abort, undefined; vector table; `IME`/`IE`/`IF` interrupt controller.
6. Memory map and bus: BIOS, EWRAM (256 KiB, 16-bit bus), IWRAM (32 KiB), I/O, palette RAM, VRAM, OAM, cartridge ROM
   (three wait-state mirrors, `WAITCNT`, prefetch buffer), cartridge save region. Access width and alignment rules; open-bus reads.
7. Trace tooling like phase 1: a `gba-cli --trace` that prints one line per instruction (registers + CPSR) so a log can be
   diffed against a reference emulator's trace, first differing line = the bug. Record in a comment which reference was used and
   that only its behaviour (not code) was consulted.
8. Interrupt-driven `HALT` (via `HALTCNT` and the BIOS call), and `IntrWait`/`VBlankIntrWait` HLE.

**M1 gate:** jsmolka `gba-tests` `arm.gba` and `thumb.gba` pass, `memory.gba` passes (or each failing sub-test is documented),
`bios.gba` passes the parts that apply to the chosen BIOS mode. Pass criterion per ROM is defined from that suite's README when
the ROMs are fetched; record it in this file and in `PROGRESS.md`.

### M2 — Timers, DMA, keypad, HLE BIOS, saves
9. Four timers with prescalers and cascade (count-up) mode, overflow interrupts; timers drive the sound FIFOs later.
10. Four DMA channels: immediate, VBlank, HBlank, special (sound FIFO / video capture) start timings, priorities, repeat,
    increment/decrement/fixed/reload address modes, the ROM-source and EEPROM-size special cases, DMA interrupts.
11. Keypad register and keypad interrupt; button mapping includes **L and R** (extend the frontend key/gamepad tables).
12. HLE BIOS calls (at least: SoftReset, RegisterRamReset, Halt, Stop/Sleep, IntrWait, VBlankIntrWait, Div, DivArm, Sqrt, ArcTan,
    ArcTan2, CpuSet, CpuFastSet, BiosChecksum, BgAffineSet, ObjAffineSet, LZ77/Huffman/RL decompress, SoundBias; sound driver
    calls only if a game needs them). Each call gets a unit test against a slow reference or documented example.
13. Save media, detected from strings in the ROM (`SRAM_V`, `FLASH_V`, `FLASH512_V`, `FLASH1M_V`, `EEPROM_V`), never from the game title:
    SRAM (32 KiB), Flash 64 KiB / 128 KiB (command sequences, chip ID, sector/chip erase, bank switch), EEPROM 512 B / 8 KiB
    (serial protocol over DMA; size inferred from the first DMA length). `.sav` files beside the ROM like phase 4, same atomic write.
14. Cartridge real-time clock over the GPIO port (S-3511 protocol), needed by Pokémon Ruby/Sapphire/Emerald; same wall-clock
    policy as phase 4's MBC3 RTC (the core never reads the clock; the frontend supplies time).

**M2 gate:** gba-tests `flash.gba`/`sram.gba`/save tests that exist pass; timer and DMA tests from the mGBA test suite are run and
recorded; unit tests for the save-media state machines; a save written and reloaded round-trips in a determinism test.

### M3 — PPU
15. Timing: 240x160 visible, 308x228 dots at 4 cycles per dot, HBlank/VBlank flags and interrupts, `VCOUNT`, `DISPSTAT` LYC match.
16. Modes 0-2 (tiled backgrounds: text and affine, 4bpp/8bpp, screen sizes, wraparound), modes 3-5 (bitmap, page flip), sprites
    (regular and affine, double-size, 1D/2D tile mapping, priority, sprite limits per line), palettes, window 0/1 and OBJ window,
    alpha blending / brightness fade, mosaic, BG/OBJ priority resolution. Scanline renderer first (like phase 3's `render_line`);
    mid-scanline register changes are accepted via per-line sampling of registers and refined in M5 if games need it.
17. VRAM/palette/OAM access rules (8-bit writes to VRAM, blank-time access flags) as far as test ROMs and games need.

**M3 gate:** the screenshot-based suites (jsmolka `gba-tests` video demos and the Tonc demo ROMs that are freely redistributable)
match their expected images, compared with `gba-cli --compare`; owner visually confirms 2+ of their games show correct graphics
(title screen and first gameplay screen).

### M4 — APU
18. The four legacy PSG channels (same as the Game Boy's channels 1-4 with GBA-specific differences, `SOUNDCNT_L/H/X`, `SOUNDBIAS`)
    plus **Direct Sound A and B**: 8-bit signed FIFOs (32 bytes) filled by DMA special mode, played at a timer-controlled rate,
    with volume and left/right enable. Mix to the frontend's sample rate. The Game Boy APU code may be adapted by hand (it is our own code),
    but the register layout and mixer differ; do not force one implementation to serve both.
19. Frontend: reuse phase 6 audio-driven pacing unchanged (the GBA core produces samples through the same `drain_audio` contract).

**M4 gate:** WAV dump of a sound test visibly correct (waveform) and owner-approved by ear; owner confirms music in 2+ of their games
on desktop; no crackle over 10 minutes (use the phase 6 queue statistics line, 0 underruns expected).

### M5 — Compatibility, performance, Deck
20. Performance: profile in release mode (e.g. `perf`); target is at least 3x real time headless on the owner's desktop and stable
    60 fps with sound on the Steam Deck. Optimise only what the profile shows (typical wins: skip ticking idle components,
    cache decoded instructions per region, avoid per-access dynamic dispatch).
21. Play-through compatibility with the owner's games: boot, intro, save/reload, a long session. Fix root causes, never per-game hacks.
    If a game needs behaviour that is obscure (prefetch quirks, open bus, DMA edge cases, mid-scanline effects), find the test ROM
    that exercises it first, as in CLAUDE.md section 6.
22. Replay tests for the owner's games, as in phase 5 (`tests/expected/local/<name>.hashes`, SKIP when the ROM is absent;
    recordings made with `gb-sdl --record`, which must start from a fresh boot and ignore the save file).
23. `docs/STEAMDECK.md` gets a GBA section (L/R on the shoulder buttons, Select/Start mapping, any layout changes).

## Known pitfalls
- ARM7TDMI details that tests and real games both hit: shifter carry for register-specified shifts, `LDR` to PC, `STM` storing PC
  (address + 12), `LDM` user-bank transfers, unaligned `LDR`/`LDRH`/`LDRSH` rotates, `MUL`/`MLA` carry, `BX` mode switch.
- Wait states and the prefetch buffer change instruction timing by large factors; many games are tolerant, but timing test ROMs
  and some hardware-banging demos are not. Get functional correctness (M1-M3) before timing refinements.
- DMA and interrupts interact (DMA blocks the CPU; IRQ latency); the order in which components are ticked within a bus access matters.
- EEPROM size cannot be read from the cartridge; it is inferred from DMA transfer lengths. Flash chips need the right manufacturer/device ID
  or games refuse to save.
- Affine backgrounds/sprites use fixed-point 8.8 parameters and reference points that latch at VBlank and advance per line.
- Games read VCOUNT/DISPSTAT in tight loops; inaccurate PPU timing shows up as hangs on "wait for VBlank".
- Do not mix GBA and Game Boy documentation: the register addresses overlap in places but mean different things.
- Keep emulation deterministic: audio output never influences emulated time inside `gba-core`.

## Gate (definition of done)
- [ ] jsmolka `gba-tests` ARM/Thumb/memory tests pass (per-ROM results recorded; exceptions documented).
- [ ] Save media tests pass (SRAM, Flash, EEPROM) and a save round-trips through the frontend.
- [ ] Graphics test suites match their reference screenshots.
- [ ] Sound: WAV dump verified, owner approves audio in at least 2 of their games, 10-minute run stable.
- [ ] Owner confirms 2+ of their own GBA games play correctly (boot, gameplay, save/reload) on desktop **and** the Steam Deck.
- [ ] Performance target met and recorded (headless multiple of real time; Deck 60 fps with sound).
- [ ] Earlier gates still pass: all Game Boy / Game Boy Color tests, replays and CI.
- [ ] No per-game hacks (grep like phase 5).

## Out of scope
Link cable / multiplayer, e-Reader, solar sensor, rumble, motion/gyro carts, cheat codes, Game Boy Player, overclocking, shaders,
debugger GUI, pixel-exact mid-scanline accuracy beyond what the owner's games and the test ROMs need, running Game Boy games
through the GBA core (the Game Boy core already does this).
