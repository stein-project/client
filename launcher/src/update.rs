/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

//! The launcher's self-update: swapping its own executable for a newer release on startup.

use std::sync::atomic::Ordering;

use stein_core::paths::Root;
use tauri::{AppHandle, Emitter, Manager};

use crate::App;

/// On startup the launcher updates itself. When there is a newer release, it tells the UI, downloads the new
/// executable (checked against the release's sha256) and swaps it in place: Windows allows renaming a running
/// executable, so the current one becomes `.old` and the new one takes its name. Then it starts the new one and
/// exits. The `.old` file is deleted on the next start. Without internet, or if the swap fails, it carries on with
/// the current version. There is no separate updater any more: it used to be a second executable just for this swap
/// (the one from versions up to 0.1.2, still left in the folder, is deleted here).
///
/// The new executable also checks the release on startup, so swaps chain up to the latest version: an old launcher
/// whose release only leads to an intermediate version (e.g. one that leads to 0.2.7) still reaches the
/// latest one in the same start. `hops` counts the swaps already done in this start; the [`MAX_UPDATE_HOPS`] cap
/// stops an endless loop (a release whose executable does not report the version in update.json).
///
/// Debug builds never update themselves: a contributor's `target/debug` executable is never replaced by a release.
pub fn self_update(handle: AppHandle, root: Root, hops: u32) {
    if cfg!(debug_assertions) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let Ok(exe) = std::env::current_exe() else { return };
        let clean = stein_core::update::clean_old;
        clean(&exe);
        let _ = std::fs::remove_file(stein_core::update::updater_exe(&root));
        let http = stein_core::net::client();
        let Ok(m) = stein_core::update::manifest(&http, &root).await else { return };
        if !stein_core::update::is_newer(&m.version, stein_core::VERSION)
            || stein_core::update::is_current(&m, "SteinClient", &exe).unwrap_or(true)
        {
            // Up to date. The .old file from the previous swap may still be in use for a moment: try again.
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            clean(&exe);
            return;
        }
        let app = handle.state::<App>();
        if app.busy.load(Ordering::SeqCst) || app.running.load(Ordering::SeqCst) {
            return; // already playing or preparing: leave it for the next start
        }
        let _ = handle.emit("self-update", m.version.clone());
        match stein_core::update::replace_file(&http, &m, "SteinClient", &exe, None).await {
            Ok(_) => {
                let next = (hops + 1).to_string();
                if std::process::Command::new(&exe).args(["--update-hop", &next]).spawn().is_ok() {
                    handle.exit(0);
                }
            }
            Err(e) => {
                stein_core::friendly::logged(&root, "launcher.log", &e.context("updating the Stein Client"));
                let _ = handle.emit("self-update", "");
            }
        }
    });
}

/// How many chained swaps [`self_update`] may still do in a single start.
pub const MAX_UPDATE_HOPS: u32 = 4;

/// The swaps already done in this start: a launcher that replaced itself relaunches the new one with
/// `--update-hop N`. Up to 0.2.8 it relaunched with `--no-update`, which counts as one.
pub fn hops() -> u32 {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--update-hop") {
        return args.get(i + 1).and_then(|n| n.parse().ok()).unwrap_or(MAX_UPDATE_HOPS);
    }
    u32::from(args.iter().any(|a| a == "--no-update"))
}
