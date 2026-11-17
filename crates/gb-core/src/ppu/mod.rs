//! The PPU (pixel processing unit), DMG and CGB: LCD registers, VRAM, OAM and the line/mode state machine
//! (Pan Docs "Rendering", "LCD Status Register", "LCD Position and Scrolling").
//!
//! The PPU is advanced by `Bus::tick`, 4 dots (T-cycles) per M-cycle. A frame is 154 lines of
//! 456 dots = 70224 dots (~59.7 Hz). Lines 0..=143 are visible, 144..=153 are VBlank.

pub mod debug;
mod render;

use crate::{SCREEN_HEIGHT, SCREEN_WIDTH};

// LCD register addresses (Pan Docs "Hardware Registers", FF40-FF4B). DMA (FF46) lives in `dma`.
pub const REG_LCDC: u16 = 0xFF40;
pub const REG_STAT: u16 = 0xFF41;
pub const REG_SCY: u16 = 0xFF42;
pub const REG_SCX: u16 = 0xFF43;
pub const REG_LY: u16 = 0xFF44;
pub const REG_LYC: u16 = 0xFF45;
pub const REG_BGP: u16 = 0xFF47;
pub const REG_OBP0: u16 = 0xFF48;
pub const REG_OBP1: u16 = 0xFF49;
pub const REG_WY: u16 = 0xFF4A;
pub const REG_WX: u16 = 0xFF4B;

pub const VRAM_SIZE: usize = 0x2000;
pub const OAM_SIZE: usize = 0xA0;

/// Dots per scanline and lines per frame (Pan Docs "Rendering").
pub const DOTS_PER_LINE: u32 = 456;
pub const LINES_PER_FRAME: u8 = 154;
pub const FIRST_VBLANK_LINE: u8 = 144;
pub const DOTS_PER_FRAME: u32 = DOTS_PER_LINE * LINES_PER_FRAME as u32;
/// Mode 2 (OAM scan) lasts 80 dots. Mode 3 (drawing) really lasts 172..=289 dots depending on
/// sprites, SCX and the window; we use the minimum, 172 (cycle-exact length is out of scope).
const OAM_SCAN_DOTS: u32 = 80;
const DRAW_DOTS: u32 = 172;

/// LCDC bits (Pan Docs "LCD Control").
pub const LCDC_ENABLE: u8 = 0x80;

/// STAT mode numbers.
pub const MODE_HBLANK: u8 = 0;
pub const MODE_VBLANK: u8 = 1;
pub const MODE_OAM_SCAN: u8 = 2;
pub const MODE_DRAWING: u8 = 3;

/// The four DMG shades, lightest first, as 0x00RRGGBB. Plain greyscale with the values the
/// dmg-acid2 README requires for pixel comparison: $FF, $AA, $55, $00.
pub const SHADES: [u32; 4] = [0x00FF_FFFF, 0x00AA_AAAA, 0x0055_5555, 0x0000_0000];

/// Interrupt requests raised by one PPU step; the bus forwards them to IF.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PpuIrq {
    pub vblank: bool,
    pub stat: bool,
    /// The PPU just entered mode 0 on a visible line (HBlank DMA moves a block now).
    pub hblank: bool,
}

/// VBK (0xFF4F): VRAM bank select, CGB mode only (Pan Docs "VRAM Banks"). Bit 0 selects the
/// bank; the other bits read as 1.
pub const REG_VBK: u16 = 0xFF4F;
/// CGB colour palette registers (Pan Docs "LCD Color Palettes (CGB only)"): index/data pairs for
/// the BG and OBJ palette RAM.
pub const REG_BCPS: u16 = 0xFF68;
pub const REG_BCPD: u16 = 0xFF69;
pub const REG_OCPS: u16 = 0xFF6A;
pub const REG_OCPD: u16 = 0xFF6B;

