//! The memory bus. Owns every component (CLAUDE.md §4.3) and is the single place where
//! emulated time advances (CLAUDE.md §4.2).
//!
//! Every CPU memory access calls [`Bus::tick`], so an instruction's cycle count *emerges* from
//! the accesses it makes. Internal delay cycles call [`Bus::idle`].

use crate::apu::Apu;
use crate::cartridge::Cartridge;
use crate::dma::Dma;
use crate::hdma::{self, Hdma};
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
const WRAM_SIZE: usize = 0x8000;
const WRAM_BANK_SIZE: usize = 0x1000;
/// SVBK: WRAM bank select, CGB mode only (Pan Docs "WRAM Bank Select").
const REG_SVBK: u16 = 0xFF70;
/// KEY1: speed switch arm bit (bit 0) and current speed (bit 7), CGB mode only.
const REG_KEY1: u16 = 0xFF4D;
const SPEED_SWITCH_M_CYCLES: u32 = 2050;
// Other CGB-only registers (Pan Docs "Registers" CGB list). They are kept as plain storage with
// the documented unused bits reading 1; nothing here changes emulation yet.
/// RP: infrared port. Bit 0 write data, bits 6-7 read enable. Bit 1 (receive) reads 1 = "no light",
/// as there is no infrared hardware to talk to.
const REG_RP: u16 = 0xFF56;
/// OPRI: bit 0 = 1 selects DMG-style sprite priority. Stored only: the CGB renderer always uses
/// OAM-index priority (what CGB-mode games and cgb-acid2 expect, OPRI = 0).
const REG_OPRI: u16 = 0xFF6C;
/// Undocumented but present on CGB: FF72/FF73 8 bits, FF74 8 bits (CGB mode), FF75 bits 4-6.
const REG_UNDOC_72: u16 = 0xFF72;
const REG_UNDOC_74: u16 = 0xFF74;
const REG_UNDOC_75: u16 = 0xFF75;
/// PCM12/PCM34: live digital audio outputs. Not modelled; read 0.
const REG_PCM12: u16 = 0xFF76;
const REG_PCM34: u16 = 0xFF77;
const HRAM_START: u16 = 0xFF80;
const HRAM_END: u16 = 0xFFFE;

// I/O registers.
const DOCTOR_LY: u8 = 0x90;

pub struct Bus {
    cartridge: Cartridge,
    ppu: Ppu,
    apu: Apu,
    /// 8 banks of 4 KiB. Bank 0 is always at 0xC000; 0xD000 shows bank `svbk` (1..=7).
    /// On DMG only banks 0 and 1 are used, which is the plain 8 KiB.
    wram: [u8; WRAM_SIZE],
    /// SVBK (0xFF70) bits 0..=2 as written; 0 selects bank 1 (see `wram_index`). CGB mode only.
    svbk: u8,
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
    /// True when running in CGB mode: CGB hardware *and* a cartridge that supports CGB.
    /// CGB hardware with a DMG-only cartridge is "DMG compatibility mode" (this stays false).
    cgb_mode: bool,
    /// CGB hardware (whether or not the cartridge uses CGB features). The CGB-only registers
    /// exist, with their post-boot values, even in DMG compatibility mode (Mooneye
    /// `misc/boot_hwio-C` runs a DMG-flagged ROM and reads them).
    cgb_hw: bool,
    /// CGB double-speed mode (KEY1 bit 7). The CPU, timer, serial port and OAM DMA run at twice
    /// the normal rate; the PPU, APU and the cartridge's real-time clock keep their real-time
    /// rate (Pan Docs "Speed Switch"). So in double speed they advance every *second* M-cycle.
    double_speed: bool,
    /// KEY1 bit 0: a speed switch is armed and the next STOP performs it.
    key1_armed: bool,
    /// Toggles every M-cycle in double speed; the PPU/APU/RTC advance when it is true.
    slow_phase: bool,
    /// VRAM DMA (GDMA / HBlank DMA), CGB mode only.
    hdma: Hdma,
    /// M-cycles the CPU is still stalled by a VRAM DMA block; the system keeps running.
    hdma_stall: u32,
}

