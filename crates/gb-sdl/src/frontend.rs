//! The window, audio device and gamepads that live for the whole run, shared by the game list
//! and the game itself.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use gb_core::{SCREEN_HEIGHT, SCREEN_WIDTH};
use sdl2::controller::GameController;
use sdl2::event::Event;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, Texture};
use sdl2::video::{FullscreenType, Window};
use sdl2::{EventPump, GameControllerSubsystem};

use crate::audio::Audio;
use crate::config::Config;
use crate::video;

const TITLE: &str = "gb-sdl";

pub struct Frontend<'a> {
    pub canvas: Canvas<Window>,
    texture: Texture<'a>,
    pub events: EventPump,
    controllers: GameControllerSubsystem,
    pads: HashMap<u32, GameController>,
    pub audio: Option<Audio>,
    pub cfg: Config,
    pub notice: Notice,
    pixels: Vec<u8>,
}

impl<'a> Frontend<'a> {
    pub fn new(
        canvas: Canvas<Window>,
        texture: Texture<'a>,
        events: EventPump,
        controllers: GameControllerSubsystem,
        audio: Option<Audio>,
        cfg: Config,
    ) -> Frontend<'a> {
        Frontend {
            canvas,
            texture,
            events,
            controllers,
            pads: HashMap::new(),
            audio,
            cfg,
            notice: Notice::default(),
            pixels: vec![0u8; SCREEN_WIDTH * SCREEN_HEIGHT * 4],
        }
    }

    /// Show one 160x144 picture (`0x00RRGGBB`), scaled by the largest whole number that fits.
    pub fn present(&mut self, frame: &[u32]) -> Result<(), String> {
        for (dst, px) in self.pixels.as_chunks_mut::<4>().0.iter_mut().zip(frame) {
            *dst = px.to_le_bytes(); // ARGB8888 little-endian: B, G, R, A
        }
        self.texture
            .update(None, &self.pixels, SCREEN_WIDTH * 4)
            .map_err(|e| e.to_string())?;
        let (ww, wh) = self.canvas.output_size()?;
        let (x, y, rw, rh) = video::dest_rect(ww, wh);
        self.canvas.set_draw_color(Color::RGB(0, 0, 0));
        self.canvas.clear();
        self.canvas
            .copy(&self.texture, None, Rect::new(x, y, rw, rh))?;
        self.canvas.present();
        Ok(())
    }

    /// Track gamepads being plugged in and out. Returns true when a pad was removed (the
    /// caller should release any buttons it was holding).
    pub fn pad_event(&mut self, event: &Event) -> bool {
        match event {
            Event::ControllerDeviceAdded { which, .. } => match self.controllers.open(*which) {
                Ok(pad) => {
                    eprintln!("gamepad connected: {}", pad.name());
                    self.pads.insert(pad.instance_id(), pad);
                }
                Err(e) => eprintln!("warning: cannot open gamepad {which}: {e}"),
            },
            Event::ControllerDeviceRemoved { which, .. } if self.pads.remove(which).is_some() => {
                eprintln!("gamepad disconnected");
                return true;
            }
            _ => {}
        }
        false
    }

    pub fn toggle_fullscreen(&mut self) -> Result<(), String> {
        let win = self.canvas.window_mut();
        let next = if win.fullscreen_state() == FullscreenType::Off {
            FullscreenType::Desktop
        } else {
            FullscreenType::Off
        };
        win.set_fullscreen(next)
    }

    pub fn reset_title(&mut self) {
        let _ = self.canvas.window_mut().set_title(TITLE);
        self.notice.until = None;
    }
}

/// Short status messages (the game picture has no text): printed to stderr and shown in the
/// window title for a few seconds.
#[derive(Default)]
pub struct Notice {
    until: Option<Instant>,
}

impl Notice {
    pub fn show(&mut self, canvas: &mut Canvas<Window>, msg: &str) {
        eprintln!("{msg}");
        let _ = canvas.window_mut().set_title(&format!("{TITLE} - {msg}"));
        self.until = Some(Instant::now() + Duration::from_secs(3));
    }

    pub fn tick(&mut self, canvas: &mut Canvas<Window>) {
        if self.until.is_some_and(|t| Instant::now() >= t) {
            self.until = None;
            let _ = canvas.window_mut().set_title(TITLE);
        }
    }
}