pub struct Ppu {
    /// VRAM bank 0 (tile data and the tile maps).
    vram: [u8; VRAM_SIZE],
    /// VRAM bank 1 (CGB only): more tile data, and the BG map *attributes* at 0x9800/0x9C00.
    vram1: [u8; VRAM_SIZE],
    /// Bank the CPU sees at 0x8000..=0x9FFF (VBK bit 0). The renderer picks banks per tile.
    vbk: u8,
    /// CGB mode: the CGB renderer (attributes, RGB555 palettes) is used.
    cgb: bool,
    /// CGB hardware: VBK and the palette registers exist, also in DMG compatibility mode.
    cgb_hw: bool,
    /// CGB palette RAM: 8 palettes x 4 colours x 2 bytes (RGB555, little-endian), BG and OBJ.
    bg_pal: [u8; 64],
    obj_pal: [u8; 64],
    /// BCPS/OCPS: bit 7 auto-increment after a data write, bits 0-5 the byte index.
    bcps: u8,
    ocps: u8,
    oam: [u8; OAM_SIZE],
    lcdc: u8,
    /// Only the four writable STAT bits (3..=6); mode and LYC flag are computed.
    stat_enable: u8,
    scy: u8,
    scx: u8,
    ly: u8,
    lyc: u8,
    bgp: u8,
    obp0: u8,
    obp1: u8,
    wy: u8,
    wx: u8,
    /// Dot position inside the current line (0..456) while the LCD is on.
    dot: u32,
    /// Dots elapsed in the current "virtual frame" while the LCD is off.
    off_dots: u32,
    /// True for the line after the LCD is switched on: it skips OAM scan and starts in mode 0
    /// (Pan Docs "LCD Control", bit 7 note; first frame after enabling differs from the rest).
    first_line_after_on: bool,
    /// Last value of the shared STAT interrupt line (see `stat_line`).
    stat_line: bool,
    frame_ready: bool,
    /// Window's internal line counter: which window row to draw next. Reset every frame.
    window_line: u8,
    /// Frame being drawn; copied to `framebuffer` at VBlank so a half-drawn frame is never shown.
    back: Vec<u32>,
    framebuffer: Vec<u32>,
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}

impl Ppu {
    /// Post-boot register values (Pan Docs "Power-Up Sequence", DMG column).
    pub fn new() -> Ppu {
        Ppu {
            vram: [0; VRAM_SIZE],
            vram1: [0; VRAM_SIZE],
            vbk: 0,
            cgb: false,
            cgb_hw: false,
            // The CGB boot ROM leaves white in the BG palettes; OBJ palette RAM is undefined
            // after boot, white is a safe choice.
            bg_pal: [0xFF, 0x7F].repeat(32).try_into().expect("64 bytes"),
            obj_pal: [0xFF, 0x7F].repeat(32).try_into().expect("64 bytes"),
            // Post-boot index registers read 0xC8 / 0xD0 (auto-increment, index 8 / 16): the boot
            // ROM left them there after writing palette data (Mooneye `misc/boot_hwio-C`).
            bcps: 0x88,
            ocps: 0x90,
            oam: [0; OAM_SIZE],
            lcdc: 0x91,
            stat_enable: 0,
            scy: 0,
            scx: 0,
            ly: 0,
            lyc: 0,
            bgp: 0xFC,
            // OBP0/OBP1 are undefined after boot; 0xFF matches what the I/O table had.
            obp0: 0xFF,
            obp1: 0xFF,
            wy: 0,
            wx: 0,
            dot: 0,
            off_dots: 0,
            first_line_after_on: false,
            stat_line: false,
            frame_ready: false,
            window_line: 0,
            back: vec![SHADES[0]; SCREEN_WIDTH * SCREEN_HEIGHT],
            framebuffer: vec![SHADES[0]; SCREEN_WIDTH * SCREEN_HEIGHT],
        }
    }

    /// Switch on CGB behaviour (called once by the bus when the console is built).
    pub fn set_cgb(&mut self, cgb: bool) {
        self.cgb = cgb;
        self.cgb_hw |= cgb;
    }

    /// CGB hardware, possibly running a DMG-only cartridge (renderer stays DMG-style).
    pub fn set_cgb_hardware(&mut self, hw: bool) {
        self.cgb_hw = hw;
    }

