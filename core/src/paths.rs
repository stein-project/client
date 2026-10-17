/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

//! The Stein Client folders.
//!
//! By default everything lives in `%APPDATA%\.steinclient` (on macOS and Linux, `~/.steinclient`). **Portable**: with
//! a file named `portable` next to the executable, the folder is the executable's own, so the whole modpack can be
//! copied to a USB stick or another computer, with worlds, options and mods. If the file holds a path (for example
//! `data\.steinclient`), the folder is that path instead, relative to the executable's folder.
//!
//! The folder has the layout of a `.minecraft` (versions, libraries, assets, mods, saves, options.txt): that is what
//! the Stein Loader installer and the game itself expect. Anything specific to the Stein Client lives in
//! `stein-client/`.

use std::path::{Path, PathBuf};

/// Name of the portable-install marker.
pub const PORTABLE_MARK: &str = "portable";

#[derive(Debug, Clone)]
pub struct Root {
    dir: PathBuf,
    portable: bool,
}

impl Root {
    /// Portable if the marker is next to the running executable (its folder, or the path the marker holds),
    /// otherwise the default.
    pub fn detect() -> Root {
        if let Some(exe_dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
            let mark = exe_dir.join(PORTABLE_MARK);
            if mark.is_file() {
                // An unreadable marker still means portable: the folder falls back to the executable's own.
                let text = std::fs::read(&mark).map(|b| marker_text(&b)).unwrap_or_default();
                return Root { dir: portable_dir(&exe_dir, &text), portable: true };
            }
        }
        Root { dir: default_dir(), portable: false }
    }

    /// A given folder (tests, command line).
    pub fn at(dir: impl Into<PathBuf>) -> Root {
        let dir: PathBuf = dir.into();
        // Always absolute: the game runs with the root as its working directory, so a relative path would point
        // somewhere else.
        let dir = std::path::absolute(&dir).unwrap_or(dir);
        let portable = dir.join(PORTABLE_MARK).is_file();
        Root { dir, portable }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn is_portable(&self) -> bool {
        self.portable
    }

    /// What belongs to the Stein Client alone: settings, the bundled Java, the installer cache, its logs.
    pub fn own(&self) -> PathBuf {
        self.dir.join("stein-client")
    }

    pub fn settings_file(&self) -> PathBuf {
        self.own().join("settings.json")
    }

    pub fn cache(&self) -> PathBuf {
        self.own().join("cache")
    }

    /// The launcher's logs (game.log, installer.log, launcher.log), next to the game's: logs/stein-client.
    pub fn logs(&self) -> PathBuf {
        self.dir.join("logs").join("stein-client")
    }

    pub fn versions(&self) -> PathBuf {
        self.dir.join("versions")
    }

    pub fn libraries(&self) -> PathBuf {
        self.dir.join("libraries")
    }

    pub fn assets(&self) -> PathBuf {
        self.dir.join("assets")
    }

    pub fn mods(&self) -> PathBuf {
        self.dir.join("mods")
    }
}

/// The marker's text whatever saved it: PowerShell 5.1's `>` writes UTF-16, and Notepad may save it as ANSI.
fn marker_text(bytes: &[u8]) -> String {
    if let Some(utf16) = bytes.strip_prefix(&[0xff, 0xfe]) {
        let units: Vec<u16> = utf16.as_chunks::<2>().0.iter().map(|&c| u16::from_le_bytes(c)).collect();
        return String::from_utf16_lossy(&units);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

/// The folder of a portable install: the first non-empty line of the marker, relative to the executable's folder
/// (an absolute path is taken as is), or the executable's folder itself when the marker is empty.
fn portable_dir(exe_dir: &Path, marker: &str) -> PathBuf {
    // Notepad may save the marker with a BOM.
    let marker = marker.trim_start_matches('\u{feff}');
    let Some(line) = marker.lines().map(str::trim).find(|l| !l.is_empty()) else {
        return exe_dir.to_path_buf();
    };
    // Explorer's "Copy as path" wraps the path in quotes.
    let line = line.strip_prefix('"').and_then(|l| l.strip_suffix('"')).unwrap_or(line).trim();
    if line.is_empty() {
        return exe_dir.to_path_buf();
    }
    // Always absolute: the game runs with the root as its working directory.
    let dir = exe_dir.join(line);
    let dir = std::path::absolute(&dir).unwrap_or(dir);
    // Text that names a file (`echo portable > portable` points at the marker itself) cannot be the folder.
    if dir.is_file() {
        exe_dir.to_path_buf()
    } else {
        dir
    }
}

fn default_dir() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata).join(".steinclient");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".steinclient");
    }
    PathBuf::from(".steinclient")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exe_dir() -> PathBuf {
        std::path::absolute("portable-test").unwrap()
    }

    #[test]
    fn empty_marker_uses_the_executable_folder() {
        for text in ["", "\n", "  \r\n\t", "\u{feff}", "\"\"", "\" \""] {
            assert_eq!(portable_dir(&exe_dir(), text), exe_dir(), "{text:?}");
        }
    }

    #[test]
    fn relative_path_is_under_the_executable_folder() {
        let want = exe_dir().join("data").join(".steinclient");
        let texts = [
            "data/.steinclient",
            "\u{feff}  data/.steinclient  \r\n",
            "\n\ndata/.steinclient\nignored",
            "\"data/.steinclient\"",
        ];
        for text in texts {
            assert_eq!(portable_dir(&exe_dir(), text), want, "{text:?}");
        }
    }

    #[test]
    fn a_file_is_not_the_folder() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert_eq!(portable_dir(crate_dir, "Cargo.toml\n"), crate_dir);
    }

    #[test]
    fn marker_text_from_utf8_utf16_and_ansi() {
        assert_eq!(marker_text(b"data/x\r\n"), "data/x\r\n");
        let utf16: Vec<u8> =
            [0xff, 0xfe].into_iter().chain("data/x".encode_utf16().flat_map(u16::to_le_bytes)).collect();
        assert_eq!(marker_text(&utf16), "data/x");
        // ANSI "Usuário": the accent is lost but the rest of the path is kept.
        assert_eq!(marker_text(b"Usu\xe1rio"), "Usu\u{fffd}rio");
    }

    #[cfg(windows)]
    #[test]
    fn backslashes_and_parent_folders() {
        assert_eq!(portable_dir(&exe_dir(), r"data\.steinclient"), exe_dir().join("data").join(".steinclient"));
        assert_eq!(portable_dir(&exe_dir(), r"..\stein-data"), exe_dir().parent().unwrap().join("stein-data"));
    }

    #[test]
    fn absolute_path_is_kept() {
        let abs = std::path::absolute("elsewhere").unwrap().join("game");
        assert_eq!(portable_dir(&exe_dir(), abs.to_str().unwrap()), abs);
    }
}
