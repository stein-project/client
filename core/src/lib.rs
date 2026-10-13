/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

//! The Stein Client core, without a UI: the launcher (`launcher/`) is the interface on top of this, and `stein-cli`
//! exercises everything from the command line.
//!
//! [`prepare`] gets the folder ready to play: the original Minecraft 1.8.9 (Mojang), the latest Stein Loader release
//! (installed by its own installer, which patches on the player's machine) and the chosen official mods. [`launch`]
//! starts the game with the account.

pub mod friendly;
pub mod launch;
pub mod mojang;
pub mod net;
pub mod paths;
pub mod progress;
pub mod settings;
pub mod sources;
pub mod stein;
pub mod steinlog;
pub mod update;

pub use stein_client_auth as auth;

use anyhow::Result;
use serde::Serialize;

use crate::paths::Root;
use crate::progress::Report;
use crate::settings::Settings;

/// The Stein Client version (the launcher's and `stein-cli`'s).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// How preparation ended.
#[derive(Debug, Clone, Serialize)]
pub struct Prepared {
    /// The Stein Loader version ready in the folder.
    pub loader: String,
    /// No internet; going ahead with what was already installed.
    pub offline: bool,
}

/// Gets everything ready to play. With no internet but an existing install, carries on with it (`offline`).
pub async fn prepare(root: &Root, report: &Report) -> Result<Prepared> {
    std::fs::create_dir_all(root.own())?;
    default_options(root)?;
    // First of all: the migration changes the enabled mods (mods that became third-party, see sources::migrate).
    sources::migrate(root)?;
    let mods = Settings::load(root).mods;
    let installed = stein::installed(root);
    if let Err(e) = mojang::ensure(root, report).await {
        if installed.is_none() || !mojang::present(root) {
            return Err(e);
        }
        return Ok(offline(installed.unwrap()));
    }
    let http = net::client();
    let index = match stein::index(&http, root).await {
        Ok(i) => i,
        Err(e) => {
            return match installed {
                Some(i) => Ok(offline(i)),
                None => Err(e),
            }
        }
    };
    stein::remember_index(root, &index);
    stein::ensure(root, &index, &mods, report).await?;
    Ok(Prepared { loader: index.loader.clone(), offline: false })
}

/// The game options Stein picked (graphics, keys, sound...) for first-time players in this folder. The player's file
/// is never replaced: it is only written when it does not exist yet. The server list is not included.
const DEFAULT_OPTIONS: &str = include_str!("../defaults/options.txt");

fn default_options(root: &Root) -> Result<()> {
    let path = root.dir().join("options.txt");
    if !path.exists() {
        std::fs::write(path, DEFAULT_OPTIONS)?;
    }
    Ok(())
}

fn offline(i: stein::Installed) -> Prepared {
    Prepared { loader: i.version, offline: true }
}

/// Can the game start right now (without downloading anything)?
pub fn ready(root: &Root) -> bool {
    mojang::present(root) && stein::installed(root).is_some() && stein::game_java(root).is_file()
}

/// The Stein accounts (the shared folder, outside this install).
pub fn accounts(root: &Root) -> auth::Store {
    // Accounts belong to Stein (%APPDATA%\.steinauth), not to this install: they serve the launcher, the bots and
    // whatever comes next. The list from the first test builds, inside .steinclient, is imported once.
    let store = auth::Store::shared();
    store.import_legacy(&root.own());
    store
}

/// Starts the game with the session.
pub fn launch(root: &Root, session: &launch::Session) -> Result<std::process::Child> {
    // A normal start puts the Stein log setting back to what the player had (if a "restart with logs" turned it on).
    let _ = steinlog::restore(root);
    launch::spawn(root, session, VERSION, false)
}

/// Same, with Stein debugging on (`-Dstein.debug=all`) and the Stein log enabled for this run only: the
/// "restart with Stein logs" after a crash.
pub fn launch_debug(root: &Root, session: &launch::Session) -> Result<std::process::Child> {
    steinlog::enable_once(root)?;
    launch::spawn(root, session, VERSION, true)
}
