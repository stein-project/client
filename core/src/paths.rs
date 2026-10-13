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
//! copied to a USB stick or another computer, with worlds, options and mods.
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
    /// The folder of the running executable: portable if the marker is next to it, otherwise the default.
    pub fn detect() -> Root {
        if let Some(exe_dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
            if exe_dir.join(PORTABLE_MARK).is_file() {
                return Root { dir: exe_dir, portable: true };
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

fn default_dir() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata).join(".steinclient");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".steinclient");
    }
    PathBuf::from(".steinclient")
}
