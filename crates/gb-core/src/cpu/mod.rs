//! SM83 CPU (Sharp LR35902 core of the Game Boy).
//!
//! Timing (CLAUDE.md §4.2): there are no cycle tables. Every instruction is written as a
//! sequence of `bus.read`, `bus.write` and `bus.idle` calls, each costing one M-cycle, so the
//! instruction's length emerges from what it does. Pan Docs' opcode tables are only used to
//! *check* the counts (see the tests at the bottom).
//!
//! Opcodes are decoded with the regular bit layout `xx yyy zzz` (Pan Docs "CPU Instruction Set";
//! the layout is the same one used by the Z80). The 3-bit register code is
//! 0..7 = B, C, D, E, H, L, (HL), A.

mod alu;
pub mod registers;

use crate::bus::Bus;
use registers::{Registers, FLAG_C, FLAG_H, FLAG_N, FLAG_Z};

/// Interrupt vectors, indexed by interrupt bit (VBlank, STAT, Timer, Serial, Joypad).
const INTERRUPT_VECTORS: [u16; 5] = [0x40, 0x48, 0x50, 0x58, 0x60];

#[derive(Debug, Clone)]
pub struct Cpu {
    pub regs: Registers,
    /// Interrupt master enable.
    ime: bool,
    /// `EI` enables interrupts only after the *following* instruction has executed.
    ime_pending: bool,
    halted: bool,
    /// HALT bug: the byte after HALT is fetched twice because PC fails to increment once.
    halt_bug: bool,
    /// Set by an illegal opcode. Real hardware hangs; we stop executing instead of panicking.
    locked: bool,
}

impl Cpu {
    /// CPU in the DMG post-boot state (no boot ROM is emulated).
    pub fn new() -> Cpu {
        Cpu {
            regs: Registers::post_boot_dmg(),
            ime: false,
            ime_pending: false,
            halted: false,
            halt_bug: false,
            locked: false,
        }
    }

    pub fn is_locked(&self) -> bool {
        self.locked
    }

    /// Execute one instruction (or one idle HALT step). Returns the T-cycles elapsed.
    pub fn step(&mut self, bus: &mut Bus) -> u32 {
        let start = bus.cycles();
        self.step_inner(bus);
        (bus.cycles() - start) as u32
    }

    fn step_inner(&mut self, bus: &mut Bus) {
        if self.locked {
            bus.idle();
            return;
        }
        let enable_ime_after = self.ime_pending;
        self.ime_pending = false;

        if self.halted {
            // HALT ends as soon as any enabled interrupt is requested, even with IME=0.
            if bus.pending_interrupts() == 0 {
                bus.idle();
                return;
            }
            self.halted = false;
        }

        if self.ime && bus.pending_interrupts() != 0 {
            self.dispatch_interrupt(bus);
            return;
        }

        let opcode = self.fetch(bus);
        self.execute(bus, opcode);

        if enable_ime_after {
            self.ime = true;
        }
    }

    // ---- bus helpers ---------------------------------------------------------------------

    /// Read the byte at PC and advance PC. 1 M-cycle.
    fn fetch(&mut self, bus: &mut Bus) -> u8 {
        let v = bus.read(self.regs.pc);
        if self.halt_bug {
            self.halt_bug = false;
        } else {
            self.regs.pc = self.regs.pc.wrapping_add(1);
        }
        v
    }

    /// Little-endian 16-bit immediate. 2 M-cycles.
    fn fetch16(&mut self, bus: &mut Bus) -> u16 {
        let lo = self.fetch(bus);
        let hi = self.fetch(bus);
        u16::from_le_bytes([lo, hi])
    }

    /// Push order (Pan Docs): high byte at SP-1 first, then low byte at SP-2. 2 M-cycles.
    fn push16(&mut self, bus: &mut Bus, v: u16) {
        self.regs.sp = self.regs.sp.wrapping_sub(1);
        bus.write(self.regs.sp, (v >> 8) as u8);
        self.regs.sp = self.regs.sp.wrapping_sub(1);
        bus.write(self.regs.sp, v as u8);
    }

    /// 2 M-cycles.
    #[allow(dead_code)] // used from task 5 (POP/RET); remove then
    fn pop16(&mut self, bus: &mut Bus) -> u16 {
        let lo = bus.read(self.regs.sp);
        self.regs.sp = self.regs.sp.wrapping_add(1);
        let hi = bus.read(self.regs.sp);
        self.regs.sp = self.regs.sp.wrapping_add(1);
        u16::from_le_bytes([lo, hi])
    }

