/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

//! Starting the game: the command line comes from the version JSON written by the Stein Loader installer (libraries,
//! main class and arguments with `${...}` variables), plus the Java arguments of its launcher profile (the memory
//! policy), with the Microsoft account session.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{anyhow, Context, Result};
use serde_json::Value;

use crate::mojang::{allowed, native_classifier};
use crate::paths::Root;
use crate::stein::{game_java, VERSION_ID};

/// The session the game signs in with: the Microsoft account's (the token is never written to any file).
#[derive(Clone)]
pub struct Session {
    pub name: String,
    /// Without dashes, as Mojang returns it.
    pub uuid: String,
    pub access_token: String,
    /// Offline account (only allowed with an original account in the launcher): the game signs in as "legacy",
    /// without a real token.
    pub offline: bool,
}

/// Prints everything but the token, so a `{:?}` in a log or an error never leaks it.
impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("name", &self.name)
            .field("uuid", &self.uuid)
            .field("access_token", &"***")
            .field("offline", &self.offline)
            .finish()
    }
}

/// The profile the installer writes; its Java arguments are the game's.
const PROFILE_NAME: &str = "Stein Loader 1.8.9";
/// The game's memory cap in the Stein Client. The launcher decides, not the profile: it also applies to anyone who
/// installed when the cap was different, and there is no option for it (the launcher comes ready).
pub const GAME_RAM_GB: u32 = 3;
/// Used if the profile is not found (it should be): the same policy the installer writes.
const DEFAULT_JAVA_ARGS: &str =
    "-Xmx3G -Xms256M -XX:SoftMaxHeapSize=768M -XX:+UseZGC -XX:-ZProactive --enable-native-access=ALL-UNNAMED";

/// Builds the game command. `client_version` is passed to the game as `-Dstein.client` (only a marker that the game
/// came from the Stein Client; the game changes nothing because of it).
pub fn command(root: &Root, session: &Session, client_version: &str, debug: bool) -> Result<std::process::Command> {
    let vdir = root.versions().join(VERSION_ID);
    let json: Value = serde_json::from_slice(
        &std::fs::read(vdir.join(format!("{VERSION_ID}.json"))).context("the Stein Loader is not installed")?,
    )?;

    let mut classpath = Vec::new();
    let natives = vdir.join("natives-steinclient");
    let _ = std::fs::remove_dir_all(&natives);
    std::fs::create_dir_all(&natives)?;
    for lib in json["libraries"].as_array().ok_or_else(|| anyhow!("version has no libraries"))? {
        if !allowed(lib) {
            continue;
        }
        if let Some(classifier) = native_classifier(lib) {
            // Native with a classifier (jinput, twitch): extracted, not put on the classpath.
            let path = lib["downloads"]["classifiers"][&classifier]["path"]
                .as_str()
                .map(|p| root.libraries().join(p))
                .or_else(|| maven_path(root, lib["name"].as_str()?, Some(&classifier)));
            if let Some(p) = path.filter(|p| p.is_file()) {
                extract_natives(&p, &natives)?;
            }
            continue;
        }
        let path = match lib["downloads"]["artifact"]["path"].as_str() {
            Some(p) => root.libraries().join(p),
            None => maven_path(root, lib["name"].as_str().unwrap_or_default(), None)
                .ok_or_else(|| anyhow!("library without a name in the JSON"))?,
        };
        classpath.push(path);
    }
    classpath.push(vdir.join(format!("{VERSION_ID}.jar")));
    let sep = if cfg!(windows) { ";" } else { ":" };
    let cp = classpath.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>().join(sep);

    let assets_index = json["assetIndex"]["id"].as_str().or(json["assets"].as_str()).unwrap_or("1.8").to_string();
    let vars = |s: &str| -> String {
        s.replace("${auth_player_name}", &session.name)
            .replace("${version_name}", "1.8.9")
            .replace("${game_directory}", &root.dir().to_string_lossy())
            .replace("${assets_root}", &root.assets().to_string_lossy())
            .replace("${assets_index_name}", &assets_index)
            .replace("${auth_uuid}", &session.uuid)
            .replace("${auth_access_token}", &session.access_token)
            .replace("${user_properties}", "{}")
            .replace("${user_type}", if session.offline { "legacy" } else { "msa" })
            .replace("${natives_directory}", &natives.to_string_lossy())
            .replace("${launcher_name}", "stein-client")
            .replace("${launcher_version}", client_version)
            .replace("${classpath}", &cp)
    };

    let mut cmd = std::process::Command::new(game_java(root));
    cmd.current_dir(root.dir());
    for a in profile_java_args(root).split_whitespace() {
        if a.starts_with("-Xmx") {
            cmd.arg(format!("-Xmx{GAME_RAM_GB}G"));
        } else {
            cmd.arg(a);
        }
    }
    cmd.arg(format!("-Dstein.client={client_version}"));
    let settings = crate::settings::Settings::load(root);
    // Stein GFX: see [`gfx`].
    cmd.arg(format!("-Dsteinbridge.gfx={}", gfx(&settings)));
    if debug {
        cmd.arg("-Dstein.debug=all");
    }
    for a in json["arguments"]["jvm"].as_array().into_iter().flatten() {
        if let Some(s) = a.as_str() {
            cmd.arg(vars(s));
        }
    }
    cmd.arg(json["mainClass"].as_str().ok_or_else(|| anyhow!("version has no main class"))?);
    match json["arguments"]["game"].as_array() {
        Some(args) => {
            for a in args {
                if let Some(s) = a.as_str() {
                    cmd.arg(vars(s));
                }
            }
        }
        None => {
            for a in json["minecraftArguments"].as_str().unwrap_or("").split_whitespace() {
                cmd.arg(vars(a));
            }
        }
    }
    Ok(cmd)
}