    /// Read VBK (0xFF4F). Unused bits read 1; the register does not exist on DMG.
    pub fn read_vbk(&self) -> u8 {
        if self.cgb_hw {
            0xFE | self.vbk
        } else {
            0xFF
        }
    }

    pub fn write_vbk(&mut self, value: u8) {
        if self.cgb_hw {
            self.vbk = value & 1;
        }
    }

    /// Read BCPS/BCPD/OCPS/OCPD. Palette data is unreadable (0xFF) during mode 3 (Pan Docs
    /// "Accessing palette RAM"). Does not exist on DMG (0xFF).
    pub fn read_palette_reg(&self, addr: u16) -> u8 {
        if !self.cgb_hw {
            return 0xFF;
        }
        match addr {
            REG_BCPS => self.bcps | 0x40,
            REG_OCPS => self.ocps | 0x40,
            REG_BCPD if self.mode() != MODE_DRAWING => self.bg_pal[usize::from(self.bcps & 0x3F)],
            REG_OCPD if self.mode() != MODE_DRAWING => self.obj_pal[usize::from(self.ocps & 0x3F)],
            _ => 0xFF,
        }
    }

    /// Write BCPS/BCPD/OCPS/OCPD. A data write during mode 3 is lost, but the index still
    /// auto-increments (Pan Docs).
    pub fn write_palette_reg(&mut self, addr: u16, value: u8) {
        if !self.cgb_hw {
            return;
        }
        let blocked = self.mode() == MODE_DRAWING;
        match addr {
            REG_BCPS => self.bcps = value & 0xBF,
            REG_OCPS => self.ocps = value & 0xBF,
            REG_BCPD => {
                if !blocked {
                    self.bg_pal[usize::from(self.bcps & 0x3F)] = value;
                }
                self.bcps = Self::auto_increment(self.bcps);
            }
            REG_OCPD => {
                if !blocked {
                    self.obj_pal[usize::from(self.ocps & 0x3F)] = value;
                }
                self.ocps = Self::auto_increment(self.ocps);
            }
            _ => {}
        }
    }

    /// Index register after a data write: bit 7 set means the 6-bit index advances (wrapping).
    fn auto_increment(spec: u8) -> u8 {
        if spec & 0x80 != 0 {
            0x80 | (spec.wrapping_add(1) & 0x3F)
        } else {
            spec
        }
    }

    pub fn lcd_on(&self) -> bool {
        self.lcdc & LCDC_ENABLE != 0
    }

    /// Current STAT mode (0..=3). 0 while the LCD is off.
    pub fn mode(&self) -> u8 {
        if !self.lcd_on() {
            MODE_HBLANK
        } else if self.ly >= FIRST_VBLANK_LINE {
            MODE_VBLANK
        } else if self.first_line_after_on && self.dot < OAM_SCAN_DOTS {
            MODE_HBLANK
        } else if self.dot < OAM_SCAN_DOTS {
            MODE_OAM_SCAN
        } else if self.dot < OAM_SCAN_DOTS + DRAW_DOTS {
            MODE_DRAWING
        } else {
            MODE_HBLANK
        }
    }

    /// The level of the internal STAT interrupt line: the OR of every enabled source
    /// (Pan Docs "STAT" bits 3-6: HBlank, VBlank, OAM scan, LYC==LY). The CPU interrupt is
    /// requested only on a *rising edge* of this OR ("STAT blocking"): while one source holds the
    /// line high, other sources becoming true do not raise another interrupt.
    fn stat_line_level(&self, enable: u8) -> bool {
        if !self.lcd_on() {
            return false;
        }
        let mode = self.mode();
        (enable & 0x08 != 0 && mode == MODE_HBLANK)
            || (enable & 0x10 != 0 && mode == MODE_VBLANK)
            || (enable & 0x20 != 0 && mode == MODE_OAM_SCAN)
            || (enable & 0x40 != 0 && self.ly == self.lyc)
    }

