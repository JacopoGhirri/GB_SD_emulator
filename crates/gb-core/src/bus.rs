//! The memory bus. Owns every component (CLAUDE.md §4.3) and is the single place where
//! emulated time advances (CLAUDE.md §4.2).
//!
//! Every CPU memory access calls [`Bus::tick`], so an instruction's cycle count *emerges* from
//! the accesses it makes. Internal delay cycles call [`Bus::idle`].

use crate::cartridge::Cartridge;

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
const IO_START: u16 = 0xFF00;
const IO_END: u16 = 0xFF7F;
const HRAM_START: u16 = 0xFF80;
const HRAM_END: u16 = 0xFFFE;

// I/O registers.
pub const REG_IF: u16 = 0xFF0F;
pub const REG_IE: u16 = 0xFFFF;

/// The five interrupt sources occupy bits 0..=4 of IE and IF (Pan Docs "Interrupts").
pub const INTERRUPT_MASK: u8 = 0x1F;

pub struct Bus {
    cartridge: Cartridge,
    vram: [u8; 0x2000],
    wram: [u8; 0x2000],
    oam: [u8; 0xA0],
    hram: [u8; 0x7F],
    /// Plain storage for I/O registers that have no behaviour yet.
    io: [u8; 0x80],
    interrupt_enable: u8,
    interrupt_flag: u8,
    /// Total T-cycles since power-on.
    cycles: u64,
}

impl Bus {
    pub fn new(cartridge: Cartridge) -> Bus {
        Bus {
            cartridge,
            vram: [0; 0x2000],
            wram: [0; 0x2000],
            oam: [0; 0xA0],
            hram: [0; 0x7F],
            io: [0; 0x80],
            interrupt_enable: 0,
            // Post-boot IF on DMG has VBlank pending (Pan Docs "Power-Up Sequence").
            interrupt_flag: 0x01,
            cycles: 0,
        }
    }

    pub fn cartridge(&self) -> &Cartridge {
        &self.cartridge
    }

    /// Total T-cycles elapsed since power-on.
    pub fn cycles(&self) -> u64 {
        self.cycles
    }

    /// Advance the whole system by one M-cycle (4 T-cycles).
    pub fn tick(&mut self) {
        self.cycles += u64::from(T_CYCLES_PER_M_CYCLE);
    }

    /// An M-cycle in which the CPU does no memory access (internal delay).
    pub fn idle(&mut self) {
        self.tick();
    }

    /// CPU read: costs one M-cycle.
    pub fn read(&mut self, addr: u16) -> u8 {
        self.tick();
        self.peek(addr)
    }

    /// CPU write: costs one M-cycle.
    pub fn write(&mut self, addr: u16, value: u8) {
        self.tick();
        self.poke(addr, value);
    }

    /// Read without advancing time. For tracing and debugging only.
    pub fn peek(&self, addr: u16) -> u8 {
        match addr {
            0..=ROM_END => self.cartridge.read_rom(addr),
            VRAM_START..=VRAM_END => self.vram[(addr - VRAM_START) as usize],
            EXT_RAM_START..=EXT_RAM_END => self.cartridge.read_ram(addr),
            WRAM_START..=WRAM_END => self.wram[(addr - WRAM_START) as usize],
            // Echo RAM mirrors WRAM (0xE000 maps to 0xC000).
            ECHO_START..=ECHO_END => self.wram[(addr - ECHO_START) as usize],
            OAM_START..=OAM_END => self.oam[(addr - OAM_START) as usize],
            UNUSABLE_START..=UNUSABLE_END => 0xFF,
            IO_START..=IO_END => self.read_io(addr),
            HRAM_START..=HRAM_END => self.hram[(addr - HRAM_START) as usize],
            REG_IE => self.interrupt_enable,
        }
    }

    fn poke(&mut self, addr: u16, value: u8) {
        match addr {
            0..=ROM_END => self.cartridge.write_rom(addr, value),
            VRAM_START..=VRAM_END => self.vram[(addr - VRAM_START) as usize] = value,
            EXT_RAM_START..=EXT_RAM_END => self.cartridge.write_ram(addr, value),
            WRAM_START..=WRAM_END => self.wram[(addr - WRAM_START) as usize] = value,
            ECHO_START..=ECHO_END => self.wram[(addr - ECHO_START) as usize] = value,
            OAM_START..=OAM_END => self.oam[(addr - OAM_START) as usize] = value,
            UNUSABLE_START..=UNUSABLE_END => {}
            IO_START..=IO_END => self.write_io(addr, value),
            HRAM_START..=HRAM_END => self.hram[(addr - HRAM_START) as usize] = value,
            REG_IE => self.interrupt_enable = value,
        }
    }

    fn read_io(&self, addr: u16) -> u8 {
        match addr {
            // The upper three bits of IF are unused and read as 1.
            REG_IF => self.interrupt_flag | !INTERRUPT_MASK,
            _ => self.io[(addr - IO_START) as usize],
        }
    }

    fn write_io(&mut self, addr: u16, value: u8) {
        match addr {
            REG_IF => self.interrupt_flag = value & INTERRUPT_MASK,
            _ => self.io[(addr - IO_START) as usize] = value,
        }
    }

    /// Interrupts that are both enabled in IE and requested in IF.
    pub fn pending_interrupts(&self) -> u8 {
        self.interrupt_enable & self.interrupt_flag & INTERRUPT_MASK
    }

    /// Clear one requested interrupt (bit index 0..=4) when the CPU starts servicing it.
    pub fn clear_interrupt(&mut self, bit: u8) {
        self.interrupt_flag &= !(1 << bit);
    }

    /// Raise an interrupt request (bit index 0..=4). Used by timer, PPU, serial, joypad.
    pub fn request_interrupt(&mut self, bit: u8) {
        self.interrupt_flag |= 1 << bit;
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
        bus.request_interrupt(2);
        assert_eq!(bus.pending_interrupts(), 0x04);
        bus.clear_interrupt(2);
        assert_eq!(bus.pending_interrupts(), 0);
    }
}
