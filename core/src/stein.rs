/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

//! The Stein Bridge and the Stein Loader in the folder: the latest published release, installed by the Stein Loader's
//! own installer in command-line mode, the same one that patches the game jar on the player's machine (EULA) and is
//! already verified for each version. This module only downloads and verifies what it needs, and decides when to run
//! it.
//!
//! Where each piece comes from:
//! - `stein.json` of the Stein registry ([`crate::registry`], signed): the latest version, the links, the official
//!   mods enabled by default and the `sources` list, the official mod indexes (`mods/<id>.json` of the registry);
//! - each mod's index (`steinmods.json`, [`crate::sources`]): version, file and sha256, for official mods (from the
//!   registry, signed) and for third-party ones the player added;
//! - `revoked.json` of the registry: mods that must not be installed (left out of the index);
//! - the `v<version>` release: the installer, the Java 25 runtime for each OS and `SHA256SUMS.txt`, which verifies
//!   both.
//!
//! The installer only handles the Loader (`--mods keep`); mods are downloaded here, straight into the mods folder,
//! verified by sha256 and by the id inside the mod. A `.steinmod` that is not in any index is never touched.
//!
//! A `stein.json` without `sources` (the format before 2026-10-12) carries the official mods in the `mods` list,
//! which is still there, frozen, for old launchers and installers.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::net::{self, Hash};
use crate::paths::Root;
use crate::progress::{Progress, Report};
use crate::{registry, sources};

/// The registry file with the index. Up to 0.3.2 it was `stein.json` of the installer's latest release (and up to
/// 0.3.0 of Stein-Project/stein-loader, frozen at Stein Loader 0.1.33).
const INDEX_FILE: &str = "stein.json";
const RELEASES: &str = "https://github.com/Stein-Project/installer/releases/download";
/// The version the installer creates in the folder.
pub const VERSION_ID: &str = "1.8.9-SteinLoader";
const INSTALL_MANIFEST: &str = "stein-loader-install.json";
/// The marker the installer puts in the Java it copied, with the zip's sha256 (Install.RUNTIME_MARK).
const RUNTIME_MARK: &str = "stein-runtime.txt";

/// An index mod: official or third-party.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexMod {
    /// The short id in the official index ("hud"); for a third-party mod, the id inside the mod.
    pub id: String,
    /// The id inside steinmod.json ("steinhud"): this is how the file in the mods folder is recognised.
    #[serde(default, rename = "modId")]
    pub mod_id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    /// The file in the mods folder, where to download it from, and its sha256.
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub sha256: String,
    /// The index it came from (`steinmods.json`); empty in the old `mods` list of `stein.json`.
    #[serde(default)]
    pub source: String,
    /// Third-party mod (from a source the player added).
    #[serde(default)]
    pub third_party: bool,
    /// The third-party source's name.
    #[serde(default)]
    pub source_name: String,
}

/// The index's news item (the notice in the game menu), in the player's language.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct News {
    pub title: String,
    pub text: String,
    pub url: String,
}

/// Addresses the project publishes (`links` in `stein.json`): the Discord invite and the player-head image service
/// (`{uuid}` and `{size}` are filled in).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Links {
    #[serde(default)]
    pub discord: String,
    #[serde(default)]
    pub heads: String,
}

/// The published index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub loader: String,
    pub mods: Vec<IndexMod>,
    #[serde(default)]
    pub news: Option<News>,
    #[serde(default)]
    pub links: Links,
    /// The official mods enabled for someone who never chose (`defaultMods`); empty in an older index.
    #[serde(default)]
    pub default_mods: Vec<String>,
}

impl Index {
    /// The Loader release tag for this index.
    pub fn loader_tag(&self) -> String {
        format!("v{}", self.loader)
    }
}

