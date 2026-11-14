//! Desktop / Steam Deck frontend: `gb-sdl <rom.gb> [--scale N] [--fullscreen] [--swap-ab]`.
//!
//! One thread: poll SDL events once per frame, set the buttons, `run_frame()`, draw, flush the
//! save if due, then wait for the frame deadline (see `pacing`).

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

const USAGE: &str = "usage: gb-sdl <rom.gb> [--scale N] [--fullscreen] [--swap-ab] [--frames N]\n\
  --scale N      initial window scale (default 4)\n\
  --fullscreen   start fullscreen (F11 toggles)\n\
  --swap-ab      swap the gamepad face buttons (default: east = B, south = A)\n\
  --frames N     run N frames (paced) then quit, for smoke testing";

struct Args {
    rom: PathBuf,
    scale: u32,
    fullscreen: bool,
    swap_ab: bool,
    frames: Option<u64>,
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut rom = None;
    let mut out = Args {
        rom: PathBuf::new(),
        scale: 4,
        fullscreen: false,
        swap_ab: false,
        frames: None,
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
            "--fullscreen" => out.fullscreen = true,
            "--swap-ab" => out.swap_ab = true,
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
    let mut save = save::SaveFile::open(&args.rom, &mut gb);

    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    let controllers = sdl.game_controller()?;
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
        pacer.wait_for_next_frame(fast_forward);
    }

    // Quit event also covers SIGINT/SIGTERM: SDL turns them into SDL_QUIT by default.
    save.flush(&mut gb, true);
    Ok(())
}
