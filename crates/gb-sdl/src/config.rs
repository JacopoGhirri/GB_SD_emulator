//! Settings file and input bindings.
//!
//! The file is plain `key = value` lines (no dependency, no TOML): `#` or `;` starts a comment,
//! blank lines are ignored. Precedence is command-line flags > config file > built-in defaults
//! (the merge happens in `main.rs`). Every problem in the file is reported with its line number
//! and the program refuses to start, rather than silently ignoring a typo.
//!
//! ```text
//! scale = 5
//! fullscreen = false
//! swap_ab = false             # Nintendo layout on the gamepad (south = B)
//! volume = 0.8                # 0.0 - 1.0
//! fast_forward_speed = 0      # 0 = as fast as possible, otherwise a multiple (2 = double speed)
//! slow_mo_speed = 0.25        # while the slow-motion key is held
//! key.fast_forward = F, Tab   # keyboard: comma-separated SDL key names
//! pad.fast_forward = righttrigger
//! ```
//! Bindable names: `right left up down a b start select quit fullscreen fast_forward slow_mo
//! pause reset screenshot save_state load_state next_slot`.
//! A `key.NAME` / `pad.NAME` line *replaces* that control's default bindings.
//! Pad names are SDL's: `a b x y back start leftshoulder rightshoulder dpup dpdown dpleft
//! dpright leftstick rightstick lefttrigger righttrigger`.

use std::path::{Path, PathBuf};

use gb_core::Buttons;
use sdl2::controller::{Axis, Button};
use sdl2::keyboard::Keycode;

/// Things the frontend can be asked to do (as opposed to Game Boy buttons).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Quit,
    Fullscreen,
    /// Held: runs at `fast_forward_speed` while pressed.
    FastForward,
    /// Held: runs at `slow_mo_speed` while pressed.
    SlowMo,
    Pause,
    /// Restart the game as if the console were reset. Battery saves are kept.
    Reset,
    Screenshot,
    /// Quick save / quick load into the current slot.
    SaveState,
    LoadState,
    /// Cycle the current slot 1..=9.
    NextSlot,
}

/// What a key or pad input does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    Button(u8),
    Action(Action),
}

/// A gamepad input. Triggers are *axes* in SDL (not buttons), so they need their own variant.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PadInput {
    Button(Button),
    Trigger(Axis),
}

/// One row per bindable control: config name, target, default keys, default pad inputs.
/// The face buttons' pad defaults are swapped by `swap_ab` (see `Bindings::new`).
type ControlRow = (
    &'static str,
    Target,
    &'static [&'static str],
    &'static [&'static str],
);
const CONTROLS: &[ControlRow] = &[
    (
        "right",
        Target::Button(Buttons::RIGHT),
        &["Right"],
        &["dpright"],
    ),
    (
        "left",
        Target::Button(Buttons::LEFT),
        &["Left"],
        &["dpleft"],
    ),
    ("up", Target::Button(Buttons::UP), &["Up"], &["dpup"]),
    (
        "down",
        Target::Button(Buttons::DOWN),
        &["Down"],
        &["dpdown"],
    ),
    ("a", Target::Button(Buttons::A), &["X"], &["a"]),
    ("b", Target::Button(Buttons::B), &["Z"], &["b"]),
    (
        "start",
        Target::Button(Buttons::START),
        &["Return"],
        &["start"],
    ),
    (
        "select",
        Target::Button(Buttons::SELECT),
        &["Backspace", "Right Shift"],
        &["back"],
    ),
    ("quit", Target::Action(Action::Quit), &["Escape"], &[]),
    (
        "fullscreen",
        Target::Action(Action::Fullscreen),
        &["F11"],
        &[],
    ),
    (
        "fast_forward",
        Target::Action(Action::FastForward),
        &["F"],
        &["righttrigger"],
    ),
    ("slow_mo", Target::Action(Action::SlowMo), &["S"], &[]),
    ("pause", Target::Action(Action::Pause), &["P"], &[]),
    ("reset", Target::Action(Action::Reset), &["F10"], &[]),
    (
        "screenshot",
        Target::Action(Action::Screenshot),
        &["F12"],
        &[],
    ),
    (
        "save_state",
        Target::Action(Action::SaveState),
        &["F5"],
        &[],
    ),
    (
        "load_state",
        Target::Action(Action::LoadState),
        &["F8"],
        &[],
    ),
    ("next_slot", Target::Action(Action::NextSlot), &["F6"], &[]),
];

