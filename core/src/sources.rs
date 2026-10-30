/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

//! Mod sources: each mod publishes its own index, `steinmods.json`, with every stable release, always at the same
//! link (`https://github.com/<owner>/<repo>/releases/latest/download/steinmods.json`). An index can list one mod or
//! several:
//!
//! ```text
//! { "format": 1, "name": "HUD", "author": "X4vieer", "homepage": "https://github.com/Stein-Project/hud",
//!   "source": "https://github.com/Stein-Project/hud/releases/latest/download/steinmods.json",
//!   "mods": [ { "id": "hud", "modId": "steinhud", "name": "HUD", "version": "0.1.11", "file": "HUD.steinmod",
//!               "url": "https://...", "sha256": "...", "loader": "0.1.21", "description": "..." } ] }
//! ```
//!
//! - **Official**: the sources in the `sources` list of the registry's `stein.json` (each one a `mods/<id>.json` of the
//!   registry, read with its signature). Enabled out of the box, without a warning.
//! - **Third-party**: the ones the player pastes into the launcher (the index link, or the JSON itself with its
//!   `source`), after a security warning. Stored in `stein-client/sources.json`.
//!
//! Both are read, downloaded and verified by the same code ([`crate::stein`]): `https` only, a plain `.steinmod` file
//! name, the sha256 of each file, and the id inside the downloaded mod must match what the index says. A third-party
//! mod never replaces an official one (the official one wins). A source that is down leaves the folder as it is.

use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};

use crate::net;
use crate::paths::Root;
use crate::settings::Settings;
use crate::stein::IndexMod;

/// The `steinmods.json` format this launcher understands.
pub const FORMAT: u64 = 1;

/// The registry file with the mods that changed source.
const MIGRATIONS_FILE: &str = "migrations.json";

/// One move of official mods to a third-party source (`migrations.json` of the registry). Combat and Mouse left the
/// official list on 2026-10-12 and became third-party mods by X4vieer: that was the first one.
#[derive(Debug, Clone, Deserialize)]
struct Migration {
    /// The marker in the settings: each migration runs once per folder.
    id: String,
    /// Who publishes the mods now (shown in the notice and as the source's author).
    #[serde(default)]
    author: String,
    /// The notice for the player, when the default one (a mod that became third-party) is not what happened.
    #[serde(default)]
    notice: String,
    mods: Vec<Moved>,
}

/// A source that changed address (`sources` in `migrations.json`). Kept apart from the migrations: those run once, and
/// an older launcher would mark as done something it does not know how to do.
#[derive(Debug, Clone, Deserialize)]
struct Repoint {
    from: String,
    to: String,
}

/// A mod that moved: its old id in the official index, the id inside the mod, its name and its new index.
#[derive(Debug, Clone, Deserialize)]
struct Moved {
    id: String,
    #[serde(rename = "modId")]
    mod_id: String,
    name: String,
    source: String,
}

/// The migrations in the registry's stored copy (none without a registry).
fn migrations(root: &Root) -> Vec<Migration> {
    crate::registry::cached_json(root, MIGRATIONS_FILE)
        .and_then(|j| serde_json::from_value(j["migrations"].clone()).ok())
        .unwrap_or_default()
}

/// A third-party source the player added.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// The `steinmods.json` link.
    pub url: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub author: String,
}

/// An index that has been read, holding only the mods that passed verification.
#[derive(Debug, Clone, Serialize)]
pub struct Fetched {
    pub url: String,
    pub name: String,
    pub author: String,
    pub homepage: String,
    pub mods: Vec<IndexMod>,
}

fn file(root: &Root) -> PathBuf {
    root.own().join("sources.json")
}

