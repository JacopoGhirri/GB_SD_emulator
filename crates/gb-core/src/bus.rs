//! The memory bus. Owns every component (CLAUDE.md §4.3) and is the single place where
//! emulated time advances (CLAUDE.md §4.2).
//!
//! Every CPU memory access calls [`Bus::tick`], so an instruction's cycle count *emerges* from
//! the accesses it makes. Internal delay cycles call [`Bus::idle`].

use crate::cartridge::Cartridge;
use crate::dma::Dma;
use crate::interrupts::{Interrupt, Interrupts};
use crate::io::{self, IO_LEN, IO_START};
pub use crate::io::{REG_DMA, REG_LY, REG_P1};
use crate::joypad::{Buttons, Joypad};
use crate::ppu::{self, Ppu};
use crate::serial::Serial;
pub use crate::serial::{REG_SB, REG_SC};
use crate::timer::{Timer, REG_DIV, REG_TAC};

pub use crate::interrupts::{INTERRUPT_MASK, REG_IE, REG_IF};

/// T-cycles per M-cycle. The CPU does one bus access per M-cycle.
pub const T_CYCLES_PER_M_CYCLE: u32 = 4;

// Memory map (Pan Docs "Memory Map").
const ROM_END: u16 = 0x7FFF;
const VRAM_START: u16 = 0x8000;
const VRAM_END: u16 = 0x9FFF;
const EXT_RAM_START: u16 = 0xA000;
const EXT_RAM_END: u16 = 0xBFFF;
const WRAM_START: u16 = 0xC000;
const WRAM_END: u16 = 0xDFFF;
const ECHO_START: u16 = 0xE000;
const ECHO_END: u16 = 0xFDFF;
const OAM_START: u16 = 0xFE00;
const OAM_END: u16 = 0xFE9F;
const UNUSABLE_START: u16 = 0xFEA0;
const UNUSABLE_END: u16 = 0xFEFF;
const IO_END: u16 = 0xFF7F;
const HRAM_START: u16 = 0xFF80;
const HRAM_END: u16 = 0xFFFE;

// I/O registers.
const DOCTOR_LY: u8 = 0x90;

pub struct Bus {
    cartridge: Cartridge,
    ppu: Ppu,
    wram: [u8; 0x2000],
    hram: [u8; 0x7F],
    /// Plain storage for I/O registers that have no behaviour yet.
    io: [u8; IO_LEN],
    interrupts: Interrupts,
    /// Total T-cycles since power-on.
    cycles: u64,
    timer: Timer,
    dma: Dma,
    joypad: Joypad,
    serial: Serial,
    /// Gameboy Doctor mode, for tracing only. Doctor's reference logs were made by an emulator
    /// where LY (0xFF44) always reads 0x90 (it had no PPU) and the unused upper bits of IF
    /// read 0 instead of 1 (real hardware reads 1; normal mode does the same as hardware).
    doctor_mode: bool,
}

impl Bus {
    pub fn new(cartridge: Cartridge) -> Bus {
        Bus {
            cartridge,
            ppu: Ppu::new(),
            wram: [0; 0x2000],
            hram: [0; 0x7F],
            io: std::array::from_fn(|i| io::power_up_value(IO_START + i as u16)),
            interrupts: Interrupts::new(),
            cycles: 0,
            timer: Timer::new(),
            dma: Dma::new(),
            joypad: Joypad::new(),
            serial: Serial::new(),
            doctor_mode: false,
        }
    }

    pub fn cartridge(&self) -> &Cartridge {
        &self.cartridge
    }

    pub fn cartridge_mut(&mut self) -> &mut Cartridge {
        &mut self.cartridge
    }

    /// Total T-cycles elapsed since power-on.
    pub fn cycles(&self) -> u64 {
        self.cycles
    }

    /// Advance the whole system by one M-cycle (4 T-cycles).
    pub fn tick(&mut self) {
        self.cycles += u64::from(T_CYCLES_PER_M_CYCLE);
        if self.timer.tick() {
            self.request_interrupt(Interrupt::Timer);
        }
        self.cartridge.tick();
        if self.serial.tick() {
            self.request_interrupt(Interrupt::Serial);
        }
        let irq = self.ppu.tick();
        if irq.vblank {
            self.request_interrupt(Interrupt::VBlank);
        }
        if irq.stat {
            self.request_interrupt(Interrupt::LcdStat);
        }
        if let Some((src, dst)) = self.dma.tick() {
            // The DMA reads the source directly, bypassing the CPU bus restriction.
            let value = self.peek(src);
            self.ppu.write_oam(dst as usize, value);
        }
    }

    /// An M-cycle in which the CPU does no memory access (internal delay).
    pub fn idle(&mut self) {
        self.tick();
    }

