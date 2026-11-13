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

    /// Branch condition code 0..3 = NZ, Z, NC, C.
    fn condition(&self, cc: u8) -> bool {
        match cc & 3 {
            0 => !self.regs.flag(FLAG_Z),
            1 => self.regs.flag(FLAG_Z),
            2 => !self.regs.flag(FLAG_C),
            _ => self.regs.flag(FLAG_C),
        }
    }

    /// JR e8: the offset is signed and relative to the PC *after* the operand.
    /// 3 M-cycles if taken, 2 if not.
    fn jr(&mut self, bus: &mut Bus, taken: bool) {
        let e = self.fetch(bus) as i8;
        if taken {
            self.regs.pc = self.regs.pc.wrapping_add(e as u16);
            bus.idle();
        }
    }

    /// Push the return address (PC after the operand) and jump. 1 idle + 2 writes.
    fn call(&mut self, bus: &mut Bus, target: u16) {
        bus.idle();
        let pc = self.regs.pc;
        self.push16(bus, pc);
        self.regs.pc = target;
    }

    /// 16-bit register pair for LD/INC/DEC/ADD: 0..3 = BC, DE, HL, SP.
    fn get_rr(&self, p: u8) -> u16 {
        match p {
            0 => self.regs.bc(),
            1 => self.regs.de(),
            2 => self.regs.hl(),
            _ => self.regs.sp,
        }
    }

    fn set_rr(&mut self, p: u8, v: u16) {
        match p {
            0 => self.regs.set_bc(v),
            1 => self.regs.set_de(v),
            2 => self.regs.set_hl(v),
            _ => self.regs.sp = v,
        }
    }

    /// Register pair for PUSH/POP: 0..3 = BC, DE, HL, AF.
    fn get_stack_rr(&self, p: u8) -> u16 {
        match p {
            3 => self.regs.af(),
            _ => self.get_rr(p),
        }
    }

    /// `POP AF` lands here: `set_af` masks the low nibble of F, which is always 0.
    fn set_stack_rr(&mut self, p: u8, v: u16) {
        match p {
            3 => self.regs.set_af(v),
            _ => self.set_rr(p, v),
        }
    }

    /// SP + signed e8, with flags. Pan Docs: Z=0, N=0, and H and C come from the carries of the
    /// *unsigned low-byte* addition, regardless of the sign of e8.
    fn sp_plus_e8(&mut self, sp: u16, e: u8) -> u16 {
        let h = (sp & 0xF) + u16::from(e & 0xF) > 0xF;
        let c = (sp & 0xFF) + u16::from(e) > 0xFF;
        self.regs.set_flags(false, false, h, c);
        sp.wrapping_add(e as i8 as u16)
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
                3 => self.jr(bus, true),
                _ => {
                    // JR cc,e8 (y = 4..7 selects NZ, Z, NC, C)
                    let taken = self.condition(y - 4);
                    self.jr(bus, taken);
                }
            },
            1 => {
                let p = y >> 1;
                if y & 1 == 0 {
                    // LD rr,nn
                    let nn = self.fetch16(bus);
                    self.set_rr(p, nn);
                } else {
                    // ADD HL,rr: 2 M-cycles (the 16-bit add takes one internal cycle).
                    let rr = self.get_rr(p);
                    bus.idle();
                    let hl = self.regs.hl();
                    // Pan Docs: H is carry out of bit 11, C out of bit 15, Z is untouched.
                    let h = (hl & 0x0FFF) + (rr & 0x0FFF) > 0x0FFF;
                    let (r, c) = hl.overflowing_add(rr);
                    self.regs.set_hl(r);
                    let z = self.regs.flag(FLAG_Z);
                    self.regs.set_flags(z, false, h, c);
                }
            }
            3 => {
                // INC rr / DEC rr: no flags; the 16-bit increment costs one internal cycle.
                let p = y >> 1;
                let v = self.get_rr(p);
                self.set_rr(
                    p,
                    if y & 1 == 0 {
                        v.wrapping_add(1)
                    } else {
                        v.wrapping_sub(1)
                    },
                );
                bus.idle();
            }
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
            // RET cc: 2 M-cycles if not taken, 5 if taken
            (0, 0..=3) => {
                bus.idle(); // condition check
                if self.condition(y) {
                    self.regs.pc = self.pop16(bus);
                    bus.idle();
                }
            }
            // RET (4 M-cycles) and RETI (same, and re-enables interrupts immediately)
            (1, 1) | (1, 3) => {
                self.regs.pc = self.pop16(bus);
                bus.idle();
                if y == 3 {
                    self.ime = true;
                }
            }
            // JP HL: just 1 M-cycle (the fetch)
            (1, 5) => self.regs.pc = self.regs.hl(),
            // JP cc,nn: 3 M-cycles if not taken, 4 if taken
            (2, 0..=3) => {
                let nn = self.fetch16(bus);
                if self.condition(y) {
                    self.regs.pc = nn;
                    bus.idle();
                }
            }
            // JP nn
            (3, 0) => {
                self.regs.pc = self.fetch16(bus);
                bus.idle();
            }
            // CALL cc,nn: 3 M-cycles if not taken, 6 if taken
            (4, 0..=3) => {
                let nn = self.fetch16(bus);
                if self.condition(y) {
                    self.call(bus, nn);
                }
            }
            // CALL nn
            (5, 1) => {
                let nn = self.fetch16(bus);
                self.call(bus, nn);
            }
            // RST: call to a fixed address y*8
            (7, _) => self.call(bus, u16::from(y) * 8),
            // POP rr / PUSH rr (BC, DE, HL, AF)
            (1, _) if y & 1 == 0 => {
                let v = self.pop16(bus);
                self.set_stack_rr(y >> 1, v);
            }
            (5, _) if y & 1 == 0 => {
                let v = self.get_stack_rr(y >> 1);
                bus.idle(); // internal cycle before the two writes
                self.push16(bus, v);
            }
            // LD SP,HL
            (1, 7) => {
                self.regs.sp = self.regs.hl();
                bus.idle();
            }
            // ADD SP,e8 and LD HL,SP+e8
            (0, 5) => {
                let e = self.fetch(bus);
                let sp = self.regs.sp;
                let r = self.sp_plus_e8(sp, e);
                bus.idle();
                bus.idle();
                self.regs.sp = r;
            }
            (0, 7) => {
                let e = self.fetch(bus);
                let r = self.sp_plus_e8(self.regs.sp, e);
                bus.idle();
                self.regs.set_hl(r);
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
    fn sixteen_bit_loads_push_pop_and_timing() {
        // LD BC,0x1234 ; PUSH BC ; POP AF ; LD SP,HL ; INC HL ; ADD HL,DE
        let (mut cpu, mut bus) = setup(&[0x01, 0x34, 0x12, 0xC5, 0xF1, 0xF9, 0x23, 0x19]);
        cpu.regs.sp = 0xDFF0;
        assert_eq!(step_m(&mut cpu, &mut bus), 3);
        assert_eq!(step_m(&mut cpu, &mut bus), 4);
        assert_eq!((bus.peek(0xDFEF), bus.peek(0xDFEE)), (0x12, 0x34));
        assert_eq!(step_m(&mut cpu, &mut bus), 3);
        assert_eq!(cpu.regs.af(), 0x1230, "POP AF masks the low nibble of F");
        cpu.regs.set_hl(0x0FFF);
        cpu.regs.set_de(0x0001);
        assert_eq!(step_m(&mut cpu, &mut bus), 2);
        assert_eq!(cpu.regs.sp, 0x0FFF);
        assert_eq!(step_m(&mut cpu, &mut bus), 2);
        assert_eq!(cpu.regs.hl(), 0x1000);
        cpu.regs.set_hl(0x0FFF);
        assert_eq!(step_m(&mut cpu, &mut bus), 2);
        assert_eq!(cpu.regs.hl(), 0x1000);
        assert!(cpu.regs.flag(FLAG_H) && !cpu.regs.flag(FLAG_C));
    }

    #[test]
    fn add_sp_e8_flags_use_low_byte_carries() {
        // ADD SP,-1 ; LD HL,SP+1
        let (mut cpu, mut bus) = setup(&[0xE8, 0xFF, 0xF8, 0x01]);
        cpu.regs.sp = 0x00FF;
        assert_eq!(step_m(&mut cpu, &mut bus), 4);
        assert_eq!(cpu.regs.sp, 0x00FE);
        assert_eq!(cpu.regs.f(), FLAG_H | FLAG_C);
        assert_eq!(step_m(&mut cpu, &mut bus), 3);
        assert_eq!(cpu.regs.hl(), 0x00FF);
        assert_eq!(cpu.regs.f(), 0);
    }

    #[test]
    fn jumps_calls_returns_timing() {
        // 0x100: JP 0x0110
        let (mut cpu, mut bus) = setup(&[0xC3, 0x10, 0x01]);
        assert_eq!(step_m(&mut cpu, &mut bus), 4);
        assert_eq!(cpu.regs.pc, 0x0110);
        // The rest runs from WRAM, because ROM is not writable.
        let (mut cpu, mut bus) = setup(&[]);
        cpu.regs.pc = 0xC000;
        cpu.regs.sp = 0xDFF0;
        let prog = [
            0xCD, 0x10, 0xC0, // C000: CALL C010
            0x20, 0x02, // C003: JR NZ,+2 (taken: Z clear)
            0x00, 0x00, // C005: skipped
            0x28, 0x7F, // C007: JR Z,+127 (not taken)
        ];
        for (i, b) in prog.iter().enumerate() {
            bus.write(0xC000 + i as u16, *b);
        }
        bus.write(0xC010, 0xC9); // RET
        cpu.regs.set_flags(false, false, false, false);
        assert_eq!(step_m(&mut cpu, &mut bus), 6, "CALL");
        assert_eq!(cpu.regs.pc, 0xC010);
        assert_eq!((bus.peek(0xDFEF), bus.peek(0xDFEE)), (0xC0, 0x03));
        assert_eq!(step_m(&mut cpu, &mut bus), 4, "RET");
        assert_eq!(cpu.regs.pc, 0xC003);
        assert_eq!(step_m(&mut cpu, &mut bus), 3, "JR taken");
        assert_eq!(cpu.regs.pc, 0xC007);
        cpu.regs.set_flags(false, false, false, false);
        assert_eq!(step_m(&mut cpu, &mut bus), 2, "JR Z not taken");
        assert_eq!(cpu.regs.pc, 0xC009);
    }

    #[test]
    fn conditional_ret_and_call_cycle_counts() {
        let (mut cpu, mut bus) = setup(&[]);
        cpu.regs.sp = 0xDFF0;
        // RET NZ at C000 (taken), RET Z at C001 (not taken), RST 0x08 at C002, CALL Z at C003
        for (i, b) in [0xC0u8, 0xC8, 0xCF, 0xCC, 0x00, 0xD0].iter().enumerate() {
            bus.write(0xC000 + i as u16, *b);
        }
        bus.write(0xDFF0, 0x01); // return address 0xC001 on the stack
        bus.write(0xDFF1, 0xC0);
        cpu.regs.pc = 0xC000;
        cpu.regs.set_flags(false, false, false, false);
        assert_eq!(step_m(&mut cpu, &mut bus), 5, "RET cc taken");
        assert_eq!(cpu.regs.pc, 0xC001);
        assert_eq!(step_m(&mut cpu, &mut bus), 2, "RET cc not taken");
        assert_eq!(step_m(&mut cpu, &mut bus), 4, "RST");
        assert_eq!(cpu.regs.pc, 0x0008);
        cpu.regs.pc = 0xC003;
        assert_eq!(step_m(&mut cpu, &mut bus), 3, "CALL cc not taken");
        assert_eq!(cpu.regs.pc, 0xC006);
    }

    #[test]
    fn ld_nn_sp_writes_little_endian() {
        let (mut cpu, mut bus) = setup(&[0x08, 0x00, 0xC0]);
        cpu.regs.sp = 0xBEEF;
        assert_eq!(step_m(&mut cpu, &mut bus), 5);
        assert_eq!((bus.peek(0xC000), bus.peek(0xC001)), (0xEF, 0xBE));
    }
}