/// The third-party sources that were added.
pub fn third_party(root: &Root) -> Vec<Source> {
    std::fs::read(file(root)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save(root: &Root, list: &[Source]) -> Result<()> {
    std::fs::create_dir_all(root.own())?;
    let tmp = file(root).with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(list)?)?;
    std::fs::rename(tmp, file(root))?;
    Ok(())
}

/// A link the launcher agrees to read: `https` only.
pub fn allowed_url(url: &str) -> bool {
    url.starts_with("https://")
}

/// The index link from what the player pasted: the link itself, or the index JSON (its `source`).
pub fn url_from_paste(text: &str) -> Result<String> {
    let text = text.trim();
    let url = if text.starts_with('{') {
        let json: serde_json::Value = serde_json::from_str(text).map_err(|_| anyhow!("Isso não é um JSON válido."))?;
        json["source"]
            .as_str()
            .ok_or_else(|| anyhow!("Esse JSON não diz de onde se atualizar (falta o campo \"source\")."))?
            .trim()
            .to_string()
    } else {
        text.to_string()
    };
    if url.is_empty() || url.contains(char::is_whitespace) {
        bail!("Cole o link do índice de mods (steinmods.json) ou o JSON dele.");
    }
    if !allowed_url(&url) {
        bail!("O link do índice precisa começar com https://");
    }
    Ok(url)
}

/// Reads and verifies an index. `third_party`: mods come out with the id from inside the mod (the short `id` is only
/// for official mods).
pub async fn fetch(http: &reqwest::Client, url: &str, third_party: bool) -> Result<Fetched> {
    let raw: serde_json::Value = serde_json::from_slice(&net::fetch(http, url).await?)
        .map_err(|_| anyhow!("o índice {url} não é um JSON válido"))?;
    parse(url, &raw, third_party)
}

fn parse(url: &str, raw: &serde_json::Value, third_party: bool) -> Result<Fetched> {
    if !raw.is_object() || !raw["mods"].is_array() {
        bail!("{url} não é um índice de mods do Stein (steinmods.json)");
    }
    if raw["format"].as_u64().unwrap_or(FORMAT) > FORMAT {
        bail!("o índice {url} é de uma versão mais nova do Stein Client: atualize o launcher");
    }
    let text = |k: &str| raw[k].as_str().unwrap_or("").trim().to_string();
    let read = |v: &serde_json::Value| -> Vec<IndexMod> {
        v.as_array()
            .map(|a| a.iter().filter_map(|m| serde_json::from_value::<IndexMod>(m.clone()).ok()).collect())
            .unwrap_or_default()
    };
    let mods = read(&raw["mods"]);
    let name = text("name");
    let mut out = Vec::new();
    for mut m in mods {
        if !valid_mod(&m) || out.iter().any(|o: &IndexMod| o.mod_id == m.mod_id) {
            continue;
        }
        m.source = url.to_string();
        m.third_party = third_party;
        if third_party {
            m.id = m.mod_id.clone();
            m.source_name = if name.is_empty() { m.name.clone() } else { name.clone() };
        }
        out.push(m);
    }
    Ok(Fetched { url: url.to_string(), name, author: text("author"), homepage: text("homepage"), mods: out })
}

/// An index mod that is safe to download: id, version, allowed link, plain `.steinmod` file name and sha256.
pub(crate) fn valid_mod(m: &IndexMod) -> bool {
    let id_ok =
        |s: &str| !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c));
    id_ok(&m.mod_id)
        && id_ok(&m.id)
        && !m.version.is_empty()
        && allowed_url(&m.url)
        && m.file.ends_with(".steinmod")
        && m.file.len() > ".steinmod".len()
        && !m.file.contains(['/', '\\', ':'])
        && !m.file.starts_with('.')
        && portable_file_name(&m.file)
        && m.sha256.len() == 64
        && m.sha256.chars().all(|c| c.is_ascii_hexdigit())
}