#[derive(Clone, Debug)]
pub struct Config {
    pub scale: u32,
    pub fullscreen: bool,
    pub swap_ab: bool,
    pub volume: f32,
    /// Multiple of normal speed while fast-forward is held; 0 = uncapped.
    pub fast_forward_speed: f64,
    /// Multiple of normal speed while slow-motion is held (0.1 to 1).
    pub slow_mo_speed: f64,
    pub(crate) key_overrides: Vec<(&'static str, Vec<Keycode>)>,
    pub(crate) pad_overrides: Vec<(&'static str, Vec<PadInput>)>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            scale: 4,
            fullscreen: false,
            swap_ab: false,
            volume: 1.0,
            fast_forward_speed: 0.0,
            slow_mo_speed: 0.25,
            key_overrides: Vec::new(),
            pad_overrides: Vec::new(),
        }
    }
}

impl Config {
    /// Parse the text of a config file. All errors are collected (one per bad line).
    pub fn parse(text: &str) -> Result<Config, Vec<String>> {
        let mut cfg = Config::default();
        let mut errors = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for (i, raw) in text.lines().enumerate() {
            let line = raw.split(['#', ';']).next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let n = i + 1;
            let Some((key, value)) = line.split_once('=') else {
                errors.push(format!("line {n}: expected `key = value`, got `{line}`"));
                continue;
            };
            let (key, value) = (key.trim().to_ascii_lowercase(), value.trim());
            if seen.contains(&key) {
                errors.push(format!("line {n}: `{key}` is set twice"));
                continue;
            }
            seen.push(key.clone());
            if let Err(e) = cfg.set(&key, value) {
                errors.push(format!("line {n}: {e}"));
            }
        }
        if errors.is_empty() {
            Ok(cfg)
        } else {
            Err(errors)
        }
    }

    fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        match key {
            "scale" => {
                self.scale = value
                    .parse::<u32>()
                    .ok()
                    .filter(|s| (1..=20).contains(s))
                    .ok_or("scale must be an integer from 1 to 20")?;
            }
            "fullscreen" => self.fullscreen = parse_bool(key, value)?,
            "swap_ab" => self.swap_ab = parse_bool(key, value)?,
            "volume" => {
                self.volume = value
                    .parse::<f32>()
                    .ok()
                    .filter(|v| (0.0..=1.0).contains(v))
                    .ok_or("volume must be a number from 0.0 to 1.0")?;
            }
            "fast_forward_speed" => {
                self.fast_forward_speed = value
                    .parse::<f64>()
                    .ok()
                    .filter(|v| *v == 0.0 || (1.0..=100.0).contains(v))
                    .ok_or("fast_forward_speed must be 0 (uncapped) or from 1 to 100")?;
            }
            "slow_mo_speed" => {
                self.slow_mo_speed = value
                    .parse::<f64>()
                    .ok()
                    .filter(|v| (0.1..=1.0).contains(v))
                    .ok_or("slow_mo_speed must be from 0.1 to 1")?;
            }
            _ => {
                if let Some(name) = key.strip_prefix("key.") {
                    let row = control(name)?;
                    let keys = value
                        .split(',')
                        .map(|k| {
                            Keycode::from_name(k.trim()).ok_or_else(|| {
                                format!("unknown key name `{}` in `{key}`", k.trim())
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    self.key_overrides.push((row.0, keys));
                } else if let Some(name) = key.strip_prefix("pad.") {
                    let row = control(name)?;
                    let pads = value
                        .split(',')
                        .filter(|p| !p.trim().is_empty())
                        .map(|p| parse_pad(p.trim()))
                        .collect::<Result<Vec<_>, _>>()?;
                    self.pad_overrides.push((row.0, pads));
                } else {
                    return Err(format!("unknown setting `{key}`"));
                }
            }
        }
        Ok(())
    }

    /// Load the config. With an explicit path the file must exist; without one, look for
    /// `config.ini` beside the program, then in `$XDG_CONFIG_HOME` / `~/.config/gb-sdl/`.
    /// No file at all is fine (defaults). Returns the path that was used.
    pub fn load(explicit: Option<&Path>) -> Result<(Config, Option<PathBuf>), Vec<String>> {
        let path = match explicit {
            Some(p) => Some(p.to_path_buf()),
            None => default_locations().into_iter().find(|p| p.is_file()),
        };
        let Some(path) = path else {
            return Ok((Config::default(), None));
        };
        let text = std::fs::read_to_string(&path)
            .map_err(|e| vec![format!("cannot read config {}: {e}", path.display())])?;
        let cfg = Config::parse(&text).map_err(|errs| {
            errs.into_iter()
                .map(|e| format!("{}: {e}", path.display()))
                .collect::<Vec<_>>()
        })?;
        Ok((cfg, Some(path)))
    }
}

fn default_locations() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        v.push(dir.join("config.ini"));
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    if let Some(base) = base {
        v.push(base.join("gb-sdl").join("config.ini"));
    }
    v
}

fn parse_bool(key: &str, v: &str) -> Result<bool, String> {
    match v.to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Ok(true),
        "false" | "no" | "off" | "0" => Ok(false),
        _ => Err(format!("{key} must be true or false")),
    }
}

fn control(name: &str) -> Result<&'static ControlRow, String> {
    CONTROLS.iter().find(|c| c.0 == name).ok_or_else(|| {
        let names: Vec<&str> = CONTROLS.iter().map(|c| c.0).collect();
        format!("unknown control `{name}` (known: {})", names.join(" "))
    })
}

