# Phase 1 — SM83 CPU

## Goal
A complete, cycle-stepped implementation of the Game Boy CPU (Sharp SM83) that passes all of Blargg's
`cpu_instrs` tests. A minimal bus (flat RAM + ROM + serial port) is enough for this phase.

## Prerequisites
Phase 0 gate met. Read Pan Docs "CPU Instruction Set", "CPU Registers and Flags", "Interrupts",
and the Power-Up Sequence page.

## Design (follow `CLAUDE.md` §4.2)
- Registers: A, F, B, C, D, E, H, L, SP, PC. Flags Z N H C live in bits 7..4 of F; **bits 3..0 of F are always 0**.
  Provide 16-bit pair accessors AF, BC, DE, HL.
- State: `ime`, `ime_pending` (EI delay), `halted`, plus a flag to model the HALT bug.
- `Bus` trait or concrete bus with `read(addr)`, `write(addr, value)`, each of which advances time by 1 M-cycle
  through `tick()`. In this phase `tick()` may just count cycles.
- Provide `idle()` = `tick()` without access, for the internal delay cycles of some instructions
  (e.g. `LD SP,HL`, `ADD HL,rr`, taken jumps, push, call, ret).
- Fetch = a bus read at PC; PC increments. Immediate operands are further bus reads.
- Write the instruction as a sequence of reads/writes/idles. Do **not** hardcode cycle tables.
  Use the opcode table only to *check* your counts.

## Tasks (in this order, commit after each)
1. Register file, flags, and unit tests for flag helpers.
2. Post-boot register state (DMG): AF=0x01B0, BC=0x0013, DE=0x00D8, HL=0x014D, SP=0xFFFE, PC=0x0100.
3. Fetch/decode loop and 8-bit loads (`LD r,r`, `LD r,n`, `LD r,(HL)`, `LD (HL),r`, `LDH`, `LD (nn),A`, `LD A,(BC/DE/HL±)` …).
4. 8-bit ALU: ADD, ADC, SUB, SBC, AND, XOR, OR, CP, INC, DEC. Unit-test flags exhaustively
   (all 256×256 operands for ADD/ADC/SUB/SBC against a slow reference formula is cheap and worth it).
5. 16-bit loads, PUSH/POP, 16-bit ALU: `ADD HL,rr`, `INC/DEC rr`, `ADD SP,e8`, `LD HL,SP+e8`.
6. Jumps, calls, returns, `RST`, `JR`, conditions. Conditional instructions take extra cycles only when taken.
7. Rotates and shifts: `RLCA/RLA/RRCA/RRA` (Z flag always 0) and the CB-prefixed group
   (`RLC RRC RL RR SLA SRA SWAP SRL BIT RES SET`, CB ops on `(HL)` take extra cycles).
8. `DAA`, `CPL`, `SCF`, `CCF`, `NOP`, `HALT`, `STOP`, `DI`, `EI`.
9. Interrupt dispatch: when `IME` set and `IE & IF != 0`, service the highest-priority bit
   (VBlank 0x40, STAT 0x48, Timer 0x50, Serial 0x58, Joypad 0x60). Dispatch takes 5 M-cycles.
   Minimal interrupt registers (IE 0xFFFF, IF 0xFF0F) on the bus now; full sources come in phase 2.
10. Serial test output: a write of 0x81 to 0xFF02 captures the byte at 0xFF01 into `take_serial()`.
11. Trace output (`--trace`) in Gameboy Doctor format. Diff against its reference logs for ROMs 01–11.
12. Illegal opcodes (0xD3 0xDB 0xDD 0xE3 0xE4 0xEB 0xEC 0xED 0xF4 0xFC 0xFD): hard-lock the CPU (stop executing); do not panic.

## Known pitfalls
- `POP AF` must mask the low nibble of F to 0.
- `ADD SP,e8` and `LD HL,SP+e8`: H and C come from carries of the **low byte** addition (unsigned), Z and N are 0.
- `DAA` is the classic source of failures: follow the N/H/C flag logic from Pan Docs exactly.
- `EI` takes effect **after the next instruction**, not immediately. `DI` is immediate.
- **HALT bug:** if `HALT` executes with IME=0 and an interrupt already pending, the CPU does not halt
  and the next byte is read twice (PC fails to increment once). Test ROM: Blargg `halt_bug`.
- `HALT` with IME=1 wakes and services the interrupt; with IME=0 it wakes and continues without servicing.
- `LD (HL+),A` / `LD (HL-),A` and `LDH (n),A` addressing: `0xFF00 + n`.
- `RLCA` etc. clear Z; the CB versions (`RLC A`) set Z from the result.
- `JR` offset is a signed byte relative to the PC *after* the operand.
- `CALL`/`RST`/interrupt push order: high byte first at SP-1, then low byte at SP-2.
- `CB` prefix fetch is its own bus read (1 M-cycle).

## Gate (definition of done)

> **Status: done.** Gate evidence, owner waivers and measured results are recorded in `PROGRESS.md` (section "Phase 1"). The boxes below are the original checklist and were not ticked individually.
- [ ] Unit tests for ALU/flag logic pass.
- [ ] Each of Blargg `cpu_instrs/individual/01…11` prints `Passed` via `gb-cli --serial --expect-serial Passed`.
- [ ] Combined `cpu_instrs.gb` passes (this one needs MBC1 — if the mapper isn't ready,
      do a minimal MBC1 now or record this as deferred to phase 2 in `PROGRESS.md`).
- [ ] Doctor trace diff clean for ROMs 01–11 (record that).
- [ ] An automated `cargo test` runs the individual ROMs (skipping if absent).
- [ ] `PROGRESS.md` scoreboard has real result lines.

## Out of scope
Timer, PPU, APU, real interrupt sources (only the dispatch logic), cartridge mappers beyond what the tests need.