/// A file name Windows can create as is: not a reserved device name (`CON`, `NUL.steinmod`, `com1.x.steinmod`...)
/// and not ending in a dot or a space, which Windows silently drops.
fn portable_file_name(name: &str) -> bool {
    if name.ends_with(['.', ' ']) {
        return false;
    }
    let stem = name.split('.').next().unwrap_or("").trim_end().to_ascii_uppercase();
    let numbered = |prefix: &str| {
        stem.strip_prefix(prefix).is_some_and(|n| n.len() == 1 && n.as_bytes()[0].is_ascii_digit() && n != "0")
    };
    !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") && !numbered("COM") && !numbered("LPT")
}

/// Reads one of the sources: a registry index is fetched with its signature ([`crate::registry`]), falling back to its
/// stored verified copy, and any other one as a plain download.
async fn fetch_source(http: &reqwest::Client, root: &Root, url: &str, third_party: bool) -> Result<Fetched> {
    match crate::registry::file_of(url) {
        Some(name) => {
            let bytes = crate::registry::load(http, root, &name).await?;
            let raw: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(|_| anyhow!("o índice {url} não é um JSON válido"))?;
            parse(url, &raw, third_party)
        }
        None => fetch(http, url, third_party).await,
    }
}

/// The mods from several sources, read concurrently. A source that did not respond keeps the mods it had in the last
/// saved index (`previous`): without internet nothing disappears from the list, and nothing leaves the folder.
pub async fn gather(
    http: &reqwest::Client,
    root: &Root,
    urls: &[String],
    third_party: bool,
    previous: &[IndexMod],
) -> Vec<IndexMod> {
    let all = futures_util::future::join_all(urls.iter().map(|u| fetch_source(http, root, u, third_party))).await;
    let mut out: Vec<IndexMod> = Vec::new();
    for (url, got) in urls.iter().zip(all) {
        let mods = match got {
            Ok(f) => f.mods,
            // Down (or a broken index): keep what was there.
            Err(_) => previous.iter().filter(|m| &m.source == url).cloned().collect(),
        };
        for m in mods {
            if !out.iter().any(|o| o.mod_id == m.mod_id) {
                out.push(m);
            }
        }
    }
    out
}

/// Adds a third-party source, already read and shown to the player ([`fetch`]), with its mods enabled.
pub fn add(root: &Root, f: &Fetched) -> Result<()> {
    let mut list = third_party(root);
    list.retain(|s| s.url != f.url);
    list.push(Source { url: f.url.clone(), name: f.name.clone(), author: f.author.clone() });
    save(root, &list)?;
    let mut s = Settings::load(root);
    for m in &f.mods {
        if !s.mods.contains(&m.id) {
            s.mods.push(m.id.clone());
        }
    }
    s.save(root)
}

/// Removes a third-party source. Its mods stay in the folder as they are: they just stop being updated.
pub fn remove(root: &Root, url: &str) -> Result<()> {
    let mut list = third_party(root);
    list.retain(|s| s.url != url);
    save(root, &list)?;
    if let Some(mut index) = crate::stein::cached_index(root) {
        let gone: Vec<String> = index.mods.iter().filter(|m| m.source == url).map(|m| m.id.clone()).collect();
        let mut s = Settings::load(root);
        s.mods.retain(|m| !gone.contains(m));
        s.save(root)?;
        index.mods.retain(|m| m.source != url);
        crate::stein::remember_index(root, &index);
    }
    Ok(())
}

/// The sources that changed address, in the registry's stored copy.
fn repoints(root: &Root) -> Vec<Repoint> {
    crate::registry::cached_json(root, MIGRATIONS_FILE)
        .and_then(|j| serde_json::from_value(j["sources"].clone()).ok())
        .unwrap_or_default()
}

/// Points the added sources at their new addresses, for whoever still has the old one; nothing changes in the folder,
/// so the mods stay enabled and with their settings. A source that already has the new address too is just dropped.
fn repoint(root: &Root) -> Result<()> {
    let mut list = third_party(root);
    let before = list.clone();
    for r in repoints(root) {
        if !allowed_url(&r.to) {
            continue;
        }
        let has_new = list.iter().any(|s| s.url == r.to);
        if has_new {
            list.retain(|s| s.url != r.from);
        } else if let Some(s) = list.iter_mut().find(|s| s.url == r.from) {
            s.url = r.to.clone();
        }
    }
    if list != before {
        save(root, &list)?;
    }
    Ok(())
}