impl Bus {
    /// A DMG bus. See [`Bus::with_mode`].
    pub fn new(cartridge: Cartridge) -> Bus {
        Bus::with_mode(cartridge, false, false)
    }

    /// `cgb_hardware`: the console is a CGB (it differs from a DMG after boot even when running
    /// a DMG-only cartridge). `cgb_mode` must already combine the hardware model with the
    /// cartridge header (see `GameBoy::new`) and implies `cgb_hardware`.
    pub fn with_mode(cartridge: Cartridge, cgb_hardware: bool, cgb_mode: bool) -> Bus {
        let mut io: [u8; IO_LEN] = std::array::from_fn(|i| io::power_up_value(IO_START + i as u16));
        if cgb_hardware && !cgb_mode {
            // The CGB boot ROM sets OPRI bit 0 (DMG-style sprite priority) for DMG-only carts.
            io[usize::from(REG_OPRI - IO_START)] = 1;
        }
        let mut ppu = Ppu::new();
        ppu.set_cgb(cgb_mode);
        ppu.set_cgb_hardware(cgb_hardware);
        Bus {
            cgb_mode,
            cgb_hw: cgb_hardware,
            double_speed: false,
            key1_armed: false,
            slow_phase: false,
            hdma: Hdma::new(),
            hdma_stall: 0,
            cartridge,
            ppu,
            apu: Apu::new(),
            wram: [0; WRAM_SIZE],
            svbk: 0,
            hram: [0; 0x7F],
            io,
            interrupts: Interrupts::new(),
            cycles: 0,
            timer: if cgb_hardware {
                Timer::new_cgb()
            } else {
                Timer::new()
            },
            dma: Dma::new(),
            joypad: if cgb_hardware {
                Joypad::post_boot_cgb()
            } else {
                Joypad::new()
            },
            serial: Serial::new(),
            doctor_mode: false,
        }
    }

    pub fn cartridge(&self) -> &Cartridge {
        &self.cartridge
    }

    /// True while the CPU runs at double speed (CGB only).
    pub fn double_speed(&self) -> bool {
        self.double_speed
    }

    /// Whether KEY1 bit 0 armed a speed switch that the next STOP will perform.
    pub fn speed_switch_armed(&self) -> bool {
        self.key1_armed
    }

    /// Perform an armed speed switch (called by STOP). Toggles the speed, clears the arm bit and
    /// leaves the CPU stopped for 2050 M-cycles while the other components keep running
    /// (Pan Docs "Speed Switch": the switch takes about 2050 M-cycles).
    pub fn switch_speed(&mut self) {
        self.double_speed = !self.double_speed;
        self.key1_armed = false;
        self.slow_phase = false;
        // The frame sequencer watches a different counter bit in double speed.
        self.apu
            .set_double_speed(self.double_speed, self.timer.counter());
        for _ in 0..SPEED_SWITCH_M_CYCLES {
            self.idle();
        }
    }

    /// True while an OAM DMA transfer owns the bus (the save-state tests use this to save in
    /// the middle of one).
    pub fn oam_dma_active(&self) -> bool {
        self.dma.is_active()
    }

    /// True while a VRAM DMA is in progress: an HBlank transfer with blocks left, or the CPU
    /// still stalled by a block.
    pub fn vram_dma_active(&self) -> bool {
        self.hdma.hblank_active() || self.hdma_stall > 0
    }

    /// Whether CGB features are active (CGB hardware running a CGB-capable cartridge).
    pub fn cgb_mode(&self) -> bool {
        self.cgb_mode
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
        self.tick_one();
        // A VRAM DMA block stalls the CPU: time passes, but the CPU's next access waits.
        while self.hdma_stall > 0 {
            self.hdma_stall -= 1;
            self.tick_one();
        }
    }

