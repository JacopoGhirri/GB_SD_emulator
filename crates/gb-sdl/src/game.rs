//! Playing one game: the per-frame loop (events, run a frame, draw, pace) plus everything
//! that belongs to a single game session: battery save, save-state slots,
//! the optional input recording.

use std::path::{Path, PathBuf};

use gb_core::{Buttons, Cartridge, GameBoy, SCREEN_HEIGHT, SCREEN_WIDTH};
use sdl2::event::{Event, WindowEvent};

use crate::config::{Action, Bindings};
use crate::frontend::Frontend;
use crate::layout::Layout;
use crate::{audio, files, input, pacing, palette, png, save};

/// How a game session ended.
pub enum Exit {
    /// Close the whole program (window closed, or Esc when started with a ROM path).
    Quit,
    /// Go back to the game list (folder mode).
    Menu,
}

pub struct PlayOpts {
    /// Stop after this many frames (smoke testing).
    pub frames: Option<u64>,
    /// Write the button presses to this file (needs a fresh boot; see `save::SaveFile::disabled`).
    pub record: Option<PathBuf>,
}

/// Holding Start+Select this long (in frames, 1.5 s) leaves the game for the list when there is
/// no keyboard. Real games rarely hold both for that long.
const MENU_COMBO_FRAMES: u32 = 90;

struct Session {
    rom: PathBuf,
    rom_bytes: Vec<u8>,
    model: gb_core::Model,
    gb: GameBoy,
    save_path: PathBuf,
    save: save::SaveFile,
    recording: Option<RecordingFile>,
    frame_count: u64,
    muted_channels: u8,
}

pub fn play(
    fe: &mut Frontend,
    rom: &Path,
    layout: &Layout,
    opts: &PlayOpts,
) -> Result<Exit, String> {
    let rom_bytes =
        std::fs::read(rom).map_err(|e| format!("cannot read ROM {}: {e}", rom.display()))?;
    let cart = Cartridge::from_bytes(rom_bytes.clone())
        .map_err(|e| format!("cannot load {}: {e}", rom.display()))?;
    eprintln!("loaded {}", cart.header().title);
    let model = GameBoy::default_model_for(&cart);
    let mut gb = GameBoy::new(cart, model);
    let save_path = layout.adopt_legacy_save(rom);
    // Recording needs a fresh boot (see SaveFile::disabled), so it skips saves altogether.
    let save = if opts.record.is_some() {
        eprintln!("recording: save file ignored for this session");
        save::SaveFile::disabled(&save_path)
    } else {
        save::SaveFile::open(&save_path, &mut gb)
    };
    let recording = match &opts.record {
        Some(path) => Some(RecordingFile::create(path, &gb.cartridge().header().title)?),
        None => None,
    };
    if let Some(a) = fe.audio.as_mut() {
        gb.set_sample_rate(a.sample_rate());
        a.prime();
    }
    let mut s = Session {
        rom: rom.to_path_buf(),
        rom_bytes,
        model,
        gb,
        save_path,
        save,
        recording,
        frame_count: 0,
        muted_channels: 0,
    };

    let result = game_loop(fe, &mut s, layout, opts);

    // Always runs, also after an error: report, close the recording, write the save.
    if let Some(a) = &fe.audio {
        a.report();
    }
    if let Some(rec) = s.recording.as_mut() {
        rec.finish(s.frame_count);
    }
    // Quit events also cover SIGINT/SIGTERM: SDL turns them into SDL_QUIT by default.
    s.save.flush(&mut s.gb, true);
    result
}

