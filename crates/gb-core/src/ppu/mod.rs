//! The DMG PPU (pixel processing unit): LCD registers, VRAM, OAM and the line/mode state machine
//! (Pan Docs "Rendering", "LCD Status Register", "LCD Position and Scrolling").
//!
//! The PPU is advanced by `Bus::tick`, 4 dots (T-cycles) per M-cycle. A frame is 154 lines of
//! 456 dots = 70224 dots (~59.7 Hz). Lines 0..=143 are visible, 144..=153 are VBlank.

pub mod debug;

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
}

pub struct Ppu {
    vram: [u8; VRAM_SIZE],
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
    frame_ready: bool,
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
            frame_ready: false,
            framebuffer: vec![SHADES[0]; SCREEN_WIDTH * SCREEN_HEIGHT],
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
            return irq;
        }
        self.dot += 4;
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
            }
        }
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

    pub fn vram(&self) -> &[u8] {
        &self.vram
    }

    pub fn oam(&self) -> &[u8] {
        &self.oam
    }

    pub fn read_vram(&self, offset: usize) -> u8 {
        self.vram[offset]
    }

    pub fn write_vram(&mut self, offset: usize, value: u8) {
        self.vram[offset] = value;
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

    pub fn write_reg(&mut self, addr: u16, value: u8) {
        match addr {
            REG_LCDC => self.write_lcdc(value),
            REG_STAT => self.stat_enable = value & 0x78,
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
        } else if !was_on && now_on {
            // Turning it on restarts the frame at line 0 (first line is special, see field).
            self.ly = 0;
            self.dot = 0;
            self.first_line_after_on = true;
        }
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
}