/// The published index. Mods come from each mod's own index: the official ones (`sources` in `stein.json`) and the
/// third-party ones; a third-party mod with the id of an official one is left out, and so is a revoked one. The
/// registry's other files (launch, compat, revoked, migrations) are refreshed along with it.
pub async fn index(http: &reqwest::Client, root: &Root) -> Result<Index> {
    let (raw, _) = futures_util::future::join(
        registry::load_json(http, root, INDEX_FILE),
        registry::refresh_auxiliary(http, root),
    )
    .await;
    let raw = raw?;
    let mut index = parse_index(&raw)?;
    let previous = cached_index(root).map(|i| i.mods).unwrap_or_default();
    let official: Vec<String> = raw["sources"]
        .as_array()
        .map(|a| a.iter().filter_map(|u| u.as_str()).filter(|u| sources::allowed_url(u)).map(str::to_string).collect())
        .unwrap_or_default();
    if !official.is_empty() {
        index.mods = sources::gather(http, root, &official, false, &previous).await;
    }
    let third: Vec<String> = sources::third_party(root).into_iter().map(|s| s.url).collect();
    for m in sources::gather(http, root, &third, true, &previous).await {
        if !index.mods.iter().any(|o| o.mod_id == m.mod_id || o.id == m.id) {
            index.mods.push(m);
        }
    }
    // Revoked (malicious or broken): never downloaded. A copy already in the folder is the player's and stays; with
    // the mod out of the index the launcher simply stops touching it.
    index.mods.retain(|m| match registry::revoked(root, &m.sha256) {
        Some(why) => {
            registry::note(
                root,
                "launcher.log",
                &format!("{} {} is revoked ({why}): not installed", m.name, m.version),
            );
            false
        }
        None => true,
    });
    Ok(index)
}

/// The index from a parsed `stein.json`.
fn parse_index(raw: &serde_json::Value) -> Result<Index> {
    let loader = raw["latest"]["version"]
        .as_str()
        .ok_or_else(|| anyhow!("the Stein Loader index does not give the latest version"))?
        .to_string();
    let mods: Vec<IndexMod> = serde_json::from_value(raw["mods"].clone()).unwrap_or_default();
    let n = &raw["news"];
    let lang = |v: &serde_json::Value| v["pt_BR"].as_str().or(v["en_US"].as_str()).unwrap_or("").to_string();
    let news = n.is_object().then(|| News {
        title: lang(&n["title"]),
        text: lang(&n["text"]),
        url: n["url"].as_str().unwrap_or("").to_string(),
    });
    let link = |k: &str| {
        raw["links"][k]
            .as_str()
            .or(if k == "discord" { raw["discord"].as_str() } else { None })
            .unwrap_or("")
            .to_string()
    };
    let links = Links { discord: link("discord"), heads: link("heads") };
    let default_mods = raw["defaultMods"]
        .as_array()
        .map(|a| a.iter().filter_map(|m| m.as_str()).map(str::to_string).collect())
        .unwrap_or_default();
    Ok(Index { loader, mods, news, links, default_mods })
}

/// The name of this OS's Java 25 asset in the release.
fn runtime_asset() -> &'static str {
    if cfg!(target_os = "windows") {
        "runtime-windows.zip"
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "runtime-macos-arm64.zip"
        } else {
            "runtime-macos.zip"
        }
    } else {
        "runtime-linux.zip"
    }
}

fn java_bin(runtime: &Path, windowless: bool) -> PathBuf {
    if cfg!(target_os = "windows") {
        runtime.join("bin").join(if windowless { "javaw.exe" } else { "java.exe" })
    } else {
        runtime.join("bin").join("java")
    }
}

/// The Stein Loader version installed in the folder (`stein-client/installed.json`, next to the installer's own
/// manifest). Files written by older launchers also carry a `mods` list, which is ignored.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Installed {
    pub version: String,
    /// The installed release tag ("v0.1.28", or a test build's "v0.1.28-rc.1" installed by an older launcher); empty
    /// in an old install.
    #[serde(default)]
    pub tag: String,
}

impl Installed {
    fn tag(&self) -> String {
        if self.tag.is_empty() {
            format!("v{}", self.version)
        } else {
            self.tag.clone()
        }
    }

    /// The version to display: the tag's ("0.1.28-rc.1" for a test build).
    pub fn label(&self) -> String {
        self.tag().trim_start_matches('v').to_string()
    }
}

fn installed_marker(root: &Root) -> PathBuf {
    root.own().join("installed.json")
}

