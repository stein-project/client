/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

//! The original Minecraft 1.8.9, straight from Mojang, on the player's machine (nothing from Mojang goes through us,
//! as the EULA requires). The chain verifies itself: the manifest gives the sha1 of the version JSON, the JSON gives
//! the sha1 of the jar, each library and the asset index, and the index gives the sha1 of each asset.
//!
//! The result has the layout of a `.minecraft` folder: `versions/1.8.9/1.8.9.{json,jar}`, `libraries/` and
//! `assets/`, which is what the Stein Loader installer expects to find.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use futures_util::{stream, StreamExt};
use serde_json::Value;

use crate::net::{self, Hash};
use crate::paths::Root;
use crate::progress::{Progress, Report};

pub const VERSION: &str = "1.8.9";
const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const RESOURCES_URL: &str = "https://resources.download.minecraft.net";
/// Concurrent downloads (libraries and assets are many small files).
const PARALLEL: usize = 16;

/// The operating system, as Mojang's JSON names it.
pub fn os_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

/// A file to download: from where, to where, and the sha1 it must have.
struct Item {
    url: String,
    dest: PathBuf,
    sha1: String,
    size: u64,
}

/// Makes sure the whole original 1.8.9 is in the folder. Whatever is already there and checks out is not
/// downloaded again.
pub async fn ensure(root: &Root, report: &Report) -> Result<()> {
    let http = net::client();
    report(Progress::new("Conferindo o Minecraft 1.8.9"));
    let dir = root.versions().join(VERSION);
    let json_path = dir.join(format!("{VERSION}.json"));
    let version = version_json(&http, &json_path).await?;

    let mut items = Vec::new();
    let client = &version["downloads"]["client"];
    items.push(Item {
        url: str_of(client, "url")?,
        dest: dir.join(format!("{VERSION}.jar")),
        sha1: str_of(client, "sha1")?,
        size: client["size"].as_u64().unwrap_or(0),
    });
    for lib in version["libraries"].as_array().ok_or_else(|| anyhow!("1.8.9 JSON has no libraries"))? {
        if !allowed(lib) {
            continue;
        }
        if let Some(a) = lib["downloads"].get("artifact") {
            items.push(Item {
                url: str_of(a, "url")?,
                dest: root.libraries().join(str_of(a, "path")?),
                sha1: str_of(a, "sha1")?,
                size: a["size"].as_u64().unwrap_or(0),
            });
        }
        if let Some(classifier) = native_classifier(lib) {
            if let Some(n) = lib["downloads"]["classifiers"].get(&classifier) {
                items.push(Item {
                    url: str_of(n, "url")?,
                    dest: root.libraries().join(str_of(n, "path")?),
                    sha1: str_of(n, "sha1")?,
                    size: n["size"].as_u64().unwrap_or(0),
                });
            }
        }
    }

    // The asset index is verified against the version JSON, and each asset against the index.
    let idx = &version["assetIndex"];
    let idx_id = str_of(idx, "id")?;
    let idx_path = root.assets().join("indexes").join(format!("{idx_id}.json"));
    let idx_hash = Hash::Sha1(str_of(idx, "sha1")?);
    if !idx_hash.matches_file(&idx_path) {
        net::download(&http, &str_of(idx, "url")?, &idx_path, &idx_hash, None).await?;
    }
    let index: Value = serde_json::from_slice(&std::fs::read(&idx_path)?)?;
    // The index repeats content (734 names, 722 files): each file is added only once.
    let mut seen = std::collections::HashSet::new();
    for (_, obj) in index["objects"].as_object().ok_or_else(|| anyhow!("asset index is empty"))? {
        let hash = str_of(obj, "hash")?;
        if !seen.insert(hash.clone()) {
            continue;
        }
        let sub = hash[..2].to_string();
        items.push(Item {
            url: format!("{RESOURCES_URL}/{sub}/{hash}"),
            dest: root.assets().join("objects").join(&sub).join(&hash),
            sha1: hash.clone(),
            size: obj["size"].as_u64().unwrap_or(0),
        });
    }

    fetch_all(&http, items, "Baixando o Minecraft 1.8.9", report).await
}