    /// Read 8-bit operand by register code. Code 6 is `(HL)` and costs a bus read.
    fn get_r8(&mut self, bus: &mut Bus, code: u8) -> u8 {
        match code {
            0 => self.regs.b,
            1 => self.regs.c,
            2 => self.regs.d,
            3 => self.regs.e,
            4 => self.regs.h,
            5 => self.regs.l,
            6 => bus.read(self.regs.hl()),
            _ => self.regs.a,
        }
    }

    fn set_r8(&mut self, bus: &mut Bus, code: u8, v: u8) {
        match code {
            0 => self.regs.b = v,
            1 => self.regs.c = v,
            2 => self.regs.d = v,
            3 => self.regs.e = v,
            4 => self.regs.h = v,
            5 => self.regs.l = v,
            6 => bus.write(self.regs.hl(), v),
            _ => self.regs.a = v,
        }
    }

    // ---- interrupts ----------------------------------------------------------------------

    /// Interrupt dispatch takes 5 M-cycles: 2 idle, push PC (2 writes), set PC (1 idle).
    fn dispatch_interrupt(&mut self, bus: &mut Bus) {
        self.ime = false;
        bus.idle();
        bus.idle();
        let pc = self.regs.pc;
        self.push16(bus, pc);
        // Priority: lowest bit number first (VBlank > STAT > Timer > Serial > Joypad).
        let pending = bus.pending_interrupts();
        if pending == 0 {
            // IE was changed by the push itself; hardware then jumps to 0x0000.
            self.regs.pc = 0;
        } else {
            let bit = pending.trailing_zeros() as u8;
            bus.clear_interrupt(bit);
            self.regs.pc = INTERRUPT_VECTORS[bit as usize];
        }
        bus.idle();
    }

    // ---- decode and execute --------------------------------------------------------------

    fn execute(&mut self, bus: &mut Bus, opcode: u8) {
        let x = opcode >> 6;
        let y = (opcode >> 3) & 7;
        let z = opcode & 7;
        match x {
            0 => self.execute_block0(bus, y, z),
            1 => {
                if opcode == 0x76 {
                    self.halt(bus);
                } else {
                    // LD r,r'
                    let v = self.get_r8(bus, z);
                    self.set_r8(bus, y, v);
                }
            }
            2 => {
                let v = self.get_r8(bus, z);
                self.alu(y, v);
            }
            _ => self.execute_block3(bus, y, z),
        }
    }

    fn halt(&mut self, bus: &Bus) {
        if !self.ime && bus.pending_interrupts() != 0 {
            // HALT bug (Pan Docs "halt"): the CPU does not halt and the next byte is read twice.
            self.halt_bug = true;
        } else {
            self.halted = true;
        }
    }

    fn execute_block0(&mut self, bus: &mut Bus, y: u8, z: u8) {
        match z {
            0 => match y {
                0 => {} // NOP
                1 => {
                    // LD (nn),SP
                    let addr = self.fetch16(bus);
                    bus.write(addr, self.regs.sp as u8);
                    bus.write(addr.wrapping_add(1), (self.regs.sp >> 8) as u8);
                }
                2 => {
                    // STOP is followed by a padding byte. Phase 1 treats it as a 2-byte NOP;
                    // the low-power mode is not needed by the CPU tests.
                    self.fetch(bus);
                }
                _ => self.todo_lock(),
            },
            2 => {
                // LD (rr),A / LD A,(rr) with HL+ / HL-
                let addr = match y >> 1 {
                    0 => self.regs.bc(),
                    1 => self.regs.de(),
                    2 => {
                        let hl = self.regs.hl();
                        self.regs.set_hl(hl.wrapping_add(1));
                        hl
                    }
                    _ => {
                        let hl = self.regs.hl();
                        self.regs.set_hl(hl.wrapping_sub(1));
                        hl
                    }
                };
                if y & 1 == 0 {
                    bus.write(addr, self.regs.a);
                } else {
                    self.regs.a = bus.read(addr);
                }
            }
            4 | 5 => {
                // INC r / DEC r. For (HL) this is a read-modify-write: 3 M-cycles in total.
                let v = self.get_r8(bus, y);
                let (r, f) = if z == 4 {
                    alu::inc(v, self.regs.f())
                } else {
                    alu::dec(v, self.regs.f())
                };
                self.set_r8(bus, y, r);
                self.regs.set_flags(
                    f & FLAG_Z != 0,
                    f & FLAG_N != 0,
                    f & FLAG_H != 0,
                    f & FLAG_C != 0,
                );
            }
            6 => {
                // LD r,n
                let n = self.fetch(bus);
                self.set_r8(bus, y, n);
            }
            _ => self.todo_lock(),
        }
    }