    /// Re-evaluate the STAT line after anything that can change it (a tick, or a write to
    /// STAT, LYC or LCDC). Returns true on a rising edge.
    fn update_stat_line(&mut self) -> bool {
        let level = self.stat_line_level(self.stat_enable);
        let rising = level && !self.stat_line;
        self.stat_line = level;
        rising
    }

    /// Whether the CPU can reach VRAM: not during mode 3 while the LCD is on
    /// (Pan Docs "Accessing VRAM and OAM"). Blocked reads return 0xFF, blocked writes are lost.
    pub fn vram_accessible(&self) -> bool {
        self.mode() != MODE_DRAWING
    }

    /// Whether the CPU can reach OAM: not during modes 2 and 3 while the LCD is on.
    /// (With the LCD off `mode()` is 0, so both are accessible.) OAM DMA is not subject to this.
    pub fn oam_accessible(&self) -> bool {
        !matches!(self.mode(), MODE_OAM_SCAN | MODE_DRAWING)
    }

    pub fn ly(&self) -> u8 {
        self.ly
    }

    pub fn lcdc(&self) -> u8 {
        self.lcdc
    }

    pub fn bgp(&self) -> u8 {
        self.bgp
    }

    /// Advance by one M-cycle (4 dots).
    pub fn tick(&mut self) -> PpuIrq {
        let mut irq = PpuIrq::default();
        if !self.lcd_on() {
            // With the LCD off the screen is blank, but frontends still need a frame boundary
            // every 70224 dots to keep pacing.
            self.off_dots += 4;
            if self.off_dots >= DOTS_PER_FRAME {
                self.off_dots -= DOTS_PER_FRAME;
                self.frame_ready = true;
            }
            // LCD off: every source is low.
            self.stat_line = false;
            return irq;
        }
        self.dot += 4;
        if self.dot == OAM_SCAN_DOTS + DRAW_DOTS && self.ly < FIRST_VBLANK_LINE {
            self.render_line();
            irq.hblank = true;
        }
        if self.dot >= DOTS_PER_LINE {
            self.dot -= DOTS_PER_LINE;
            self.first_line_after_on = false;
            self.ly += 1;
            if self.ly == LINES_PER_FRAME {
                self.ly = 0;
            }
            if self.ly == FIRST_VBLANK_LINE {
                irq.vblank = true;
                self.frame_ready = true;
                self.framebuffer.copy_from_slice(&self.back);
                self.window_line = 0;
            }
        }
        irq.stat = self.update_stat_line();
        irq
    }

    /// True once per finished frame; reading clears it.
    pub fn take_frame_ready(&mut self) -> bool {
        std::mem::take(&mut self.frame_ready)
    }

    /// The last completed frame, 160*144 pixels as 0x00RRGGBB.
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    // --- Memory access. Access blocking by mode is added on top of these (see `Bus::read`). ---

    /// VRAM bank 0.
    pub fn vram(&self) -> &[u8] {
        &self.vram
    }

    /// VRAM bank 1 (all zero unless the game uses CGB mode).
    pub fn vram_bank1(&self) -> &[u8] {
        &self.vram1
    }

    pub fn oam(&self) -> &[u8] {
        &self.oam
    }

    /// CPU/DMA view of VRAM: the bank chosen by VBK (always bank 0 outside CGB mode).
    pub fn read_vram(&self, offset: usize) -> u8 {
        if self.vbk == 1 {
            self.vram1[offset]
        } else {
            self.vram[offset]
        }
    }

    pub fn write_vram(&mut self, offset: usize, value: u8) {
        if self.vbk == 1 {
            self.vram1[offset] = value;
        } else {
            self.vram[offset] = value;
        }
    }

    pub fn read_oam(&self, offset: usize) -> u8 {
        self.oam[offset]
    }

    pub fn write_oam(&mut self, offset: usize, value: u8) {
        self.oam[offset] = value;
    }

    // --- Registers ---