fn game_loop(
    fe: &mut Frontend,
    s: &mut Session,
    layout: &Layout,
    opts: &PlayOpts,
) -> Result<Exit, String> {
    let cfg = fe.cfg.clone();
    let mut input = input::InputState::new(Bindings::new(&cfg));
    let mut fast_forward = false;
    let mut slow_mo = false;
    let mut paused = false;
    let mut slot: u8 = 1;
    let mut palette = cfg.palette;
    // What is on screen: the core's frame with the DMG colour scheme applied.
    let mut shown = vec![0u32; SCREEN_WIDTH * SCREEN_HEIGHT];
    let mut pacer = pacing::Pacer::new();
    let mut audio_scratch: Vec<f32> = Vec::new();
    let mut combo_frames: u32 = 0;
    // Leaving a game goes to the list in folder mode, and closes the program otherwise.
    let leave = if layout.is_folder() {
        Exit::Menu
    } else {
        Exit::Quit
    };

    loop {
        // Events are polled once per frame; buttons are set before run_frame().
        let mut actions: Vec<input::ActionEvent> = Vec::new();
        let events: Vec<Event> = fe.events.poll_iter().collect();
        for event in events {
            if fe.pad_event(&event) {
                actions.extend(input.clear_pad());
            }
            match event {
                Event::Quit { .. } => return Ok(Exit::Quit),
                Event::KeyDown {
                    keycode: Some(k),
                    repeat: false,
                    ..
                } => {
                    if let Some(ch) = input::mute_channel(k) {
                        s.muted_channels ^= 1 << ch;
                        s.gb.set_muted_channels(s.muted_channels);
                        let state = if s.muted_channels & (1 << ch) != 0 {
                            "muted"
                        } else {
                            "on"
                        };
                        eprintln!("channel {} {state}", ch + 1);
                    } else if let Some(up) = input::volume_key(k) {
                        if let Some(a) = fe.audio.as_mut() {
                            a.volume = audio::step_volume(a.volume, up);
                            eprintln!("volume {:.0}%", a.volume * 100.0);
                        }
                    } else {
                        actions.extend(input.key(k, true));
                    }
                }
                Event::KeyUp {
                    keycode: Some(k), ..
                } => actions.extend(input.key(k, false)),
                Event::ControllerButtonDown { button, .. } => {
                    actions.extend(input.pad_button(button, true))
                }
                Event::ControllerButtonUp { button, .. } => {
                    actions.extend(input.pad_button(button, false))
                }
                Event::ControllerAxisMotion { axis, value, .. } => {
                    actions.extend(input.pad_axis(axis, value))
                }
                Event::Window {
                    win_event: WindowEvent::FocusLost,
                    ..
                } => {
                    // Flush on focus loss (the user may be about to kill the process).
                    s.save.flush(&mut s.gb, true);
                }
                _ => {}
            }
        }

        for (action, down) in actions {
            // Reset and state files would make a recorded input script unreproducible.
            let recording_blocks = s.recording.is_some()
                && matches!(
                    action,
                    Action::Reset | Action::SaveState | Action::LoadState
                );
            if recording_blocks {
                if down {
                    fe.notice.show(&mut fe.canvas, "disabled while recording");
                }
                continue;
            }
            match (action, down) {
                (Action::Quit, true) => return Ok(leave),
                (Action::Fullscreen, true) => fe.toggle_fullscreen()?,
                (Action::FastForward, held) => fast_forward = held,
                (Action::SlowMo, held) => slow_mo = held,
                (Action::Pause, true) => {
                    paused = !paused;
                    fe.notice
                        .show(&mut fe.canvas, if paused { "paused" } else { "running" });
                }
                (Action::Reset, true) => {
                    // Keep the battery save (real hardware keeps SRAM across a reset): flush it,
                    // rebuild the console from the ROM and read the save back.
                    s.save.flush(&mut s.gb, true);
                    match Cartridge::from_bytes(s.rom_bytes.clone()) {
                        Ok(cart) => {
                            s.gb = GameBoy::new(cart, s.model);
                            if let Some(a) = &fe.audio {
                                s.gb.set_sample_rate(a.sample_rate());
                            }
                            s.gb.set_muted_channels(s.muted_channels);
                            s.save = save::SaveFile::open(&s.save_path, &mut s.gb);
                            fe.notice.show(&mut fe.canvas, "reset");
                        }
                        Err(e) => fe
                            .notice
                            .show(&mut fe.canvas, &format!("reset failed: {e}")),
                    }
                }
                (Action::Screenshot, true) => {
                    let path = layout.next_screenshot_path(&s.rom);
                    let png = png::encode_rgb(SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32, &shown);
                    let written = path
                        .parent()
                        .map_or(Ok(()), std::fs::create_dir_all)
                        .and_then(|()| std::fs::write(&path, png));
                    match written {
                        Ok(()) => fe
                            .notice
                            .show(&mut fe.canvas, &format!("saved {}", path.display())),
                        Err(e) => fe
                            .notice
                            .show(&mut fe.canvas, &format!("screenshot failed: {e}")),
                    }
                }
                (Action::NextPalette, true) => {
                    palette = palette::next(palette);
                    let note = if s.gb.cgb_mode() {
                        format!(
                            "palette {} (applies to DMG games only)",
                            palette::name(palette)
                        )
                    } else {
                        format!("palette {}", palette::name(palette))
                    };
                    fe.notice.show(&mut fe.canvas, &note);
                }
                (Action::NextSlot, true) => {
                    slot = files::next_slot(slot);
                    fe.notice
                        .show(&mut fe.canvas, &format!("state slot {slot}"));
                }
                (Action::SaveState, true) => match files::save_state(&s.gb, layout, &s.rom, slot) {
                    Ok(n) => fe
                        .notice
                        .show(&mut fe.canvas, &format!("saved state {slot} ({n} bytes)")),
                    Err(e) => fe.notice.show(&mut fe.canvas, &e),
                },
                (Action::LoadState, true) => {
                    match files::load_state(&mut s.gb, layout, &s.rom, slot) {
                        Ok(()) => fe
                            .notice
                            .show(&mut fe.canvas, &format!("loaded state {slot}")),
                        Err(e) => fe.notice.show(&mut fe.canvas, &e),
                    }
                }
                _ => {} // key releases of one-shot actions
            }
        }
        fe.notice.tick(&mut fe.canvas);

        // No keyboard in Game Mode: holding Start+Select returns to the game list.
        let both = Buttons::START | Buttons::SELECT;
        if layout.is_folder() && input.buttons().0 & both == both {
            combo_frames += 1;
            if combo_frames == MENU_COMBO_FRAMES / 3 {
                fe.notice.show(
                    &mut fe.canvas,
                    "keep holding Start+Select for the game list",
                );
            }
            if combo_frames >= MENU_COMBO_FRAMES {
                return Ok(Exit::Menu);
            }
        } else {
            combo_frames = 0;
        }

        if !paused {
            s.gb.set_buttons(input.buttons());
            if let Some(rec) = s.recording.as_mut() {
                rec.frame(s.frame_count, input.buttons().0);
            }
            s.gb.run_frame();
            s.frame_count += 1;
        }

        // The scheme only recolours DMG output; CGB games already carry their own colours.
        if s.gb.cgb_mode() {
            shown.copy_from_slice(s.gb.framebuffer());
        } else {
            palette::apply(palette, s.gb.framebuffer(), &mut shown);
        }
        fe.present(&shown)?;

        s.save.flush(&mut s.gb, false);

        if !paused && opts.frames.is_some_and(|n| s.frame_count >= n) {
            return Ok(Exit::Quit);
        }
        // Audio-driven pacing at normal speed when a device is open; otherwise (and while
        // paused, fast-forwarding or in slow motion) the timer pacer. Non-normal speeds
        // discard the audio instead of playing it at the wrong pitch.
        let speed = if fast_forward {
            Some(cfg.fast_forward_speed)
        } else if slow_mo {
            Some(cfg.slow_mo_speed)
        } else if paused || fe.audio.is_none() {
            Some(1.0)
        } else {
            None
        };
        match (fe.audio.as_mut(), speed) {
            (Some(a), None) => a.push_and_wait(&mut s.gb, &mut audio_scratch),
            (Some(a), Some(sp)) => {
                a.discard(&mut s.gb, &mut audio_scratch);
                pacer.wait_for_next_frame(sp);
            }
            (None, sp) => pacer.wait_for_next_frame(sp.unwrap_or(1.0)),
        }
    }
}

