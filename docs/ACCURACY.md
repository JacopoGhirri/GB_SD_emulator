# Accuracy scoreboard

Produced by `scripts/run_all_tests.sh` (run in WSL Ubuntu, rustc 1.100.0, test ROMs v7.0 present) on 2026-11-16.
A "passed" test in a *recorded* sweep (Mooneye CGB, `cgb_sound`, `dmg_sound`, `acceptance/ppu`) means the recorded
results did not change; the per-ROM detail is under the table. SKIPPED counts tests that skipped for a missing ROM:
the one skip here is the deliberate missing-ROM test in `harness_smoke`.

```
suite                               passed  failed ignored  SKIPPED
unit: gb_cli                             9       0       0        0
acid2                                    7       0       0        0
wav                                      1       0       0        0
unit: gb_core                          162       0       0        0
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
unit: gb_sdl                            20       0       0        0
doc-tests gb_core                        0       0       0        0
TOTAL                                  248       0       0        1


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

## Known failures (triage is in the next section, added in task A2)
- Not in the table because the recorded tests treat them as expected (counts copied from PROGRESS.md except CGB-SUMMARY and dmg_sound 10, which the run above printed; the 13 DMG root ROMs and the AGE ROMs are not run by the script yet): 13 Mooneye ROMs on CGB (`CGB-SUMMARY: 66 passed, 13 failed`),
  6 of 12 Mooneye `acceptance/ppu` on DMG, 13 non-gated DMG `acceptance/` root ROMs, `dmg_sound` 10, `cgb_sound` 5 of 12, AGE `spsw-*` (6 ROMs).
