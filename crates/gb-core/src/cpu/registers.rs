//! SM83 register file and flags (Pan Docs "CPU Registers and Flags").

/// Flag bit masks inside F. Only the upper nibble of F exists in hardware.
pub const FLAG_Z: u8 = 0x80; // zero
pub const FLAG_N: u8 = 0x40; // subtraction
pub const FLAG_H: u8 = 0x20; // half carry
pub const FLAG_C: u8 = 0x10; // carry

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Registers {
    pub a: u8,
    /// Private so the low nibble can never be set: it always reads 0 on hardware.
    f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub sp: u16,
    pub pc: u16,
}

impl Registers {
    /// Register values after the DMG boot ROM hands over control
    /// (Pan Docs "Power-Up Sequence"). No boot ROM is emulated.
    pub fn post_boot_dmg() -> Registers {
        let mut r = Registers {
            a: 0,
            f: 0,
            b: 0,
            c: 0,
            d: 0,
            e: 0,
            h: 0,
            l: 0,
            sp: 0xFFFE,
            pc: 0x0100,
        };
        r.set_af(0x01B0);
        r.set_bc(0x0013);
        r.set_de(0x00D8);
        r.set_hl(0x014D);
        r
    }

    /// Register values after the CGB boot ROM hands over to a CGB-aware cartridge
    /// (Pan Docs "Power-Up Sequence"; checked by Mooneye `misc/boot_regs-cgb`).
    /// A = 0x11 is how software detects a CGB.
    pub fn post_boot_cgb() -> Registers {
        let mut r = Registers::post_boot_dmg();
        r.set_af(0x1180);
        r.set_bc(0x0000);
        r.set_de(0xFF56);
        r.set_hl(0x000D);
        r
    }

    /// CGB hardware running a DMG-only cartridge ("DMG compatibility mode"). Values read out of
    /// the expected-register table inside Mooneye `misc/boot_regs-cgb` (a DMG-flagged ROM titled
    /// "mooneye-gb test"). The real boot ROM derives B (and with it the DMG palette) from a hash
    /// of the title; other titles may differ, which is not modelled (phase 8 if a game needs it).
    pub fn post_boot_cgb_compat() -> Registers {
        let mut r = Registers::post_boot_dmg();
        r.set_af(0x1180);
        r.set_bc(0x0000);
        r.set_de(0x0008);
        r.set_hl(0x007C);
        r
    }

    pub fn f(&self) -> u8 {
        self.f
    }

    pub fn af(&self) -> u16 {
        u16::from_be_bytes([self.a, self.f])
    }
    /// Also used by `POP AF`, which therefore masks the low nibble of F for free.
    pub fn set_af(&mut self, v: u16) {
        self.a = (v >> 8) as u8;
        self.f = (v as u8) & 0xF0;
    }
    pub fn bc(&self) -> u16 {
        u16::from_be_bytes([self.b, self.c])
    }
    pub fn set_bc(&mut self, v: u16) {
        self.b = (v >> 8) as u8;
        self.c = v as u8;
    }
    pub fn de(&self) -> u16 {
        u16::from_be_bytes([self.d, self.e])
    }
    pub fn set_de(&mut self, v: u16) {
        self.d = (v >> 8) as u8;
        self.e = v as u8;
    }
    pub fn hl(&self) -> u16 {
        u16::from_be_bytes([self.h, self.l])
    }
    pub fn set_hl(&mut self, v: u16) {
        self.h = (v >> 8) as u8;
        self.l = v as u8;
    }

    pub fn flag(&self, mask: u8) -> bool {
        self.f & mask != 0
    }
    pub fn set_flag(&mut self, mask: u8, on: bool) {
        if on {
            self.f |= mask;
        } else {
            self.f &= !mask;
        }
    }
    /// Set all four flags at once.
    pub fn set_flags(&mut self, z: bool, n: bool, h: bool, c: bool) {
        self.f = (if z { FLAG_Z } else { 0 })
            | (if n { FLAG_N } else { 0 })
            | (if h { FLAG_H } else { 0 })
            | (if c { FLAG_C } else { 0 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_boot_values() {
        let r = Registers::post_boot_dmg();
        assert_eq!(r.af(), 0x01B0);
        assert_eq!(r.bc(), 0x0013);
        assert_eq!(r.de(), 0x00D8);
        assert_eq!(r.hl(), 0x014D);
        assert_eq!((r.sp, r.pc), (0xFFFE, 0x0100));
    }

    #[test]
    fn f_low_nibble_is_always_zero() {
        let mut r = Registers::post_boot_dmg();
        r.set_af(0x12FF);
        assert_eq!(r.af(), 0x12F0);
        assert_eq!(r.f() & 0x0F, 0);
    }

    #[test]
    fn pair_accessors_round_trip() {
        let mut r = Registers::post_boot_dmg();
        r.set_bc(0xABCD);
        assert_eq!((r.b, r.c), (0xAB, 0xCD));
        r.set_de(0x1234);
        assert_eq!((r.d, r.e), (0x12, 0x34));
        r.set_hl(0xBEEF);
        assert_eq!((r.h, r.l), (0xBE, 0xEF));
        assert_eq!((r.bc(), r.de(), r.hl()), (0xABCD, 0x1234, 0xBEEF));
    }

    #[test]
    fn flag_helpers() {
        let mut r = Registers::post_boot_dmg();
        r.set_flags(true, false, true, false);
        assert_eq!(r.f(), FLAG_Z | FLAG_H);
        r.set_flag(FLAG_C, true);
        assert!(r.flag(FLAG_C) && r.flag(FLAG_Z) && !r.flag(FLAG_N));
        r.set_flag(FLAG_Z, false);
        assert!(!r.flag(FLAG_Z));
    }

    #[test]
    fn cgb_post_boot_values() {
        let r = Registers::post_boot_cgb();
        assert_eq!(
            (r.af(), r.bc(), r.de(), r.hl()),
            (0x1180, 0x0000, 0xFF56, 0x000D)
        );
        let r = Registers::post_boot_cgb_compat();
        assert_eq!(
            (r.af(), r.bc(), r.de(), r.hl()),
            (0x1180, 0x0000, 0x0008, 0x007C)
        );
        assert_eq!((r.sp, r.pc), (0xFFFE, 0x0100));
    }
}