/// Starts the game, with its output in `stein-client/logs/game.log`.
pub fn spawn(root: &Root, session: &Session, client_version: &str, debug: bool) -> Result<std::process::Child> {
    let mut cmd = command(root, session, client_version, debug)?;
    std::fs::create_dir_all(root.logs())?;
    let log = std::fs::File::create(root.logs().join("game.log"))?;
    cmd.stdout(Stdio::from(log.try_clone()?)).stderr(Stdio::from(log)).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console
    }
    cmd.spawn().context("could not start the game")
}

/// The Java arguments of the profile the installer wrote.
fn profile_java_args(root: &Root) -> String {
    let read = || -> Option<String> {
        let v: Value = serde_json::from_slice(&std::fs::read(root.dir().join("launcher_profiles.json")).ok()?).ok()?;
        v["profiles"].as_object()?.values().find(|p| p["name"] == PROFILE_NAME)?["javaArgs"]
            .as_str()
            .map(str::to_string)
    };
    read().unwrap_or_else(|| DEFAULT_JAVA_ARGS.to_string())
}

/// The Maven path of a library from its name ("group:artifact:version"), the way the installer stores ours.
fn maven_path(root: &Root, name: &str, classifier: Option<&str>) -> Option<PathBuf> {
    let mut parts = name.split(':');
    let (group, artifact, version) = (parts.next()?, parts.next()?, parts.next()?);
    let file = match classifier {
        Some(c) => format!("{artifact}-{version}-{c}.jar"),
        None => format!("{artifact}-{version}.jar"),
    };
    Some(root.libraries().join(group.replace('.', "/")).join(artifact).join(version).join(file))
}

fn extract_natives(jar: &Path, dir: &Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(jar)?)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(rel) = entry.enclosed_name() else { continue };
        if entry.is_dir() || rel.starts_with("META-INF") {
            continue;
        }
        let out = dir.join(rel);
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::io::copy(&mut entry, &mut std::fs::File::create(out)?)?;
    }
    Ok(())
}

/// The Stein GFX setting the game starts with. On a machine without OpenGL 4.1 (the game already closed on startup
/// because of it, see [`gfx_unsupported_crash`]), off; otherwise the launcher toggle, if the player touched it;
/// otherwise on.
pub fn gfx(settings: &crate::settings::Settings) -> bool {
    !settings.gfx_unsupported && settings.gfx.unwrap_or(true)
}

/// Did the game that just exited close on startup for lack of OpenGL 4.1 for Stein GFX? (Detected from the Stein
/// Bridge message in `logs/game.log`; both the Portuguese and English wordings are matched.)
pub fn gfx_unsupported_crash(root: &Root) -> bool {
    std::fs::read_to_string(root.logs().join("game.log"))
        .map(|log| log.contains("precisa de OpenGL 4.1") || log.contains("needs OpenGL 4.1"))
        .unwrap_or(false)
}
