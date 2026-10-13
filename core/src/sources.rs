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
//! - **Official**: the sources in the `sources` list of `stein.json`. Enabled out of the box, without a warning.
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

/// Combat and Mouse left the official list on 2026-10-12 and became third-party mods by X4vieer: (old id in the
/// official index, id inside the mod, name, new index).
const MOVED: &[(&str, &str, &str, &str)] = &[
    (
        "combat",
        "steincombat",
        "Combat",
        "https://github.com/brenoxavier/SteinCombat/releases/latest/download/steinmods.json",
    ),
    (
        "mouse",
        "steinmouse",
        "Mouse",
        "https://github.com/brenoxavier/SteinMouse/releases/latest/download/steinmods.json",
    ),
];
const MOVED_AUTHOR: &str = "X4vieer";
/// The migration's marker in the settings: it runs once.
const MIGRATION: &str = "third-party-combat-mouse";

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

/// The mods from several sources, read concurrently. A source that did not respond keeps the mods it had in the last
/// saved index (`previous`): without internet nothing disappears from the list, and nothing leaves the folder.
pub async fn gather(
    http: &reqwest::Client,
    urls: &[String],
    third_party: bool,
    previous: &[IndexMod],
) -> Vec<IndexMod> {
    let all = futures_util::future::join_all(urls.iter().map(|u| fetch(http, u, third_party))).await;
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

/// One-off: anyone with Combat or Mouse in the folder, enabled, starts receiving them from the new source
/// (third-party mods by X4vieer) without a warning, since it is the same author. The old ids are removed from
/// everyone's list of enabled official mods. Returns the notice to show, when something changed.
pub fn migrate(root: &Root) -> Result<Option<String>> {
    let mut s = Settings::load(root);
    if s.migrations.iter().any(|m| m == MIGRATION) {
        return Ok(None);
    }
    // Without a settings file the old default applied: every official mod enabled, Combat and Mouse included.
    let saved = root.settings_file().is_file();
    let files = crate::stein::mod_files(root);
    let mut list = third_party(root);
    let mut moved = Vec::new();
    for (id, mod_id, name, url) in MOVED {
        let on = (!saved || s.mods.iter().any(|m| m == id)) && files.contains_key(*mod_id);
        s.mods.retain(|m| m != id);
        if !on {
            continue;
        }
        if !list.iter().any(|x| x.url == *url) {
            list.push(Source { url: url.to_string(), name: name.to_string(), author: MOVED_AUTHOR.to_string() });
        }
        if !s.mods.iter().any(|m| m == mod_id) {
            s.mods.push(mod_id.to_string());
        }
        moved.push(*name);
    }
    save(root, &list)?;
    s.migrations.push(MIGRATION.to_string());
    let notice = (!moved.is_empty()).then(|| {
        let names = moved.join(" e ");
        let verb = if moved.len() > 1 { "agora são mods de terceiros" } else { "agora é um mod de terceiros" };
        format!(
            "{names} {verb} do {MOVED_AUTHOR}, com o código aberto. Continuam instalados e sendo atualizados: \
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
        (Root::at(&dir), dir)
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
