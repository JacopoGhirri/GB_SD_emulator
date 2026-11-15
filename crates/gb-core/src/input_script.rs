//! Input scripts: a text format recording which buttons are held on which frame.
//!
//! The emulator is deterministic (same ROM + same inputs => same output), so a recorded
//! playthrough replays identically. `gb-sdl --record` writes this format, `gb-cli --input` and
//! the local ROM tests read it. This module only parses and formats text; file I/O lives in the
//! frontends.
//!
//! ```text
//! # comments start with '#'
//! 120 A+START      from frame 120 on, A and Start are held
//! 130 -            from frame 130 on, nothing is held
//! # end 5000       optional: the recording lasted 5000 frames
//! ```
//!
//! Frame `n` is the n-th call to `run_frame` since boot, counting from 0. The buttons listed for
//! frame `n` are set *before* that frame runs. Before the first line nothing is held.

use crate::joypad::Buttons;

/// Button names, in the order they are written.
const NAMES: [(&str, u8); 8] = [
    ("RIGHT", Buttons::RIGHT),
    ("LEFT", Buttons::LEFT),
    ("UP", Buttons::UP),
    ("DOWN", Buttons::DOWN),
    ("A", Buttons::A),
    ("B", Buttons::B),
    ("SELECT", Buttons::SELECT),
    ("START", Buttons::START),
];

/// Format a button mask as `A+START`, or `-` for nothing held.
pub fn format_buttons(mask: u8) -> String {
    let names: Vec<&str> = NAMES
        .iter()
        .filter(|(_, bit)| mask & bit != 0)
        .map(|(name, _)| *name)
        .collect();
    if names.is_empty() {
        "-".to_string()
    } else {
        names.join("+")
    }
}

/// Parse `A+START` (case-insensitive) or `-` into a button mask.
pub fn parse_buttons(text: &str) -> Result<u8, String> {
    if text == "-" {
        return Ok(0);
    }
    let mut mask = 0;
    for part in text.split('+') {
        let (_, bit) = NAMES
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(part))
            .ok_or_else(|| format!("unknown button {part:?}"))?;
        mask |= bit;
    }
    Ok(mask)
}

/// A parsed script: button changes sorted by frame, plus the optional recorded length.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputScript {
    changes: Vec<(u64, u8)>,
    end: Option<u64>,
}

impl InputScript {
    /// Parse script text. Errors name the offending line (1-based).
    pub fn parse(text: &str) -> Result<InputScript, String> {
        let mut script = InputScript::default();
        for (i, raw) in text.lines().enumerate() {
            let line = raw.trim();
            let err = |msg: String| format!("line {}: {msg}", i + 1);
            if let Some(comment) = line.strip_prefix('#') {
                if let Some(n) = comment.trim().strip_prefix("end ") {
                    script.end = Some(
                        n.trim()
                            .parse()
                            .map_err(|_| err("bad frame count after \"# end\"".into()))?,
                    );
                }
                continue;
            }
            if line.is_empty() {
                continue;
            }
            let mut fields = line.split_whitespace();
            let (Some(frame), Some(buttons), None) = (fields.next(), fields.next(), fields.next())
            else {
                return Err(err("expected \"<frame> <buttons>\"".into()));
            };
            let frame: u64 = frame
                .parse()
                .map_err(|_| err(format!("bad frame number {frame:?}")))?;
            let mask = parse_buttons(buttons).map_err(err)?;
            if script.changes.last().is_some_and(|&(f, _)| frame < f) {
                return Err(err("frames must not go backwards".into()));
            }
            script.changes.push((frame, mask));
        }
        Ok(script)
    }

    /// Buttons held while `frame` runs: the last change at or before it, or nothing.
    pub fn buttons_at(&self, frame: u64) -> Buttons {
        let n = self.changes.partition_point(|&(f, _)| f <= frame);
        Buttons(if n == 0 { 0 } else { self.changes[n - 1].1 })
    }

    /// Recorded length in frames (`# end N`), if the script has one.
    pub fn end(&self) -> Option<u64> {
        self.end
    }

    /// The frame of the last button change, or 0 for an empty script.
    pub fn last_change_frame(&self) -> u64 {
        self.changes.last().map_or(0, |&(f, _)| f)
    }
}

/// Turns a stream of per-frame button states into script lines, emitting only changes.
#[derive(Debug, Default)]
pub struct Recorder {
    last: u8,
}

impl Recorder {
    /// Call once per frame, before the frame runs. Returns a line when the state changed.
    pub fn frame(&mut self, frame: u64, mask: u8) -> Option<String> {
        if mask == self.last {
            return None;
        }
        self.last = mask;
        Some(format!("{frame} {}", format_buttons(mask)))
    }

    /// The closing line, so a replay knows how long the session was.
    pub fn finish(frames: u64) -> String {
        format!("# end {frames}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_changes_comments_and_end() {
        let s = InputScript::parse("# hi\n\n120 a+start\n130 -\n# end 500\n").unwrap();
        assert_eq!(s.buttons_at(0), Buttons(0));
        assert_eq!(s.buttons_at(119), Buttons(0));
        assert_eq!(s.buttons_at(120), Buttons(Buttons::A | Buttons::START));
        assert_eq!(s.buttons_at(129), Buttons(Buttons::A | Buttons::START));
        assert_eq!(s.buttons_at(130), Buttons(0));
        assert_eq!(s.end(), Some(500));
        assert_eq!(s.last_change_frame(), 130);
    }

    #[test]
    fn rejects_bad_lines() {
        assert!(InputScript::parse("x A").unwrap_err().contains("line 1"));
        assert!(InputScript::parse("1 JUMP").unwrap_err().contains("JUMP"));
        assert!(InputScript::parse("1").is_err());
        assert!(InputScript::parse("1 A extra").is_err());
        assert!(InputScript::parse("5 A\n4 B")
            .unwrap_err()
            .contains("line 2"));
        assert!(InputScript::parse("# end x").is_err());
    }

    #[test]
    fn recorder_output_round_trips() {
        let held = |f: u64| match f {
            3..=5 => Buttons::A | Buttons::DOWN,
            8..=9 => Buttons::START,
            _ => 0,
        };
        let mut rec = Recorder::default();
        let mut text = String::new();
        for f in 0..12 {
            if let Some(line) = rec.frame(f, held(f)) {
                text.push_str(&line);
                text.push('\n');
            }
        }
        text.push_str(&Recorder::finish(12));
        let script = InputScript::parse(&text).unwrap();
        for f in 0..12 {
            assert_eq!(script.buttons_at(f), Buttons(held(f)), "frame {f}");
        }
        assert_eq!(script.end(), Some(12));
        // Only changes are written: 3 A+DOWN, 6 -, 8 START, 10 -.
        assert_eq!(text.lines().count(), 5);
    }

    #[test]
    fn all_buttons_round_trip() {
        for mask in 0..=255u8 {
            assert_eq!(parse_buttons(&format_buttons(mask)), Ok(mask));
        }
    }
}