    /// CPU read: costs one M-cycle. The access sees the state *before* this M-cycle's tick.
    ///
    /// Ordering decided by test ROMs, not by reasoning (2026-11-14): I ran Mooneye
    /// `acceptance/timer/*` plus Blargg `instr_timing`/`mem_timing` with all four combinations of
    /// "tick before / after the access" for reads and writes:
    ///   tick-before for both (phase-1 design): timer 12/13 (rapid_toggle fails), Blargg pass.
    ///   access-before for reads only, or for writes only: timer 5-6 fail and Blargg fail.
    ///   access-before-tick for both (this code): timer 13/13, Blargg pass.
    /// The two timing suites constrain reads and writes together, so change them together.
    pub fn read(&mut self, addr: u16) -> u8 {
        let value = if self.cpu_blocked_by_dma(addr) {
            0xFF
        } else {
            self.peek(addr)
        };
        self.tick();
        value
    }

    /// CPU write: costs one M-cycle. The write lands before this M-cycle's tick (see `read`).
    pub fn write(&mut self, addr: u16, value: u8) {
        if !self.cpu_blocked_by_dma(addr) {
            self.poke(addr, value);
        }
        self.tick();
    }

    /// While OAM DMA runs the CPU can only reach HRAM and the I/O registers (including IE);
    /// everything else reads 0xFF and ignores writes (Pan Docs "OAM DMA Transfer").
    fn cpu_blocked_by_dma(&self, addr: u16) -> bool {
        self.dma.is_active() && addr < IO_START
    }

    /// Read without advancing time. For tracing and debugging only.
    pub fn peek(&self, addr: u16) -> u8 {
        match addr {
            0..=ROM_END => self.cartridge.read_rom(addr),
            VRAM_START..=VRAM_END => self.ppu.read_vram((addr - VRAM_START) as usize),
            EXT_RAM_START..=EXT_RAM_END => self.cartridge.read_ram(addr),
            WRAM_START..=WRAM_END => self.wram[(addr - WRAM_START) as usize],
            // Echo RAM mirrors WRAM (0xE000 maps to 0xC000).
            ECHO_START..=ECHO_END => self.wram[(addr - ECHO_START) as usize],
            OAM_START..=OAM_END => self.ppu.read_oam((addr - OAM_START) as usize),
            UNUSABLE_START..=UNUSABLE_END => 0xFF,
            IO_START..=IO_END => self.read_io(addr),
            HRAM_START..=HRAM_END => self.hram[(addr - HRAM_START) as usize],
            REG_IE => self.interrupts.read_enable(),
        }
    }

    fn poke(&mut self, addr: u16, value: u8) {
        match addr {
            0..=ROM_END => self.cartridge.write_rom(addr, value),
            VRAM_START..=VRAM_END => self.ppu.write_vram((addr - VRAM_START) as usize, value),
            EXT_RAM_START..=EXT_RAM_END => self.cartridge.write_ram(addr, value),
            WRAM_START..=WRAM_END => self.wram[(addr - WRAM_START) as usize] = value,
            ECHO_START..=ECHO_END => self.wram[(addr - ECHO_START) as usize] = value,
            OAM_START..=OAM_END => self.ppu.write_oam((addr - OAM_START) as usize, value),
            UNUSABLE_START..=UNUSABLE_END => {}
            IO_START..=IO_END => self.write_io(addr, value),
            HRAM_START..=HRAM_END => self.hram[(addr - HRAM_START) as usize] = value,
            REG_IE => self.interrupts.write_enable(value),
        }
    }

    fn read_io(&self, addr: u16) -> u8 {
        match addr {
            // The upper three bits of IF are unused and read as 1.
            // Doctor's reference emulator does not set the unused IF bits; real hardware does.
            REG_IF if self.doctor_mode => self.interrupts.flag_bits(),
            REG_IF => self.interrupts.read_flag(),
            REG_P1 => self.joypad.read(),
            REG_SB | REG_SC => self.serial.read(addr),
            REG_DIV..=REG_TAC => self.timer.read(addr),
            REG_LY if self.doctor_mode => DOCTOR_LY,
            REG_DMA => self.io[(addr - IO_START) as usize] | io::read_mask(addr),
            ppu::REG_LCDC..=ppu::REG_WX => self.ppu.read_reg(addr),
            _ => self.io[(addr - IO_START) as usize] | io::read_mask(addr),
        }
    }

