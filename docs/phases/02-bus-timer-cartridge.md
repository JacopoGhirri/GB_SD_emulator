# Phase 2 — Bus, Interrupts, Timer, DMA, Cartridge Mappers

## Goal
Replace the minimal bus with the real memory map and add the non-graphics hardware:
timer, interrupt sources, OAM DMA, joypad register, serial, and the cartridge mappers
needed by Pokémon (MBC1, MBC3 with RTC, MBC5). Pass the timing test suites.

## Prerequisites
Phase 1 gate met. Read Pan Docs: Memory Map, Timer and Divider Registers, Interrupts,
OAM DMA Transfer, Joypad Input, Serial Data Transfer, "The Cartridge Header", MBC1/MBC3/MBC5 pages,
and the Power-Up Sequence (initial I/O register values).

## Tasks
1. **Memory map** (`bus.rs`): ROM 0000–7FFF (via cartridge), VRAM 8000–9FFF, cartridge RAM A000–BFFF,
   WRAM C000–DFFF, echo E000–FDFF (mirror of C000–DDFF), OAM FE00–FE9F, unusable FEA0–FEFF,
   I/O FF00–FF7F, HRAM FF80–FFFE, IE FFFF. VRAM/OAM are plain storage for now (PPU phase adds access rules).
2. **Power-up I/O values**: initialise every I/O register to its documented post-boot value.
3. **Interrupts** (`interrupts.rs`): IF (0xFF0F, upper 3 bits read as 1), IE, `request(Interrupt)` API for components.
4. **Timer** (`timer.rs`):
   - Internal 16-bit system counter; `DIV` (0xFF04) is its **upper byte**; writing DIV resets the whole counter.
   - `TAC` (0xFF07) selects a bit of the counter (4096/262144/65536/16384 Hz). TIMA increments on the
     **falling edge** of `(selected_bit AND timer_enable)`. Writing DIV or TAC can therefore cause an increment.
   - TIMA overflow: for one M-cycle TIMA reads 0x00, then it is reloaded from TMA and the interrupt is requested.
     Writing TIMA during that window cancels the reload; writing TMA during the reload cycle affects the value loaded.
5. **OAM DMA** (`dma.rs`): writing 0xFF46 copies 160 bytes from `value<<8` to OAM, one byte per M-cycle,
   starting one M-cycle after the write. While active the CPU can only access HRAM (other reads return 0xFF).
6. **Joypad register** 0xFF00: select bits 4/5, button state in low nibble (active low), joypad interrupt on a high→low transition.
   `set_buttons` API feeds it.
7. **Serial**: keep the test-capture behaviour. (Optionally implement real transfer timing with no partner: internal clock completes, byte read back as 0xFF.)
8. **Cartridge** (`cartridge/`):
   - Header parse: type, ROM size (banks), RAM size, title, checksum; reject unsupported types with a clear error.
   - Trait for mappers. Implement `RomOnly`, **MBC1** (ROM/RAM banking, mode select; note the 0→1 bank quirk and the upper-bits behaviour),
     **MBC3** (ROM bank 1–127, RAM banks 0–3, RTC registers 08–0C selected through the RAM bank register, latch via 0→1 write to 6000–7FFF),
     **MBC5** (9-bit ROM bank, 4-bit RAM bank; bank 0 selectable in the switchable area).
   - Battery RAM: expose `save_data()` / `load_save()`. RTC state is appended to the save data (define a simple, documented format).
   - RTC: derive time from emulated cycles by default (deterministic, used by tests).
     A wall-clock mode is added in phase 4/8 for the real frontend.
   - RAM enable (0x0A written to 0000–1FFF) must gate RAM access.
9. Re-run all Phase 1 tests to make sure nothing regressed.

## Known pitfalls
- Reading unmapped/disabled regions returns 0xFF.
- Writes to ROM space go to the mapper registers, never to ROM.
- MBC bank numbers must be masked to the actual number of banks (ROMs smaller than the max rely on wraparound).
- Timer behaviour is the most common source of subtle failures in later games; do not guess, use the Mooneye timer tests.
- Don't make DIV a free-running 8-bit counter. It's the top byte of a 16-bit counter (this matters for APU later too).
- IF upper three bits are always 1 on read.

## Gate (definition of done)

> **Status: done.** Gate evidence, owner waivers and measured results are recorded in `PROGRESS.md` (section "Phase 2"). The boxes below are the original checklist and were not ticked individually.
- [ ] All Phase 1 tests still pass, including combined `cpu_instrs.gb` (MBC1).
- [ ] Blargg `instr_timing` passes.
- [ ] Blargg `mem_timing` and `mem_timing-2` pass (if they fail due to unfinished PPU/other, record exactly why).
- [ ] Mooneye `acceptance/timer/*` all pass; `acceptance/oam_dma/*`, `acceptance/interrupts/*`,
      and `acceptance/bits/*` — record pass/fail counts (document each failing test and the reason).
- [ ] Mooneye MBC tests for MBC1/MBC5 (`emulator-only/`) pass; MBC3 unit-tested (bank math, RTC latch).
- [ ] Unit tests for the timer falling-edge cases and the TIMA overflow window.
- [ ] `PROGRESS.md` has the scoreboard with real counts.

## Out of scope
PPU behaviour (modes, STAT, LY), audio, CGB-specific registers.