    fn execute_block3(&mut self, bus: &mut Bus, y: u8, z: u8) {
        match (z, y) {
            // LDH (n),A and LDH A,(n): high-RAM / IO page at 0xFF00 + n
            (0, 4) => {
                let n = self.fetch(bus);
                bus.write(0xFF00 | u16::from(n), self.regs.a);
            }
            (0, 6) => {
                let n = self.fetch(bus);
                self.regs.a = bus.read(0xFF00 | u16::from(n));
            }
            // LD (C),A and LD A,(C)
            (2, 4) => bus.write(0xFF00 | u16::from(self.regs.c), self.regs.a),
            (2, 6) => self.regs.a = bus.read(0xFF00 | u16::from(self.regs.c)),
            // LD (nn),A and LD A,(nn)
            (2, 5) => {
                let addr = self.fetch16(bus);
                bus.write(addr, self.regs.a);
            }
            (2, 7) => {
                let addr = self.fetch16(bus);
                self.regs.a = bus.read(addr);
            }
            // ALU A,n
            (6, _) => {
                let n = self.fetch(bus);
                self.alu(y, n);
            }
            _ => self.todo_lock(),
        }
    }

    /// Placeholder for instructions added in later tasks of phase 1.
    fn todo_lock(&mut self) {
        self.locked = true;
    }

    /// 8-bit ALU operation `y` on A (0..7 = ADD ADC SUB SBC AND XOR OR CP).
    fn alu(&mut self, y: u8, v: u8) {
        let a = self.regs.a;
        let carry = self.regs.flag(FLAG_C);
        let (result, f) = match y {
            0 => alu::add(a, v, false),
            1 => alu::add(a, v, carry),
            2 | 7 => alu::sub(a, v, false), // CP is SUB without storing the result
            3 => alu::sub(a, v, carry),
            4 => alu::and(a, v),
            5 => alu::xor(a, v),
            _ => alu::or(a, v),
        };
        if y != 7 {
            self.regs.a = result;
        }
        self.regs.set_flags(
            f & FLAG_Z != 0,
            f & FLAG_N != 0,
            f & FLAG_H != 0,
            f & FLAG_C != 0,
        );
    }
}

impl Default for Cpu {
    fn default() -> Cpu {
        Cpu::new()
    }
}

