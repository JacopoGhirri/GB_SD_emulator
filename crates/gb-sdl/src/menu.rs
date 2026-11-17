//! The game list: a screen drawn in the same 160x144 picture as the games, so it uses the same
//! window, scaling and pacing. Keyboard (arrows, Enter, Esc) and gamepad (D-pad or left stick,
//! A / Start to play, B to quit).

use std::path::PathBuf;

use gb_core::{SCREEN_HEIGHT, SCREEN_WIDTH};
use sdl2::controller::{Axis, Button};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;

use crate::font::{self, ADVANCE};
use crate::frontend::Frontend;
use crate::input::STICK_DEADZONE;
use crate::library::Game;
use crate::pacing::Pacer;

const BG: u32 = 0x0008_1820;
const FG: u32 = 0x00E0_F8D0;
const DIM: u32 = 0x0088_C070;
const BAR: u32 = 0x0034_6856;

const W: usize = SCREEN_WIDTH;
const H: usize = SCREEN_HEIGHT;
const LIST_TOP: i32 = 16;
const ROW_H: i32 = 9;
/// Rows visible at once.
pub const ROWS: usize = 12;
/// Characters that fit in a row after the cursor column.
const NAME_CHARS: usize = (W - 12 - 4) / ADVANCE;

pub enum Choice {
    Play(PathBuf),
    Quit,
}

pub struct Menu {
    games: Vec<Game>,
    /// Set when the games folder could not be read at all.
    folder_missing: bool,
    selected: usize,
    top: usize,
    message: Option<String>,
}

impl Menu {
    pub fn new(scan: Result<Vec<Game>, String>) -> Menu {
        let mut m = Menu {
            games: Vec::new(),
            folder_missing: false,
            selected: 0,
            top: 0,
            message: None,
        };
        m.refresh(scan);
        m
    }

    /// Replace the list (after coming back from a game, or at start), keeping the cursor on
    /// the same game when it is still there.
    pub fn refresh(&mut self, scan: Result<Vec<Game>, String>) {
        let keep = self.selected().map(|g| g.path.clone());
        match scan {
            Ok(games) => {
                self.games = games;
                self.folder_missing = false;
            }
            Err(e) => {
                eprintln!("warning: {e}");
                self.games.clear();
                self.folder_missing = true;
            }
        }
        self.selected = keep
            .and_then(|p| self.games.iter().position(|g| g.path == p))
            .unwrap_or(0);
        self.top = 0;
        self.scroll_to_selection();
    }

    pub fn selected(&self) -> Option<&Game> {
        self.games.get(self.selected)
    }

    pub fn set_message(&mut self, msg: String) {
        self.message = Some(msg);
    }

    pub fn clear_message(&mut self) {
        self.message = None;
    }

    fn scroll_to_selection(&mut self) {
        if self.selected < self.top {
            self.top = self.selected;
        } else if self.selected >= self.top + ROWS {
            self.top = self.selected + 1 - ROWS;
        }
    }

    /// Move the cursor by one row, wrapping from the last game to the first and back.
    pub fn step(&mut self, delta: i32) {
        let n = self.games.len() as i32;
        if n == 0 {
            return;
        }
        self.selected = (self.selected as i32 + delta).rem_euclid(n) as usize;
        self.scroll_to_selection();
    }

    /// Move by a page, stopping at the ends.
    pub fn page(&mut self, delta: i32) {
        if self.games.is_empty() {
            return;
        }
        let last = self.games.len() - 1;
        let by = delta * (ROWS as i32 - 1);
        self.selected = (self.selected as i32 + by).clamp(0, last as i32) as usize;
        self.scroll_to_selection();
    }

    pub fn jump_to(&mut self, end: bool) {
        if !self.games.is_empty() {
            self.selected = if end { self.games.len() - 1 } else { 0 };
            self.scroll_to_selection();
        }
    }

