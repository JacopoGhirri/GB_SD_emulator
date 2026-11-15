//! Joypad register P1 at 0xFF00 (Pan Docs "Joypad Input").
//!
//! The eight buttons form a 2x4 matrix. Software writes bit 4 = 0 to select the directions
//! and/or bit 5 = 0 to select the action buttons, then reads the low nibble, where **0 means
//! pressed**. If both groups are selected the nibbles are ANDed. The joypad interrupt fires
//! when any of the four low bits goes from 1 to 0 (a selected button becomes pressed).

/// Which buttons are held. Bit set = pressed (the *register* uses the opposite, active-low).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Buttons(pub u8);

impl Buttons {
    pub const RIGHT: u8 = 1 << 0;
    pub const LEFT: u8 = 1 << 1;
    pub const UP: u8 = 1 << 2;
    pub const DOWN: u8 = 1 << 3;
    pub const A: u8 = 1 << 4;
    pub const B: u8 = 1 << 5;
    pub const SELECT: u8 = 1 << 6;
    pub const START: u8 = 1 << 7;
}

/// P1 bit 4 low selects the direction keys, bit 5 low selects the action buttons.
const SELECT_DIRECTIONS: u8 = 0x10;
const SELECT_ACTIONS: u8 = 0x20;
const SELECT_MASK: u8 = 0x30;
/// Unused upper bits read as 1.
const UNUSED_BITS: u8 = 0xC0;

#[derive(Debug, Clone, Default)]
pub struct Joypad {
    buttons: Buttons,
    /// Bits 4 and 5 as last written. Post-boot P1 is 0xCF, i.e. both groups selected (0).
    select: u8,
}

impl Joypad {
    pub fn new() -> Joypad {
        Joypad::default()
    }

    /// Post-boot state of CGB hardware: no group selected, so P1 reads 0xFF (the DMG boot ROM
    /// leaves 0xCF). Checked by Mooneye `misc/boot_hwio-C`.
    pub fn post_boot_cgb() -> Joypad {
        Joypad {
            select: SELECT_MASK,
            ..Joypad::default()
        }
    }

    /// Low nibble of P1 for the current selection; 0 = pressed.
    fn nibble(&self) -> u8 {
        let b = self.buttons.0;
        let mut pressed = 0;
        if self.select & SELECT_DIRECTIONS == 0 {
            pressed |= b & 0x0F;
        }
        if self.select & SELECT_ACTIONS == 0 {
            pressed |= b >> 4;
        }
        !pressed & 0x0F
    }

    pub fn read(&self) -> u8 {
        UNUSED_BITS | self.select | self.nibble()
    }

    /// Returns true if the write caused a high-to-low transition (joypad interrupt).
    pub fn write(&mut self, value: u8) -> bool {
        let before = self.nibble();
        self.select = value & SELECT_MASK;
        before & !self.nibble() != 0
    }

    /// Update the held buttons. Returns true if a selected line went high-to-low.
    pub fn set_buttons(&mut self, buttons: Buttons) -> bool {
        let before = self.nibble();
        self.buttons = buttons;
        before & !self.nibble() != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_boot_value_and_nothing_pressed() {
        assert_eq!(Joypad::new().read(), 0xCF);
    }

    #[test]
    fn directions_selected_by_bit_4_low() {
        let mut j = Joypad::new();
        j.set_buttons(Buttons(Buttons::RIGHT | Buttons::A));
        j.write(0x20); // bit 4 low: directions
        assert_eq!(j.read(), 0xC0 | 0x20 | 0b1110);
        j.write(0x10); // bit 5 low: actions
        assert_eq!(j.read(), 0xC0 | 0x10 | 0b1110);
    }

    #[test]
    fn nothing_selected_reads_all_released() {
        let mut j = Joypad::new();
        j.set_buttons(Buttons(0xFF));
        j.write(0x30);
        assert_eq!(j.read(), 0xFF);
    }

    #[test]
    fn both_groups_selected_are_combined() {
        let mut j = Joypad::new();
        j.set_buttons(Buttons(Buttons::UP | Buttons::START));
        j.write(0x00);
        assert_eq!(j.read(), 0xC0 | 0b0011); // UP = bit 2, START = bit 3 of the action nibble
    }

    #[test]
    fn interrupt_on_press_of_selected_button_only() {
        let mut j = Joypad::new();
        j.write(0x20); // directions
        assert!(!j.set_buttons(Buttons(Buttons::A)), "A is not selected");
        assert!(j.set_buttons(Buttons(Buttons::A | Buttons::LEFT)));
        assert!(
            !j.set_buttons(Buttons(Buttons::A | Buttons::LEFT)),
            "no new edge"
        );
        assert!(!j.set_buttons(Buttons(0)), "release is low-to-high");
    }

    #[test]
    fn interrupt_when_selecting_a_group_with_a_held_button() {
        let mut j = Joypad::new();
        j.write(0x30);
        j.set_buttons(Buttons(Buttons::B));
        assert!(j.write(0x10), "selecting actions exposes the held B");
    }
}
