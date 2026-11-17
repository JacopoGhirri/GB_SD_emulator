//! Where the frontend keeps its files.
//!
//! * **Direct mode** (`gb-sdl game.gb`): everything goes beside the ROM, as before:
//!   `game.gb.sav`, `game.gb.state1`, `game.gb-shot-001.png`.
//! * **Folder mode** (`gb-sdl` or `gb-sdl <folder>`): the folder holds the program and a
//!   few sub-folders, so it stays tidy.
//!
//! ```text
//! <root>/games/        your ROMs (.gb / .gbc), the only thing the game list scans
//! <root>/saves/        <rom file name>.sav and <rom file name>.state1 .. 9
//! <root>/screenshots/  <rom file name>-shot-001.png ...
//! ```
//! Sub-folders are created only when the first file is written into them.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Layout {
    /// `Some(root)` in folder mode.
    root: Option<PathBuf>,
}

impl Layout {
    pub fn direct() -> Layout {
        Layout { root: None }
    }

    pub fn folder(root: &Path) -> Layout {
        Layout {
            root: Some(root.to_path_buf()),
        }
    }

    pub fn is_folder(&self) -> bool {
        self.root.is_some()
    }

    pub fn games_dir(&self) -> Option<PathBuf> {
        self.root.as_ref().map(|r| r.join("games"))
    }

    /// `<name>` with `suffix` appended, placed in `sub` (folder mode) or beside the ROM.
    fn sibling(&self, rom: &Path, sub: &str, suffix: &str) -> PathBuf {
        let name = rom.file_name().unwrap_or(rom.as_os_str());
        let mut file = name.to_owned();
        file.push(suffix);
        match &self.root {
            Some(root) => root.join(sub).join(file),
            None => {
                let mut s = rom.as_os_str().to_owned();
                s.push(suffix);
                PathBuf::from(s)
            }
        }
    }

    pub fn save_path(&self, rom: &Path) -> PathBuf {
        self.sibling(rom, "saves", ".sav")
    }

    pub fn state_path(&self, rom: &Path, slot: u8) -> PathBuf {
        self.sibling(rom, "saves", &format!(".state{slot}"))
    }

    /// First unused screenshot file name.
    pub fn next_screenshot_path(&self, rom: &Path) -> PathBuf {
        (1..)
            .map(|n| self.sibling(rom, "screenshots", &format!("-shot-{n:03}.png")))
            .find(|p| !p.exists())
            .expect("an unused screenshot number exists")
    }

    /// Folder mode only: a save that still sits beside the ROM (from before the `saves/`
    /// folder existed, or copied there by hand) is moved into `saves/`. Returns the path the
    /// save should be read from and written to; if the move fails, the old location is kept
    /// for this session so nothing is lost.
    pub fn adopt_legacy_save(&self, rom: &Path) -> PathBuf {
        let new = self.save_path(rom);
        if !self.is_folder() || new.exists() {
            return new;
        }
        let mut old = rom.as_os_str().to_owned();
        old.push(".sav");
        let old = PathBuf::from(old);
        if !old.is_file() {
            return new;
        }
        let moved = new
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::rename(&old, &new));
        match moved {
            Ok(()) => {
                eprintln!("moved {} to {}", old.display(), new.display());
                new
            }
            Err(e) => {
                eprintln!(
                    "warning: could not move {}: {e}; using it where it is",
                    old.display()
                );
                old
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gb-sdl-layout-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn direct_mode_keeps_files_beside_the_rom() {
        let l = Layout::direct();
        let rom = Path::new("roms/pokemon.gb");
        assert_eq!(l.save_path(rom), PathBuf::from("roms/pokemon.gb.sav"));
        assert_eq!(
            l.state_path(rom, 3),
            PathBuf::from("roms/pokemon.gb.state3")
        );
        assert!(l.games_dir().is_none());
    }

    #[test]
    fn folder_mode_uses_the_subfolders() {
        let l = Layout::folder(Path::new("/x/gb_emu"));
        let rom = Path::new("/x/gb_emu/games/gold.gbc");
        assert_eq!(l.games_dir(), Some(PathBuf::from("/x/gb_emu/games")));
        assert_eq!(
            l.save_path(rom),
            PathBuf::from("/x/gb_emu/saves/gold.gbc.sav")
        );
        assert_eq!(
            l.state_path(rom, 2),
            PathBuf::from("/x/gb_emu/saves/gold.gbc.state2")
        );
        assert_eq!(
            l.next_screenshot_path(rom),
            PathBuf::from("/x/gb_emu/screenshots/gold.gbc-shot-001.png")
        );
    }

    #[test]
    fn screenshot_numbers_skip_existing_files() {
        let dir = temp_dir("shots");
        let l = Layout::folder(&dir);
        let rom = Path::new("a.gb");
        fs::create_dir_all(dir.join("screenshots")).unwrap();
        fs::write(dir.join("screenshots/a.gb-shot-001.png"), b"x").unwrap();
        fs::write(dir.join("screenshots/a.gb-shot-002.png"), b"x").unwrap();
        assert_eq!(
            l.next_screenshot_path(rom),
            dir.join("screenshots/a.gb-shot-003.png")
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_save_beside_the_rom_moves_into_saves() {
        let dir = temp_dir("legacy");
        fs::create_dir_all(dir.join("games")).unwrap();
        let rom = dir.join("games/red.gb");
        fs::write(&rom, b"rom").unwrap();
        fs::write(dir.join("games/red.gb.sav"), b"save").unwrap();
        let l = Layout::folder(&dir);
        let path = l.adopt_legacy_save(&rom);
        assert_eq!(path, dir.join("saves/red.gb.sav"));
        assert_eq!(fs::read(&path).unwrap(), b"save");
        assert!(!dir.join("games/red.gb.sav").exists(), "moved, not copied");
        // A second call changes nothing.
        assert_eq!(l.adopt_legacy_save(&rom), path);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn an_existing_new_save_is_never_overwritten_by_an_old_one() {
        let dir = temp_dir("both");
        fs::create_dir_all(dir.join("games")).unwrap();
        fs::create_dir_all(dir.join("saves")).unwrap();
        let rom = dir.join("games/red.gb");
        fs::write(dir.join("games/red.gb.sav"), b"old").unwrap();
        fs::write(dir.join("saves/red.gb.sav"), b"new").unwrap();
        let path = Layout::folder(&dir).adopt_legacy_save(&rom);
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(fs::read(dir.join("games/red.gb.sav")).unwrap(), b"old");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn direct_mode_never_moves_anything() {
        let rom = Path::new("somewhere/x.gb");
        assert_eq!(
            Layout::direct().adopt_legacy_save(rom),
            PathBuf::from("somewhere/x.gb.sav")
        );
    }

    #[test]
    fn no_save_means_no_directories_are_created() {
        let dir = temp_dir("lazy");
        let rom = dir.join("games/none.gb");
        let _ = Layout::folder(&dir).adopt_legacy_save(&rom);
        assert!(!dir.join("saves").exists());
        let _ = fs::remove_dir_all(dir);
    }
}