pub fn installed(root: &Root) -> Option<Installed> {
    let man = root.versions().join(VERSION_ID).join(INSTALL_MANIFEST);
    if !man.is_file() || !root.versions().join(VERSION_ID).join(format!("{VERSION_ID}.json")).is_file() {
        return None;
    }
    let ours: Option<Installed> =
        std::fs::read(installed_marker(root)).ok().and_then(|b| serde_json::from_slice(&b).ok());
    let theirs: serde_json::Value = serde_json::from_slice(&std::fs::read(man).ok()?).ok()?;
    let version = theirs["version"].as_str()?.to_string();
    // The tag only counts if it belongs to the same version the installer recorded (another installer may have run
    // since).
    let tag = ours
        .as_ref()
        .map(|o| o.tag.clone())
        .filter(|t| *t == format!("v{version}") || t.starts_with(&format!("v{version}-")));
    Some(Installed { version, tag: tag.unwrap_or_default() })
}

/// Versions of the mods in the mods folder, keyed by the id inside each `.steinmod` (its steinmod.json).
pub fn mod_files(root: &Root) -> std::collections::HashMap<String, String> {
    mod_paths(root).into_iter().map(|(id, (v, _))| (id, v)).collect()
}

/// The mods in the folder: id inside the `.steinmod` -> (version, file).
fn mod_paths(root: &Root) -> std::collections::HashMap<String, (String, PathBuf)> {
    let mut out = std::collections::HashMap::new();
    for e in std::fs::read_dir(root.mods()).into_iter().flatten().flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("steinmod") {
            continue;
        }
        if let Some((id, v)) = mod_manifest(&p) {
            out.insert(id, (v, p.clone()));
        }
    }
    out
}

/// The id and version inside a `.steinmod` (its steinmod.json).
fn mod_manifest(p: &Path) -> Option<(String, String)> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(p).ok()?).ok()?;
    let json: serde_json::Value = serde_json::from_reader(zip.by_name("steinmod.json").ok()?).ok()?;
    Some((json["id"].as_str()?.to_string(), json["version"].as_str()?.to_string()))
}

/// What changes in an update: `from` the current version (none: it will be installed), `to` the new one (none: it
/// will be removed).
#[derive(Debug, Clone, Serialize)]
pub struct Change {
    /// "loader" or the mod's index id.
    pub id: String,
    pub name: String,
    pub from: Option<String>,
    pub to: Option<String>,
    /// Third-party mod.
    #[serde(default)]
    pub third_party: bool,
}

/// The enabled mod needs downloading: it is missing from the folder or at another version.
fn stale(m: &IndexMod, have: Option<&(String, PathBuf)>) -> bool {
    have.is_none_or(|(v, _)| *v != m.version)
}

/// What the next "Play" will install, update or remove, comparing the folder with the index: the Stein Loader and each
/// index mod, official or third-party (enabled ones that are missing or outdated, and disabled ones still in the
/// folder).
pub fn pending(root: &Root, index: &Index, selected: &[String]) -> Vec<Change> {
    let mut out = Vec::new();
    let now = installed(root);
    if now.as_ref().map(Installed::tag) != Some(index.loader_tag()) {
        let from = now.map(|i| i.tag().trim_start_matches('v').to_string());
        out.push(Change {
            id: "loader".into(),
            name: "Stein Loader".into(),
            from,
            to: Some(index.loader.clone()),
            third_party: false,
        });
    }
    let files = mod_paths(root);
    for m in &index.mods {
        let have = if m.mod_id.is_empty() { None } else { files.get(&m.mod_id) };
        let change = |to: Option<String>| Change {
            id: m.id.clone(),
            name: m.name.clone(),
            from: have.map(|(v, _)| v.clone()),
            to,
            third_party: m.third_party,
        };
        if !selected.contains(&m.id) {
            if have.is_some() {
                out.push(change(None));
            }
        } else if stale(m, have) {
            out.push(change(Some(m.version.clone())));
        }
    }
    out
}

/// The installer needs to run: nothing installed, or the Loader at another tag. Mods do not go through it
/// ([`sync_mods`]).
fn needs_installer(root: &Root, wanted: &Index) -> bool {
    installed(root).map(|now| now.tag() != wanted.loader_tag()).unwrap_or(true)
}