    pub fn read_reg(&self, addr: u16) -> u8 {
        match addr {
            REG_LCDC => self.lcdc,
            // Bit 7 is unused and reads 1. Bits 0-1 mode, bit 2 LYC==LY.
            REG_STAT => {
                0x80 | self.stat_enable
                    | (u8::from(self.lcd_on() && self.ly == self.lyc) << 2)
                    | self.mode()
            }
            REG_SCY => self.scy,
            REG_SCX => self.scx,
            REG_LY => self.ly,
            REG_LYC => self.lyc,
            REG_BGP => self.bgp,
            REG_OBP0 => self.obp0,
            REG_OBP1 => self.obp1,
            REG_WY => self.wy,
            REG_WX => self.wx,
            _ => 0xFF,
        }
    }

    /// Write an LCD register. Returns true if the write raised a STAT interrupt request.
    pub fn write_reg(&mut self, addr: u16, value: u8) -> bool {
        let mut spurious = false;
        match addr {
            REG_LCDC => self.write_lcdc(value),
            REG_STAT => {
                // DMG STAT-write quirk (Pan Docs "STAT" / "Spurious STAT interrupts"): during
                // the write, all four sources behave as enabled for a moment. If the line was low
                // and any source condition is true (HBlank, VBlank or LYC==LY; OAM scan is not
                // part of the quirk), an interrupt is requested even when the new value enables
                // nothing. Some games depend on this (Pan Docs); we implement it on DMG.
                let all_on = self.stat_line_level(0x78 & !0x20);
                spurious = all_on && !self.stat_line;
                self.stat_enable = value & 0x78;
            }
            REG_SCY => self.scy = value,
            REG_SCX => self.scx = value,
            // LY is read-only.
            REG_LY => {}
            REG_LYC => self.lyc = value,
            REG_BGP => self.bgp = value,
            REG_OBP0 => self.obp0 = value,
            REG_OBP1 => self.obp1 = value,
            REG_WY => self.wy = value,
            REG_WX => self.wx = value,
            _ => {}
        }
        let rising = self.update_stat_line();
        spurious || rising
    }

    fn write_lcdc(&mut self, value: u8) {
        let was_on = self.lcd_on();
        self.lcdc = value;
        let now_on = self.lcd_on();
        if was_on && !now_on {
            // Turning the LCD off: LY reads 0, mode 0, screen goes blank.
            self.ly = 0;
            self.dot = 0;
            self.off_dots = 0;
            self.framebuffer.fill(SHADES[0]);
            self.back.fill(SHADES[0]);
            self.window_line = 0;
        } else if !was_on && now_on {
            // Turning it on restarts the frame at line 0 (first line is special, see field).
            self.ly = 0;
            self.dot = 0;
            self.first_line_after_on = true;
        }
    }
}

impl Ppu {
    /// Save-state support (see `crate::state`). Includes both frame buffers: a state saved
    /// mid-frame must continue the half-drawn frame and show the same picture when loaded.
    /// `cgb` / `cgb_hw` are fixed by the console model and checked by the caller.
    pub fn save_state(&self, w: &mut crate::state::StateWriter) {
        w.bytes(&self.vram);
        w.bytes(&self.vram1);
        w.u8(self.vbk);
        w.bytes(&self.bg_pal);
        w.bytes(&self.obj_pal);
        w.u8(self.bcps);
        w.u8(self.ocps);
        w.bytes(&self.oam);
        for v in [
            self.lcdc,
            self.stat_enable,
            self.scy,
            self.scx,
            self.ly,
            self.lyc,
            self.bgp,
            self.obp0,
            self.obp1,
            self.wy,
            self.wx,
        ] {
            w.u8(v);
        }
        w.u32(self.dot);
        w.u32(self.off_dots);
        w.bool(self.first_line_after_on);
        w.bool(self.stat_line);
        w.bool(self.frame_ready);
        w.u8(self.window_line);
        w.u32s(&self.back);
        w.u32s(&self.framebuffer);
    }

