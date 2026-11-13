//! Pure 8-bit ALU functions: no CPU state, so they can be tested exhaustively.
//! Flag rules: Pan Docs "CPU Instruction Set" (ADD/ADC/SUB/SBC/AND/XOR/OR/CP).

use super::registers::{FLAG_C, FLAG_H, FLAG_N, FLAG_Z};

fn flags(z: bool, n: bool, h: bool, c: bool) -> u8 {
    (if z { FLAG_Z } else { 0 })
        | (if n { FLAG_N } else { 0 })
        | (if h { FLAG_H } else { 0 })
        | (if c { FLAG_C } else { 0 })
}

/// ADD (carry_in = false) and ADC. Returns (result, flags).
pub fn add(a: u8, v: u8, carry_in: bool) -> (u8, u8) {
    let c = u8::from(carry_in);
    let result = a.wrapping_add(v).wrapping_add(c);
    // H: carry out of bit 3. C: carry out of bit 7.
    let h = (a & 0xF) + (v & 0xF) + c > 0xF;
    let carry = u16::from(a) + u16::from(v) + u16::from(c) > 0xFF;
    (result, flags(result == 0, false, h, carry))
}

/// SUB/CP (carry_in = false) and SBC. Returns (result, flags). H and C are *borrows*.
pub fn sub(a: u8, v: u8, carry_in: bool) -> (u8, u8) {
    let c = u8::from(carry_in);
    let result = a.wrapping_sub(v).wrapping_sub(c);
    let h = (a & 0xF) < (v & 0xF) + c;
    let borrow = u16::from(a) < u16::from(v) + u16::from(c);
    (result, flags(result == 0, true, h, borrow))
}

pub fn and(a: u8, v: u8) -> (u8, u8) {
    let r = a & v;
    (r, flags(r == 0, false, true, false)) // AND always sets H
}

pub fn xor(a: u8, v: u8) -> (u8, u8) {
    let r = a ^ v;
    (r, flags(r == 0, false, false, false))
}

pub fn or(a: u8, v: u8) -> (u8, u8) {
    let r = a | v;
    (r, flags(r == 0, false, false, false))
}

/// INC r. Returns (result, new flags given the old flags): C is not affected.
pub fn inc(v: u8, old_flags: u8) -> (u8, u8) {
    let r = v.wrapping_add(1);
    (
        r,
        flags(r == 0, false, v & 0xF == 0xF, old_flags & FLAG_C != 0),
    )
}

/// DEC r. C is not affected.
pub fn dec(v: u8, old_flags: u8) -> (u8, u8) {
    let r = v.wrapping_sub(1);
    (
        r,
        flags(r == 0, true, v & 0xF == 0, old_flags & FLAG_C != 0),
    )
}

/// Rotate/shift group, selected by `y` as in the CB opcode layout:
/// 0 RLC, 1 RRC, 2 RL, 3 RR, 4 SLA, 5 SRA, 6 SWAP, 7 SRL.
/// Returns (result, carry out). The caller decides Z: the CB forms set it from the result,
/// the accumulator forms (RLCA/RRCA/RLA/RRA) always clear it.
pub fn rot_shift(y: u8, v: u8, carry_in: bool) -> (u8, bool) {
    match y {
        0 => (v.rotate_left(1), v & 0x80 != 0),
        1 => (v.rotate_right(1), v & 1 != 0),
        2 => ((v << 1) | u8::from(carry_in), v & 0x80 != 0),
        3 => ((v >> 1) | (u8::from(carry_in) << 7), v & 1 != 0),
        4 => (v << 1, v & 0x80 != 0),
        // SRA keeps the sign bit (bit 7).
        5 => ((v >> 1) | (v & 0x80), v & 1 != 0),
        // SWAP exchanges the nibbles and clears carry.
        6 => (v.rotate_left(4), false),
        _ => (v >> 1, v & 1 != 0),
    }
}

/// DAA (Pan Docs "DAA"): adjust A to packed BCD after an add or subtract.
/// Returns (new A, new flags). H is cleared, N is preserved, Z comes from the result.
pub fn daa(a: u8, n: bool, h: bool, c: bool) -> (u8, u8) {
    let mut a = a;
    let mut carry = c;
    if !n {
        // After an addition.
        if c || a > 0x99 {
            a = a.wrapping_add(0x60);
            carry = true;
        }
        if h || (a & 0x0F) > 0x09 {
            a = a.wrapping_add(0x06);
        }
    } else {
        // After a subtraction only the flags say what to correct.
        if c {
            a = a.wrapping_sub(0x60);
        }
        if h {
            a = a.wrapping_sub(0x06);
        }
    }
    (a, flags(a == 0, n, false, carry))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Slow reference using wide integers and explicit nibble arithmetic.
    fn ref_add(a: u8, v: u8, c: u8) -> (u8, u8) {
        let full = i32::from(a) + i32::from(v) + i32::from(c);
        let low = i32::from(a & 0xF) + i32::from(v & 0xF) + i32::from(c);
        let r = (full & 0xFF) as u8;
        let mut f = 0;
        if r == 0 {
            f |= FLAG_Z;
        }
        if low > 0xF {
            f |= FLAG_H;
        }
        if full > 0xFF {
            f |= FLAG_C;
        }
        (r, f)
    }

    fn ref_sub(a: u8, v: u8, c: u8) -> (u8, u8) {
        let full = i32::from(a) - i32::from(v) - i32::from(c);
        let low = i32::from(a & 0xF) - i32::from(v & 0xF) - i32::from(c);
        let r = (full & 0xFF) as u8;
        let mut f = FLAG_N;
        if r == 0 {
            f |= FLAG_Z;
        }
        if low < 0 {
            f |= FLAG_H;
        }
        if full < 0 {
            f |= FLAG_C;
        }
        (r, f)
    }

    #[test]
    fn add_adc_match_reference_for_all_operands() {
        for a in 0..=255u8 {
            for v in 0..=255u8 {
                for c in 0..=1u8 {
                    assert_eq!(
                        add(a, v, c == 1),
                        ref_add(a, v, c),
                        "a={a:#x} v={v:#x} c={c}"
                    );
                }
            }
        }
    }

    #[test]
    fn sub_sbc_match_reference_for_all_operands() {
        for a in 0..=255u8 {
            for v in 0..=255u8 {
                for c in 0..=1u8 {
                    assert_eq!(
                        sub(a, v, c == 1),
                        ref_sub(a, v, c),
                        "a={a:#x} v={v:#x} c={c}"
                    );
                }
            }
        }
    }

    #[test]
    fn logic_ops_flags() {
        assert_eq!(and(0xF0, 0x0F), (0, FLAG_Z | FLAG_H));
        assert_eq!(and(0xFF, 0x81), (0x81, FLAG_H));
        assert_eq!(xor(0xFF, 0xFF), (0, FLAG_Z));
        assert_eq!(or(0x00, 0x00), (0, FLAG_Z));
        assert_eq!(or(0x01, 0x80), (0x81, 0));
    }

    #[test]
    fn inc_dec_keep_carry_and_set_half_carry() {
        assert_eq!(inc(0x0F, FLAG_C), (0x10, FLAG_H | FLAG_C));
        assert_eq!(inc(0xFF, 0), (0x00, FLAG_Z | FLAG_H));
        assert_eq!(dec(0x10, 0), (0x0F, FLAG_N | FLAG_H));
        assert_eq!(dec(0x01, FLAG_C), (0x00, FLAG_Z | FLAG_N | FLAG_C));
        assert_eq!(dec(0x00, 0), (0xFF, FLAG_N | FLAG_H));
    }
}
