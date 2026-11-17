# Accuracy scoreboard

Produced by `scripts/run_all_tests.sh` (run in WSL Ubuntu, rustc 1.100.0, test ROMs v7.0 present) on 2026-11-17 (after save states were added).
A "passed" test in a *recorded* sweep (Mooneye CGB, `cgb_sound`, `dmg_sound`, `acceptance/ppu`) means the recorded
results did not change; the per-ROM detail is under the table. SKIPPED counts tests that skipped for a missing ROM:
the one skip here is the deliberate missing-ROM test in `harness_smoke`.

```
suite                               passed  failed ignored  SKIPPED
unit: gb_cli                             9       0       0        0
acid2                                    7       0       0        0
wav                                      1       0       0        0
unit: gb_core                          164       0       0        0
audio_determinism                        1       0       0        0
blargg_timing                            3       0       0        0
cgb_sound                                1       0       0        0
cpu_instrs                              12       0       0        0
determinism                              2       0       0        0
dmg_sound                                1       0       0        0
doctor_trace                            11       0       0        0
harness_smoke                            2       0       0        1
mooneye                                  9       0       0        0
pokemon_local                            2       0       0        0
save_roundtrip                           5       0       0        0
save_state                              18       0       0        0
unit: gb_sdl                            20       0       0        0
doc-tests gb_core                        0       0       0        0
TOTAL                                  268       0       0        1


per-ROM summaries printed by the sweeps:
  mem_timing
  mem_timing
  02-len ctr: PASS (status 0x00, 3944448 instructions)
  03-trigger: PASS (status 0x00, 7016448 instructions)
  04-sweep: PASS (status 0x00, 536576 instructions)
  05-sweep details: PASS (status 0x00, 524288 instructions)
  06-overflow on trigger: PASS (status 0x00, 401408 instructions)
  07-len sweep period sync: PASS (status 0x00, 270336 instructions)
  08-len ctr during power: PASS (status 0x00, 593920 instructions)
  09-wave read while on: PASS (status 0x00, 290816 instructions)
  10-wave trigger while on: FAIL (status 0x01, 1970176 instructions)  [known: DMG wave-RAM corruption on retrigger: needs a different first-sample delay than 09/12 allow]
  11-regs after power: PASS (status 0x00, 323584 instructions)
  12-wave write while on: PASS (status 0x00, 1961984 instructions)
  mooneye-test-suite/acceptance/bits: 3 passed, 0 failed
  mooneye-test-suite/acceptance/interrupts: 1 passed, 0 failed
  mooneye-test-suite/acceptance/oam_dma: 3 passed, 0 failed
  mooneye-test-suite/acceptance/timer: 13 passed, 0 failed
  CGB-SUMMARY: 66 passed, 13 failed
  mooneye-test-suite/emulator-only/mbc1: 13 passed, 0 failed
  mooneye-test-suite/emulator-only/mbc5: 8 passed, 0 failed
  test approved_replays_match ... pokemon_red: 4 hashes match after 48789 frames
  pokemon_red.run2: 6 hashes match after 80740 frames

```

## Known failures
- Not in the table because the recorded tests treat them as expected (counts copied from PROGRESS.md except CGB-SUMMARY and dmg_sound 10, which the run above printed; the 13 DMG root ROMs and the AGE ROMs are not run by the script yet): 13 Mooneye ROMs on CGB (`CGB-SUMMARY: 66 passed, 13 failed`),
  6 of 12 Mooneye `acceptance/ppu` on DMG, 13 non-gated DMG `acceptance/` root ROMs, `dmg_sound` 10, `cgb_sound` 5 of 12, AGE `spsw-*` (6 ROMs).

## Triage (by impact on real games)
Impact is my judgement from what each ROM tests, not measured against a game list. Owner-played so far: Pokemon Red, Blue (DMG) and Gold (CGB): no visible problems.

| Failure | Impact | Decision |
|---|---|---|
| Mooneye `jp/call/ret/add_sp/ld_hl_sp_e/reti` timing (9 ROMs, DMG and CGB) | Low for games, but one root cause: the order of internal-delay M-cycles versus bus accesses inside these instructions. Matters only when a game reads a timer or PPU register in the middle of such an instruction. | Left. Candidate for a later accuracy phase: one fix likely clears all 9. |
| Mooneye `halt_ime0_*`, `di_timing-GS`, `halt_ime1_timing2-GS` (DMG) | Low: HALT/EI edge timing. | Left. |
| Mooneye `ppu` `intr_2_mode0_timing_sprites`, `hblank_ly_scx_timing`, `lcdon_*`, `stat_lyc_onoff`, `vblank_stat_intr` | Medium in theory (mode-3 length depends on SCX and sprites; games with mid-line raster effects), none seen in the games played. Needs a pixel-FIFO PPU. | Left: deep work, owner declined (2026-11-16). |
| `dmg_sound` 10 (wave retrigger corruption) | Very low: games do not rely on it. | Left. |
| `cgb_sound` 08, 09, 10, 11, 12 | Low: CGB length-counter/NR41 power-off and wave-RAM access rules; Gold's audio was judged fine on desktop and Deck. | Left. |
| AGE `speed-switch/spsw-*` (6 ROMs) | Low: exact DIV/TIMA/PPU phase across a speed switch; Gold switches speed without visible effect. | Left. |
| Mooneye `unused_hwio-C` (fails at FF68 in DMG-compat mode) | Very low. | Left. |
| DMG-compat palettes on CGB (`mbc3-tester-cgb`, `rtc3test-*-cgb` references not run) | Only for a DMG-only ROM forced onto `--model cgb`: the frontend and CLI choose the model from the header, so a DMG game runs in DMG mode with the normal DMG renderer. No real game is affected. | Not fixed. Real DMG palettes arrive with task B7 (selectable palettes), which is the user-visible version of this. |
| CGB DMG-compat register values (`boot_regs-cgb` table for all titles) | Very low. | Left. |

Net: no accuracy fix is justified by a game the owner plays, so task A2 makes no code change.