/// The version JSON: the one on disk if it matches the manifest; otherwise downloaded. With no internet and a JSON on
/// disk, the one on disk is used (the game starts offline).
async fn version_json(http: &reqwest::Client, path: &Path) -> Result<Value> {
    let manifest = match net::fetch(http, MANIFEST_URL).await {
        Ok(b) => Some(serde_json::from_slice::<Value>(&b)?),
        Err(e) if path.is_file() => {
            eprintln!("Mojang manifest unreachable ({e:#}); using the JSON on disk");
            None
        }
        Err(e) => return Err(e),
    };
    if let Some(m) = manifest {
        let entry = m["versions"]
            .as_array()
            .and_then(|v| v.iter().find(|x| x["id"] == VERSION))
            .ok_or_else(|| anyhow!("the Mojang manifest does not list {VERSION}"))?;
        let hash = Hash::Sha1(str_of(entry, "sha1")?);
        if !hash.matches_file(path) {
            net::download(http, &str_of(entry, "url")?, path, &hash, None).await?;
        }
    }
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

/// Downloads whatever is missing, several at a time, counting files for the progress report.
async fn fetch_all(http: &reqwest::Client, items: Vec<Item>, step: &str, report: &Report) -> Result<()> {
    // Files already on disk are checked by size (hashing thousands of assets on every start would cost seconds);
    // anything missing or with a different size is downloaded, and the download verifies the sha1.
    let missing: Vec<Item> = items
        .into_iter()
        .filter(|i| match std::fs::metadata(&i.dest) {
            Ok(m) => i.size != 0 && m.len() != i.size,
            Err(_) => true,
        })
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let total = missing.len() as u64;
    let done = Arc::new(AtomicU64::new(0));
    report(Progress::count(step, 0, total));
    let results: Vec<Result<()>> = stream::iter(missing)
        .map(|item| {
            let http = http.clone();
            let done = done.clone();
            let report = report.clone();
            let step = step.to_string();
            async move {
                net::download(&http, &item.url, &item.dest, &Hash::Sha1(item.sha1.clone()), None).await?;
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                report(Progress::count(step, n, total));
                Ok(())
            }
        })
        .buffer_unordered(PARALLEL)
        .collect()
        .await;
    match results.into_iter().find_map(Result::err) {
        Some(first) => Err(first),
        None => Ok(()),
    }
}

/// Does the library apply to this OS? (The per-OS "allow/disallow" rules in Mojang's JSON.)
pub fn allowed(lib: &Value) -> bool {
    let Some(rules) = lib["rules"].as_array() else {
        return true;
    };
    let mut ok = false;
    for r in rules {
        let applies = match r["os"]["name"].as_str() {
            Some(name) => name == os_name(),
            None => true,
        };
        if applies {
            ok = r["action"] == "allow";
        }
    }
    ok
}

/// The natives classifier for this OS ("natives-windows", "natives-windows-64"), if the library has one.
pub fn native_classifier(lib: &Value) -> Option<String> {
    let c = lib["natives"][os_name()].as_str()?;
    Some(c.replace("${arch}", if cfg!(target_pointer_width = "64") { "64" } else { "32" }))
}

fn str_of(v: &Value, key: &str) -> Result<String> {
    v[key].as_str().map(str::to_string).with_context(|| format!("field \"{key}\" missing from the Mojang JSON"))
}

/// Is the original 1.8.9 already there? (To know whether the game can start without internet.)
pub fn present(root: &Root) -> bool {
    let dir = root.versions().join(VERSION);
    dir.join(format!("{VERSION}.json")).is_file() && dir.join(format!("{VERSION}.jar")).is_file()
}
