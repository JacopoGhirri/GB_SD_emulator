//! Game Boy / Game Boy Color emulator core.
//!
//! Cartridge, bus, SM83 CPU, timer, DMA and (phase 3) the DMG PPU. No APU yet; audio methods
//! return placeholder values.

pub mod bus;
pub mod cartridge;
pub mod cpu;
pub mod dma;
pub mod input_script;
pub mod interrupts;
pub mod io;
pub mod joypad;
pub mod mooneye;
pub mod ppu;
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
}

impl GameBoy {
    pub fn new(cartridge: Cartridge, model: Model) -> GameBoy {
        GameBoy {
            model,
            cpu: Cpu::new(),
            bus: Bus::new(cartridge),
        }
    }

    pub fn model(&self) -> Model {
        self.model
    }

    pub fn cartridge(&self) -> &Cartridge {
        self.bus.cartridge()
    }

    /// Mutable cartridge access, for frontends (wall-clock RTC stamping).
    pub fn cartridge_mut(&mut self) -> &mut Cartridge {
        self.bus.cartridge_mut()
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

    /// Read-only view of the CPU, for tracing and tests.
    pub fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    /// Read-only view of the bus. `Bus::peek` reads memory without advancing time.
    pub fn bus(&self) -> &Bus {
        &self.bus
    }

    /// Run until the PPU finishes a frame (VBlank starts, or 70224 dots pass with the LCD off).
    /// `framebuffer()` then holds that complete frame.
    pub fn run_frame(&mut self) {
        while !self.bus.ppu_mut().take_frame_ready() {
            self.step();
        }
    }

    /// 160*144 pixels, 0x00RRGGBB.
    pub fn framebuffer(&self) -> &[u32] {
        self.bus.ppu().framebuffer()
    }

    /// FNV-1a hash of the visible framebuffer. Used to pin "this frame looks like what the owner
    /// approved" in local ROM tests without storing images of commercial games.
    pub fn framebuffer_hash(&self) -> u64 {
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        for px in self.framebuffer() {
            for b in px.to_le_bytes() {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        h
    }

    /// Held buttons (bit set = pressed, see [`Buttons`] constants).
    pub fn set_buttons(&mut self, buttons: Buttons) {
        self.bus.set_buttons(buttons);
    }

    /// Bytes written to the serial port since the last call.
    pub fn take_serial(&mut self) -> Vec<u8> {
        self.bus.take_serial()
    }

    /// Battery-backed cartridge data for a `.sav` file (RAM, plus the RTC footer on MBC3+TIMER).
    /// Empty if the cartridge has no battery.
    pub fn save_ram(&self) -> Vec<u8> {
        self.bus.cartridge().save_data()
    }

    /// Restore a save made by [`GameBoy::save_ram`]. Short or oversized data never panics.
    pub fn load_save_ram(&mut self, data: &[u8]) {
        self.bus.cartridge_mut().load_save(data);
    }

    /// Append interleaved stereo f32 samples. Placeholder: appends nothing.
    pub fn drain_audio(&mut self, _out: &mut Vec<f32>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 32 KiB ROM that spins in place: `JR -2` at 0x0100.
    fn spinning_rom() -> Cartridge {
        let mut rom = vec![0u8; 0x8000];
        rom[0x100] = 0x18;
        rom[0x101] = 0xFE;
        Cartridge::from_bytes(rom).unwrap()
    }

    #[test]
    fn run_frame_advances_exactly_one_frame_of_dots() {
        let mut gb = GameBoy::new(spinning_rom(), Model::Dmg);
        gb.run_frame(); // first call: finishes the partial frame that was in progress
        let start = gb.bus().cycles();
        gb.run_frame();
        let frame = gb.bus().cycles() - start;
        assert!(
            frame.abs_diff(70224) < 24,
            "a frame is 70224 T-cycles, got {frame} (instruction granularity allows a few)"
        );
    }
}