// Silence unused-import warnings until later tasks use every flag.
const _: (u8, u8, u8, u8) = (FLAG_Z, FLAG_N, FLAG_H, FLAG_C);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::Cartridge;

    /// Build a CPU+bus with `program` placed at 0x0100 in an otherwise empty ROM-only cartridge.
    pub(super) fn setup(program: &[u8]) -> (Cpu, Bus) {
        let mut rom = vec![0u8; 0x8000];
        rom[0x100..0x100 + program.len()].copy_from_slice(program);
        let bus = Bus::new(Cartridge::from_bytes(rom).unwrap());
        (Cpu::new(), bus)
    }

    /// Run one instruction, returning M-cycles.
    pub(super) fn step_m(cpu: &mut Cpu, bus: &mut Bus) -> u32 {
        cpu.step(bus) / 4
    }

    #[test]
    fn ld_r_n_and_ld_r_r() {
        // LD B,0x42 ; LD C,B ; LD A,C
        let (mut cpu, mut bus) = setup(&[0x06, 0x42, 0x48, 0x79]);
        assert_eq!(step_m(&mut cpu, &mut bus), 2);
        assert_eq!(step_m(&mut cpu, &mut bus), 1);
        assert_eq!(step_m(&mut cpu, &mut bus), 1);
        assert_eq!((cpu.regs.b, cpu.regs.c, cpu.regs.a), (0x42, 0x42, 0x42));
    }

    #[test]
    fn hl_indirect_loads_cost_extra_cycle() {
        // LD (HL),0x99 ; LD B,(HL) ; LD (HL),B  with HL=0xC000
        let (mut cpu, mut bus) = setup(&[0x36, 0x99, 0x46, 0x70]);
        cpu.regs.set_hl(0xC000);
        assert_eq!(step_m(&mut cpu, &mut bus), 3);
        assert_eq!(step_m(&mut cpu, &mut bus), 2);
        assert_eq!(cpu.regs.b, 0x99);
        assert_eq!(step_m(&mut cpu, &mut bus), 2);
    }

    #[test]
    fn hl_plus_minus_and_bc_de_loads() {
        // LD (HL+),A ; LD (HL-),A ; LD A,(BC) ; LD (DE),A
        let (mut cpu, mut bus) = setup(&[0x22, 0x32, 0x0A, 0x12]);
        cpu.regs.a = 0x7E;
        cpu.regs.set_hl(0xC000);
        cpu.regs.set_bc(0xC010);
        cpu.regs.set_de(0xC020);
        step_m(&mut cpu, &mut bus);
        assert_eq!(cpu.regs.hl(), 0xC001);
        assert_eq!(bus.peek(0xC000), 0x7E);
        step_m(&mut cpu, &mut bus);
        assert_eq!(cpu.regs.hl(), 0xC000);
        assert_eq!(bus.peek(0xC001), 0x7E);
        bus.write(0xC010, 0x11);
        step_m(&mut cpu, &mut bus);
        assert_eq!(cpu.regs.a, 0x11);
        step_m(&mut cpu, &mut bus);
        assert_eq!(bus.peek(0xC020), 0x11);
    }

    #[test]
    fn ldh_and_absolute_loads() {
        // LDH (0x80),A ; LDH A,(0x80) ; LD (0xC123),A ; LD A,(0xC123) ; LD (C),A ; LD A,(C)
        let (mut cpu, mut bus) = setup(&[
            0xE0, 0x80, 0xF0, 0x80, 0xEA, 0x23, 0xC1, 0xFA, 0x23, 0xC1, 0xE2, 0xF2,
        ]);
        cpu.regs.a = 0x33;
        cpu.regs.c = 0x81;
        assert_eq!(step_m(&mut cpu, &mut bus), 3);
        assert_eq!(bus.peek(0xFF80), 0x33);
        cpu.regs.a = 0;
        assert_eq!(step_m(&mut cpu, &mut bus), 3);
        assert_eq!(cpu.regs.a, 0x33);
        assert_eq!(step_m(&mut cpu, &mut bus), 4);
        assert_eq!(bus.peek(0xC123), 0x33);
        cpu.regs.a = 0;
        assert_eq!(step_m(&mut cpu, &mut bus), 4);
        assert_eq!(cpu.regs.a, 0x33);
        assert_eq!(step_m(&mut cpu, &mut bus), 2);
        assert_eq!(bus.peek(0xFF81), 0x33);
        cpu.regs.a = 0;
        assert_eq!(step_m(&mut cpu, &mut bus), 2);
        assert_eq!(cpu.regs.a, 0x33);
    }

    #[test]
    fn alu_instructions_and_timing() {
        // ADD A,B ; ADC A,0x01 ; SUB (HL) ; CP 0x10 ; INC (HL) ; DEC A
        let (mut cpu, mut bus) = setup(&[0x80, 0xCE, 0x01, 0x96, 0xFE, 0x10, 0x34, 0x3D]);
        cpu.regs.a = 0x0F;
        cpu.regs.b = 0x01;
        cpu.regs.set_hl(0xC000);
        bus.write(0xC000, 0x05);
        assert_eq!(step_m(&mut cpu, &mut bus), 1);
        assert_eq!(cpu.regs.a, 0x10);
        assert_eq!(cpu.regs.f(), FLAG_H);
        assert_eq!(step_m(&mut cpu, &mut bus), 2);
        assert_eq!(cpu.regs.a, 0x11);
        assert_eq!(step_m(&mut cpu, &mut bus), 2);
        assert_eq!(cpu.regs.a, 0x0C);
        assert_eq!(step_m(&mut cpu, &mut bus), 2);
        assert_eq!(cpu.regs.a, 0x0C, "CP must not store");
        assert!(cpu.regs.flag(FLAG_C), "0x0C < 0x10 borrows");
        assert_eq!(step_m(&mut cpu, &mut bus), 3);
        assert_eq!(bus.peek(0xC000), 0x06);
        assert_eq!(step_m(&mut cpu, &mut bus), 1);
        assert_eq!(cpu.regs.a, 0x0B);
    }

    #[test]
    fn ld_nn_sp_writes_little_endian() {
        let (mut cpu, mut bus) = setup(&[0x08, 0x00, 0xC0]);
        cpu.regs.sp = 0xBEEF;
        assert_eq!(step_m(&mut cpu, &mut bus), 5);
        assert_eq!((bus.peek(0xC000), bus.peek(0xC001)), (0xEF, 0xBE));
    }
}
