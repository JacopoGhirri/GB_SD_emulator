# REFERENCES

Verify each URL on first use; repositories move. Prefer sources higher in the list.

## Hardware documentation
1. **Pan Docs** (the canonical reference): https://gbdev.io/pandocs/
   Memory map, registers, timer, PPU modes, interrupts, MBCs, power-up state, CGB features, audio.
2. **Opcode table** with flags and cycle info: https://gbdev.io/gb-opcodes/optables/
   Also: https://gbdev.io/pandocs/CPU_Instruction_Set.html
3. **GB Programming Manual / Game Boy CPU manual** for instruction semantics (cross-check against Pan Docs).
4. **Gekkio's "Game Boy: Complete Technical Reference"** — deep detail on timing and CPU internals.
5. **The Cycle-Accurate Game Boy Docs** (AntonioND) — PPU/timing details when you reach accuracy work.

## Test ROMs (all freely redistributable test suites)
| Suite | What it checks | Pass criterion |
|---|---|---|
| Blargg `cpu_instrs` (01–11 + combined) | Every CPU instruction | Serial output ends with `Passed` |
| Blargg `instr_timing` | Instruction cycle counts | Serial `Passed` |
| Blargg `mem_timing`, `mem_timing-2` | Cycle timing of memory accesses | Serial `Passed` |
| Blargg `halt_bug`, `interrupt_time` | HALT bug, interrupt timing | Serial `Passed` / screenshot |
| Blargg `dmg_sound`, `cgb_sound` | APU | Serial `Passed` |
| Mooneye test suite (`acceptance/`) | Timing, DMA, timer, MBCs, PPU | Registers B,C,D,E,H,L = 3,5,8,13,21,34 after `ld b,b` (0x40) |
| `dmg-acid2` | DMG PPU rendering | Screenshot equals reference image |
| `cgb-acid2` | CGB PPU rendering | Screenshot equals reference image |

### Pinned bundle (used by `scripts/fetch_test_roms.sh`)
- Source: https://github.com/c-sp/game-boy-test-roms, release **v7.0** (published 2024-02-25), licence MIT.
  Each suite inside keeps its own licence; see the README files in the bundle.
- Archive `game-boy-test-roms-v7.0.zip`, 3,756,786 bytes,
  SHA-256 `B9A9D7A1075AA35A3D07C07C34974048672D8520DCA9E07A50178F5860C3832C`
  (computed by hand on 2026-11-13, since GitHub reports no digest for this asset; the script verifies it).
- Extracted to `tests/roms/game-boy-test-roms-v7.0/`. Key paths:
  - Blargg `cpu_instrs`: `blargg/cpu_instrs/cpu_instrs.gb` and `blargg/cpu_instrs/individual/*.gb`
  - Mooneye acceptance: `mooneye-test-suite/`
  - `dmg-acid2/`, `cgb-acid2/`

Other sources (not used yet): `retrio/gb-test-roms`, `Gekkio/mooneye-test-suite`,
`mattcurrie/dmg-acid2`, `mattcurrie/cgb-acid2`.

## Debug tooling
- **Gameboy Doctor** (`robert/gameboy-doctor`): known-good execution logs for Blargg cpu_instrs ROMs
  and a script that diffs your log against them. Needs LY (0xFF44) to read constant 0x90 in its "doctor mode".
  Used in phase 1 via `scripts/fetch_doctor_logs.sh` (pinned commit `cc742777af7d71de3061c65733ecfa250237012b`).

## Reference emulators (study behaviour, do not copy code)
- SameBoy (very accurate), Gambatte (accurate), mGBA (also emulates the Game Boy),
  Mooneye GB (Rust, readable).

## Cartridge header quick facts
- 0x0100–0x014F header. Title 0x134–0x143, CGB flag 0x143, cart type 0x147, ROM size 0x148,
  RAM size 0x149, header checksum 0x14D.
- Pokémon cart types: Red/Blue = MBC3+RAM+BATTERY (0x13); Yellow = MBC5+RAM+BATTERY (0x1B);
  Gold/Silver/Crystal = MBC3+TIMER+RAM+BATTERY (0x10). Confirm from the header, do not hardcode.
