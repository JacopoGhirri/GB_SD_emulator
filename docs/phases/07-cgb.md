# Phase 7 — Game Boy Color

## Goal
Support CGB mode: colour palettes, banked VRAM/WRAM, double-speed CPU, HDMA. Pokémon Gold (the owner's CGB game; Silver
and Crystal share its hardware needs) plays correctly; `cgb-acid2` passes.

**Scope changes (owner, 2026-11-15):** the owner has Pokémon **Gold**, not Crystal, so Gold is the human gate game. Yellow in
colour is dropped (the owner has no Yellow ROM; same decision as phase 5). Crystal/Silver are untested but need no extra features.

## Prerequisites
Phase 6 gate met (or consciously deferred in `PROGRESS.md`). Read the CGB sections of Pan Docs:
CGB Registers, VRAM Banks, WRAM Banks, Palettes (CGB), Tile Attributes (BG map attributes), OAM attributes,
VRAM DMA (HDMA/GDMA), Speed Switch, and the CGB power-up register values.

## Tasks
1. **Mode detection**: header byte 0x143 = 0x80 (CGB-enhanced, works on DMG too) or 0xC0 (CGB only).
   `Model::Cgb` + CGB flag ⇒ run in CGB mode; CGB hardware with a DMG-only cart uses DMG compatibility mode
   (monochrome games get CGB-assigned palettes on real hardware — start with plain grey/green, refine in phase 8).
2. **Post-boot state for CGB** (e.g. A=0x11), per Pan Docs.
3. **VRAM bank** (VBK 0xFF4F): two 8 KiB banks. Bank 1 holds tile data and BG map **attributes**.
4. **WRAM banks** (SVBK 0xFF70): 8 × 4 KiB; bank 0 fixed at C000, banks 1–7 switchable at D000 (value 0 selects 1).
5. **Palettes**: BCPS/BCPD (0xFF68/69) and OCPS/OCPD (0xFF6A/6B) with auto-increment, 8 palettes × 4 colours × RGB555.
   Convert RGB555 → RGB888 (simple `x*255/31`; optional colour-correction later). Palette RAM is inaccessible during mode 3.
6. **BG map attributes**: palette number, VRAM bank, X flip, Y flip, BG-to-OAM priority.
7. **Sprite attributes (CGB)**: palette number (OBP 0–7), VRAM bank bit; priority is by **OAM index only** in CGB mode.
   LCDC bit 0 in CGB mode means "master priority" (BG always behind sprites when cleared), not "BG enable".
8. **Double speed**: KEY1 (0xFF4D) arming + `STOP` triggers the switch. CPU, timer, DMA run at 2×; **PPU and (mostly) APU stay at normal speed**.
   Take care that your cycle-stepped `tick()` handles two clock domains.
9. **HDMA/GDMA** (FF51–FF55): general-purpose DMA (stalls the CPU) and HBlank DMA (16 bytes per HBlank), start/cancel/status semantics.
10. **MBC3 RTC** (Gold/Silver/Crystal): confirm wall-clock behaviour from phase 4 works with these games; add the day-counter overflow/halt flag per Pan Docs.
11. Infrared port (FF56), undocumented CGB registers: return documented defaults; don't crash.
12. Run `cgb-acid2` and fix rendering until the screenshot matches.

## Known pitfalls
- Mixed-mode ROMs: CGB-enhanced carts must still behave on `Model::Dmg`; keep both paths tested (all earlier gates are re-run for DMG).
- Double-speed changes the cycle ratios: audio and PPU timing must not accidentally speed up.
- HBlank HDMA must only transfer while the PPU is in mode 0, and be cancellable.
- RGB555 colour order is little-endian R,G,B in 5 bits each (`0bBBBBBGGGGGRRRRR`).
- Tile bank selection applies per tile via attribute bit 3; the tile data addressing mode (LCDC.4) is unchanged.
- Gold/Silver/Crystal rely heavily on HDMA and mid-frame effects; failures there usually point to PPU mode/STAT timing, not to CGB features.

## Gate (definition of done)

> **Status: done.** Gate evidence, owner waivers and measured results are recorded in `PROGRESS.md` (section "Phase 7"). The boxes below are the original checklist and were not ticked individually.
- [ ] `cgb-acid2` matches its reference screenshot.
- [ ] All DMG gates from phases 1–3 still pass in DMG mode.
- [ ] Mooneye CGB-relevant acceptance tests: record results; Blargg `cgb_sound` recorded.
- [ ] Human confirms Pokémon **Gold** plays (new game → first town → first battle → save/reload, RTC-driven day/night behaves) on desktop and Steam Deck. (Yellow in colour: dropped by owner decision, see Goal.) The Deck session also covers the Steam Deck audio check deferred from phase 6.
- [ ] No per-game hacks.

## Out of scope
Colour correction/shaders, CGB boot ROM, link cable, infrared.
