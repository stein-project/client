/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

//! The little the Stein Client stores: which official mods are enabled and which account signs in. Deliberately no
//! more options: everything else comes ready.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::paths::Root;

/// Official mods enabled out of the box when the registry has not said which (`defaultMods` in its `stein.json`): only
/// a reserve for a first start without internet.
pub const DEFAULT_MODS: &[&str] = &["hud", "connect", "keybind", "replay"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Enabled mods: the index id for official mods, and the mod's own internal id for third-party ones.
    pub mods: Vec<String>,
    /// The uuid of the account that signs in. The account list is `accounts.json` in the shared Stein accounts folder
    /// (see `stein_client_auth::Store`); only each account's secret is in the system vault.
    pub account: Option<String>,
    /// Close the launcher when the game opens (the default; otherwise it only minimises). While closed it keeps
    /// watching the game and comes back if the game crashes. Stored under a different key than before so the new
    /// default applies to everyone.
    #[serde(rename = "close_launcher")]
    pub close_on_play: bool,
    /// Stein GFX, the new renderer, as chosen with the settings toggle (`-Dsteinbridge.gfx`); `None`: the default
    /// (on).
    pub gfx: Option<bool>,
    /// This machine has no OpenGL 4.1: the game closed on startup with Stein GFX, so the launcher now starts it
    /// without. Holds until the player turns it back on with the toggle.
    pub gfx_unsupported: bool,
    /// Migrations already applied in this folder (each runs once).
    pub migrations: Vec<String>,
    /// A notice shown once in the launcher (from a migration) until the player dismisses it.
    pub notice: Option<String>,
    /// Developer options: extra Java arguments for the game (the Stein `-D` flags), added last so they win over the
    /// launcher's own.
    pub jvm_args: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            mods: DEFAULT_MODS.iter().map(|s| s.to_string()).collect(),
            account: None,
            close_on_play: true,
            gfx: None,
            gfx_unsupported: false,
            migrations: Vec::new(),
            jvm_args: String::new(),
            notice: None,
        }
    }
}

/// The official mods enabled out of the box: the registry's `defaultMods` (from the last index read, or the stored
/// `stein.json`), or [`DEFAULT_MODS`] when there is neither.
pub fn default_mods(root: &Root) -> Vec<String> {
    let from_index = crate::stein::cached_index(root).map(|i| i.default_mods).filter(|m| !m.is_empty());
    let from_registry = || {
        crate::registry::cached_json(root, "stein.json")?["defaultMods"]
            .as_array()
            .map(|a| a.iter().filter_map(|m| m.as_str()).map(str::to_string).collect::<Vec<_>>())
            .filter(|m| !m.is_empty())
    };
    from_index.or_else(from_registry).unwrap_or_else(|| DEFAULT_MODS.iter().map(|s| s.to_string()).collect())
}

impl Settings {
    /// The saved settings; without them, the defaults with the official mods the registry enables out of the box.
    pub fn load(root: &Root) -> Settings {
        std::fs::read(root.settings_file())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_else(|| Settings { mods: default_mods(root), ..Settings::default() })
    }

    /// Writes atomically (temporary file, then rename) so the settings are never left half-written.
    pub fn save(&self, root: &Root) -> Result<()> {
        let path = root.settings_file();
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }
}