    pub fn load_state(
        &mut self,
        r: &mut crate::state::StateReader,
    ) -> Result<(), crate::state::StateError> {
        use crate::state::StateError;
        r.bytes(&mut self.vram)?;
        r.bytes(&mut self.vram1)?;
        self.vbk = r.u8()? & 1;
        r.bytes(&mut self.bg_pal)?;
        r.bytes(&mut self.obj_pal)?;
        self.bcps = r.u8()?;
        self.ocps = r.u8()?;
        r.bytes(&mut self.oam)?;
        self.lcdc = r.u8()?;
        self.stat_enable = r.u8()?;
        self.scy = r.u8()?;
        self.scx = r.u8()?;
        self.ly = r.u8()?;
        self.lyc = r.u8()?;
        self.bgp = r.u8()?;
        self.obp0 = r.u8()?;
        self.obp1 = r.u8()?;
        self.wy = r.u8()?;
        self.wx = r.u8()?;
        self.dot = r.u32()?;
        self.off_dots = r.u32()?;
        self.first_line_after_on = r.bool()?;
        self.stat_line = r.bool()?;
        self.frame_ready = r.bool()?;
        self.window_line = r.u8()?;
        r.u32s(&mut self.back)?;
        r.u32s(&mut self.framebuffer)?;
        // The line/dot counters index the timing tables; reject values the PPU never has.
        if self.ly >= LINES_PER_FRAME
            || self.dot >= DOTS_PER_LINE
            || self.off_dots >= DOTS_PER_FRAME
        {
            return Err(StateError::Corrupt("PPU position"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticks(ppu: &mut Ppu, n: u32) -> PpuIrq {
        let mut seen = PpuIrq::default();
        for _ in 0..n {
            let irq = ppu.tick();
            seen.vblank |= irq.vblank;
            seen.stat |= irq.stat;
        }
        seen
    }

    /// A PPU restarted at line 0, dot 0 with the LCD on and a normal first line.
    fn fresh() -> Ppu {
        let mut ppu = Ppu::new();
        ppu.write_reg(REG_LCDC, 0x00);
        ppu.write_reg(REG_LCDC, 0x91);
        ppu.first_line_after_on = false;
        ppu
    }

    #[test]
    fn line_is_456_dots_and_frame_is_70224() {
        let mut ppu = fresh();
        ticks(&mut ppu, 456 / 4);
        assert_eq!(ppu.ly(), 1);
        let mut ppu = fresh();
        ticks(&mut ppu, 70224 / 4 - 1);
        assert_eq!(ppu.ly(), 153);
        ticks(&mut ppu, 1);
        assert_eq!(ppu.ly(), 0);
    }

    #[test]
    fn mode_sequence_on_a_visible_line() {
        let mut ppu = fresh();
        let mut modes = Vec::new();
        for _ in 0..(456 / 4) {
            modes.push(ppu.mode());
            ppu.tick();
        }
        assert_eq!(modes[0], MODE_OAM_SCAN);
        assert_eq!(modes[19], MODE_OAM_SCAN); // dots 76..79
        assert_eq!(modes[20], MODE_DRAWING); // dot 80
        assert_eq!(modes[62], MODE_DRAWING); // dot 248
        assert_eq!(modes[63], MODE_HBLANK); // dot 252
        assert_eq!(modes[113], MODE_HBLANK);
    }

    #[test]
    fn vblank_irq_and_mode_at_line_144() {
        let mut ppu = fresh();
        let irq = ticks(&mut ppu, 144 * 456 / 4 - 1);
        assert!(!irq.vblank);
        assert_eq!(ppu.mode(), MODE_HBLANK);
        let irq = ticks(&mut ppu, 1);
        assert!(irq.vblank);
        assert_eq!(ppu.ly(), 144);
        assert_eq!(ppu.mode(), MODE_VBLANK);
        assert!(ppu.take_frame_ready());
        assert!(!ppu.take_frame_ready());
    }

    #[test]
    fn lcd_off_resets_ly_and_still_paces_frames() {
        let mut ppu = fresh();
        ticks(&mut ppu, 1000);
        ppu.write_reg(REG_LCDC, 0x11);
        assert_eq!(ppu.ly(), 0);
        assert_eq!(ppu.mode(), MODE_HBLANK);
        assert_eq!(ppu.read_reg(REG_STAT) & 0x03, 0);
        ppu.take_frame_ready();
        ticks(&mut ppu, DOTS_PER_FRAME / 4 - 1);
        assert!(!ppu.take_frame_ready());
        ticks(&mut ppu, 1);
        assert!(ppu.take_frame_ready());
        assert_eq!(ppu.ly(), 0, "LY stays 0 while off");
    }

    #[test]
    fn lcd_on_starts_a_frame_with_a_mode_0_first_line() {
        let mut ppu = Ppu::new();
        ppu.write_reg(REG_LCDC, 0x00);
        ppu.write_reg(REG_LCDC, 0x91);
        assert_eq!(ppu.ly(), 0);
        assert_eq!(ppu.mode(), MODE_HBLANK, "no OAM scan on the first line");
        ticks(&mut ppu, 456 / 4);
        assert_eq!(ppu.ly(), 1);
        assert_eq!(ppu.mode(), MODE_OAM_SCAN);
    }

    #[test]
    fn ly_is_read_only_and_stat_masks_bits() {
        let mut ppu = fresh();
        ppu.write_reg(REG_LY, 55);
        assert_eq!(ppu.ly(), 0);
        ppu.write_reg(REG_STAT, 0xFF);
        assert_eq!(ppu.read_reg(REG_STAT) & 0xF8, 0xF8);
    }

    #[test]
    fn lyc_interrupt_fires_once_when_ly_reaches_lyc() {
        let mut ppu = fresh();
        ppu.write_reg(REG_LYC, 2);
        ppu.write_reg(REG_STAT, 0x40);
        assert!(!ticks(&mut ppu, 2 * 456 / 4 - 1).stat);
        assert!(ticks(&mut ppu, 1).stat, "LY 1 -> 2");
        assert_eq!(ppu.read_reg(REG_STAT) & 0x04, 0x04, "coincidence flag");
        assert!(
            !ticks(&mut ppu, 456 / 4).stat,
            "LY 3: line low again, no new edge"
        );
        assert_eq!(ppu.read_reg(REG_STAT) & 0x04, 0);
    }

    #[test]
    fn stat_blocking_hblank_into_vblank_is_one_continuous_high_line() {
        // HBlank and VBlank sources both enabled: the line goes high at the end of line 143's
        // mode 3 and stays high through the HBlank -> VBlank change at line 144, so the VBlank
        // source adds no second edge.
        let mut ppu = fresh();
        ppu.write_reg(REG_STAT, 0x18);
        let mut edges = 0;
        for _ in 0..(144 * 456 + 456) / 4 {
            let irq = ppu.tick();
            edges += u32::from(irq.stat);
            if ppu.ly() == 144 && ppu.mode() == MODE_VBLANK && irq.vblank {
                assert!(!irq.stat, "no new edge entering VBlank");
            }
        }
        // 144 HBlanks (lines 0..=143) plus nothing for VBlank lines.
        assert_eq!(edges, 144);
    }

    #[test]
    fn stat_write_during_hblank_raises_spurious_interrupt_on_dmg() {
        let mut ppu = fresh();
        ticks(&mut ppu, 70); // dot 280: HBlank
        assert_eq!(ppu.mode(), MODE_HBLANK);
        assert!(ppu.write_reg(REG_STAT, 0x00), "spurious request");
        // The enabled sources are all off afterwards, so the line never really rose: every such
        // write during HBlank requests again (this matches the hardware bug).
        assert!(ppu.write_reg(REG_STAT, 0x00));
        // During OAM scan (mode 2) the quirk does not apply.
        let mut ppu = fresh();
        ppu.write_reg(REG_LYC, 5); // LY (0) != LYC, so the LYC part of the quirk is quiet too
        assert_eq!(ppu.mode(), MODE_OAM_SCAN);
        assert!(!ppu.write_reg(REG_STAT, 0x00));
    }

    #[test]
    fn no_stat_interrupt_while_lcd_is_off() {
        let mut ppu = fresh();
        ppu.write_reg(REG_STAT, 0x78);
        ppu.write_reg(REG_LCDC, 0x11);
        assert!(!ticks(&mut ppu, 2000).stat);
    }
}
