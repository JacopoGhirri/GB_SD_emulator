//! Desktop / Steam Deck frontend: `gb-sdl <rom.gb> [--scale N] [--fullscreen] [--swap-ab]`.
//!
//! One thread: poll SDL events once per frame, set the buttons, `run_frame()`, draw, flush the
//! save if due, then wait for the frame deadline (see `pacing`).

mod audio;
mod config;
mod files;
mod input;
mod pacing;
mod palette;
mod png;
mod rewind;
mod save;
mod video;

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use config::{Action, Bindings, Config};
use gb_core::{Cartridge, GameBoy, SCREEN_HEIGHT, SCREEN_WIDTH};
use sdl2::controller::GameController;
use sdl2::event::{Event, WindowEvent};
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::video::FullscreenType;

const USAGE: &str =
    "usage: gb-sdl <rom.gb> [--config FILE] [--scale N] [--fullscreen] [--swap-ab] [--frames N] [--record FILE] [--no-audio]\n\
  --config FILE  settings file (default: config.ini beside the program, then\n\
                 ~/.config/gb-sdl/config.ini; see docs/STEAMDECK.md). Flags below override it.\n\
  --scale N      initial window scale (default 4)\n\
  --fullscreen   start fullscreen (F11 toggles)\n\
  --swap-ab      swap the gamepad face buttons (default: east = B, south = A)\n\
  --frames N     run N frames (paced) then quit, for smoke testing\n\
  --no-audio     no sound; pace the game with a timer instead of the audio clock\n\
  --record FILE  write your button presses, by frame, to FILE (replay with gb-cli --input).\n\
                 Starts from a fresh boot and neither reads nor writes the .sav file.";