    pub fn render(&self, buf: &mut [u32]) {
        buf.fill(BG);
        font::draw_text(buf, W, 4, 3, "GB-SDL", FG);
        if !self.games.is_empty() {
            let count = format!("{}/{}", self.selected + 1, self.games.len());
            let x = (W - 4 - font::text_width(&count)) as i32;
            font::draw_text(buf, W, x, 3, &count, DIM);
        }
        hline(buf, 12, DIM);
        hline(buf, 127, DIM);

        if self.games.is_empty() {
            let lines: &[&str] = if self.folder_missing {
                &[
                    "NO GAMES FOLDER",
                    "",
                    "MAKE A FOLDER CALLED",
                    "GAMES NEXT TO THE",
                    "PROGRAM AND PUT",
                    "YOUR ROMS IN IT",
                ]
            } else {
                &[
                    "NO GAMES FOUND",
                    "",
                    "PUT .GB OR .GBC FILES",
                    "IN THE GAMES FOLDER",
                ]
            };
            for (i, line) in lines.iter().enumerate() {
                let x = ((W - font::text_width(line)) / 2) as i32;
                font::draw_text(buf, W, x, 40 + 10 * i as i32, line, FG);
            }
        }

        let visible = self.games.iter().enumerate().skip(self.top).take(ROWS);
        for (row, (i, game)) in visible.enumerate() {
            let y = LIST_TOP + row as i32 * ROW_H;
            if i == self.selected {
                fill(buf, 2, y - 1, W - 4, ROW_H as usize, BAR);
                font::draw_text(buf, W, 4, y, ">", FG);
            }
            let colour = if i == self.selected { FG } else { DIM };
            font::draw_text(buf, W, 12, y, &fit(&game.name, NAME_CHARS), colour);
        }

        let footer = match &self.message {
            Some(m) => fit(m, (W - 8) / ADVANCE),
            None => "A:PLAY   B:QUIT".to_string(),
        };
        font::draw_text(
            buf,
            W,
            4,
            132,
            &footer,
            if self.message.is_some() { FG } else { DIM },
        );
    }
}

/// Shorten `name` to at most `max` characters, ending in `..` when it was cut.
fn fit(name: &str, max: usize) -> String {
    if name.chars().count() <= max {
        name.to_string()
    } else {
        let head: String = name.chars().take(max.saturating_sub(2)).collect();
        format!("{head}..")
    }
}

fn hline(buf: &mut [u32], y: usize, colour: u32) {
    buf[y * W + 4..y * W + W - 4].fill(colour);
}

fn fill(buf: &mut [u32], x: usize, y: i32, w: usize, h: usize, colour: u32) {
    for row in y.max(0) as usize..((y.max(0) as usize) + h).min(H) {
        buf[row * W + x..row * W + x + w].fill(colour);
    }
}

/// Auto-repeat for a held Up/Down: fires at once, then after a delay at a steady rate
/// (counted in frames, so it needs no clock).
#[derive(Default)]
pub struct Repeater {
    dir: i32,
    frames: u32,
}

impl Repeater {
    const DELAY: u32 = 22;
    const RATE: u32 = 4;

    /// Call once per frame with the held direction (-1, 0 or 1). Returns the step to take.
    pub fn tick(&mut self, dir: i32) -> i32 {
        if dir != self.dir {
            self.dir = dir;
            self.frames = 0;
            return dir;
        }
        if dir == 0 {
            return 0;
        }
        self.frames += 1;
        if self.frames >= Self::DELAY && (self.frames - Self::DELAY).is_multiple_of(Self::RATE) {
            dir
        } else {
            0
        }
    }
}

