/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

//! Starting the game: the command line comes from the version JSON written by the Stein Loader installer (libraries,
//! main class and arguments with `${...}` variables), plus the Java arguments (the memory policy) with the Microsoft
//! account session.
//!
//! The Java arguments come from the registry's `launch.json` (`ramGb`, `javaArgs`), or from the installer's launcher
//! profile without one; the registry's `compat.json` adds arguments for a given system or graphics card (matched
//! against `config/steinloader-gpu.json`, which the Loader writes on every start). Arguments from the registry go
//! through [`registry_arg_allowed`]: a signed file is still not a way to run code on a player's machine.

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
/// The game's memory cap in the Stein Client when the registry does not say (`ramGb` in `launch.json`). The launcher
/// decides, not the profile: it also applies to anyone who installed when the cap was different, and there is no
/// option for it (the launcher comes ready).
pub const GAME_RAM_GB: u32 = 3;

/// The game's memory cap: the registry's `ramGb` (1 to 32 GB), or [`GAME_RAM_GB`].
pub fn ram_gb(root: &Root) -> u32 {
    crate::registry::cached_json(root, "launch.json")
        .and_then(|j| j["ramGb"].as_u64())
        .filter(|g| (1..=32).contains(g))
        .map(|g| g as u32)
        .unwrap_or(GAME_RAM_GB)
}

/// The `-XX:` flags the registry may set: memory and the garbage collector only. Others are dangerous here
/// (`OnOutOfMemoryError` and `OnError` run a command, `Flags` and `VMOptionsFile` read more options from a file).
const XX_ALLOWED: &[&str] = &[
    "SoftMaxHeapSize",
    "MaxHeapSize",
    "InitialHeapSize",
    "MaxRAMPercentage",
    "InitialRAMPercentage",
    "MaxDirectMemorySize",
    "MaxMetaspaceSize",
    "MetaspaceSize",
    "ReservedCodeCacheSize",
    "UseZGC",
    "UseG1GC",
    "UseParallelGC",
    "UseSerialGC",
    "MaxGCPauseMillis",
    "ParallelGCThreads",
    "ConcGCThreads",
    "UseStringDeduplication",
    "AlwaysPreTouch",
    "UseCompressedOops",
];

/// Can a Java argument that came from the registry go to the game? Memory and garbage-collector flags, native access,
/// and `-D` properties of Stein's own components only: never an agent, a classpath, a command, another program's
/// property, nor the diagnostic properties (`steinbridge.diag.*`), which type, press keys and attack on their own.
pub fn registry_arg_allowed(arg: &str) -> bool {
    let property = |p: &str| {
        ["stein.", "steinbridge.", "steinloader."].iter().any(|ok| p.starts_with(ok))
            && !p.starts_with("steinbridge.diag.")
    };
    arg.starts_with("-Xmx")
        || arg.starts_with("-Xms")
        || arg.strip_prefix("-XX:").is_some_and(xx_allowed)
        || arg.starts_with("--enable-native-access=")
        || arg.strip_prefix("-D").is_some_and(property)
}

/// A `-XX:` flag (without the prefix) from [`XX_ALLOWED`], ZGC's (`Z...`) or G1's (`G1...`), with a plain value.
fn xx_allowed(flag: &str) -> bool {
    let flag = flag.strip_prefix(['+', '-']).unwrap_or(flag);
    let (name, value) = flag.split_once('=').unwrap_or((flag, ""));
    let known = XX_ALLOWED.contains(&name)
        || (name.len() > 1
            && (name.starts_with('Z') || name.starts_with("G1"))
            && name.chars().all(|c| c.is_ascii_alphanumeric()));
    known && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '%')
}

/// The registry's arguments, with the ones outside [`registry_arg_allowed`] dropped (and logged).
fn registry_args(root: &Root, args: &str, from: &str) -> Vec<String> {
    args.split_whitespace()
        .filter(|a| {
            let ok = registry_arg_allowed(a);
            if !ok {
                crate::registry::note(
                    root,
                    "launcher.log",
                    &format!("{from}: Java argument not allowed, dropped: {a}"),
                );
            }
            ok
        })
        .map(str::to_string)
        .collect()
}

/// The Java arguments the game starts with: the registry's `launch.json`, or the installer's profile without one.
fn base_java_args(root: &Root) -> Vec<String> {
    let registry = crate::registry::cached_json(root, "launch.json")
        .and_then(|j| j["javaArgs"].as_str().map(str::to_string))
        .filter(|a| !a.trim().is_empty());
    match registry {
        Some(a) => registry_args(root, &a, "launch.json"),
        None => profile_java_args(root).split_whitespace().map(str::to_string).collect(),
    }
}

/// What the game recorded about the machine on its last start (`config/steinloader-gpu.json`), all lowercase.
#[derive(Debug, Default)]
struct Machine {
    os: String,
    arch: String,
    vendor: String,
    renderer: String,
    version: String,
}

