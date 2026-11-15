//! Desktop / Steam Deck frontend: `gb-sdl <rom.gb> [--scale N] [--fullscreen] [--swap-ab]`.
//!
//! One thread: poll SDL events once per frame, set the buttons, `run_frame()`, draw, flush the
//! save if due, then wait for the frame deadline (see `pacing`).

mod audio;
mod input;
mod pacing;
mod save;
mod video;

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use gb_core::{Cartridge, GameBoy, Model, SCREEN_HEIGHT, SCREEN_WIDTH};
use sdl2::controller::GameController;
use sdl2::event::{Event, WindowEvent};
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::video::FullscreenType;

const USAGE: &str =
    "usage: gb-sdl <rom.gb> [--scale N] [--fullscreen] [--swap-ab] [--frames N] [--record FILE] [--no-audio]\n\
  --scale N      initial window scale (default 4)\n\
  --fullscreen   start fullscreen (F11 toggles)\n\
  --swap-ab      swap the gamepad face buttons (default: east = B, south = A)\n\
  --frames N     run N frames (paced) then quit, for smoke testing\n\
  --no-audio     no sound; pace the game with a timer instead of the audio clock\n\
  --record FILE  write your button presses, by frame, to FILE (replay with gb-cli --input).\n\
                 Starts from a fresh boot and neither reads nor writes the .sav file.";

struct Args {
    rom: PathBuf,
    scale: u32,
    fullscreen: bool,
    swap_ab: bool,
    frames: Option<u64>,
    record: Option<PathBuf>,
    no_audio: bool,
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut rom = None;
    let mut out = Args {
        rom: PathBuf::new(),
        scale: 4,
        fullscreen: false,
        swap_ab: false,
        frames: None,
        record: None,
        no_audio: false,
    };
    let mut it = args;
    while let Some(a) = it.next() {
        match a.as_str() {
            "--scale" => {
                let v = it.next().ok_or("--scale needs a number")?;
                out.scale = v
                    .parse::<u32>()
                    .ok()
                    .filter(|s| (1..=20).contains(s))
                    .ok_or("--scale must be an integer from 1 to 20")?;
            }
            "--frames" => {
                let v = it.next().ok_or("--frames needs a number")?;
                out.frames = Some(v.parse().map_err(|_| "--frames must be a number")?);
            }
            "--record" => {
                out.record = Some(PathBuf::from(it.next().ok_or("--record needs a file")?));
            }
            "--fullscreen" => out.fullscreen = true,
            "--swap-ab" => out.swap_ab = true,
            "--no-audio" => out.no_audio = true,
            "-h" | "--help" => return Err(String::new()),
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            _ => {
                if rom.replace(PathBuf::from(a)).is_some() {
                    return Err("only one ROM path is accepted".into());
                }
            }
        }
    }
    out.rom = rom.ok_or("no ROM given")?;
    Ok(out)
}

fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(msg) => {
            if !msg.is_empty() {
                eprintln!("error: {msg}");
            }
            eprintln!("{USAGE}");
            return ExitCode::from(if msg.is_empty() { 0 } else { 2 });
        }
    };
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<(), String> {
    let rom_bytes = std::fs::read(&args.rom)
        .map_err(|e| format!("cannot read ROM {}: {e}", args.rom.display()))?;
    let cart = Cartridge::from_bytes(rom_bytes)
        .map_err(|e| format!("cannot load {}: {e}", args.rom.display()))?;
    eprintln!("loaded {}", cart.header().title);
    let mut gb = GameBoy::new(cart, Model::Dmg);
    // Recording needs a fresh boot (see SaveFile::disabled), so it skips saves altogether.
    let mut save = if args.record.is_some() {
        eprintln!("recording: save file ignored for this session");
        save::SaveFile::disabled(&args.rom)
    } else {
        save::SaveFile::open(&args.rom, &mut gb)
    };
    let mut recording = match &args.record {
        Some(path) => Some(RecordingFile::create(path, &gb.cartridge().header().title)?),
        None => None,
    };

    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    let controllers = sdl.game_controller()?;
    // Audio is optional: without a device we fall back to timer pacing instead of failing.
    let mut audio = if args.no_audio {
        None
    } else {
        match sdl.audio().and_then(|a| audio::Audio::open(&a)) {
            Ok(a) => Some(a),
            Err(e) => {
                eprintln!("warning: no audio ({e}); using timer pacing");
                None
            }
        }
    };
    if let Some(a) = &audio {
        gb.set_sample_rate(a.sample_rate());
    }
    let mut muted_channels: u8 = 0;
    let mut audio_scratch: Vec<f32> = Vec::new();
    let (w, h) = (
        SCREEN_WIDTH as u32 * args.scale,
        SCREEN_HEIGHT as u32 * args.scale,
    );
    // Nearest-neighbour scaling (SDL hint "0"). Integer scale makes this crisp anyway.
    sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "0");
    let mut window = video.window("gb-sdl", w, h);
    window.position_centered().resizable();
    let window = window.build().map_err(|e| e.to_string())?;
    let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
    if args.fullscreen {
        canvas
            .window_mut()
            .set_fullscreen(FullscreenType::Desktop)?;
    }
    let creator = canvas.texture_creator();
    let mut texture = creator
        .create_texture_streaming(
            PixelFormatEnum::ARGB8888,
            SCREEN_WIDTH as u32,
            SCREEN_HEIGHT as u32,
        )
        .map_err(|e| e.to_string())?;
    let mut events = sdl.event_pump()?;

    let mut pads: HashMap<u32, GameController> = HashMap::new();
    let mut input = input::InputState::default();
    let mut fast_forward = false;
    let mut pacer = pacing::Pacer::new();
    let mut pixels = vec![0u8; SCREEN_WIDTH * SCREEN_HEIGHT * 4];
    let mut frame_count: u64 = 0;

    'main: loop {
        // Events are polled once per frame; buttons are set before run_frame().
        for event in events.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main,
                Event::KeyDown {
                    keycode: Some(k),
                    repeat: false,
                    ..
                } => {
                    if k == input::KEY_QUIT {
                        break 'main;
                    } else if k == input::KEY_FULLSCREEN {
                        let win = canvas.window_mut();
                        let next = if win.fullscreen_state() == FullscreenType::Off {
                            FullscreenType::Desktop
                        } else {
                            FullscreenType::Off
                        };
                        win.set_fullscreen(next)?;
                    } else if k == input::KEY_FAST_FORWARD {
                        fast_forward = true;
                    } else if let Some(ch) = input::mute_channel(k) {
                        muted_channels ^= 1 << ch;
                        gb.set_muted_channels(muted_channels);
                        let state = if muted_channels & (1 << ch) != 0 {
                            "muted"
                        } else {
                            "on"
                        };
                        eprintln!("channel {} {state}", ch + 1);
                    } else if let Some(up) = input::volume_key(k) {
                        if let Some(a) = audio.as_mut() {
                            a.volume = audio::step_volume(a.volume, up);
                            eprintln!("volume {:.0}%", a.volume * 100.0);
                        }
                    } else {
                        input.key(k, true);
                    }
                }
                Event::KeyUp {
                    keycode: Some(k), ..
                } => {
                    if k == input::KEY_FAST_FORWARD {
                        fast_forward = false;
                    } else {
                        input.key(k, false);
                    }
                }
                Event::ControllerDeviceAdded { which, .. } => match controllers.open(which) {
                    Ok(pad) => {
                        eprintln!("gamepad connected: {}", pad.name());
                        pads.insert(pad.instance_id(), pad);
                    }
                    Err(e) => eprintln!("warning: cannot open gamepad {which}: {e}"),
                },
                Event::ControllerDeviceRemoved { which, .. } => {
                    if pads.remove(&which).is_some() {
                        eprintln!("gamepad disconnected");
                        input.clear_pad();
                    }
                }
                Event::ControllerButtonDown { button, .. } => {
                    input.pad_button(button, true, args.swap_ab)
                }
                Event::ControllerButtonUp { button, .. } => {
                    input.pad_button(button, false, args.swap_ab)
                }
                Event::ControllerAxisMotion { axis, value, .. } => input.pad_axis(axis, value),
                Event::Window {
                    win_event: WindowEvent::FocusLost,
                    ..
                } => {
                    // Flush on focus loss (the user may be about to kill the process).
                    save.flush(&mut gb, true);
                }
                _ => {}
            }
        }

        gb.set_buttons(input.buttons());
        if let Some(rec) = recording.as_mut() {
            rec.frame(frame_count, input.buttons().0);
        }
        gb.run_frame();

        for (dst, px) in pixels
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(gb.framebuffer())
        {
            *dst = px.to_le_bytes(); // ARGB8888 little-endian: B, G, R, A
        }
        texture
            .update(None, &pixels, SCREEN_WIDTH * 4)
            .map_err(|e| e.to_string())?;
        let (ww, wh) = canvas.output_size()?;
        let (x, y, rw, rh) = video::dest_rect(ww, wh);
        canvas.set_draw_color(Color::RGB(0, 0, 0));
        canvas.clear();
        canvas.copy(&texture, None, Rect::new(x, y, rw, rh))?;
        canvas.present();

        save.flush(&mut gb, false);

        frame_count += 1;
        if args.frames.is_some_and(|n| frame_count >= n) {
            break;
        }
        // Audio-driven pacing when a device is open; otherwise the timer pacer.
        match audio.as_mut() {
            Some(a) if fast_forward => a.discard(&mut gb, &mut audio_scratch),
            Some(a) => a.push_and_wait(&mut gb, &mut audio_scratch),
            None => pacer.wait_for_next_frame(fast_forward),
        }
    }
    if let Some(a) = &audio {
        a.report();
    }

    if let Some(rec) = recording.as_mut() {
        rec.finish(frame_count);
    }
    // Quit event also covers SIGINT/SIGTERM: SDL turns them into SDL_QUIT by default.
    save.flush(&mut gb, true);
    Ok(())
}

/// Writes the input script while you play. Each line is flushed at once, so a crash or kill
/// loses at most the last change.
struct RecordingFile {
    out: std::io::BufWriter<std::fs::File>,
    recorder: gb_core::input_script::Recorder,
    path: PathBuf,
}

impl RecordingFile {
    fn create(path: &std::path::Path, title: &str) -> Result<RecordingFile, String> {
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
    fn frame(&mut self, frame: u64, mask: u8) {
        if let Some(line) = self.recorder.frame(frame, mask) {
            self.write_line(&line);
        }
    }

    fn finish(&mut self, frames: u64) {
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
    fn record_option_is_parsed() {
        let args = parse_args(
            ["x.gb", "--record", "run.txt"]
                .map(String::from)
                .into_iter(),
        );
        assert_eq!(args.unwrap().record, Some(PathBuf::from("run.txt")));
        assert!(parse_args(["x.gb", "--record"].map(String::from).into_iter()).is_err());
    }

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
