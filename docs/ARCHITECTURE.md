# Architecture

How the emulator is put together, why, and how to extend it. Hardware facts come from
[Pan Docs](https://gbdev.io/pandocs/) and the test ROMs (see [REFERENCES.md](REFERENCES.md));
`CLAUDE.md` holds the project rules these choices follow.

## 1. The big picture

```
gb-sdl (window, audio, gamepad)        gb-cli (headless: tests, traces, screenshots)
        \                                     /
         `-------------- gb-core ------------'      std only: no I/O, no threads, no unsafe
                  GameBoy { Cpu, Bus }
                                 Bus owns: Cartridge, Ppu, Apu, Timer, Joypad, Serial,
                                           Dma (OAM), Hdma (CGB), Interrupts, WRAM, HRAM
```

`gb-core` is a library with one handle, `GameBoy`:

```rust
GameBoy::new(cartridge, Model::Dmg | Model::Cgb)
gb.step() -> u32            // one CPU instruction; returns T-cycles
gb.run_frame()              // until the PPU finishes a frame
gb.framebuffer() -> &[u32]  // 160x144, 0x00RRGGBB
gb.set_buttons(Buttons)     // joypad state
gb.drain_audio(&mut Vec<f32>)   // interleaved stereo
gb.save_ram() / load_save_ram(..)      // battery data
gb.save_state() / load_state(..)       // whole-machine snapshot
```

The core never reads a clock, a file or a random source, so **the same ROM and the same inputs give
the same pictures and audio**. That is what makes recorded playthroughs, hash-based regression tests
and save states reliable. The frontends do the I/O: files, the window, the sound device, wall-clock
time for the real-time clock.

## 2. The timing model (the most important design decision)

Emulated time advances in exactly one place: `Bus::tick`, which steps the timer, PPU, APU, DMA and
serial port by one **M-cycle** (4 T-cycles). Every CPU memory access (`Bus::read`, `Bus::write`)
calls it once; instructions with internal delays call `Bus::idle` for each delay cycle.

```rust
// a CPU instruction is a sequence of bus accesses, e.g. LD A,(HL):
let v = bus.read(hl);   // costs one M-cycle: every component moves forward by 4 T-cycles
```

So an instruction's length is not looked up in a table, it *emerges* from the accesses it makes, and
a component that is polled in the middle of an instruction sees the machine in the right state. The
alternative ("run the instruction, then add its cycle count to everyone") passes simple tests but
cannot pass the timing test ROMs (`instr_timing`, `mem_timing`, Mooneye), and retrofitting it is a
rewrite. Pan Docs' cycle tables are used only to *check* the counts, in unit tests.

In CGB double-speed mode the CPU, timer, serial port and OAM DMA run every M-cycle, while the PPU,
APU and the cartridge clock advance only every second one, so they keep their real-time rate (see
`Bus::tick_one` in `bus.rs`). A VRAM-DMA block additionally stalls the CPU: time passes but its
next access waits (`hdma_stall`).

## 3. Components

| File | Role | Notes |
|---|---|---|
| `lib.rs` | `GameBoy`: wires CPU and bus, public API, save-state container | |
| `cpu/` | SM83: `registers.rs`, `alu.rs`, decode and instructions in `mod.rs` | Opcodes decoded by bit layout `xx yyy zzz`; registers indexed B,C,D,E,H,L,(HL),A |
| `bus.rs` | Memory map, ownership of all components, `tick` | The only place time moves; also OAM-DMA bus conflicts and HDMA stalls |
| `interrupts.rs` | IE / IF | Components call `request()`; they never touch IF directly |
| `timer.rs` | DIV / TIMA / TMA / TAC | DIV is the top byte of a 16-bit counter; TIMA ticks on a falling edge, with the one-cycle reload quirk |
| `dma.rs`, `hdma.rs` | OAM DMA; CGB general and HBlank DMA | These count; the bus performs the copies |
| `ppu/` | LCD registers, mode state machine, `render.rs` scanline renderer, `debug.rs` VRAM views | See below |
| `apu/` | Two square channels, wave, noise, frame sequencer, mixer | Clocked from the timer's system counter; output rate is configurable |
| `joypad.rs`, `serial.rs` | Buttons and the select lines; serial transfer (test ROMs print here) | |
| `cartridge/` | Header parsing, `Mapper` trait, `mbc1.rs`, `mbc3.rs` (with RTC), `mbc5.rs` | See section 5 |
| `io.rs` | Register addresses, power-up values, read masks | Named constants, never magic numbers |
| `state.rs` | Little-endian writer / reader and `StateError` | See section 6 |
| `input_script.rs` | Text format for recorded button presses | Shared by `gb-sdl --record`, `gb-cli --input`, tests |
| `mooneye.rs` | The Mooneye pass/fail register signature | Shared by `gb-cli --mooneye` and the ROM tests |

**Power-up state.** No boot ROM is emulated or shipped. `Cpu::post_boot` and the component
constructors set the register values Pan Docs documents for the end of the boot sequence.

### The PPU

The PPU is a state machine advanced four dots per M-cycle (a frame is 154 lines of 456 dots). At
the end of mode 3, `render::render_line` draws the *whole* visible line from the registers, VRAM and
OAM at that moment. This is the one seam a pixel-FIFO renderer would replace: it would only need to
produce the same finished line. The known cost is that register changes *during* a line, and the
exact length of mode 3 (which depends on SCX and sprites), are not modelled. That accounts for the
failing Mooneye PPU timing ROMs, none of which affect the games tested
([ACCURACY.md](ACCURACY.md)). CGB colour palettes, VRAM banks and tile attributes are in the same
renderer.

## 4. The frontend (`gb-sdl`)

One thread. Each frame: poll SDL events, set the buttons, `run_frame()`, draw, flush the battery
save if due, then wait. Pacing is **audio-driven** when a sound device is open (the queue is kept at
about 50 ms; the wait that keeps it there is what holds 59.73 fps), and timer-driven otherwise,
during pause or slow motion.

| Module | Role |
|---|---|
| `main.rs` | Arguments, choosing direct or folder mode, SDL set-up, the list-then-play loop |
| `frontend.rs` | Window, audio device and gamepads shared by the list and the game; status messages |
| `menu.rs` / `library.rs` / `font.rs` | The game list: scanning `games/`, drawing in the 160x144 picture, a built-in 5x7 font |
| `game.rs` | One game session: the per-frame loop, battery save, state slots, recording |
| `layout.rs` | Where files go (beside the ROM, or `saves/` and `screenshots/`) |
| `config.rs` / `input.rs` | `key = value` settings file; key and gamepad bindings; the left stick |
| `save.rs` / `files.rs` | Atomic battery-save writes, state slots |
| `audio.rs` / `pacing.rs` / `video.rs` / `palette.rs` / `png.rs` | Audio queue, timer pacer, integer scaling, DMG colour schemes, a small PNG writer for screenshots |

The game list is drawn into the same 160x144 picture as a game, so it shares the window, scaling and
pacing. Leaving a game returns to the list; `Exit::Menu` versus `Exit::Quit` in `game.rs` decides.

## 5. Adding a cartridge mapper

Cartridges implement the small `Mapper` trait in `cartridge/mod.rs`. The ROM bytes stay in
`Cartridge`; the mapper decides which byte an address means, and owns the cartridge RAM and any clock.

1. **Read the hardware description first** (Pan Docs "MBC..." section) and note the register ranges.
2. **Write `cartridge/mbcN.rs`** implementing `Mapper`: `read_rom`, `write_rom` (programs registers),
   `read_ram`, `write_ram`, `save_data` / `load_save` (battery data; a short or long file must never
   panic: `Ram::load` copies what fits), and `save_state` / `load_state` (registers, RAM, any clock).
   Wrap bank numbers to the real ROM size.
3. **Register it** in `Cartridge::from_bytes` (the `match` on the type byte), in `has_battery` /
   `has_rtc` if relevant, and in `CartridgeType::name` (`cartridge/header.rs`). Until a type is
   listed, loading it is refused with `UnsupportedType`, never run with wrong banking.
4. **Test it**: unit tests for the bank arithmetic next to the code (see `mbc1.rs`); a Mooneye
   `emulator-only` ROM if one exists for it; add its type byte to `SUPPORTED` in
   `tests/robustness.rs` and to the save-state tests (`tests/save_state.rs`) so a forgotten field is
   caught. Mapper tests live beside the existing ones; copy their pattern.
5. Update the supported list in the `UnsupportedType` message and in the README.

## 6. Save states

`GameBoy::save_state() -> Vec<u8>` / `load_state(&[u8]) -> Result<(), StateError>`. The format is a
header (magic `GBST`, version, model, CGB-mode flag, a hash of the cartridge header, payload length,
a checksum) followed by every component's fields, written by hand in a fixed order by `save_state`
and read back by `load_state` (`state.rs`). There is no serde: the core has no dependencies, and an
explicit field list makes a forgotten field visible in review.

- A state holds everything that changes the machine's future, including both framebuffers. It does
  **not** hold the ROM, the audio output queue (loading must not replay stale samples), or
  frontend settings.
- A failed load leaves the machine unchanged (a backup is restored). A state for another ROM or
  model, a truncated or corrupt file, or a different format version is an `Err`, never a panic.
- **When you add a field to any component**, add it to both its `save_state` and `load_state`, and
  bump `STATE_VERSION` in `lib.rs` if the layout changed. `tests/save_state.rs` saves at awkward moments
  (mid-frame, mid-DMA, mid-HDMA, with timers about to fire) and compares what happens afterwards, so
  a forgotten field shows up as a difference.

## 7. Tests: how a claim becomes true

Tests are the source of truth for hardware behaviour (CLAUDE.md section 6).

| Kind | Where | Pass criterion |
|---|---|---|
| Unit tests | next to the code | ALU flags, timer edge cases, mapper bank maths, ... |
| Blargg | `tests/cpu_instrs.rs`, `blargg_timing.rs`, `dmg_sound.rs`, `cgb_sound.rs` | Text on the serial port or in cartridge RAM ("Passed") |
| Gameboy Doctor | `tests/doctor_trace.rs` | One line per instruction diffed against a known-good log |
| Mooneye | `tests/mooneye.rs` | Registers B,C,D,E,H,L = 3,5,8,13,21,34 at `LD B,B` |
| acid2 and screenshots | `gb-cli/tests/acid2.rs` | Final frame equals a reference PNG |
| Determinism and save states | `tests/determinism.rs`, `save_state.rs`, `save_roundtrip.rs` | Identical hashes and bytes after a round trip |
| Robustness | `tests/robustness.rs` | Corrupt ROMs, saves and states: error or harmless play, never a panic |
| Local game checks | `tests/pokemon_local.rs` | Recorded playthroughs reach the owner-approved picture hashes (skips without the ROM) |

Third-party ROMs are never committed: `scripts/fetch_test_roms.sh` downloads a pinned release and
verifies its SHA-256. **A ROM test must skip with a message, not fail, when its ROM is absent**;
`scripts/run_all_tests.sh` shows the skips in a SKIPPED column so a missing ROM cannot pass as green.

**Adding a test suite**

1. Find the ROMs in the pinned bundle (or bump the pin in `fetch_test_roms.sh` and
   [REFERENCES.md](REFERENCES.md) together).
2. Write `crates/gb-core/tests/<name>.rs` using the helpers in `tests/common/mod.rs`:
   `run_rom`, `run_blargg_ram`, `run_mooneye_rom`, `mooneye_sweep`. They return `None` (and print
   `SKIP`) when the ROM is missing.
3. Choose the criterion above. For a suite with known failures, **record** the exact set that passes
   today and assert it does not change, rather than weakening the test (see `tests/mooneye.rs`).
4. Run `scripts/run_all_tests.sh`, paste the new row into [ACCURACY.md](ACCURACY.md).

## 8. Debugging a wrong picture or a failing test

In order (CLAUDE.md section 8): reproduce headlessly with `gb-cli` and a frame or instruction limit;
for CPU bugs diff a Gameboy Doctor trace (`gb-cli --trace`), where the first differing line is the
bug; narrow with Blargg's individual ROMs; dump state (`--screenshot`, `--dump-tiles`,
`--dump-bgmap-*`); compare behaviour (never code) with a reference emulator; add a regression test.

## 9. Where it is deliberately simple

- Scanline PPU, not pixel-FIFO (section 3).
- The APU is the DMG model for both consoles; a few CGB-only audio details are not modelled.
- Mappers: ROM only, MBC1, MBC3, MBC5. Anything else is refused.
- No link cable, no rewind, no fast-forward, no shaders.

The full, measured list of what fails and why is [ACCURACY.md](ACCURACY.md).