fn parse_pad(name: &str) -> Result<PadInput, String> {
    let lower = name.to_ascii_lowercase();
    if let Some(b) = Button::from_string(&lower) {
        return Ok(PadInput::Button(b));
    }
    match Axis::from_string(&lower) {
        Some(a @ (Axis::TriggerLeft | Axis::TriggerRight)) => Ok(PadInput::Trigger(a)),
        _ => Err(format!("unknown gamepad input `{name}`")),
    }
}

/// The resolved key and pad tables used at run time.
#[derive(Clone, Debug)]
pub struct Bindings {
    pub keys: Vec<(Keycode, Target)>,
    pub pads: Vec<(PadInput, Target)>,
}

impl Bindings {
    pub fn new(cfg: &Config) -> Bindings {
        let mut keys = Vec::new();
        let mut pads = Vec::new();
        for &(name, target, default_keys, default_pads) in CONTROLS {
            match cfg.key_overrides.iter().find(|o| o.0 == name) {
                Some((_, list)) => keys.extend(list.iter().map(|k| (*k, target))),
                None => keys.extend(
                    default_keys
                        .iter()
                        .filter_map(|k| Keycode::from_name(k))
                        .map(|k| (k, target)),
                ),
            }
            match cfg.pad_overrides.iter().find(|o| o.0 == name) {
                Some((_, list)) => pads.extend(list.iter().map(|p| (*p, target))),
                None => {
                    for p in default_pads {
                        // SDL names face buttons by physical position (south = "a"). By default
                        // the Deck's A (south) is the Game Boy A; `swap_ab` gives the Nintendo
                        // layout (GB A is the right-hand button) by trading the two defaults.
                        let p = match (cfg.swap_ab, name, *p) {
                            (true, "a", "a") => "b",
                            (true, "b", "b") => "a",
                            (_, _, p) => p,
                        };
                        if let Ok(p) = parse_pad(p) {
                            pads.push((p, target));
                        }
                    }
                }
            }
        }
        Bindings { keys, pads }
    }

    pub fn key(&self, key: Keycode) -> Option<Target> {
        self.keys.iter().find(|(k, _)| *k == key).map(|(_, t)| *t)
    }

