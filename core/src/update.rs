/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

//! Launcher self-update. On start it checks the release; if there is a new version, it downloads the new executable,
//! puts it in its own place and restarts (launcher/src/main.rs, `self_update`).
//!
//! Up to 0.1.2 a separate SteinUpdater.exe did this swap; it was removed in 0.2.0, and the launcher deletes whatever
//! was left in the folder. Launchers from those versions no longer update themselves: SteinClient.exe has to be
//! downloaded again.
//!
//! The Stein Client release publishes `update.json`: the version and the sha256 of each executable for this OS.
//!
//! Up to 0.2.9 the file was `client.json`. Releases after that publish only `update.json`, so launchers up to 0.2.9
//! find no `client.json` in the latest release and stay on 0.2.9 for good (the `client.json` of the v0.2.9 release is
//! never touched); only a downloaded new executable reads `update.json` and keeps updating. Each new executable is
//! downloaded to `<name>.new`, verified, and only then moved into place. Since the target is the running executable,
//! the current one becomes `<name>.old` (Windows allows renaming an open exe, but not overwriting it), and that `.old`
//! is deleted on the next start.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::net::{self, Hash};

const MANIFEST_URL: &str = "https://github.com/Stein-Project/stein-client/releases/latest/download/update.json";

/// What the Stein Client release publishes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientManifest {
    pub version: String,
    /// Per OS ("windows"), each file name and its sha256.
    pub files: BTreeMap<String, BTreeMap<String, String>>,
    /// Where to download from: `{base}/{file}`.
    pub base: String,
}

/// The update.json of the latest release.
pub async fn manifest(http: &reqwest::Client) -> Result<ClientManifest> {
    parse_manifest(&net::fetch(http, MANIFEST_URL).await?)
}

/// Parses an update.json. The executable is only ever downloaded over HTTPS, whatever the manifest says.
fn parse_manifest(bytes: &[u8]) -> Result<ClientManifest> {
    let m: ClientManifest = serde_json::from_slice(bytes)?;
    if !m.base.starts_with("https://") {
        bail!("Stein Client release {} has a download base that is not https: {}", m.version, m.base);
    }
    Ok(m)
}

/// The Stein Client executable names on this OS.
pub fn exe_name(which: &str) -> String {
    if cfg!(windows) {
        format!("{which}.exe")
    } else {
        which.to_string()
    }
}

/// Where the updater from versions up to 0.1.2 lived (the launcher deletes it).
pub fn updater_exe(root: &crate::paths::Root) -> PathBuf {
    root.own().join(exe_name("SteinUpdater"))
}

fn release_hash(m: &ClientManifest, which: &str) -> Result<Hash> {
    let name = exe_name(which);
    let sha = m
        .files
        .get(crate::mojang::os_name())
        .and_then(|f| f.get(&name))
        .ok_or_else(|| anyhow!("Stein Client release {} has no {name} for this OS", m.version))?;
    Ok(Hash::Sha256(sha.clone()))
}

/// Is the release version newer than `current`? Compares component by component ("0.2.10" > "0.2.9"); anything that
/// is not a number counts as 0. The launcher only replaces itself with a newer version: a test build (or one newer
/// than the release) never goes back to the release.
pub fn is_newer(release: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> { v.split(['.', '-']).map(|p| p.parse().unwrap_or(0)).collect() };
    let (r, c) = (parse(release), parse(current));
    for i in 0..r.len().max(c.len()) {
        let (a, b) = (r.get(i).copied().unwrap_or(0), c.get(i).copied().unwrap_or(0));
        if a != b {
            return a > b;
        }
    }
    false
}

/// Is the file `target` already the release's `which`?
pub fn is_current(m: &ClientManifest, which: &str, target: &Path) -> Result<bool> {
    Ok(release_hash(m, which)?.matches_file(target))
}

/// Replaces `target` with the release's `which` if it changed. Returns true if it was replaced. `on_bytes`: download
/// progress.
pub async fn replace_file(
    http: &reqwest::Client,
    m: &ClientManifest,
    which: &str,
    target: &Path,
    on_bytes: Option<&(dyn Fn(u64) + Send + Sync)>,
) -> Result<bool> {
    let hash = release_hash(m, which)?;
    if hash.matches_file(target) {
        return Ok(false);
    }
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut new = target.as_os_str().to_owned();
    new.push(".new");
    let new = PathBuf::from(new);
    net::download(http, &format!("{}/{}", m.base, exe_name(which)), &new, &hash, on_bytes).await?;
    install_file(&new, target)?;
    Ok(true)
}

/// Puts `new` in place of `target`, even if `target` is the running executable.
pub fn install_file(new: &Path, target: &Path) -> Result<()> {
    if target.exists() {
        let old = old_of(target);
        let _ = std::fs::remove_file(&old);
        if std::fs::rename(target, &old).is_err() {
            // Could not move it out of the way (open by another process that does not allow it): try overwriting.
            std::fs::copy(new, target).with_context(|| format!("could not replace {}", target.display()))?;
            let _ = std::fs::remove_file(new);
            return Ok(());
        }
    }
    std::fs::rename(new, target).with_context(|| format!("could not move {} into place", target.display()))?;
    Ok(())
}

fn old_of(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(".old");
    PathBuf::from(s)
}

/// Deletes the `<exe>.old` a swap of `exe` left behind (called on start, when it is no longer in use). Only that one
/// file: the executable may sit in a shared folder such as the Desktop, where other `.old` files are not ours.
pub fn clean_old(exe: &Path) {
    let _ = std::fs::remove_file(old_of(exe));
}

#[cfg(test)]
mod newer_tests {
    #[test]
    fn only_newer_versions() {
        assert!(super::is_newer("0.2.4", "0.2.3"));
        assert!(super::is_newer("0.2.10", "0.2.9"));
        assert!(!super::is_newer("0.2.3", "0.2.4"));
        assert!(!super::is_newer("0.2.4", "0.2.4"));
        assert!(super::is_newer("1.0", "0.9.9"));
    }

    #[test]
    fn manifest_base_must_be_https() {
        let json = |base: &str| format!(r#"{{"version":"0.3.0","base":"{base}","files":{{}}}}"#);
        assert!(super::parse_manifest(json("https://github.com/x/y/releases/download/v0.3.0").as_bytes()).is_ok());
        assert!(super::parse_manifest(json("http://example.com").as_bytes()).is_err());
        assert!(super::parse_manifest(json("file:///C:/x").as_bytes()).is_err());
    }
}
