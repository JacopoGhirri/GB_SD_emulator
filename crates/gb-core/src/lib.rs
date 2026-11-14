//! Game Boy / Game Boy Color emulator core.
//!
//! Phase 1 in progress: cartridge header, bus, and the SM83 CPU. No PPU, APU or full timer yet.
//! Methods for components that do not exist yet return placeholder values.

pub mod bus;
pub mod cartridge;
pub mod cpu;
pub mod dma;
pub mod interrupts;
pub mod io;
pub mod joypad;
pub mod mooneye;
pub mod serial;
pub mod timer;

pub use bus::Bus;
pub use cartridge::{Cartridge, CartridgeError};
pub use cpu::Cpu;

/// Hardware model to emulate (CLAUDE.md §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Model {
    Dmg,
    Cgb,
}

pub use joypad::Buttons;

/// Width and height of the LCD in pixels.
pub const SCREEN_WIDTH: usize = 160;
pub const SCREEN_HEIGHT: usize = 144;

/// Top-level emulator handle. Public API from CLAUDE.md §4.1.
///
/// Placeholder: nothing is executed yet. The methods exist so the CLI and tests
/// can be written against the final signatures.
pub struct GameBoy {
    model: Model,
    cpu: Cpu,
    bus: Bus,
    framebuffer: Vec<u32>,
}

impl GameBoy {
    pub fn new(cartridge: Cartridge, model: Model) -> GameBoy {
        GameBoy {
            model,
            cpu: Cpu::new(),
            bus: Bus::new(cartridge),
            framebuffer: vec![0; SCREEN_WIDTH * SCREEN_HEIGHT],
        }
    }

    pub fn model(&self) -> Model {
        self.model
    }

    pub fn cartridge(&self) -> &Cartridge {
        self.bus.cartridge()
    }

    /// Execute one CPU instruction (or one idle HALT step). Returns T-cycles elapsed.
    pub fn step(&mut self) -> u32 {
        self.cpu.step(&mut self.bus)
    }

    /// Program counter.
    pub fn pc(&self) -> u16 {
        self.cpu.regs.pc
    }

    /// Gameboy Doctor trace line for the instruction about to execute.
    pub fn trace_line(&self) -> String {
        self.cpu.trace_line(&self.bus)
    }

    /// See [`Cpu::is_trace_point`].
    pub fn is_trace_point(&self) -> bool {
        self.cpu.is_trace_point()
    }

    /// Make LY read a constant 0x90, as Gameboy Doctor expects. For tracing only.
    pub fn set_doctor_mode(&mut self, on: bool) {
        self.bus.set_doctor_mode(on);
    }

    /// Make LY read a constant 0x90 (no PPU yet). Used by the Mooneye runner; see `Bus`.
    pub fn set_ly_stub(&mut self, on: bool) {
        self.bus.set_ly_stub(on);
    }

    /// Read-only view of the CPU, for tracing and tests.
    pub fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    /// Read-only view of the bus. `Bus::peek` reads memory without advancing time.
    pub fn bus(&self) -> &Bus {
        &self.bus
    }

    /// Run until a frame is complete. Placeholder: no-op.
    pub fn run_frame(&mut self) {}

    /// 160*144 pixels, 0x00RRGGBB.
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    /// Held buttons (bit set = pressed, see [`Buttons`] constants).
    pub fn set_buttons(&mut self, buttons: Buttons) {
        self.bus.set_buttons(buttons);
    }

    /// Bytes written to the serial port since the last call.
    pub fn take_serial(&mut self) -> Vec<u8> {
        self.bus.take_serial()
    }

    /// Cartridge RAM for battery saves. Placeholder: empty.
    pub fn save_ram(&self) -> Vec<u8> {
        Vec::new()
    }

    /// Restore cartridge RAM from a save file. Placeholder: ignores input.
    pub fn load_save_ram(&mut self, _data: &[u8]) {}

    /// Append interleaved stereo f32 samples. Placeholder: appends nothing.
    pub fn drain_audio(&mut self, _out: &mut Vec<f32>) {}
}
