//! Input mapping: keyboard and gamepad -> Game Boy buttons. Pure functions over bit masks
//! so they can be unit-tested without SDL devices.

use crate::config::{Action, Bindings, Target};
use gb_core::Buttons;
use sdl2::controller::{Axis, Button};
use sdl2::keyboard::Keycode;

/// Keys 1-4 mute/unmute sound channels 1-4 (debugging aid). Returns the channel index 0..=3.
pub fn mute_channel(key: Keycode) -> Option<u8> {
    match key {
        Keycode::Num1 => Some(0),
        Keycode::Num2 => Some(1),
        Keycode::Num3 => Some(2),
        Keycode::Num4 => Some(3),
        _ => None,
    }
}

/// `+`/`=` raise and `-` lowers the volume (Some(true) = up).
pub fn volume_key(key: Keycode) -> Option<bool> {
    match key {
        Keycode::Plus | Keycode::Equals | Keycode::KpPlus => Some(true),
        Keycode::Minus | Keycode::KpMinus => Some(false),
        _ => None,
    }
}

/// Left stick dead zone (of 32767).
pub const STICK_DEADZONE: i16 = 12_000;

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

/// An action key/button went down (`true`) or up (`false`).
pub type ActionEvent = (Action, bool);

/// Held-input state, combined into one `Buttons` per frame.
pub struct InputState {
    bindings: Bindings,
    keys: u8,
    pad_buttons: u8,
    stick_x: i16,
    stick_y: i16,
    /// Pad inputs currently holding an action down, so unplugging can release them.
    held_pad_actions: Vec<Action>,
}

impl InputState {
    pub fn new(bindings: Bindings) -> InputState {
        InputState {
            bindings,
            keys: 0,
            pad_buttons: 0,
            stick_x: 0,
            stick_y: 0,
            held_pad_actions: Vec::new(),
        }
    }

    pub fn key(&mut self, key: Keycode, down: bool) -> Option<ActionEvent> {
        match self.bindings.key(key)? {
            Target::Button(mask) => {
                set(&mut self.keys, mask, down);
                None
            }
            Target::Action(a) => Some((a, down)),
        }
    }

    pub fn pad_button(&mut self, button: Button, down: bool) -> Option<ActionEvent> {
        match self.bindings.pad(button)? {
            Target::Button(mask) => {
                set(&mut self.pad_buttons, mask, down);
                None
            }
            Target::Action(a) => {
                self.held_pad_actions.retain(|h| *h != a);
                if down {
                    self.held_pad_actions.push(a);
                }
                Some((a, down))
            }
        }
    }

    /// The left stick acts as a second D-pad; other axes (triggers, right stick) are ignored.
    pub fn pad_axis(&mut self, axis: Axis, value: i16) {
        match axis {
            Axis::LeftX => self.stick_x = value,
            Axis::LeftY => self.stick_y = value,
            _ => {}
        }
    }

    /// Forget gamepad state (controller unplugged) so no button stays stuck. Returns release
    /// events for any frontend action (e.g. slow motion on a pad button) that was held.
    pub fn clear_pad(&mut self) -> Vec<ActionEvent> {
        self.pad_buttons = 0;
        self.stick_x = 0;
        self.stick_y = 0;
        self.held_pad_actions
            .drain(..)
            .map(|a| (a, false))
            .collect()
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

    use crate::config::Config;

    fn state(cfg: &Config) -> InputState {
        InputState::new(Bindings::new(cfg))
    }

    #[test]
    fn keyboard_maps_to_buttons_and_actions() {
        let mut s = state(&Config::default());
        assert_eq!(s.key(Keycode::Z, true), None);
        assert_eq!(s.buttons().0, Buttons::B);
        assert_eq!(s.key(Keycode::Z, false), None);
        assert_eq!(s.key(Keycode::Q, true), None, "unbound key");
        assert_eq!(s.key(Keycode::S, true), Some((Action::SlowMo, true)));
        assert_eq!(s.key(Keycode::S, false), Some((Action::SlowMo, false)));
        assert_eq!(s.key(Keycode::F, true), None, "F is not bound to anything");
        assert_eq!(
            s.key(Keycode::Tab, true),
            None,
            "Tab is not bound to anything"
        );
        assert_eq!(s.key(Keycode::Escape, true), Some((Action::Quit, true)));
    }

    #[test]
    fn face_buttons_default_and_swapped() {
        let mut s = state(&Config::default());
        s.pad_button(Button::B, true);
        assert_eq!(s.buttons().0, Buttons::B);
        let cfg = Config {
            swap_ab: true,
            ..Config::default()
        };
        let mut s = state(&cfg);
        s.pad_button(Button::B, true);
        assert_eq!(s.buttons().0, Buttons::A);
        assert_eq!(s.pad_button(Button::Guide, true), None);
        assert_eq!(s.buttons().0, Buttons::A, "Guide is never mapped");
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
        let mut s = state(&Config::default());
        s.key(Keycode::X, true);
        s.pad_button(Button::DPadLeft, true);
        assert_eq!(s.buttons().0, Buttons::A | Buttons::LEFT);
        s.clear_pad();
        assert_eq!(s.buttons().0, Buttons::A);
        s.key(Keycode::X, false);
        assert_eq!(s.buttons().0, 0);
    }

    #[test]
    fn the_left_stick_moves_the_dpad_and_triggers_do_nothing() {
        let mut s = state(&Config::default());
        s.pad_axis(Axis::LeftX, 30_000);
        s.pad_axis(Axis::TriggerRight, 32_000);
        s.pad_axis(Axis::TriggerLeft, 32_000);
        assert_eq!(s.buttons().0, Buttons::RIGHT);
        s.pad_axis(Axis::LeftX, 0);
        assert_eq!(s.buttons().0, 0);
    }

    #[test]
    fn unplugging_releases_a_held_pad_action() {
        // Slow motion bound to a pad button (not a default) must not stay stuck on unplug.
        let cfg = Config::parse("pad.slow_mo = x").unwrap();
        let mut s = state(&cfg);
        assert_eq!(s.pad_button(Button::X, true), Some((Action::SlowMo, true)));
        assert_eq!(s.clear_pad(), vec![(Action::SlowMo, false)]);
        assert!(s.clear_pad().is_empty());
    }
}
