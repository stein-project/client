/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

//! The Stein Client from the command line, for testing the core without a UI.
//!
//!   stein-cli [--dir DIR] prepare            download and install whatever is missing
//!   stein-cli [--dir DIR] login              sign in with a Microsoft account (device code)
//!   stein-cli [--dir DIR] status             what the next prepare will install, update or remove
//!   stein-cli [--dir DIR] sources            the third-party mod sources
//!   stein-cli [--dir DIR] source add LINK|JSON    add a third-party source (without the UI's warning)
//!   stein-cli [--dir DIR] source remove LINK      remove a source (its mods stay in the folder)
//!   stein-cli [--dir DIR] migrate            run the migration of mods that moved from official to third-party
//!   stein-cli [--dir DIR] accounts           the saved accounts
//!   stein-cli [--dir DIR] play [UUID]        prepare, sign in (no code, from the keychain) and start the game
//!   stein-cli [--dir DIR] command [UUID]     only print the game command line (the token shows as ***)
//!   stein-cli help                           this list
//!
//! Without `--dir` it uses the launcher's own folder (`%APPDATA%\.steinclient`); `--dir` points it at a separate
//! one, which is how to test without touching your own install. Accounts always live in the shared Stein accounts
//! folder (`STEIN_AUTH_DIR` overrides it).

use std::sync::Arc;

use anyhow::{bail, Result};
use stein_core::launch::Session;
use stein_core::paths::Root;
use stein_core::progress::{Progress, Report};
use stein_core::settings::Settings;

/// The commands, as listed in the module documentation above.
const USAGE: &str = r"usage: stein-cli [--dir DIR] COMMAND

  prepare                 download and install whatever is missing
  login                   sign in with a Microsoft account (device code)
  status                  what the next prepare will install, update or remove
  sources                 the third-party mod sources
  source add LINK|JSON    add a third-party source (without the UI's warning)
  source remove LINK      remove a source (its mods stay in the folder)
  migrate                 run the migration of mods that moved from official to third-party
  accounts                the saved accounts
  play [UUID]             prepare, sign in (no code, from the keychain) and start the game
  command [UUID]          only print the game command line (the token shows as ***)
  help                    this list

--dir DIR uses a separate folder instead of the launcher's own (%APPDATA%\.steinclient).
Accounts live in the shared Stein accounts folder; STEIN_AUTH_DIR overrides it.";

#[tokio::main]
async fn main() -> Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let root = if args.first().map(String::as_str) == Some("--dir") {
        let dir = args.get(1).cloned().unwrap_or_default();
        args.drain(..2);
        Root::at(dir)
    } else {
        Root::detect()
    };
    if matches!(args.first().map(String::as_str), Some("help" | "--help" | "-h")) {
        println!("{USAGE}");
        return Ok(());
    }
    let report: Report = Arc::new(|p: Progress| {
        if p.total > 0 {
            eprintln!("{} {}/{} {}", p.step, p.done, p.total, p.detail);
        } else {
            eprintln!("{} {}", p.step, p.detail);
        }
    });
    let settings = Settings::load(&root);
    let store = stein_core::accounts(&root);
    match args.first().map(String::as_str) {
        Some("prepare") => {
            let p = stein_core::prepare(&root, &report).await?;
            println!(
                "ready: Stein Loader {} in {}{}",
                p.loader,
                root.dir().display(),
                if p.offline { " (offline)" } else { "" }
            );
        }
        Some("login") => {
            let (code, pending) = stein_core::auth::begin().await?;
            println!("Open {} and enter the code {}", code.verification_uri, code.user_code);
            let c = pending.finish(&store).await?;
            println!("signed in: {} ({})", c.username, c.uuid);
        }
        Some("status") => {
            let index = stein_core::stein::index(&stein_core::net::client(), &root).await?;
            match stein_core::stein::components(&root) {
                Some((bridge, loader)) => println!("installed: Stein Bridge {bridge}, Stein Loader (library) {loader}"),
                None => println!("Stein Loader not installed"),
            }
            let changes = stein_core::stein::pending(&root, &index, &settings.mods);
            if changes.is_empty() {
                println!("up to date");
            }
            for c in changes {
                println!(
                    "{}: {} -> {}",
                    c.name,
                    c.from.as_deref().unwrap_or("(none)"),
                    c.to.as_deref().unwrap_or("(remove)")
                );
            }
        }
        Some("sources") => {
            for s in stein_core::sources::third_party(&root) {
                println!("{} ({}) {}", s.name, s.author, s.url);
            }
        }
        Some("source") => {
            let what = args.get(2).cloned().unwrap_or_default();
            match args.get(1).map(String::as_str) {
                Some("add") => {
                    let http = stein_core::net::client();
                    let url = stein_core::sources::url_from_paste(&what)?;
                    let f = stein_core::sources::fetch(&http, &url, true).await?;
                    for m in &f.mods {
                        println!("{} {} ({})", m.name, m.version, m.mod_id);
                    }
                    stein_core::sources::add(&root, &f)?;
                    println!("source added: {} by {}", f.name, f.author);
                }
                Some("remove") => stein_core::sources::remove(&root, &what)?,
                _ => bail!("usage: source add LINK|JSON, source remove LINK"),
            }
        }
        Some("migrate") => match stein_core::sources::migrate(&root)? {
            Some(n) => println!("{n}"),
            None => println!("nothing to migrate"),
        },
        Some("accounts") => {
            for a in store.accounts() {
                println!("{} {}", a.uuid, a.name);
            }
        }
        Some("play") => {
            let uuid = match args.get(1) {
                Some(u) => u.clone(),
                None => match store.accounts().first() {
                    Some(a) => a.uuid.clone(),
                    None => bail!("no account: run login first"),
                },
            };
            stein_core::prepare(&root, &report).await?;
            let c = stein_core::auth::refresh(&store, &uuid).await?;
            let mut child = stein_core::launch(
                &root,
                &Session { name: c.username, uuid: c.uuid, access_token: c.access_token, offline: false },
            )?;
            println!("game started (pid {})", child.id());
            let status = child.wait()?;
            println!("game exited: {status}");
        }
        Some("command") => {
            let uuid = args.get(1).cloned().unwrap_or_else(|| "0".repeat(32));
            let cmd = stein_core::launch::command(
                &root,
                &Session { name: "Player".into(), uuid, access_token: "***".into(), offline: false },
                stein_core::VERSION,
                false,
            )?;
            println!("{:?}", cmd);
        }
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
    Ok(())
}
