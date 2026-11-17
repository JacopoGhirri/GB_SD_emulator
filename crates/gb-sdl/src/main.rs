//! Desktop / Steam Deck frontend.
//!
//! * `gb-sdl` (or `gb-sdl <folder>`): a game list. The folder holds the program, `games/` with
//!   your ROMs, and `saves/` / `screenshots/` which are created when first needed.
//! * `gb-sdl <rom.gb>`: play that one game, files beside the ROM.
//!
//! One thread: poll SDL events once per frame, set the buttons, `run_frame()`, draw, flush the
//! save if due, then wait for the frame deadline (see `pacing`).

mod audio;
mod config;
mod files;
mod font;
mod frontend;
mod game;
mod input;
mod layout;
mod library;
mod menu;
mod pacing;
mod palette;
mod png;
mod rewind;
mod save;
mod video;

use std::path::PathBuf;
use std::process::ExitCode;

use config::Config;
use frontend::Frontend;
use game::{Exit, PlayOpts};
use gb_core::{SCREEN_HEIGHT, SCREEN_WIDTH};
use layout::Layout;
use menu::{Choice, Menu};
use sdl2::pixels::PixelFormatEnum;
use sdl2::video::FullscreenType;

const USAGE: &str =
    "usage: gb-sdl [<rom.gb> | <folder>] [--config FILE] [--scale N] [--fullscreen] [--swap-ab] [--frames N] [--record FILE] [--no-audio]\n\
  (no argument)  game list for the folder the program is in: ROMs go in games/, saves in saves/\n\
  <folder>       the same for another folder\n\
  <rom.gb>       play one game; its .sav / state / screenshot files go beside the ROM\n\
  --config FILE  settings file (default: config.ini beside the program, then\n\
                 ~/.config/gb-sdl/config.ini; see docs/STEAMDECK.md). Flags below override it.\n\
  --scale N      initial window scale (default 4)\n\
  --fullscreen   start fullscreen (F11 toggles)\n\
  --swap-ab      swap the gamepad face buttons (default: east = B, south = A)\n\
  --frames N     run N frames (paced) then quit, for smoke testing\n\
  --no-audio     no sound; pace the game with a timer instead of the audio clock\n\
  --record FILE  write your button presses, by frame, to FILE (replay with gb-cli --input).\n\
                 Needs a ROM path. Starts from a fresh boot and neither reads nor writes the .sav file.";

struct Args {
    /// A ROM file, a folder, or nothing (= the folder the program is in).
    rom: Option<PathBuf>,
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
    let mut out = Args {
        rom: None,
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
                if out.rom.replace(PathBuf::from(a)).is_some() {
                    return Err("only one ROM path or folder is accepted".into());
                }
            }
        }
    }
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

/// The folder the program lives in (the default root of folder mode).
fn program_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// A ROM file means direct mode; a folder (or no argument) means folder mode.
fn choose_mode(rom: Option<&PathBuf>) -> (Layout, Option<PathBuf>) {
    match rom {
        Some(p) if !p.is_dir() => (Layout::direct(), Some(p.clone())),
        Some(dir) => (Layout::folder(dir), None),
        None => (Layout::folder(&program_dir()), None),
    }
}

fn run(args: Args) -> Result<(), String> {
    let (mut cfg, cfg_path) =
        Config::load(args.config.as_deref()).map_err(|errs| errs.join("\n"))?;
    apply_cli(&mut cfg, &args);
    match &cfg_path {
        Some(p) => eprintln!("config: {}", p.display()),
        None => eprintln!("config: none found, using defaults"),
    }
    let (layout, first_rom) = choose_mode(args.rom.as_ref());
    if args.record.is_some() && first_rom.is_none() {
        return Err("--record needs a ROM path (it records one game from a fresh boot)".into());
    }

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
        a.volume = cfg.volume;
    }
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
    let texture = creator
        .create_texture_streaming(
            PixelFormatEnum::ARGB8888,
            SCREEN_WIDTH as u32,
            SCREEN_HEIGHT as u32,
        )
        .map_err(|e| e.to_string())?;
    let events = sdl.event_pump()?;
    let mut fe = Frontend::new(canvas, texture, events, controllers, audio, cfg);
    let opts = PlayOpts {
        frames: args.frames,
        record: args.record.clone(),
    };

    // One game, files beside the ROM.
    if let Some(rom) = first_rom {
        game::play(&mut fe, &rom, &layout, &opts)?;
        return Ok(());
    }

    // The game list: pick, play, come back.
    let games_dir = layout
        .games_dir()
        .ok_or("internal error: folder mode without a games folder")?;
    let mut menu = Menu::new(library::scan(&games_dir));
    loop {
        match menu::run(&mut fe, &mut menu)? {
            Choice::Quit => break,
            Choice::Play(rom) => match game::play(&mut fe, &rom, &layout, &opts) {
                Ok(Exit::Quit) => break,
                Ok(Exit::Menu) => {}
                // A game that cannot be loaded is shown on the list screen, not fatal.
                Err(e) => {
                    eprintln!("error: {e}");
                    menu.set_message(e);
                }
            },
        }
        // The folder may have changed while we played (a ROM copied in).
        menu.refresh(library::scan(&games_dir));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(a: &[&str]) -> Result<Args, String> {
        parse_args(a.iter().map(|s| s.to_string()))
    }

    #[test]
    fn command_line_overrides_the_config() {
        let mut cfg = Config::parse("scale = 6\nfullscreen = true\n").unwrap();
        apply_cli(&mut cfg, &parse(&["x.gb"]).unwrap());
        assert_eq!(
            (cfg.scale, cfg.fullscreen),
            (6, true),
            "config beats default"
        );
        apply_cli(
            &mut cfg,
            &parse(&["x.gb", "--scale", "2", "--swap-ab"]).unwrap(),
        );
        assert_eq!((cfg.scale, cfg.swap_ab), (2, true), "flag beats config");
        assert!(parse(&["x.gb", "--config"]).is_err());
    }

    #[test]
    fn record_option_is_parsed() {
        let args = parse(&["x.gb", "--record", "run.txt"]);
        assert_eq!(args.unwrap().record, Some(PathBuf::from("run.txt")));
        assert!(parse(&["x.gb", "--record"]).is_err());
    }

    #[test]
    fn no_argument_means_the_game_list() {
        assert_eq!(parse(&[]).unwrap().rom, None);
        assert_eq!(parse(&["--fullscreen"]).unwrap().rom, None);
        assert!(parse(&["a.gb", "b.gb"]).is_err());
    }

    #[test]
    fn a_rom_path_is_direct_mode_and_a_folder_is_folder_mode() {
        let (l, rom) = choose_mode(Some(&PathBuf::from("does/not/exist.gb")));
        assert!(!l.is_folder() && rom.is_some());
        let dir = std::env::temp_dir();
        let (l, rom) = choose_mode(Some(&dir));
        assert!(l.is_folder() && rom.is_none());
        assert_eq!(l.games_dir(), Some(dir.join("games")));
        let (l, rom) = choose_mode(None);
        assert!(l.is_folder() && rom.is_none());
    }
}