/// Makes the mods folder match the index and the player's choice: enabled mods that are missing or outdated are
/// downloaded directly (verified by sha256 and by the id inside the mod), and disabled ones are removed. Only mods
/// from some index are touched.
async fn sync_mods(
    root: &Root,
    wanted: &Index,
    mods: &[String],
    http: &reqwest::Client,
    report: &Report,
) -> Result<()> {
    let files = mod_paths(root);
    for m in &wanted.mods {
        let have = if m.mod_id.is_empty() { None } else { files.get(&m.mod_id) };
        if !mods.contains(&m.id) {
            if let Some((_, path)) = have {
                std::fs::remove_file(path).with_context(|| format!("could not remove {}", path.display()))?;
            }
            continue;
        }
        if !stale(m, have) || !sources::valid_mod(m) {
            continue;
        }
        let dest = root.mods().join(&m.file);
        // The file name belongs to another mod already in the folder: do not overwrite it.
        if let Some((other, _)) = files.iter().find(|(id, (_, p))| *p == dest && **id != m.mod_id) {
            bail!(
                "{} was not installed: the file {} in the mods folder belongs to another mod ({other})",
                m.name,
                m.file
            );
        }
        let step = format!("Baixando {} {}", m.name, m.version);
        download_counted(http, &m.url, &dest, &Hash::Sha256(m.sha256.clone()), &step, report).await?;
        // The mod that arrived must be the one the index named (an index cannot swap one mod for another).
        let inside = mod_manifest(&dest).map(|(id, _)| id);
        if inside.as_deref() != Some(m.mod_id.as_str()) {
            let _ = std::fs::remove_file(&dest);
            bail!("{} was not installed: the downloaded file is not the mod {}", m.name, m.mod_id);
        }
        if let Some((_, old)) = have.filter(|(_, p)| *p != dest) {
            let _ = std::fs::remove_file(old);
        }
    }
    Ok(())
}

/// The libraries the installed Stein Loader uses: (Stein Bridge, Stein Loader), read from the version JSON.
pub fn components(root: &Root) -> Option<(String, String)> {
    let path = root.versions().join(VERSION_ID).join(format!("{VERSION_ID}.json"));
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let lib = |name: &str| {
        json["libraries"].as_array()?.iter().find_map(|l| {
            let n = l["name"].as_str()?;
            n.strip_prefix(&format!("dev.xavier.stein:{name}:")).map(str::to_string)
        })
    };
    Some((lib("steinbridge")?, lib("steinloader")?))
}

/// Leaves the folder with Stein Loader `wanted.loader` and the enabled mods `mods` (index ids). Only acts when there
/// is something to change ([`pending`]): the installer for a new Loader, and mods directly ([`sync_mods`]).
pub async fn ensure(root: &Root, wanted: &Index, mods: &[String], report: &Report) -> Result<()> {
    let mut mods: Vec<String> = mods.to_vec();
    mods.sort();
    if installed(root).is_some() && pending(root, wanted, &mods).is_empty() {
        return Ok(());
    }
    let http = net::client();
    if needs_installer(root, wanted) {
        install(root, wanted, &http, report).await?;
    }
    sync_mods(root, wanted, &mods, &http, report).await
}

