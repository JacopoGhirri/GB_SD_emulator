# Phase 3 — PPU (DMG graphics)

## Goal
Render correct Game Boy (DMG, 4-shade) graphics with correct enough timing that games and
`dmg-acid2` work. Design so a more accurate pixel-FIFO can replace the renderer later without touching the rest.

## Prerequisites
Phase 2 gate met. Read Pan Docs: LCD Control, LCD Status and Interrupts, LCD Position and Scrolling,
Palettes, Tile Data, Tile Maps, OAM, Rendering (modes and timing), Window, "Object Attribute Memory".

## Timing facts (verify in Pan Docs)
- 456 dots (T-cycles) per scanline, 154 lines (0–143 visible, 144–153 VBlank), 70224 dots per frame (~59.7275 Hz).
- Visible lines: mode 2 (OAM scan, 80 dots) → mode 3 (drawing, 172–289 dots) → mode 0 (HBlank, remainder).
- Lines 144–153: mode 1 (VBlank). VBlank interrupt requested at the start of line 144.
- LY (0xFF44), LYC (0xFF45), STAT (0xFF41).

## Tasks
1. **State machine**: PPU `tick()` is called by `Bus::tick()` every M-cycle (4 dots). Track dot counter, LY, mode.
   Mode 3 length: start with a fixed 172 dots; refine later (sprites, SCX, window extend it).
2. **LCDC (0xFF40)**: bit 7 LCD enable, 6 window map, 5 window enable, 4 BG/window tile data select, 3 BG map,
   2 sprite size, 1 sprite enable, 0 BG enable.
   Turning the LCD off: LY resets to 0, mode 0, screen blank; turning on restarts a frame (first frame behaves slightly differently — note it).
3. **STAT interrupts**: sources (HBlank, VBlank, OAM, LYC==LY). The interrupt fires on a **rising edge of the
   OR of all enabled sources** (STAT "blocking"). Implement the shared line, not independent triggers.
4. **Background renderer**: tile data (8000/8800 addressing), maps (9800/9C00), SCX/SCY scrolling with wrap, BGP palette.
5. **Window**: WX/WY, WX-7 offset, the window's **internal line counter** (only increments on lines where the window was actually drawn).
6. **Sprites**: OAM scan picks the first **10 sprites per line** in OAM order. 8×8 and 8×16 (bit 0 of tile index ignored in 16 mode).
   X/Y flip, palette select (OBP0/OBP1), BG-over-OBJ priority flag, colour 0 is transparent.
   DMG priority: lower X wins; on equal X the earlier OAM entry wins.
7. **VRAM/OAM access rules**: VRAM is inaccessible (reads 0xFF, writes ignored) during mode 3; OAM during modes 2 and 3.
   (Also while the LCD is on only; with LCD off both are accessible.)
8. **Output**: write final 2-bit colour indices through BGP/OBP to a `0x00RRGGBB` framebuffer using a fixed greenish or grayscale palette (configurable later).
   Provide a "frame ready" signal for `run_frame()`.
9. **Debug tooling** in `gb-cli`: `--screenshot` already exists; add dump commands for the tile data as a PNG atlas,
   and the two BG maps as PNGs. These save huge amounts of time.

## Suggested approach
Start with a **per-scanline renderer**: render the whole line at the point where mode 3 ends (or starts).
This is simple and passes most games. Leave a clear seam (`fn render_line(&mut self)`) for a later pixel-FIFO rewrite
if phase 8 decides to pursue accuracy.

## Known pitfalls
- Tile data select (LCDC.4) uses signed indexing for the 8800 method; map tile numbers are interpreted accordingly.
- Window enabled but WX ≥ 167 or WY > 143 means it never shows.
- STAT write quirks: on DMG, writing STAT can briefly trigger a spurious interrupt (see Pan Docs). Document your choice.
- LYC=LY flag/interrupt must be re-evaluated whenever LY or LYC changes.
- Don't stall emulation when the LCD is off: VBlank-timed game loops still need LY and frame pacing; when off, still produce a frame boundary every 70224 dots so the frontend keeps pacing.
- Framebuffer must hold a stable image at frame end; do not expose a half-drawn frame.

## Gate (definition of done)
- [ ] `dmg-acid2` output matches the reference screenshot exactly via `gb-cli --compare`.
- [ ] Mooneye PPU tests (`acceptance/ppu/*`): record pass/fail per test; no regression elsewhere.
- [ ] Blargg `mem_timing` etc. still pass; all earlier gates still pass.
- [ ] A homebrew/test ROM with scrolling, sprites and window renders visibly correct screenshots (human reviews the PNGs).
- [ ] `cargo test` includes acid2 as an automated screenshot test.

## Out of scope
Cycle-exact mode-3 length, pixel FIFO, CGB colour, audio.