/// Writes the input script while you play. Each line is flushed at once, so a crash or kill
/// loses at most the last change.
pub struct RecordingFile {
    out: std::io::BufWriter<std::fs::File>,
    recorder: gb_core::input_script::Recorder,
    path: PathBuf,
}

impl RecordingFile {
    pub fn create(path: &Path, title: &str) -> Result<RecordingFile, String> {
        use std::io::Write;
        let file = std::fs::File::create(path)
            .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        let mut out = std::io::BufWriter::new(file);
        writeln!(out, "# gb-sdl recording of {title}, from a fresh boot")
            .and_then(|()| out.flush())
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        Ok(RecordingFile {
            out,
            recorder: Default::default(),
            path: path.to_path_buf(),
        })
    }

    fn write_line(&mut self, line: &str) {
        use std::io::Write;
        if let Err(e) = writeln!(self.out, "{line}").and_then(|()| self.out.flush()) {
            eprintln!("error: writing {}: {e}", self.path.display());
        }
    }

    /// Call before each frame runs with the buttons held for it.
    pub fn frame(&mut self, frame: u64, mask: u8) {
        if let Some(line) = self.recorder.frame(frame, mask) {
            self.write_line(&line);
        }
    }

    pub fn finish(&mut self, frames: u64) {
        self.write_line(&gb_core::input_script::Recorder::finish(frames));
        eprintln!(
            "recording saved to {} ({frames} frames)",
            self.path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_file_writes_changes_and_end() {
        let path = std::env::temp_dir().join("gb-sdl-record-test.txt");
        let mut rec = RecordingFile::create(&path, "TEST").unwrap();
        rec.frame(0, 0);
        rec.frame(5, gb_core::Buttons::A);
        rec.frame(6, gb_core::Buttons::A);
        rec.frame(9, 0);
        rec.finish(20);
        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let script = gb_core::input_script::InputScript::parse(&text).unwrap();
        assert_eq!(script.buttons_at(5), gb_core::Buttons(gb_core::Buttons::A));
        assert_eq!(script.buttons_at(9), gb_core::Buttons(0));
        assert_eq!(script.end(), Some(20));
        assert_eq!(text.lines().filter(|l| !l.starts_with('#')).count(), 2);
    }
}