    pub fn pad(&self, input: PadInput) -> Option<Target> {
        self.pads.iter().find(|(p, _)| *p == input).map(|(_, t)| *t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_gives_defaults() {
        let c = Config::parse("").unwrap();
        assert_eq!((c.scale, c.fullscreen, c.swap_ab), (4, false, false));
        assert_eq!(c.volume, 1.0);
        assert_eq!(c.fast_forward_speed, 0.0);
    }

    #[test]
    fn values_comments_and_case() {
        let c = Config::parse(
            "# a comment\n  Scale = 6  ; trailing\nfullscreen=YES\nvolume = 0.5\nfast_forward_speed = 4\n",
        )
        .unwrap();
        assert_eq!((c.scale, c.fullscreen), (6, true));
        assert_eq!((c.volume, c.fast_forward_speed), (0.5, 4.0));
    }

    #[test]
    fn every_bad_line_is_reported_with_its_number() {
        let errs = Config::parse(
            "scale = 0\nbogus = 1\nvolume = loud\nno equals sign\nscale = 2\nkey.nothing = A\nfast_forward_speed = 0.5\n",
        )
        .unwrap_err();
        let all = errs.join("\n");
        assert!(all.contains("line 1: scale must be"), "{all}");
        assert!(all.contains("line 2: unknown setting `bogus`"), "{all}");
        assert!(all.contains("line 3: volume must be"), "{all}");
        assert!(all.contains("line 4: expected `key = value`"), "{all}");
        assert!(all.contains("line 5: `scale` is set twice"), "{all}");
        assert!(all.contains("line 6: unknown control `nothing`"), "{all}");
        assert!(all.contains("line 7: fast_forward_speed must be"), "{all}");
        assert_eq!(errs.len(), 7);
    }

    #[test]
    fn new_action_defaults_and_slow_mo_range() {
        let b = Bindings::new(&Config::default());
        for (k, a) in [
            (Keycode::P, Action::Pause),
            (Keycode::F10, Action::Reset),
            (Keycode::F12, Action::Screenshot),
            (Keycode::F5, Action::SaveState),
            (Keycode::F8, Action::LoadState),
            (Keycode::F6, Action::NextSlot),
            (Keycode::S, Action::SlowMo),
        ] {
            assert_eq!(b.key(k), Some(Target::Action(a)), "{k:?}");
        }
        assert_eq!(Config::default().slow_mo_speed, 0.25);
        assert!(Config::parse("slow_mo_speed = 0.5").is_ok());
        assert!(Config::parse("slow_mo_speed = 0").is_err());
        assert!(Config::parse("slow_mo_speed = 2").is_err());
    }

    #[test]
    fn extreme_numbers_are_rejected_not_clamped() {
        for bad in [
            "scale = 21",
            "scale = -1",
            "volume = 1.5",
            "volume = nan",
            "fast_forward_speed = inf",
        ] {
            assert!(Config::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn default_bindings_resolve_and_all_default_names_are_valid() {
        let b = Bindings::new(&Config::default());
        assert_eq!(b.key(Keycode::Z), Some(Target::Button(Buttons::B)));
        assert_eq!(b.key(Keycode::X), Some(Target::Button(Buttons::A)));
        assert_eq!(
            b.key(Keycode::RShift),
            Some(Target::Button(Buttons::SELECT))
        );
        assert_eq!(b.key(Keycode::Escape), Some(Target::Action(Action::Quit)));
        assert_eq!(b.key(Keycode::F), Some(Target::Action(Action::FastForward)));
        assert_eq!(b.key(Keycode::Tab), None, "Tab is no longer fast-forward");
        assert_eq!(
            b.pad(PadInput::Trigger(Axis::TriggerRight)),
            Some(Target::Action(Action::FastForward))
        );
        // Every built-in name must parse, or the default would silently vanish.
        let keys: usize = CONTROLS.iter().map(|c| c.2.len()).sum();
        let pads: usize = CONTROLS.iter().map(|c| c.3.len()).sum();
        assert_eq!((b.keys.len(), b.pads.len()), (keys, pads));
    }

    #[test]
    fn swap_ab_trades_the_face_button_defaults() {
        let plain = Bindings::new(&Config::default());
        assert_eq!(
            plain.pad(PadInput::Button(Button::B)),
            Some(Target::Button(Buttons::B))
        );
        assert_eq!(
            plain.pad(PadInput::Button(Button::A)),
            Some(Target::Button(Buttons::A))
        );
        let swapped_cfg = Config {
            swap_ab: true,
            ..Config::default()
        };
        let swapped = Bindings::new(&swapped_cfg);
        assert_eq!(
            swapped.pad(PadInput::Button(Button::B)),
            Some(Target::Button(Buttons::A))
        );
        assert_eq!(
            swapped.pad(PadInput::Button(Button::A)),
            Some(Target::Button(Buttons::B))
        );
    }

    #[test]
    fn overrides_replace_the_defaults() {
        let c = Config::parse(
            "key.fast_forward = Tab, Q\npad.fast_forward = rightshoulder\nkey.quit = \n",
        );
        // `key.quit =` with no key names is an error: it would leave no way to quit by key.
        assert!(c.is_err());
        let c =
            Config::parse("key.fast_forward = Tab, Q\npad.fast_forward = rightshoulder\n").unwrap();
        let b = Bindings::new(&c);
        assert_eq!(
            b.key(Keycode::Tab),
            Some(Target::Action(Action::FastForward))
        );
        assert_eq!(b.key(Keycode::Q), Some(Target::Action(Action::FastForward)));
        assert_eq!(b.key(Keycode::F), None);
        assert_eq!(b.pad(PadInput::Trigger(Axis::TriggerRight)), None);
        assert_eq!(
            b.pad(PadInput::Button(Button::RightShoulder)),
            Some(Target::Action(Action::FastForward))
        );
    }

    #[test]
    fn unknown_key_and_pad_names_are_errors() {
        assert!(Config::parse("key.a = NotAKey").is_err());
        assert!(Config::parse("pad.a = lefty").is_err());
        assert!(
            Config::parse("pad.a = leftx").is_err(),
            "a stick axis is not a button"
        );
        assert!(Config::parse("pad.a = lefttrigger").is_ok());
    }

    #[test]
    fn missing_explicit_file_is_an_error() {
        let r = Config::load(Some(Path::new("/definitely/not/here.ini")));
        assert!(r.unwrap_err()[0].contains("cannot read config"));
    }
}
