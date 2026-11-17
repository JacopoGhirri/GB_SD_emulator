//! Finding the games to list: `.gb` and `.gbc` files under the `games/` folder.

use std::fs;
use std::path::{Path, PathBuf};

/// Sub-folders are searched this many levels down, so a messy tree cannot make the scan slow.
const MAX_DEPTH: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub path: PathBuf,
    /// File name without the extension, shown in the list.
    pub name: String,
}

fn is_game_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("gb") || e.eq_ignore_ascii_case("gbc"))
}

/// All games under `dir`, sorted by name (case-insensitive) so the list order never changes
/// between runs. Hidden files and folders (leading `.`) are skipped. Unreadable sub-folders
/// are skipped; an unreadable `dir` itself is an error with a message fit to show on screen.
pub fn scan(dir: &Path) -> Result<Vec<Game>, String> {
    let mut games = Vec::new();
    let entries = fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    collect(entries, 0, &mut games);
    games.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.path.cmp(&b.path))
    });
    Ok(games)
}

fn collect(entries: fs::ReadDir, depth: usize, out: &mut Vec<Game>) {
    for entry in entries.flatten() {
        let path = entry.path();
        let hidden = entry.file_name().to_string_lossy().starts_with('.');
        if hidden {
            continue;
        }
        // `metadata` follows symlinks, so a linked ROM or folder works too.
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            if depth < MAX_DEPTH {
                if let Ok(sub) = fs::read_dir(&path) {
                    collect(sub, depth + 1, out);
                }
            }
        } else if meta.is_file() && is_game_file(&path) {
            let name = path
                .file_stem()
                .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
            out.push(Game { path, name });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gb-sdl-lib-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"x").unwrap();
    }

    fn names(games: &[Game]) -> Vec<&str> {
        games.iter().map(|g| g.name.as_str()).collect()
    }

    #[test]
    fn finds_gb_and_gbc_case_insensitively_and_sorts_by_name() {
        let d = temp_dir("basic");
        for f in ["Tetris.gb", "gold.GBC", "Alleyway.gb", "red.Gb"] {
            touch(&d.join(f));
        }
        // Not games: saves, states, screenshots, other consoles' files, no extension.
        for f in [
            "red.gb.sav",
            "red.gb.state1",
            "shot.png",
            "x.gba",
            "README",
            "notes.txt",
        ] {
            touch(&d.join(f));
        }
        let games = scan(&d).unwrap();
        assert_eq!(names(&games), ["Alleyway", "gold", "red", "Tetris"]);
        assert_eq!(games[1].path, d.join("gold.GBC"));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn looks_in_subfolders_but_not_too_deep_and_skips_hidden() {
        let d = temp_dir("nested");
        touch(&d.join("a/one.gb"));
        touch(&d.join("a/b/c/d/four_deep.gb")); // depth 4: beyond MAX_DEPTH
        touch(&d.join("a/b/c/three.gb")); // depth 3: still found
        touch(&d.join(".hidden/secret.gb"));
        touch(&d.join(".dot.gb"));
        let games = scan(&d).unwrap();
        assert_eq!(names(&games), ["one", "three"]);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn order_is_stable_for_equal_names() {
        let d = temp_dir("dupes");
        touch(&d.join("x/game.gb"));
        touch(&d.join("y/game.gb"));
        let a = scan(&d).unwrap();
        let b = scan(&d).unwrap();
        assert_eq!(a, b);
        assert!(a[0].path < a[1].path);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn empty_folder_is_not_an_error_but_a_missing_one_is() {
        let d = temp_dir("empty");
        assert_eq!(scan(&d).unwrap(), []);
        let err = scan(&d.join("nope")).unwrap_err();
        assert!(err.starts_with("cannot read "), "{err}");
        let _ = fs::remove_dir_all(d);
    }
}