    fn tick_one(&mut self) {
        // `cycles` counts CPU T-cycles, so a frame is 140448 of them in double speed.
        self.cycles += u64::from(T_CYCLES_PER_M_CYCLE);
        // The PPU, APU and RTC run on the unchanged real-time clock: every M-cycle normally,
        // every second one in double speed.
        let slow_tick = if self.double_speed {
            self.slow_phase = !self.slow_phase;
            self.slow_phase
        } else {
            true
        };
        if self.timer.tick() {
            self.request_interrupt(Interrupt::Timer);
        }
        if slow_tick {
            self.apu.tick(self.timer.counter());
            self.cartridge.tick();
        }
        if self.serial.tick() {
            self.request_interrupt(Interrupt::Serial);
        }
        if slow_tick {
            let irq = self.ppu.tick();
            if irq.vblank {
                self.request_interrupt(Interrupt::VBlank);
            }
            if irq.stat {
                self.request_interrupt(Interrupt::LcdStat);
            }
            if irq.hblank && self.hdma.hblank_active() {
                self.hdma_copy_block();
            }
        }
        if let Some((src, dst)) = self.dma.tick() {
            // The DMA reads the source directly, bypassing the CPU bus restriction.
            let value = self.peek(src);
            self.ppu.write_oam(dst as usize, value);
        }
    }

    /// Copy one 16-byte VRAM DMA block into the current VRAM bank and stall the CPU for it:
    /// 8 M-cycles per block, 16 in double speed (the DMA engine runs in real time, Pan Docs).
    fn hdma_copy_block(&mut self) {
        let (src, dst) = self.hdma.next_block();
        for i in 0..hdma::BLOCK_BYTES {
            let from = src.wrapping_add(i);
            // Valid sources are ROM and external/work RAM; anything else is not modelled.
            let value = match from {
                0..=ROM_END | EXT_RAM_START..=WRAM_END => self.peek(from),
                _ => 0xFF,
            };
            self.ppu.write_vram(usize::from((dst + i) & 0x1FFF), value);
        }
        self.hdma_stall += if self.double_speed { 16 } else { 8 };
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
        let value = if self.cpu_blocked_by_dma(addr) || self.cpu_blocked_by_ppu(addr) {
            0xFF
        } else {
            self.peek(addr)
        };
        self.tick();
        value
    }

    /// CPU write: costs one M-cycle. The write lands before this M-cycle's tick (see `read`).
    pub fn write(&mut self, addr: u16, value: u8) {
        if !self.cpu_blocked_by_dma(addr) && !self.cpu_blocked_by_ppu(addr) {
            self.poke(addr, value);
        }
        self.tick();
    }

    /// While OAM DMA runs the CPU can only reach HRAM and the I/O registers (including IE);
    /// everything else reads 0xFF and ignores writes (Pan Docs "OAM DMA Transfer").
    fn cpu_blocked_by_dma(&self, addr: u16) -> bool {
        self.dma.is_active() && addr < IO_START
    }

    /// VRAM is locked to the CPU in mode 3, OAM in modes 2 and 3 (only while the LCD is on).
    fn cpu_blocked_by_ppu(&self, addr: u16) -> bool {
        match addr {
            VRAM_START..=VRAM_END => !self.ppu.vram_accessible(),
            OAM_START..=OAM_END => !self.ppu.oam_accessible(),
            _ => false,
        }
    }

    /// Read without advancing time. For tracing and debugging only.
    pub fn peek(&self, addr: u16) -> u8 {
        match addr {
            0..=ROM_END => self.cartridge.read_rom(addr),
            VRAM_START..=VRAM_END => self.ppu.read_vram((addr - VRAM_START) as usize),
            EXT_RAM_START..=EXT_RAM_END => self.cartridge.read_ram(addr),
            WRAM_START..=WRAM_END => self.wram[self.wram_index(addr - WRAM_START)],
            // Echo RAM mirrors WRAM (0xE000 maps to 0xC000), banking included.
            ECHO_START..=ECHO_END => self.wram[self.wram_index(addr - ECHO_START)],
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
            WRAM_START..=WRAM_END => {
                let i = self.wram_index(addr - WRAM_START);
                self.wram[i] = value;
            }
            ECHO_START..=ECHO_END => {
                let i = self.wram_index(addr - ECHO_START);
                self.wram[i] = value;
            }
            OAM_START..=OAM_END => self.ppu.write_oam((addr - OAM_START) as usize, value),
            UNUSABLE_START..=UNUSABLE_END => {}
            IO_START..=IO_END => self.write_io(addr, value),
            HRAM_START..=HRAM_END => self.hram[(addr - HRAM_START) as usize] = value,
            REG_IE => self.interrupts.write_enable(value),
        }
    }