impl Machine {
    fn read(root: &Root) -> Machine {
        let gpu: Value = std::fs::read(root.dir().join("config").join("steinloader-gpu.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(Value::Null);
        let text = |k: &str| gpu[k].as_str().unwrap_or("").to_lowercase();
        Machine {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            vendor: text("vendor"),
            renderer: text("renderer"),
            version: text("version"),
        }
    }

    /// Does a `compat.json` rule's `when` match? Every field it has must be a case-insensitive substring of the
    /// machine's (a rule about the card never matches before the game has recorded one).
    fn matches(&self, when: &Value) -> bool {
        let Some(w) = when.as_object() else { return false };
        w.iter().all(|(k, v)| {
            let want = v.as_str().unwrap_or("").to_lowercase();
            let have = match k.as_str() {
                "os" => &self.os,
                "arch" => &self.arch,
                "gpuVendor" => &self.vendor,
                "gpuRenderer" => &self.renderer,
                "driver" => &self.version,
                _ => return false, // a condition this launcher does not know: the rule is not for it
            };
            !want.is_empty() && have.contains(&want)
        })
    }
}

/// The arguments of every `compat.json` rule that matches this machine, in the file's order.
fn compat_args(root: &Root) -> Vec<String> {
    let Some(compat) = crate::registry::cached_json(root, "compat.json") else { return Vec::new() };
    let machine = Machine::read(root);
    let mut out = Vec::new();
    for rule in compat["rules"].as_array().into_iter().flatten() {
        if machine.matches(&rule["when"]) {
            out.extend(registry_args(root, rule["javaArgs"].as_str().unwrap_or(""), "compat.json"));
        }
    }
    out
}
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
    let ram = ram_gb(root);
    for a in base_java_args(root) {
        if a.starts_with("-Xmx") {
            cmd.arg(format!("-Xmx{ram}G"));
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
    // The registry's arguments for this system or card, then the developer options: last, so a repeated -D wins
    // over the launcher's (Java keeps the last one).
    for a in compat_args(root) {
        cmd.arg(a);
    }
    for a in settings.jvm_args.split_whitespace() {
        cmd.arg(a);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_args_are_limited() {
        for ok in [
            "-Xmx4G",
            "-Xms256M",
            "-XX:+UseZGC",
            "-XX:-ZProactive",
            "-XX:SoftMaxHeapSize=768M",
            "-XX:MaxRAMPercentage=50.0",
            "--enable-native-access=ALL-UNNAMED",
            "-Dsteinbridge.gfx.gpuHeldItem=false",
            "-Dstein.debug=render",
            "-Dsteinloader.noAnnounce=true",
        ] {
            assert!(registry_arg_allowed(ok), "{ok}");
        }
        for bad in [
            "-javaagent:x.jar",
            "-agentpath:x.dll",
            "-XX:OnOutOfMemoryError=calc.exe",
            "-XX:OnError=cmd /c x",
            "-XX:Flags=x.txt",
            "-XX:VMOptionsFile=x.txt",
            "-XX:+UnlockDiagnosticVMOptions",
            "-XX:SoftMaxHeapSize=1G;x",
            "-Dsteinbridge.diag.chatFile=x.txt",
            "-Dsteinbridge.diag.keys=1:30:1",
            "-cp",
            "-Djava.library.path=x",
            "-Dsteinbridgex=1",
            "-Dstein",
            "-jar",
            "--add-opens=java.base/java.lang=ALL-UNNAMED",
        ] {
            assert!(!registry_arg_allowed(bad), "{bad}");
        }
    }

    #[test]
    fn compat_rules_match_the_recorded_card() {
        let m = Machine {
            os: "windows".into(),
            arch: "x86_64".into(),
            vendor: "nvidia corporation".into(),
            renderer: "nvidia geforce rtx 4060/pcie/sse2".into(),
            version: "4.6.0 nvidia 617.42".into(),
        };
        assert!(m.matches(&serde_json::json!({"gpuVendor": "NVIDIA"})));
        assert!(m.matches(&serde_json::json!({"os": "windows", "gpuRenderer": "RTX 40", "driver": "617."})));
        assert!(!m.matches(&serde_json::json!({"gpuVendor": "AMD"})));
        assert!(!m.matches(&serde_json::json!({"os": "macos"})));
        assert!(!m.matches(&serde_json::json!({"gpuVendor": ""}))); // an empty condition matches nothing
        assert!(!m.matches(&serde_json::json!({"cpu": "x"}))); // an unknown condition: not for this launcher
        assert!(!m.matches(&serde_json::json!("NVIDIA")));
        let none = Machine { os: "windows".into(), ..Machine::default() };
        assert!(!none.matches(&serde_json::json!({"gpuVendor": "NVIDIA"}))); // no card recorded yet
        assert!(none.matches(&serde_json::json!({"os": "windows"})));
    }

    #[test]
    fn launch_and_compat_from_the_registry() {
        let dir = std::env::temp_dir().join(format!("stein-launch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = Root::at(&dir);
        let reg = root.own().join("registry");
        std::fs::create_dir_all(&reg).unwrap();
        std::fs::create_dir_all(root.dir().join("config")).unwrap();
        assert_eq!(ram_gb(&root), GAME_RAM_GB);
        std::fs::write(
            reg.join("launch.json"),
            r#"{"schema":1,"ramGb":4,"javaArgs":"-Xmx4G -javaagent:evil.jar -XX:+UseZGC"}"#,
        )
        .unwrap();
        std::fs::write(
            reg.join("compat.json"),
            r#"{"schema":1,"rules":[{"when":{"gpuVendor":"nvidia"},"javaArgs":"-Dsteinbridge.gfx.gpuHeldItem=false -Dfoo=1"},
                                    {"when":{"gpuVendor":"amd"},"javaArgs":"-Dsteinbridge.x=1"}]}"#,
        )
        .unwrap();
        std::fs::write(root.dir().join("config").join("steinloader-gpu.json"), r#"{"vendor":"NVIDIA Corporation"}"#)
            .unwrap();
        assert_eq!(ram_gb(&root), 4);
        assert_eq!(base_java_args(&root), vec!["-Xmx4G".to_string(), "-XX:+UseZGC".to_string()]);
        assert_eq!(compat_args(&root), vec!["-Dsteinbridge.gfx.gpuHeldItem=false".to_string()]);
        let log = std::fs::read_to_string(root.logs().join("launcher.log")).unwrap();
        assert!(log.contains("-javaagent:evil.jar") && log.contains("-Dfoo=1"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