/// Runs the installer from the Loader release (`wanted.loader_tag()`) in the folder, with the bundled Java.
async fn install(root: &Root, wanted: &Index, http: &reqwest::Client, report: &Report) -> Result<()> {
    let http = http.clone();
    let tag = wanted.loader_tag();
    let base = format!("{RELEASES}/{tag}");
    report(Progress::new("Conferindo o Stein Loader"));
    let sums = sums(&net::fetch(&http, &format!("{base}/SHA256SUMS.txt")).await?);
    let sum = |name: &str| -> Result<String> {
        sums.get(name).cloned().ok_or_else(|| anyhow!("{name} missing from SHA256SUMS.txt of {tag}"))
    };

    // Java 25: the tool copy (runs the installer) and the game's, inside the version, with the installer's marker so
    // it does not download the 50 MB again. The installer refuses to run with the version's Java ("game open"),
    // hence two copies.
    let rt_name = runtime_asset();
    let rt_sha = sum(rt_name)?;
    let tool = root.own().join("runtime");
    let game_rt = root.versions().join(VERSION_ID).join("runtime");
    let need_tool = !runtime_ok(&tool, &rt_sha);
    let need_game = !runtime_ok(&game_rt, &rt_sha);
    if need_tool || need_game {
        let zip = root.cache().join(rt_name);
        if !Hash::Sha256(rt_sha.clone()).matches_file(&zip) {
            download_counted(
                &http,
                &format!("{base}/{rt_name}"),
                &zip,
                &Hash::Sha256(rt_sha.clone()),
                "Baixando o Java 25",
                report,
            )
            .await?;
        }
        report(Progress::new("Preparando o Java 25"));
        for (dir, need) in [(&tool, need_tool), (&game_rt, need_game)] {
            if need {
                unzip(&zip, dir)?;
                std::fs::write(dir.join(RUNTIME_MARK), format!("{rt_sha}\n"))?;
            }
        }
    }

    let label = &wanted.loader;
    let jar = root.cache().join(format!("SteinLoader-Installer-{label}.jar"));
    let jar_hash = Hash::Sha256(sum("SteinLoader-Installer.jar")?);
    if !jar_hash.matches_file(&jar) {
        download_counted(
            &http,
            &format!("{base}/SteinLoader-Installer.jar"),
            &jar,
            &jar_hash,
            "Baixando o Stein Loader",
            report,
        )
        .await?;
    }

    report(Progress::new(format!("Instalando o Stein Loader {label}")));
    run_installer(root, &java_bin(&tool, false), &jar, report).await?;
    std::fs::write(
        installed_marker(root),
        serde_json::to_vec_pretty(&Installed { version: wanted.loader.clone(), tag: tag.clone() })?,
    )?;
    // Installers from older versions are cleaned up.
    if let Ok(dir) = std::fs::read_dir(root.cache()) {
        for e in dir.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.starts_with("SteinLoader-Installer-") && e.path() != jar {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    Ok(())
}

async fn download_counted(
    http: &reqwest::Client,
    url: &str,
    dest: &Path,
    hash: &Hash,
    step: &str,
    report: &Report,
) -> Result<()> {
    let total = http.head(url).send().await.ok().and_then(|r| r.content_length()).unwrap_or(0);
    let done = std::sync::atomic::AtomicU64::new(0);
    let r = report.clone();
    let step_s = step.to_string();
    let cb = move |n: u64| {
        let d = done.fetch_add(n, std::sync::atomic::Ordering::Relaxed) + n;
        r(Progress::count(step_s.clone(), d, total));
    };
    net::download(http, url, dest, hash, Some(&cb)).await
}

/// Runs the command-line installer in the folder. Its output becomes the progress detail.
async fn run_installer(root: &Root, java: &Path, jar: &Path, report: &Report) -> Result<()> {
    let mut cmd = tokio::process::Command::new(java);
    cmd.arg("-jar")
        .arg(jar)
        .arg("--cli")
        .arg("--mc")
        .arg(root.dir())
        .arg("--lang")
        .arg("pt")
        // The installer only handles the Loader ("keep" neither downloads nor removes mods): mods come via
        // `sync_mods`.
        .arg("--mods")
        .arg("keep")
        .arg("--ram")
        .arg(crate::launch::ram_gb(root).to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW: no console window flashing.
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().with_context(|| format!("could not run the installer with {}", java.display()))?;
    let out = child.stdout.take().unwrap();
    let err = child.stderr.take().unwrap();
    let r = report.clone();
    let step = "Instalando o Stein Loader".to_string();
    let log = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let log2 = log.clone();
    let reader = tokio::spawn(async move {
        let mut lines = BufReader::new(out).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let mut p = Progress::new(step.clone());
            p.detail = line.clone();
            r(p);
            log2.lock().unwrap().push_str(&line);
            log2.lock().unwrap().push('\n');
        }
    });
    let mut err_text = String::new();
    let mut elines = BufReader::new(err).lines();
    while let Ok(Some(line)) = elines.next_line().await {
        err_text.push_str(&line);
        err_text.push('\n');
    }
    let status = child.wait().await?;
    let _ = reader.await;
    let _ = std::fs::create_dir_all(root.logs());
    let _ = std::fs::write(
        root.logs().join("installer.log"),
        format!("{}\n--- stderr ---\n{err_text}", log.lock().unwrap()),
    );
    if !status.success() {
        let why = err_text.lines().last().unwrap_or("").to_string();
        bail!("the Stein Loader installer failed ({status}): {why}");
    }
    Ok(())
}

fn runtime_ok(dir: &Path, sha: &str) -> bool {
    java_bin(dir, false).is_file()
        && std::fs::read_to_string(dir.join(RUNTIME_MARK)).map(|m| m.trim().eq_ignore_ascii_case(sha)).unwrap_or(false)
}

/// A release's `SHA256SUMS.txt`: file -> sha256.
fn sums(text: &[u8]) -> BTreeMap<String, String> {
    String::from_utf8_lossy(text)
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let sha = it.next()?;
            let name = it.next()?.trim_start_matches('*');
            Some((name.to_string(), sha.to_lowercase()))
        })
        .collect()
}