    fn write_io(&mut self, addr: u16, value: u8) {
        match addr {
            REG_IF => self.interrupts.write_flag(value),
            REG_DIV..=REG_TAC => self.timer.write(addr, value),
            REG_SB | REG_SC => self.serial.write(addr, value),
            REG_P1 => {
                if self.joypad.write(value) {
                    self.request_interrupt(Interrupt::Joypad);
                }
            }
            REG_DMA => {
                self.io[(addr - IO_START) as usize] = value;
                self.dma.start(value);
            }
            ppu::REG_LCDC..=ppu::REG_WX => {
                if self.ppu.write_reg(addr, value) {
                    self.request_interrupt(Interrupt::LcdStat);
                }
            }
            _ => self.io[(addr - IO_START) as usize] = value,
        }
    }

    /// See the `doctor_mode` field.
    pub fn set_doctor_mode(&mut self, on: bool) {
        self.doctor_mode = on;
    }

    /// Update the held buttons; raises the joypad interrupt on a high-to-low transition.
    pub fn set_buttons(&mut self, buttons: Buttons) {
        if self.joypad.set_buttons(buttons) {
            self.request_interrupt(Interrupt::Joypad);
        }
    }

    pub fn ppu(&self) -> &Ppu {
        &self.ppu
    }

    pub fn ppu_mut(&mut self) -> &mut Ppu {
        &mut self.ppu
    }

    /// Bytes written to the serial port since the last call.
    pub fn take_serial(&mut self) -> Vec<u8> {
        self.serial.take_output()
    }

    /// Interrupts that are both enabled in IE and requested in IF.
    pub fn pending_interrupts(&self) -> u8 {
        self.interrupts.pending()
    }

    /// Clear one requested interrupt (bit index 0..=4) when the CPU starts servicing it.
    pub fn clear_interrupt(&mut self, bit: u8) {
        self.interrupts.clear(bit);
    }

    /// Raise an interrupt request. Used by timer, PPU, serial, joypad.
    pub fn request_interrupt(&mut self, which: Interrupt) {
        self.interrupts.request(which);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 32 KiB ROM-only cartridge with a valid-enough header.
    fn test_bus() -> Bus {
        let rom = vec![0u8; 0x8000];
        Bus::new(Cartridge::from_bytes(rom).unwrap())
    }

    #[test]
    fn each_access_costs_one_m_cycle() {
        let mut bus = test_bus();
        bus.read(0x0000);
        assert_eq!(bus.cycles(), 4);
        bus.write(0xC000, 1);
        assert_eq!(bus.cycles(), 8);
        bus.idle();
        assert_eq!(bus.cycles(), 12);
        bus.peek(0x0000);
        assert_eq!(bus.cycles(), 12, "peek must not advance time");
    }

    #[test]
    fn echo_ram_mirrors_wram() {
        let mut bus = test_bus();
        bus.write(0xC123, 0x5A);
        assert_eq!(bus.peek(0xE123), 0x5A);
        bus.write(0xE456, 0xA5);
        assert_eq!(bus.peek(0xC456), 0xA5);
    }

    #[test]
    fn interrupt_registers() {
        let mut bus = test_bus();
        bus.write(REG_IF, 0xFF);
        assert_eq!(bus.peek(REG_IF), 0xFF);
        bus.write(REG_IF, 0x00);
        assert_eq!(bus.peek(REG_IF), 0xE0, "unused IF bits read as 1");
        bus.write(REG_IE, 0x04);
        bus.request_interrupt(Interrupt::Timer);
        assert_eq!(bus.pending_interrupts(), 0x04);
        bus.clear_interrupt(2);
        assert_eq!(bus.pending_interrupts(), 0);
    }

    #[test]
    fn serial_write_of_0x81_captures_sb() {
        let mut bus = test_bus();
        bus.write(REG_SB, b'H');
        bus.write(REG_SC, 0x81);
        bus.write(REG_SB, b'i');
        bus.write(REG_SC, 0x80); // not a start-transfer value: ignored
        bus.write(REG_SC, 0x81);
        assert_eq!(bus.take_serial(), b"Hi");
        assert!(bus.take_serial().is_empty(), "buffer is drained");
    }

    #[test]
    fn timer_overflow_raises_interrupt_flag() {
        let mut bus = test_bus();
        bus.write(REG_DIV, 0); // known counter phase
        bus.write(crate::timer::REG_TIMA, 0xFF);
        bus.write(REG_TAC, 0b101); // enabled, TIMA ticks every 4 M-cycles
        let mut m_cycles = 0;
        while bus.peek(REG_IF) & 0x04 == 0 {
            bus.idle();
            m_cycles += 1;
            assert!(m_cycles < 20, "timer interrupt never requested");
        }
        // Exact cycle counts are pinned by the timer unit tests and Mooneye; here we only check
        // that the bus forwards the timer's interrupt request into IF.
        assert!(m_cycles <= 8, "took {m_cycles} M-cycles");
    }
}