/// Show the list until the user picks a game or quits.
pub fn run(fe: &mut Frontend, menu: &mut Menu) -> Result<Choice, String> {
    let mut pacer = Pacer::new();
    let mut buf = vec![BG; W * H];
    let mut repeater = Repeater::default();
    let (mut key_up, mut key_down, mut pad_up, mut pad_down, mut stick) =
        (false, false, false, false, 0i32);
    fe.reset_title();
    loop {
        let events: Vec<Event> = fe.events.poll_iter().collect();
        for event in events {
            fe.pad_event(&event);
            let mut play = false;
            match event {
                Event::Quit { .. } => return Ok(Choice::Quit),
                Event::KeyDown {
                    keycode: Some(k),
                    repeat: false,
                    ..
                } => {
                    menu.clear_message();
                    match k {
                        Keycode::Up => key_up = true,
                        Keycode::Down => key_down = true,
                        Keycode::PageUp | Keycode::Left => menu.page(-1),
                        Keycode::PageDown | Keycode::Right => menu.page(1),
                        Keycode::Home => menu.jump_to(false),
                        Keycode::End => menu.jump_to(true),
                        Keycode::Return | Keycode::KpEnter | Keycode::Space => play = true,
                        Keycode::Escape => return Ok(Choice::Quit),
                        _ => {}
                    }
                }
                Event::KeyUp {
                    keycode: Some(k), ..
                } => match k {
                    Keycode::Up => key_up = false,
                    Keycode::Down => key_down = false,
                    _ => {}
                },
                Event::ControllerButtonDown { button, .. } => {
                    menu.clear_message();
                    match button {
                        Button::DPadUp => pad_up = true,
                        Button::DPadDown => pad_down = true,
                        Button::DPadLeft | Button::LeftShoulder => menu.page(-1),
                        Button::DPadRight | Button::RightShoulder => menu.page(1),
                        Button::A | Button::Start => play = true,
                        Button::B | Button::Back => return Ok(Choice::Quit),
                        _ => {}
                    }
                }
                Event::ControllerButtonUp { button, .. } => match button {
                    Button::DPadUp => pad_up = false,
                    Button::DPadDown => pad_down = false,
                    _ => {}
                },
                Event::ControllerAxisMotion {
                    axis: Axis::LeftY,
                    value,
                    ..
                } => {
                    stick = if value > STICK_DEADZONE {
                        1
                    } else if value < -STICK_DEADZONE {
                        -1
                    } else {
                        0
                    };
                }
                _ => {}
            }
            if play {
                if let Some(game) = menu.selected() {
                    return Ok(Choice::Play(game.path.clone()));
                }
            }
        }
        let held = i32::from(key_down || pad_down) - i32::from(key_up || pad_up) + stick;
        let step = repeater.tick(held.clamp(-1, 1));
        if step != 0 {
            menu.step(step);
        }

        menu.render(&mut buf);
        fe.present(&buf)?;
        pacer.wait_for_next_frame(1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn games(n: usize) -> Vec<Game> {
        (0..n)
            .map(|i| Game {
                path: Path::new("games").join(format!("g{i:02}.gb")),
                name: format!("game {i:02}"),
            })
            .collect()
    }

    fn picture(menu: &Menu) -> Vec<u32> {
        let mut buf = vec![0; W * H];
        menu.render(&mut buf);
        buf
    }

    fn pixel(buf: &[u32], x: usize, y: usize) -> u32 {
        buf[y * W + x]
    }

    #[test]
    fn cursor_wraps_around_both_ways() {
        let mut m = Menu::new(Ok(games(3)));
        m.step(-1);
        assert_eq!(m.selected().unwrap().name, "game 02");
        m.step(1);
        assert_eq!(m.selected().unwrap().name, "game 00");
    }

    #[test]
    fn paging_stops_at_the_ends_and_the_list_scrolls_with_the_cursor() {
        let mut m = Menu::new(Ok(games(30)));
        m.page(1);
        assert_eq!(m.selected, ROWS - 1);
        m.page(1);
        m.page(1);
        m.page(1);
        assert_eq!(m.selected, 29, "clamped to the last game");
        assert_eq!(m.top, 30 - ROWS, "scrolled so the cursor is visible");
        m.page(-1);
        m.page(-1);
        m.page(-1);
        m.page(-1);
        assert_eq!((m.selected, m.top), (0, 0));
        m.jump_to(true);
        assert_eq!(m.selected, 29);
    }

    #[test]
    fn stepping_down_past_the_bottom_row_scrolls_one_row_at_a_time() {
        let mut m = Menu::new(Ok(games(20)));
        for _ in 0..ROWS {
            m.step(1);
        }
        assert_eq!((m.selected, m.top), (ROWS, 1));
    }

    #[test]
    fn refresh_keeps_the_cursor_on_the_same_game() {
        let mut m = Menu::new(Ok(games(5)));
        m.step(1);
        m.step(1);
        let mut more = games(5);
        more.insert(
            0,
            Game {
                path: "games/aaa.gb".into(),
                name: "aaa".into(),
            },
        );
        m.refresh(Ok(more));
        assert_eq!(m.selected().unwrap().name, "game 02");
        m.refresh(Ok(vec![]));
        assert!(m.selected().is_none());
        m.step(1); // no panic on an empty list
        m.page(1);
        m.jump_to(true);
    }

    #[test]
    fn selected_row_has_the_highlight_bar_and_others_do_not() {
        let mut m = Menu::new(Ok(games(3)));
        m.step(1);
        let buf = picture(&m);
        let row = |i: i32| (LIST_TOP + i * ROW_H - 1) as usize;
        assert_eq!(pixel(&buf, 3, row(1)), BAR, "selected row 1 is highlighted");
        assert_eq!(pixel(&buf, 3, row(0)), BG);
        assert_eq!(pixel(&buf, 3, row(2)), BG);
    }

    #[test]
    fn names_and_footer_are_drawn() {
        let m = Menu::new(Ok(games(2)));
        let buf = picture(&m);
        let lit = |y0: usize, y1: usize| {
            (y0..y1).any(|y| (0..W).any(|x| matches!(pixel(&buf, x, y), FG | DIM)))
        };
        assert!(lit(3, 10), "title");
        assert!(lit(16, 24), "first game");
        assert!(lit(132, 139), "footer");
    }

    #[test]
    fn empty_and_missing_folder_screens_say_so() {
        let differs = |m: &Menu| {
            let blank = Menu::new(Ok(games(1)));
            let (a, mut b) = (picture(m), picture(&blank));
            // Clear the single game and footer of the blank so only the message area is compared.
            b[40 * W..100 * W].fill(BG);
            a[40 * W..100 * W] != b[40 * W..100 * W]
        };
        assert!(differs(&Menu::new(Ok(vec![]))));
        assert!(differs(&Menu::new(Err("cannot read games".into()))));
    }

    #[test]
    fn long_names_are_cut_with_dots_and_fit_the_screen() {
        assert_eq!(fit("short", 24), "short");
        let cut = fit(&"X".repeat(40), 24);
        assert_eq!(cut.chars().count(), 24);
        assert!(cut.ends_with(".."));
        assert!(font::text_width(&cut) + 12 <= W - 4);
    }

    #[test]
    fn a_message_replaces_the_footer_until_cleared() {
        let mut m = Menu::new(Ok(games(1)));
        let plain = picture(&m);
        m.set_message("CANNOT LOAD X".into());
        assert_ne!(picture(&m), plain);
        m.clear_message();
        assert_eq!(picture(&m), plain);
    }

    #[test]
    fn repeat_fires_at_once_then_after_a_delay_then_steadily() {
        let mut r = Repeater::default();
        let mut at = Vec::new();
        for frame in 0..40u32 {
            if r.tick(1) != 0 {
                at.push(frame);
            }
        }
        assert_eq!(at, [0, 22, 26, 30, 34, 38]);
        assert_eq!(r.tick(0), 0);
        assert_eq!(r.tick(-1), -1, "a new direction fires at once");
    }

    /// `GB_SDL_DUMP_MENU=file.png cargo test -p gb-sdl dump_menu` writes a picture of the list
    /// (used for the README and for eyeballing the layout). Does nothing otherwise.
    #[test]
    fn dump_menu_picture_when_asked() {
        let Some(path) = std::env::var_os("GB_SDL_DUMP_MENU") else {
            return;
        };
        let names = [
            "Alleyway",
            "Dr Mario",
            "Kirby's Dream Land",
            "Metroid II - Return of Samus",
            "Pokemon Blue",
            "Pokemon Gold",
            "Pokemon Red",
            "Tetris",
            "Wario Land",
            "Zelda - Link's Awakening",
        ];
        let list = names
            .iter()
            .map(|n| Game {
                path: Path::new("games").join(format!("{n}.gb")),
                name: (*n).to_string(),
            })
            .collect();
        let mut m = Menu::new(Ok(list));
        m.step(6);
        let png = crate::png::encode_rgb(W as u32, H as u32, &picture(&m));
        std::fs::write(path, png).unwrap();
    }
}