    /// Index into `wram` for an offset 0..0x2000 from 0xC000. The upper 4 KiB shows the bank
    /// selected by SVBK, where 0 means 1 (Pan Docs "WRAM Bank Select").
    fn wram_index(&self, offset: u16) -> usize {
        let offset = usize::from(offset);
        if offset < WRAM_BANK_SIZE {
            offset
        } else {
            let bank = usize::from(self.svbk.max(1));
            bank * WRAM_BANK_SIZE + (offset - WRAM_BANK_SIZE)
        }
    }

    fn io_reg(&self, addr: u16) -> u8 {
        self.io[(addr - IO_START) as usize]
    }

    fn set_io_reg(&mut self, addr: u16, value: u8) {
        self.io[(addr - IO_START) as usize] = value;
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
            io::REG_NR10..=io::REG_NR52 | io::WAVE_RAM_START..=io::WAVE_RAM_END => {
                self.apu.read(addr)
            }
            REG_LY if self.doctor_mode => DOCTOR_LY,
            REG_DMA => self.io[(addr - IO_START) as usize] | io::read_mask(addr),
            hdma::REG_HDMA1..=hdma::REG_HDMA5 if self.cgb_mode => self.hdma.read(addr),
            REG_RP if self.cgb_mode => 0x3E | (self.io_reg(addr) & 0xC1),
            REG_OPRI if self.cgb_hw => 0xFE | (self.io_reg(addr) & 1),
            REG_UNDOC_72..=0xFF73 if self.cgb_hw => self.io_reg(addr),
            REG_UNDOC_74 if self.cgb_mode => self.io_reg(addr),
            REG_UNDOC_75 if self.cgb_hw => 0x8F | (self.io_reg(addr) & 0x70),
            REG_PCM12 | REG_PCM34 if self.cgb_hw => 0x00,
            REG_KEY1 if self.cgb_mode => {
                0x7E | (u8::from(self.double_speed) << 7) | u8::from(self.key1_armed)
            }
            ppu::REG_VBK => self.ppu.read_vbk(),
            ppu::REG_BCPS..=ppu::REG_OCPD => self.ppu.read_palette_reg(addr),
            REG_SVBK if self.cgb_mode => 0xF8 | self.svbk,
            ppu::REG_LCDC..=ppu::REG_WX => self.ppu.read_reg(addr),
            _ => self.io[(addr - IO_START) as usize] | io::read_mask(addr),
        }
    }

    fn write_io(&mut self, addr: u16, value: u8) {
        match addr {
            REG_IF => self.interrupts.write_flag(value),
            REG_DIV..=REG_TAC => {
                self.timer.write(addr, value);
                // A DIV write resets the counter, which can be a falling edge for the APU.
                self.apu.clock_div(self.timer.counter());
            }
            io::REG_NR10..=io::REG_NR52 | io::WAVE_RAM_START..=io::WAVE_RAM_END => {
                self.apu.write(addr, value)
            }
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
            hdma::REG_HDMA1..=hdma::REG_HDMA5 if self.cgb_mode => {
                match self.hdma.write(addr, value) {
                    hdma::Start::Nothing => {}
                    hdma::Start::General(blocks) => {
                        for _ in 0..blocks {
                            self.hdma_copy_block();
                        }
                    }
                    hdma::Start::HBlank => {
                        // Started during HBlank: this HBlank's block goes right away.
                        if self.ppu.lcd_on()
                            && self.ppu.mode() == ppu::MODE_HBLANK
                            && self.ppu.ly() < 144
                        {
                            self.hdma_copy_block();
                        }
                    }
                }
            }
            REG_RP if self.cgb_mode => self.set_io_reg(addr, value & 0xC1),
            REG_OPRI if self.cgb_hw => self.set_io_reg(addr, value & 1),
            REG_UNDOC_72..=0xFF73 if self.cgb_hw => self.set_io_reg(addr, value),
            REG_UNDOC_74 if self.cgb_mode => self.set_io_reg(addr, value),
            REG_UNDOC_75 if self.cgb_hw => self.set_io_reg(addr, value & 0x70),
            REG_PCM12 | REG_PCM34 => {}
            REG_KEY1 if self.cgb_mode => self.key1_armed = value & 1 != 0,
            ppu::REG_VBK => self.ppu.write_vbk(value),
            ppu::REG_BCPS..=ppu::REG_OCPD => self.ppu.write_palette_reg(addr, value),
            REG_SVBK if self.cgb_mode => self.svbk = value & 7,
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

    pub fn apu_mut(&mut self) -> &mut Apu {
        &mut self.apu
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

impl Bus {
    /// Save-state support (see `crate::state`). `cgb_mode`, `cgb_hw` and `doctor_mode` are
    /// fixed when the console is built (`GameBoy::load_state` checks the model), not state.
    pub fn save_state(&self, w: &mut crate::state::StateWriter) {
        self.cartridge.save_state(w);
        self.ppu.save_state(w);
        self.apu.save_state(w);
        w.bytes(&self.wram);
        w.u8(self.svbk);
        w.bytes(&self.hram);
        w.bytes(&self.io);
        self.interrupts.save_state(w);
        w.u64(self.cycles);
        self.timer.save_state(w);
        self.dma.save_state(w);
        self.joypad.save_state(w);
        self.serial.save_state(w);
        w.bool(self.double_speed);
        w.bool(self.key1_armed);
        w.bool(self.slow_phase);
        self.hdma.save_state(w);
        // Always 0 between CPU accesses (`tick` drains it before returning), so no test can
        // catch a mistake here; saved anyway so the format stays complete if that ever changes.
        w.u32(self.hdma_stall);
    }

    pub fn load_state(
        &mut self,
        r: &mut crate::state::StateReader,
    ) -> Result<(), crate::state::StateError> {
        self.cartridge.load_state(r)?;
        self.ppu.load_state(r)?;
        self.apu.load_state(r)?;
        r.bytes(&mut self.wram)?;
        self.svbk = r.u8()? & 7;
        r.bytes(&mut self.hram)?;
        r.bytes(&mut self.io)?;
        self.interrupts.load_state(r)?;
        self.cycles = r.u64()?;
        self.timer.load_state(r)?;
        self.dma.load_state(r)?;
        self.joypad.load_state(r)?;
        self.serial.load_state(r)?;
        self.double_speed = r.bool()?;
        self.key1_armed = r.bool()?;
        self.slow_phase = r.bool()?;
        self.hdma.load_state(r)?;
        self.hdma_stall = r.u32()?;
        Ok(())
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
    fn vram_and_oam_are_locked_by_ppu_mode() {
        let mut bus = test_bus();
        // Post-boot the PPU sits at line 0 dot 0 (mode 2): OAM locked, VRAM open.
        bus.write(0x8000, 0x12);
        assert_eq!(bus.peek(0x8000), 0x12);
        bus.write(0xFE00, 0x34);
        assert_eq!(bus.peek(0xFE00), 0x00, "OAM write ignored in mode 2");
        assert_eq!(bus.read(0xFE00), 0xFF);
        while bus.ppu().mode() != 3 {
            bus.idle();
        }
        assert_eq!(bus.read(0x8000), 0xFF, "VRAM reads 0xFF in mode 3");
        bus.write(0x8000, 0x99);
        assert_eq!(bus.peek(0x8000), 0x12, "VRAM write ignored in mode 3");
        while bus.ppu().mode() != 0 {
            bus.idle();
        }
        assert_eq!(bus.read(0x8000), 0x12);
        bus.write(0xFE00, 0x56);
        assert_eq!(bus.peek(0xFE00), 0x56);
        // LCD off: everything open at any time.
        bus.write(ppu::REG_LCDC, 0x11);
        bus.idle();
        bus.write(0xFE01, 0x77);
        assert_eq!(bus.read(0xFE01), 0x77);
    }

    #[test]
    fn echo_ram_mirrors_wram() {
        let mut bus = test_bus();
        bus.write(0xC123, 0x5A);
        assert_eq!(bus.peek(0xE123), 0x5A);
        bus.write(0xE456, 0xA5);
        assert_eq!(bus.peek(0xC456), 0xA5);
    }

    fn cgb_bus() -> Bus {
        let mut rom = vec![0u8; 0x8000];
        rom[0x143] = 0x80;
        Bus::with_mode(Cartridge::from_bytes(rom).unwrap(), true, true)
    }

    #[test]
    fn cgb_wram_banks_switch_at_d000_and_zero_selects_bank_one() {
        let mut bus = cgb_bus();
        bus.write(0xC000, 0x11); // bank 0, always visible
        for bank in 1..=7u8 {
            bus.write(REG_SVBK, bank);
            bus.write(0xD000, 0x20 + bank);
        }
        for bank in 1..=7u8 {
            bus.write(REG_SVBK, bank);
            assert_eq!(bus.peek(0xD000), 0x20 + bank, "bank {bank}");
            assert_eq!(bus.peek(0xC000), 0x11);
            assert_eq!(bus.peek(0xF000), 0x20 + bank, "echo follows the bank");
        }
        bus.write(REG_SVBK, 0);
        assert_eq!(bus.peek(0xD000), 0x21, "0 selects bank 1");
        assert_eq!(
            bus.peek(REG_SVBK),
            0xF8,
            "unused bits read 1, bits 0-2 as written"
        );
        bus.write(REG_SVBK, 0xFD);
        assert_eq!(bus.peek(REG_SVBK), 0xFD, "only bits 0-2 are stored");
        assert_eq!(bus.peek(0xD000), 0x25);
    }

    #[test]
    fn cgb_vram_banks_are_separate() {
        let mut bus = cgb_bus();
        bus.write(0x8000, 0xAA);
        bus.write(ppu::REG_VBK, 1);
        assert_eq!(bus.peek(0x8000), 0, "bank 1 starts empty");
        bus.write(0x8000, 0xBB);
        assert_eq!(bus.peek(ppu::REG_VBK), 0xFF);
        assert_eq!(bus.ppu().vram()[0], 0xAA);
        assert_eq!(bus.ppu().vram_bank1()[0], 0xBB);
        bus.write(ppu::REG_VBK, 0);
        assert_eq!(bus.peek(0x8000), 0xAA);
        assert_eq!(bus.peek(ppu::REG_VBK), 0xFE);
    }

    #[test]
    fn dmg_has_no_bank_registers() {
        let mut bus = test_bus();
        bus.write(REG_SVBK, 3);
        bus.write(ppu::REG_VBK, 1);
        assert_eq!(bus.peek(REG_SVBK), 0xFF);
        assert_eq!(bus.peek(ppu::REG_VBK), 0xFF);
        bus.write(0x8000, 0x77);
        assert_eq!(bus.ppu().vram()[0], 0x77, "VBK ignored: still bank 0");
    }

    /// Program HDMA1-4 to copy from `src` to VRAM offset `dst`.
    fn set_hdma_addresses(bus: &mut Bus, src: u16, dst: u16) {
        bus.write(hdma::REG_HDMA1, (src >> 8) as u8);
        bus.write(hdma::REG_HDMA2, src as u8);
        bus.write(hdma::REG_HDMA3, (dst >> 8) as u8);
        bus.write(hdma::REG_HDMA4, dst as u8);
    }

    #[test]
    fn general_dma_copies_blocks_and_stalls_the_cpu() {
        let mut bus = cgb_bus();
        for i in 0..32u16 {
            bus.write(0xC000 + i, 0x40 + i as u8);
        }
        set_hdma_addresses(&mut bus, 0xC000, 0x8100);
        let before = bus.cycles();
        bus.write(hdma::REG_HDMA5, 0x01); // 2 blocks = 32 bytes
        for i in 0..32usize {
            assert_eq!(bus.ppu().vram()[0x100 + i], 0x40 + i as u8, "byte {i}");
        }
        // The write itself is 1 M-cycle, then 8 M-cycles per block in normal speed.
        assert_eq!(bus.cycles() - before, 4 * (1 + 16));
        assert_eq!(bus.peek(hdma::REG_HDMA5), 0xFF);
    }

    #[test]
    fn general_dma_writes_into_the_selected_vram_bank() {
        let mut bus = cgb_bus();
        bus.write(0xC000, 0x99);
        set_hdma_addresses(&mut bus, 0xC000, 0x8000);
        bus.write(ppu::REG_VBK, 1);
        bus.write(hdma::REG_HDMA5, 0x00);
        assert_eq!(bus.ppu().vram_bank1()[0], 0x99);
        assert_eq!(bus.ppu().vram()[0], 0);
    }

    #[test]
    fn hblank_dma_moves_one_block_per_hblank_and_stops_after_the_last() {
        let mut bus = cgb_bus();
        for i in 0..48u16 {
            bus.write(0xC000 + i, 1 + i as u8);
        }
        set_hdma_addresses(&mut bus, 0xC000, 0x8000);
        // Wait for the start of a frame's visible area with the LCD on (default LCDC 0x91).
        while bus.ppu().ly() != 0 || bus.ppu().mode() != ppu::MODE_OAM_SCAN {
            bus.tick();
        }
        bus.write(hdma::REG_HDMA5, 0x82); // 3 blocks, HBlank mode
        assert_eq!(bus.peek(hdma::REG_HDMA5), 0x02, "active, 3 blocks left");
        assert_eq!(
            bus.ppu().vram()[0],
            0,
            "nothing copied before the first HBlank"
        );
        let mut seen = Vec::new();
        for _ in 0..(456 * 4 / 4) {
            bus.tick();
            let n = (0..48).filter(|&i| bus.ppu().vram()[i] != 0).count();
            if seen.last() != Some(&n) {
                seen.push(n);
            }
        }
        // Lines 0, 1 and 2 each had an HBlank in these 456 M-cycles (4 lines): 16, 32, 48 bytes.
        assert_eq!(seen, vec![0, 16, 32, 48]);
        assert_eq!(bus.peek(hdma::REG_HDMA5), 0xFF, "finished");
        assert_eq!(bus.ppu().vram()[47], 48);
    }

    #[test]
    fn hblank_dma_can_be_cancelled() {
        let mut bus = cgb_bus();
        set_hdma_addresses(&mut bus, 0xC000, 0x8000);
        bus.write(hdma::REG_HDMA5, 0x8F);
        bus.write(hdma::REG_HDMA5, 0x0F); // bit 7 clear: cancel
        assert_eq!(bus.peek(hdma::REG_HDMA5), 0x8F);
        for _ in 0..(456 * 3) {
            bus.tick();
        }
        assert_eq!(
            bus.peek(hdma::REG_HDMA5),
            0x8F,
            "no blocks moved after cancel"
        );
    }

    #[test]
    fn dmg_has_no_vram_dma_registers() {
        let mut bus = test_bus();
        bus.write(hdma::REG_HDMA5, 0x00);
        assert_eq!(bus.peek(hdma::REG_HDMA5), 0xFF);
    }

    #[test]
    fn cgb_misc_registers_have_documented_defaults_and_masks() {
        let mut bus = cgb_bus();
        assert_eq!(bus.peek(REG_RP), 0x3E);
        bus.write(REG_RP, 0xFF);
        assert_eq!(bus.peek(REG_RP), 0xFF, "bits 0, 6, 7 stored, bit 1 reads 1");
        bus.write(REG_RP, 0x00);
        assert_eq!(bus.peek(REG_RP), 0x3E);
        assert_eq!(bus.peek(REG_OPRI), 0xFE);
        bus.write(REG_OPRI, 0xFF);
        assert_eq!(bus.peek(REG_OPRI), 0xFF);
        bus.write(REG_UNDOC_72, 0x5A);
        assert_eq!(bus.peek(REG_UNDOC_72), 0x5A);
        bus.write(REG_UNDOC_75, 0xFF);
        assert_eq!(bus.peek(REG_UNDOC_75), 0xFF);
        bus.write(REG_UNDOC_75, 0x00);
        assert_eq!(bus.peek(REG_UNDOC_75), 0x8F);
        assert_eq!(bus.peek(REG_PCM12), 0x00);
    }

    #[test]
    fn dmg_does_not_have_the_cgb_misc_registers() {
        let mut bus = test_bus();
        bus.write(REG_RP, 0x00);
        bus.write(REG_UNDOC_72, 0x5A);
        for addr in [REG_RP, REG_OPRI, REG_UNDOC_72, REG_UNDOC_75, REG_PCM12] {
            assert_eq!(bus.peek(addr), 0xFF, "{addr:#06X}");
        }
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
