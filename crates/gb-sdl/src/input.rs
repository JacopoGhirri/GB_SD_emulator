//! Input mapping: keyboard and gamepad -> Game Boy buttons. Pure functions over bit masks
//! so they can be unit-tested without SDL devices.

use gb_core::Buttons;
use sdl2::controller::{Axis, Button};
use sdl2::keyboard::Keycode;

// ---- Mapping tables (edit here) -------------------------------------------------------
pub const KEY_MAP: &[(Keycode, u8)] = &[
    (Keycode::Right, Buttons::RIGHT),
    (Keycode::Left, Buttons::LEFT),
    (Keycode::Up, Buttons::UP),
    (Keycode::Down, Buttons::DOWN),
    (Keycode::Z, Buttons::B),
    (Keycode::X, Buttons::A),
    (Keycode::Return, Buttons::START),
    (Keycode::Backspace, Buttons::SELECT),
    (Keycode::RShift, Buttons::SELECT),
];
pub const KEY_QUIT: Keycode = Keycode::Escape;
pub const KEY_FULLSCREEN: Keycode = Keycode::F11;
pub const KEY_FAST_FORWARD: Keycode = Keycode::Tab;

/// Gamepad buttons that do not depend on the A/B swap. SDL names face buttons by physical
/// position (A = south, B = east), independent of the printed label.
pub const PAD_MAP: &[(Button, u8)] = &[
    (Button::DPadRight, Buttons::RIGHT),
    (Button::DPadLeft, Buttons::LEFT),
    (Button::DPadUp, Buttons::UP),
    (Button::DPadDown, Buttons::DOWN),
    (Button::Start, Buttons::START),
    (Button::Back, Buttons::SELECT),
];
/// Left stick dead zone (of 32767).
pub const STICK_DEADZONE: i16 = 12_000;
// ---------------------------------------------------------------------------------------

pub fn key_bit(key: Keycode) -> u8 {
    KEY_MAP
        .iter()
        .find(|(k, _)| *k == key)
        .map_or(0, |(_, b)| *b)
}

/// Face buttons. Default (phase 4 spec): east -> GB B, south -> GB A, so the buttons sit where
/// the Game Boy's own do. `swap_ab` exchanges them. Guide is never mapped (left to Steam).
pub fn face_bit(button: Button, swap_ab: bool) -> u8 {
    let (east, south) = if swap_ab {
        (Buttons::A, Buttons::B)
    } else {
        (Buttons::B, Buttons::A)
    };
    match button {
        Button::B => east,
        Button::A => south,
        _ => 0,
    }
}

pub fn pad_bit(button: Button, swap_ab: bool) -> u8 {
    PAD_MAP
        .iter()
        .find(|(b, _)| *b == button)
        .map_or_else(|| face_bit(button, swap_ab), |(_, bit)| *bit)
}

/// Left stick -> D-pad bits (SDL y axis is positive downwards).
pub fn stick_bits(x: i16, y: i16) -> u8 {
    let mut bits = 0;
    if x > STICK_DEADZONE {
        bits |= Buttons::RIGHT;
    } else if x < -STICK_DEADZONE {
        bits |= Buttons::LEFT;
    }
    if y > STICK_DEADZONE {
        bits |= Buttons::DOWN;
    } else if y < -STICK_DEADZONE {
        bits |= Buttons::UP;
    }
    bits
}

/// Real hardware cannot press Left+Right or Up+Down together, and some games glitch when they
/// see it. Choice: when both of an opposing pair are held, report neither.
pub fn filter_opposites(bits: u8) -> u8 {
    let mut b = bits;
    if b & Buttons::LEFT != 0 && b & Buttons::RIGHT != 0 {
        b &= !(Buttons::LEFT | Buttons::RIGHT);
    }
    if b & Buttons::UP != 0 && b & Buttons::DOWN != 0 {
        b &= !(Buttons::UP | Buttons::DOWN);
    }
    b
}

/// Held-input state, combined into one `Buttons` per frame.
#[derive(Default)]
pub struct InputState {
    keys: u8,
    pad_buttons: u8,
    stick_x: i16,
    stick_y: i16,
}

impl InputState {
    pub fn key(&mut self, key: Keycode, down: bool) {
        set(&mut self.keys, key_bit(key), down);
    }
    pub fn pad_button(&mut self, button: Button, down: bool, swap_ab: bool) {
        set(&mut self.pad_buttons, pad_bit(button, swap_ab), down);
    }
    pub fn pad_axis(&mut self, axis: Axis, value: i16) {
        match axis {
            Axis::LeftX => self.stick_x = value,
            Axis::LeftY => self.stick_y = value,
            _ => {}
        }
    }
    /// Forget gamepad state (controller unplugged) so no button stays stuck.
    pub fn clear_pad(&mut self) {
        self.pad_buttons = 0;
        self.stick_x = 0;
        self.stick_y = 0;
    }
    pub fn buttons(&self) -> Buttons {
        Buttons(filter_opposites(
            self.keys | self.pad_buttons | stick_bits(self.stick_x, self.stick_y),
        ))
    }
}

fn set(bits: &mut u8, mask: u8, down: bool) {
    if down {
        *bits |= mask;
    } else {
        *bits &= !mask;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_mapping() {
        assert_eq!(key_bit(Keycode::Z), Buttons::B);
        assert_eq!(key_bit(Keycode::X), Buttons::A);
        assert_eq!(key_bit(Keycode::RShift), Buttons::SELECT);
        assert_eq!(key_bit(Keycode::Q), 0);
    }

    #[test]
    fn face_buttons_default_and_swapped() {
        assert_eq!(face_bit(Button::B, false), Buttons::B);
        assert_eq!(face_bit(Button::A, false), Buttons::A);
        assert_eq!(face_bit(Button::B, true), Buttons::A);
        assert_eq!(face_bit(Button::A, true), Buttons::B);
        assert_eq!(face_bit(Button::Guide, false), 0);
    }

    #[test]
    fn stick_deadzone_and_directions() {
        assert_eq!(stick_bits(0, 0), 0);
        assert_eq!(stick_bits(STICK_DEADZONE, 0), 0);
        assert_eq!(stick_bits(20_000, -20_000), Buttons::RIGHT | Buttons::UP);
        assert_eq!(stick_bits(-20_000, 20_000), Buttons::LEFT | Buttons::DOWN);
    }

    #[test]
    fn opposing_directions_cancel() {
        let both = Buttons::LEFT | Buttons::RIGHT | Buttons::A;
        assert_eq!(filter_opposites(both), Buttons::A);
        assert_eq!(filter_opposites(Buttons::UP | Buttons::DOWN), 0);
        assert_eq!(
            filter_opposites(Buttons::UP | Buttons::LEFT),
            Buttons::UP | Buttons::LEFT
        );
    }

    #[test]
    fn keyboard_and_pad_combine_and_pad_clears() {
        let mut s = InputState::default();
        s.key(Keycode::X, true);
        s.pad_button(Button::DPadLeft, true, false);
        assert_eq!(s.buttons().0, Buttons::A | Buttons::LEFT);
        s.clear_pad();
        assert_eq!(s.buttons().0, Buttons::A);
        s.key(Keycode::X, false);
        assert_eq!(s.buttons().0, 0);
    }
}