/// Runs each migration of the registry (`migrations.json`) that has not run in this folder yet: anyone with a moved mod
/// in the folder, enabled, starts receiving it from the new source without a warning, since it is the same author. The
/// old ids are removed from everyone's list of enabled official mods. Returns the notice to show, when something
/// changed. Without a registry copy there is nothing to run.
pub fn migrate(root: &Root) -> Result<Option<String>> {
    let mut last = None;
    repoint(root)?;
    for m in migrations(root) {
        if let Some(n) = run_migration(root, &m)? {
            last = Some(n);
        }
    }
    Ok(last)
}

fn run_migration(root: &Root, migration: &Migration) -> Result<Option<String>> {
    let mut s = Settings::load(root);
    if migration.id.is_empty() || s.migrations.contains(&migration.id) {
        return Ok(None);
    }
    // Without a settings file the old default applied: every official mod enabled, the moved ones included.
    let saved = root.settings_file().is_file();
    let files = crate::stein::mod_files(root);
    let mut list = third_party(root);
    let mut moved = Vec::new();
    for m in &migration.mods {
        if !allowed_url(&m.source) {
            continue;
        }
        let on = (!saved || s.mods.contains(&m.id)) && files.contains_key(&m.mod_id);
        s.mods.retain(|x| *x != m.id);
        if !on {
            continue;
        }
        if !list.iter().any(|x| x.url == m.source) {
            list.push(Source { url: m.source.clone(), name: m.name.clone(), author: migration.author.clone() });
        }
        if !s.mods.contains(&m.mod_id) {
            s.mods.push(m.mod_id.clone());
        }
        moved.push(m.name.as_str());
    }
    save(root, &list)?;
    s.migrations.push(migration.id.clone());
    let notice = (!moved.is_empty()).then(|| {
        if !migration.notice.is_empty() {
            return migration.notice.clone();
        }
        let names = moved.join(" e ");
        let verb = if moved.len() > 1 { "agora são mods de terceiros" } else { "agora é um mod de terceiros" };
        let author = &migration.author;
        format!(
            "{names} {verb} do {author}, com o código aberto. Continuam instalados e sendo atualizados: \
             a fonte nova já foi adicionada em Mods de terceiros."
        )
    });
    if notice.is_some() {
        s.notice = notice.clone();
    }
    s.save(root)?;
    Ok(notice)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> (Root, PathBuf) {
        let dir = std::env::temp_dir().join(format!("stein-sources-{}-{}", std::process::id(), rand_suffix()));
        std::fs::create_dir_all(&dir).unwrap();
        let r = Root::at(&dir);
        // The registry's migrations.json as published (the stored copy counts as verified).
        std::fs::create_dir_all(r.own().join("registry")).unwrap();
        std::fs::write(
            r.own().join("registry").join("migrations.json"),
            r#"{"schema": 1, "migrations": [{"id": "third-party-combat-mouse", "author": "X4vieer", "mods": [
                {"id": "combat", "modId": "steincombat", "name": "Combat",
                 "source": "https://github.com/brenoxavier/SteinCombat/releases/latest/download/steinmods.json"},
                {"id": "mouse", "modId": "steinmouse", "name": "Mouse",
                 "source": "https://github.com/brenoxavier/SteinMouse/releases/latest/download/steinmods.json"}]}]}"#,
        )
        .unwrap();
        (r, dir)
    }

    fn rand_suffix() -> u64 {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    fn entry(mod_id: &str, file: &str) -> serde_json::Value {
        serde_json::json!({"id": "x", "modId": mod_id, "name": "X", "version": "1.0", "file": file,
                           "url": format!("https://example.com/{file}"), "sha256": "a".repeat(64)})
    }

    /// A fake `.steinmod` in the mods folder, holding only steinmod.json.
    fn fake_mod(dir: &std::path::Path, file: &str, id: &str) {
        std::fs::create_dir_all(dir.join("mods")).unwrap();
        let mut z = zip::ZipWriter::new(std::fs::File::create(dir.join("mods").join(file)).unwrap());
        z.start_file("steinmod.json", zip::write::SimpleFileOptions::default()).unwrap();
        std::io::Write::write_all(&mut z, format!(r#"{{"id":"{id}","version":"0.1.0"}}"#).as_bytes()).unwrap();
        z.finish().unwrap();
    }

    #[test]
    fn a_moved_source_is_repointed_every_time_and_keeps_the_mods() {
        let (r, dir) = root();
        std::fs::create_dir_all(r.own().join("registry")).unwrap();
        std::fs::write(
            r.own().join("registry").join("migrations.json"),
            r#"{"schema": 1, "sources": [{"from": "https://old.example/i.json", "to": "https://new.example/i.json"}]}"#,
        )
        .unwrap();
        let old = Source { url: "https://old.example/i.json".into(), name: "Combat".into(), author: "X4vieer".into() };
        let other = Source { url: "https://other.example/i.json".into(), name: "x".into(), author: "y".into() };
        save(&r, &[old, other.clone()]).unwrap();
        let mut s = Settings::load(&r);
        s.mods.push("steincombat".into());
        s.save(&r).unwrap();

        assert_eq!(migrate(&r).unwrap(), None);
        let list = third_party(&r);
        assert_eq!(list[0].url, "https://new.example/i.json");
        assert_eq!(list[0].name, "Combat");
        assert_eq!(list[1], other);
        assert!(Settings::load(&r).mods.contains(&"steincombat".to_string()));

        // Both addresses in the list (the player added the new one by hand): no duplicate.
        let mut both = third_party(&r);
        both.push(Source { url: "https://old.example/i.json".into(), name: "Combat".into(), author: "".into() });
        save(&r, &both).unwrap();
        migrate(&r).unwrap();
        assert_eq!(
            third_party(&r).iter().filter(|s| s.url.contains("example/i.json") && s.name == "Combat").count(),
            1
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pasted_link_or_json() {
        let u = "https://github.com/a/b/releases/latest/download/steinmods.json";
        assert_eq!(url_from_paste(&format!("  {u}\n")).unwrap(), u);
        assert_eq!(url_from_paste(&format!("{{\"source\": \"{u}\", \"mods\": []}}")).unwrap(), u);
        assert!(url_from_paste("http://example.com/x.json").is_err());
        assert!(url_from_paste("{\"mods\": []}").is_err());
        assert!(url_from_paste("").is_err());
    }

    #[test]
    fn only_safe_mods() {
        let raw = serde_json::json!({"name": "Source", "mods": [
            entry("good", "Good.steinmod"),
            entry("path", "../Path.steinmod"),
            entry("jar", "Thing.jar"),
            entry("good", "Repeated.steinmod"),
            entry("device", "NUL.steinmod"),
            {"id": "x", "modId": "nohash", "name": "X", "version": "1", "file": "S.steinmod", "url": "https://e/S.steinmod", "sha256": ""},
            {"id": "x", "modId": "http", "name": "X", "version": "1", "file": "H.steinmod", "url": "http://e/H.steinmod", "sha256": "a".repeat(64)}
        ]});
        let f = parse("https://e/steinmods.json", &raw, true).unwrap();
        assert_eq!(f.mods.len(), 1);
        assert_eq!(f.mods[0].id, "good"); // third-party: the id is the one from inside the mod
        assert!(f.mods[0].third_party);
        assert_eq!(f.mods[0].source, "https://e/steinmods.json");
        assert_eq!(f.mods[0].source_name, "Source");
    }

    #[test]
    fn windows_reserved_file_names() {
        for bad in ["CON.steinmod", "nul.steinmod", "Aux.x.steinmod", "COM1.steinmod", "lpt9.steinmod", "Mod.steinmod."]
        {
            assert!(!portable_file_name(bad), "{bad}");
        }
        for good in ["Console.steinmod", "COM0.steinmod", "COM10.steinmod", "LPT.steinmod", "HUD.steinmod"] {
            assert!(portable_file_name(good), "{good}");
        }
        let m: IndexMod = serde_json::from_value(entry("x", "PRN.steinmod")).unwrap();
        assert!(!valid_mod(&m));
        let m: IndexMod = serde_json::from_value(entry("x", "Mod .steinmod")).unwrap();
        assert!(valid_mod(&m)); // a space inside the name is fine; only a trailing one is not
    }

    #[test]
    fn newer_format_needs_newer_launcher() {
        let raw = serde_json::json!({"format": 99, "mods": []});
        assert!(parse("https://e/x.json", &raw, true).is_err());
        assert!(parse("https://e/x.json", &serde_json::json!({"latest": {}}), true).is_err());
    }

    #[test]
    fn migration_only_for_players_with_the_mod() {
        let (r, dir) = root();
        fake_mod(&dir, "Combat.steinmod", "steincombat");
        // Old factory settings: Combat and Mouse enabled; only Combat in the folder.
        let s = Settings { mods: vec!["combat".into(), "hud".into(), "mouse".into()], ..Settings::default() };
        s.save(&r).unwrap();

        let notice = migrate(&r).unwrap().unwrap();
        assert!(notice.starts_with("Combat agora é um mod de terceiros"), "{notice}");
        let s = Settings::load(&r);
        assert_eq!(s.mods, vec!["hud".to_string(), "steincombat".to_string()]);
        assert_eq!(s.notice.as_deref(), Some(notice.as_str()));
        let list = third_party(&r);
        assert_eq!(list.len(), 1);
        assert!(list[0].url.contains("brenoxavier/SteinCombat"));
        // Only once.
        assert!(migrate(&r).unwrap().is_none());
        assert_eq!(third_party(&r).len(), 1);
    }

    #[test]
    fn migration_without_settings_uses_old_default() {
        // Players who never touched their mods have no settings.json: "all enabled" applied.
        let (r, dir) = root();
        fake_mod(&dir, "Combat.steinmod", "steincombat");
        fake_mod(&dir, "Mouse.steinmod", "steinmouse");
        let notice = migrate(&r).unwrap().unwrap();
        assert!(notice.starts_with("Combat e Mouse agora são mods de terceiros"), "{notice}");
        let s = Settings::load(&r);
        assert!(s.mods.contains(&"steincombat".to_string()) && s.mods.contains(&"steinmouse".to_string()));
        assert!(!s.mods.contains(&"combat".to_string()));
        assert_eq!(third_party(&r).len(), 2);
    }

    #[test]
    fn migration_needs_the_registry() {
        // Without the registry's copy there is nothing to run, and the marker is not set (it runs once one arrives).
        let (r, dir) = root();
        std::fs::remove_file(r.own().join("registry").join("migrations.json")).unwrap();
        fake_mod(&dir, "Combat.steinmod", "steincombat");
        assert!(migrate(&r).unwrap().is_none());
        assert!(Settings::load(&r).migrations.is_empty());
        assert!(third_party(&r).is_empty());
    }

    #[test]
    fn migration_disabled_stays_disabled() {
        // The player had disabled Mouse (and the file has not left the folder yet): it stays disabled.
        let (r, dir) = root();
        fake_mod(&dir, "Mouse.steinmod", "steinmouse");
        let s = Settings { mods: vec!["hud".into()], ..Settings::default() };
        s.save(&r).unwrap();
        assert!(migrate(&r).unwrap().is_none());
        assert!(third_party(&r).is_empty());
        assert_eq!(Settings::load(&r).mods, vec!["hud".to_string()]);
    }
}