/// Extracts a zip into a clean folder (whatever was there is removed), making sure nothing escapes it.
pub fn unzip(zip: &Path, dir: &Path) -> Result<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).with_context(|| format!("could not clear {}", dir.display()))?;
    }
    std::fs::create_dir_all(dir)?;
    let mut archive = zip::ZipArchive::new(std::fs::File::open(zip)?)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(rel) = entry.enclosed_name() else {
            bail!("unsafe path in zip {}", zip.display());
        };
        let out = dir.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p)?;
        }
        let mut f = std::fs::File::create(&out)?;
        std::io::copy(&mut entry, &mut f)?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode))?;
        }
    }
    Ok(())
}

/// The game's Java (no console), used to start it.
pub fn game_java(root: &Root) -> PathBuf {
    java_bin(&root.versions().join(VERSION_ID).join("runtime"), true)
}

fn index_cache(root: &Root) -> PathBuf {
    root.own().join("index.json")
}

/// Saves the index so the mod list and the news show up without internet.
pub fn remember_index(root: &Root, index: &Index) {
    let _ = std::fs::create_dir_all(root.own());
    let _ = std::fs::write(index_cache(root), serde_json::to_vec_pretty(index).unwrap_or_default());
}

/// The last index received.
pub fn cached_index(root: &Root) -> Option<Index> {
    std::fs::read(index_cache(root)).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw() -> serde_json::Value {
        serde_json::json!({
            "latest": {"version": "0.1.27"},
            "mods": [
                {"id": "replay", "modId": "steinreplay", "name": "Replay", "version": "0.1.17", "sha256": "a"},
                {"id": "hud", "modId": "steinhud", "name": "HUD", "version": "0.1.9"}
            ],
            "test": {
                "loader": {"version": "0.1.28", "tag": "v0.1.28-rc.1"},
                "mods": [{"id": "replay", "modId": "steinreplay", "name": "Replay", "version": "0.1.18-rc.1",
                          "file": "Replay.steinmod", "url": "https://x/Replay.steinmod", "sha256": "b"}]
            }
        })
    }

    #[test]
    fn ignores_test_block() {
        let i = parse_index(&raw()).unwrap();
        assert_eq!(i.loader, "0.1.27");
        assert_eq!(i.loader_tag(), "v0.1.27");
        assert_eq!(i.mods.len(), 2);
        assert_eq!(i.mods[0].version, "0.1.17");
    }

    #[test]
    fn links_and_default_mods() {
        let raw = serde_json::json!({
            "schema": 1, "discord": "https://discord.gg/old",
            "links": {"heads": "https://heads.example/{uuid}/{size}"},
            "latest": {"version": "0.1.35"}, "defaultMods": ["hud", "replay"]
        });
        let i = parse_index(&raw).unwrap();
        assert_eq!(i.links.discord, "https://discord.gg/old"); // the old top-level key still counts
        assert_eq!(i.links.heads, "https://heads.example/{uuid}/{size}");
        assert_eq!(i.default_mods, vec!["hud".to_string(), "replay".to_string()]);
        // An index cached by an older launcher (no links, no defaultMods) still reads.
        let old: Index = serde_json::from_str(r#"{"loader":"0.1.27","mods":[]}"#).unwrap();
        assert!(old.default_mods.is_empty() && old.links.heads.is_empty());
    }

    #[test]
    fn install_tag() {
        let inst = Installed { version: "0.1.28".into(), tag: "v0.1.28-rc.1".into() };
        assert_eq!(inst.label(), "0.1.28-rc.1");
        let old = Installed { version: "0.1.27".into(), tag: String::new() };
        assert_eq!(old.tag(), "v0.1.27");
    }
}