struct Args {
    rom: PathBuf,
    config: Option<PathBuf>,
    /// `None` = not given on the command line (the config file / default decides).
    scale: Option<u32>,
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
        config: None,
        scale: None,
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
                out.scale = Some(
                    v.parse::<u32>()
                        .ok()
                        .filter(|s| (1..=20).contains(s))
                        .ok_or("--scale must be an integer from 1 to 20")?,
                );
            }
            "--config" => {
                out.config = Some(PathBuf::from(it.next().ok_or("--config needs a file")?));
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

/// Precedence: command-line flags > config file > defaults. A flag can only switch a
/// boolean on; to turn one off, edit the config file.
fn apply_cli(cfg: &mut Config, args: &Args) {
    if let Some(scale) = args.scale {
        cfg.scale = scale;
    }
    cfg.fullscreen |= args.fullscreen;
    cfg.swap_ab |= args.swap_ab;
}

fn run(args: Args) -> Result<(), String> {
    let (mut cfg, cfg_path) =
        Config::load(args.config.as_deref()).map_err(|errs| errs.join("\n"))?;
    apply_cli(&mut cfg, &args);
    match &cfg_path {
        Some(p) => eprintln!("config: {}", p.display()),
        None => eprintln!("config: none found, using defaults"),
    }
    let rom_bytes = std::fs::read(&args.rom)
        .map_err(|e| format!("cannot read ROM {}: {e}", args.rom.display()))?;
    let cart = Cartridge::from_bytes(rom_bytes.clone())
        .map_err(|e| format!("cannot load {}: {e}", args.rom.display()))?;
    eprintln!("loaded {}", cart.header().title);
    let model = GameBoy::default_model_for(&cart);
    let mut gb = GameBoy::new(cart, model);
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
    if let Some(a) = audio.as_mut() {
        gb.set_sample_rate(a.sample_rate());
        a.volume = cfg.volume;
    }
    let mut muted_channels: u8 = 0;
    let mut audio_scratch: Vec<f32> = Vec::new();
    let (w, h) = (
        SCREEN_WIDTH as u32 * cfg.scale,
        SCREEN_HEIGHT as u32 * cfg.scale,
    );
    // Nearest-neighbour scaling (SDL hint "0"). Integer scale makes this crisp anyway.
    sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "0");
    let mut window = video.window("gb-sdl", w, h);
    window.position_centered().resizable();
    let window = window.build().map_err(|e| e.to_string())?;
    let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
    if cfg.fullscreen {
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
    let mut input = input::InputState::new(Bindings::new(&cfg));
    let mut fast_forward = false;
    let mut slow_mo = false;
    let mut paused = false;
    let mut slot: u8 = 1;
    let mut palette = cfg.palette;
    let mut rewind = if cfg.rewind {
        let capacity = cfg.rewind_capacity()?;
        eprintln!(
            "rewind on: {capacity} states, one every {} frames",
            cfg.rewind_interval_frames
        );
        Some(rewind::RewindBuffer::new(capacity))
    } else {
        None
    };
    let mut rewind_held = false;
    // What is on screen: the core's frame with the DMG colour scheme applied.
    let mut shown = vec![0u32; SCREEN_WIDTH * SCREEN_HEIGHT];
    let mut notice = Notice::default();
    let mut pacer = pacing::Pacer::new();
    let mut pixels = vec![0u8; SCREEN_WIDTH * SCREEN_HEIGHT * 4];
    let mut frame_count: u64 = 0;

    'main: loop {
        // Events are polled once per frame; buttons are set before run_frame().
        let mut actions: Vec<input::ActionEvent> = Vec::new();
        for event in events.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main,
                Event::KeyDown {
                    keycode: Some(k),
                    repeat: false,
                    ..
                } => {
                    if let Some(ch) = input::mute_channel(k) {
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
                        actions.extend(input.key(k, true));
                    }
                }
                Event::KeyUp {
                    keycode: Some(k), ..
                } => actions.extend(input.key(k, false)),
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
                        actions.extend(input.clear_pad());
                    }
                }
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
                    save.flush(&mut gb, true);
                }
                _ => {}
            }
        }

        for (action, down) in actions {
            // Reset and state files would make a recorded input script unreproducible.
            let recording_blocks = recording.is_some()
                && matches!(
                    action,
                    Action::Reset | Action::SaveState | Action::LoadState | Action::Rewind
                );
            if recording_blocks {
                if down {
                    notice.show(&mut canvas, "disabled while recording");
                }
                continue;
            }
            match (action, down) {
                (Action::Quit, true) => break 'main,
                (Action::Fullscreen, true) => {
                    let win = canvas.window_mut();
                    let next = if win.fullscreen_state() == FullscreenType::Off {
                        FullscreenType::Desktop
                    } else {
                        FullscreenType::Off
                    };
                    win.set_fullscreen(next)?;
                }
                (Action::FastForward, held) => fast_forward = held,
                (Action::SlowMo, held) => slow_mo = held,
                (Action::Rewind, held) => {
                    if held && rewind.is_none() {
                        notice.show(
                            &mut canvas,
                            "rewind is off (set rewind = true in the config)",
                        );
                    }
                    rewind_held = held;
                }
                (Action::Pause, true) => {
                    paused = !paused;
                    notice.show(&mut canvas, if paused { "paused" } else { "running" });
                }
                (Action::Reset, true) => {
                    // Keep the battery save (real hardware keeps SRAM across a reset): flush it,
                    // rebuild the console from the ROM and read the save back.
                    save.flush(&mut gb, true);
                    match Cartridge::from_bytes(rom_bytes.clone()) {
                        Ok(cart) => {
                            gb = GameBoy::new(cart, model);
                            if let Some(a) = &audio {
                                gb.set_sample_rate(a.sample_rate());
                            }
                            gb.set_muted_channels(muted_channels);
                            save = save::SaveFile::open(&args.rom, &mut gb);
                            if let Some(r) = rewind.as_mut() {
                                r.clear();
                            }
                            notice.show(&mut canvas, "reset");
                        }
                        Err(e) => notice.show(&mut canvas, &format!("reset failed: {e}")),
                    }
                }
                (Action::Screenshot, true) => {
                    let path = files::next_screenshot_path(&args.rom);
                    let png = png::encode_rgb(SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32, &shown);
                    match std::fs::write(&path, png) {
                        Ok(()) => notice.show(&mut canvas, &format!("saved {}", path.display())),
                        Err(e) => notice.show(&mut canvas, &format!("screenshot failed: {e}")),
                    }
                }
                (Action::NextPalette, true) => {
                    palette = palette::next(palette);
                    let note = if gb.cgb_mode() {
                        format!(
                            "palette {} (applies to DMG games only)",
                            palette::name(palette)
                        )
                    } else {
                        format!("palette {}", palette::name(palette))
                    };
                    notice.show(&mut canvas, &note);
                }
                (Action::NextSlot, true) => {
                    slot = files::next_slot(slot);
                    notice.show(&mut canvas, &format!("state slot {slot}"));
                }
                (Action::SaveState, true) => match files::save_state(&gb, &args.rom, slot) {
                    Ok(n) => notice.show(&mut canvas, &format!("saved state {slot} ({n} bytes)")),
                    Err(e) => notice.show(&mut canvas, &e),
                },
                (Action::LoadState, true) => match files::load_state(&mut gb, &args.rom, slot) {
                    Ok(()) => {
                        if let Some(r) = rewind.as_mut() {
                            r.clear();
                        }
                        notice.show(&mut canvas, &format!("loaded state {slot}"));
                    }
                    Err(e) => notice.show(&mut canvas, &e),
                },
                _ => {} // key releases of one-shot actions
            }
        }
        notice.tick(&mut canvas);

        let rewinding = rewind_held && rewind.is_some() && !paused;
        if rewinding {
            // Go back one stored state per displayed frame instead of running the game. A
            // loaded state carries its own picture, so nothing else is needed to redraw.
            if let Some(state) = rewind.as_mut().and_then(|r| r.step_back()) {
                if let Err(e) = gb.load_state(&state) {
                    notice.show(&mut canvas, &format!("rewind failed: {e}"));
                }
            }
        } else if !paused {
            gb.set_buttons(input.buttons());
            if let Some(rec) = recording.as_mut() {
                rec.frame(frame_count, input.buttons().0);
            }
            gb.run_frame();
            frame_count += 1;
            if let Some(r) = rewind.as_mut() {
                if frame_count.is_multiple_of(u64::from(cfg.rewind_interval_frames)) {
                    r.push(gb.save_state());
                }
            }
        }

        // The scheme only recolours DMG output; CGB games already carry their own colours.
        if gb.cgb_mode() {
            shown.copy_from_slice(gb.framebuffer());
        } else {
            palette::apply(palette, gb.framebuffer(), &mut shown);
        }
        for (dst, px) in pixels.as_chunks_mut::<4>().0.iter_mut().zip(&shown) {
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

        if !paused && args.frames.is_some_and(|n| frame_count >= n) {
            break;
        }
        // Audio-driven pacing at normal speed when a device is open; otherwise (and while
        // paused, fast-forwarding or in slow motion) the timer pacer. Non-normal speeds
        // discard the audio instead of playing it at the wrong pitch.
        let speed = if rewinding {
            Some(1.0)
        } else if fast_forward {
            Some(cfg.fast_forward_speed)
        } else if slow_mo {
            Some(cfg.slow_mo_speed)
        } else if rewinding || paused || audio.is_none() {
            Some(1.0)
        } else {
            None
        };
        match (audio.as_mut(), speed) {
            (Some(a), None) => a.push_and_wait(&mut gb, &mut audio_scratch),
            (Some(a), Some(s)) => {
                a.discard(&mut gb, &mut audio_scratch);
                pacer.wait_for_next_frame(s);
            }
            (None, s) => pacer.wait_for_next_frame(s.unwrap_or(1.0)),
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

/// Short status messages (no text rendering in the window): printed to stderr and shown in the
/// window title for a few seconds.
#[derive(Default)]
struct Notice {
    until: Option<std::time::Instant>,
}

impl Notice {
    fn show(&mut self, canvas: &mut sdl2::render::Canvas<sdl2::video::Window>, msg: &str) {
        eprintln!("{msg}");
        let _ = canvas.window_mut().set_title(&format!("gb-sdl - {msg}"));
        self.until = Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
    }

    fn tick(&mut self, canvas: &mut sdl2::render::Canvas<sdl2::video::Window>) {
        if self.until.is_some_and(|t| std::time::Instant::now() >= t) {
            self.until = None;
            let _ = canvas.window_mut().set_title("gb-sdl");
        }
    }
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
    fn command_line_overrides_the_config() {
        let parse = |a: &[&str]| parse_args(a.iter().map(|s| s.to_string())).unwrap();
        let mut cfg = Config::parse("scale = 6\nfullscreen = true\n").unwrap();
        apply_cli(&mut cfg, &parse(&["x.gb"]));
        assert_eq!(
            (cfg.scale, cfg.fullscreen),
            (6, true),
            "config beats default"
        );
        apply_cli(&mut cfg, &parse(&["x.gb", "--scale", "2", "--swap-ab"]));
        assert_eq!((cfg.scale, cfg.swap_ab), (2, true), "flag beats config");
        assert!(parse_args(["x.gb", "--config"].map(String::from).into_iter()).is_err());
    }

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
