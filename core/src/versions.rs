/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

//! Which versions the player plays with: the Stein pack (the Stein Bridge and the Stein Loader together, from
//! `packs.json` of the registry) and each mod's version (its `versions` history in the mod's index).
//!
//! Nothing is picked by hand: the player follows the latest stable pack and each enabled mod its newest stable version
//! that works with that pack's Loader (the `loader` minimum the mod declares). With "Versões de teste" on, the newest
//! test version (`-rc.N`, a pre-release) of the pack and of each mod is taken instead, while it is ahead of the stable
//! one. A mod with no version that works with the Loader is left as it is in the folder (neither downloaded nor removed)
//! and shown with the reason.
//!
//! The history starts with the versions out when it began (pack 0.1.44): nothing older is offered.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::settings::Settings;
use crate::stein::{Index, IndexMod};

/// The registry file with the pack history.
pub const PACKS_FILE: &str = "packs.json";

/// One pack release (`packs.json`): the installer release with the Stein Bridge and the Stein Loader inside.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pack {
    /// "0.1.44", or "0.1.45-rc.1" for a test version.
    pub version: String,
    /// The installer release tag: "v0.1.44", "v0.1.45-rc.1".
    pub tag: String,
    /// A test version (a pre-release).
    pub test: bool,
    /// The day it came out (YYYY-MM-DD).
    pub date: String,
    pub bridge: String,
    /// The Stein Loader library inside (what a mod's `loader` minimum is compared with).
    pub loader: String,
    /// The release notes by language ("pt_BR", "en_US", ...), in Markdown.
    pub changelog: BTreeMap<String, String>,
}

impl Pack {
    /// The version without the test label: what the installer records ("0.1.45" for "0.1.45-rc.1").
    pub fn base_version(&self) -> &str {
        self.version.split('-').next().unwrap_or(&self.version)
    }
}

/// The pack history from `packs.json`, newest first; entries without a tag or a Loader are left out.
pub fn parse_packs(raw: &serde_json::Value) -> Vec<Pack> {
    let mut packs: Vec<Pack> = raw["packs"]
        .as_array()
        .map(|a| a.iter().filter_map(|p| serde_json::from_value::<Pack>(p.clone()).ok()).collect())
        .unwrap_or_default();
    packs.retain(|p| {
        !p.version.is_empty() && p.tag == format!("v{}", p.version) && !p.loader.is_empty() && safe_tag(&p.tag)
    });
    packs.sort_by(|a, b| compare(&b.version, &a.version));
    packs.dedup_by(|a, b| a.tag == b.tag);
    packs
}

/// A tag that can go into a download address as is.
fn safe_tag(tag: &str) -> bool {
    tag.len() <= 64 && tag.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}

/// Compares two versions: numbers part by part ("0.1.10" after "0.1.9"), and a test version before the release it
/// leads to ("0.1.45-rc.1" < "0.1.45-rc.2" < "0.1.45").
pub fn compare(a: &str, b: &str) -> Ordering {
    let split = |v: &str| {
        let v = v.trim().trim_start_matches('v');
        let (main, pre) = v.split_once('-').unwrap_or((v, ""));
        (main.split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>(), pre.to_string())
    };
    let ((ma, pa), (mb, pb)) = (split(a), split(b));
    let n = ma.len().max(mb.len());
    for i in 0..n {
        let o = ma.get(i).unwrap_or(&0).cmp(mb.get(i).unwrap_or(&0));
        if o != Ordering::Equal {
            return o;
        }
    }
    match (pa.is_empty(), pb.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            let (xa, xb): (Vec<&str>, Vec<&str>) = (pa.split('.').collect(), pb.split('.').collect());
            for i in 0..xa.len().max(xb.len()) {
                let o = match (xa.get(i), xb.get(i)) {
                    (Some(x), Some(y)) => match (x.parse::<u64>(), y.parse::<u64>()) {
                        (Ok(x), Ok(y)) => x.cmp(&y),
                        (Ok(_), Err(_)) => Ordering::Less,
                        (Err(_), Ok(_)) => Ordering::Greater,
                        _ => x.cmp(y),
                    },
                    (None, Some(_)) => Ordering::Less,
                    (Some(_), None) => Ordering::Greater,
                    (None, None) => Ordering::Equal,
                };
                if o != Ordering::Equal {
                    return o;
                }
            }
            Ordering::Equal
        }
    }
}

/// Does this version of a mod work with the Stein Loader library `loader`? Unknown values never block.
pub fn compatible(m: &IndexMod, loader: Option<&str>) -> bool {
    let Some(loader) = loader.filter(|l| !l.is_empty()) else { return true };
    m.loader.is_empty() || compare(loader, &m.loader) != Ordering::Less
}

/// Every version of a mod the index offers, newest first: its history, or (an index without one) the current version
/// plus its test version.
pub fn versions_of(m: &IndexMod) -> Vec<IndexMod> {
    let mut out: Vec<IndexMod> = m.versions.clone();
    if !out.iter().any(|v| v.version == m.version) {
        let mut cur = m.clone();
        cur.versions.clear();
        out.push(cur);
    }
    out.sort_by(|a, b| compare(&b.version, &a.version));
    out.dedup_by(|a, b| a.version == b.version);
    out
}

/// A mod that stays as it is: no version of it works with the chosen Stein Loader.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Blocked {
    pub id: String,
    /// The minimum Loader the mod's newest version asks for.
    pub needs_loader: String,
}

/// What the folder should hold: the pack and each enabled mod's version.
#[derive(Debug, Clone)]
pub struct Plan {
    /// The chosen pack; none when the registry has no pack history (then the index's latest version applies).
    pub pack: Option<Pack>,
    /// The index with the chosen versions: `loader`/`tag` of the pack, and each mod at its chosen version. Blocked
    /// mods are not in it, so nothing touches their file.
    pub index: Index,
    pub blocked: Vec<Blocked>,
    /// The Stein Loader library the mods were checked against; none when it is not known.
    pub loader: Option<String>,
}

/// The pack to install: the latest stable one (the index's `latest`) or, with test versions on, the newest test pack
/// while it is ahead of it.
pub fn chosen_pack(index: &Index, s: &Settings) -> Option<Pack> {
    let stable = index.packs.iter().find(|p| !p.test && p.version == index.loader);
    if s.test_versions {
        let ahead = |t: &&Pack| stable.is_none_or(|st| compare(&t.version, &st.version) == Ordering::Greater);
        if let Some(t) = index.packs.iter().filter(|p| p.test).find(ahead) {
            return Some(t.clone());
        }
    }
    stable.cloned()
}

/// The Stein Loader library a pack carries, or for no known pack the one installed now (`installed_loader`).
fn loader_of(pack: Option<&Pack>, installed_loader: Option<&str>) -> Option<String> {
    pack.map(|p| p.loader.clone()).or_else(|| installed_loader.map(str::to_string))
}

/// The version of a mod to install: the newest that works with the Loader, a test one only with test versions on (and
/// only while it is ahead of the stable ones, the list being newest first). `Err` with what the mod asks for when none
/// works.
pub fn chosen_mod(m: &IndexMod, s: &Settings, loader: Option<&str>) -> Result<IndexMod, Blocked> {
    let all = versions_of(m);
    if let Some(v) = all.iter().find(|v| (!v.test || s.test_versions) && compatible(v, loader)) {
        return Ok(v.clone());
    }
    let newest = all.iter().find(|v| !v.test).or(all.first()).unwrap_or(m);
    Err(Blocked { id: m.id.clone(), needs_loader: newest.loader.clone() })
}

/// The plan for the folder from the index and the player's choices. `installed_loader`: the Stein Loader library in the
/// folder now, used to check the mods when the registry has no pack history.
pub fn plan(index: &Index, s: &Settings, installed_loader: Option<&str>) -> Plan {
    let pack = chosen_pack(index, s);
    let loader = loader_of(pack.as_ref(), installed_loader);
    let mut out = index.clone();
    if let Some(p) = &pack {
        out.loader = p.base_version().to_string();
        out.tag = p.tag.clone();
    }
    let mut blocked = Vec::new();
    out.mods = index
        .mods
        .iter()
        .filter_map(|m| match chosen_mod(m, s, loader.as_deref()) {
            Ok(mut v) => {
                // The entry keeps the mod's own fields (source, third-party) and its history, for the next plan.
                v.source = m.source.clone();
                v.third_party = m.third_party;
                v.source_name = m.source_name.clone();
                v.versions = m.versions.clone();
                Some(v)
            }
            Err(b) => {
                blocked.push(b);
                None
            }
        })
        .collect();
    Plan { pack, index: out, blocked, loader }
}

/// [`plan`] for the folder: the Stein Loader library installed there is the reserve when the pack is unknown.
pub fn plan_for(root: &crate::paths::Root, index: &Index, s: &Settings) -> Plan {
    let installed = crate::stein::components(root).map(|c| c.1);
    plan(index, s, installed.as_deref())
}

/// The release notes of a version in the player's language (Portuguese, then English, then any).
pub fn notes(changelog: &BTreeMap<String, String>) -> Option<String> {
    changelog.get("pt_BR").or_else(|| changelog.get("en_US")).or_else(|| changelog.values().next()).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(version: &str, loader: &str, test: bool) -> IndexMod {
        serde_json::from_value(serde_json::json!({
            "id": "combat", "modId": "steincombat", "name": "Combat", "version": version, "file": "Combat.steinmod",
            "url": "https://e/Combat.steinmod", "sha256": "a".repeat(64), "loader": loader, "test": test
        }))
        .unwrap()
    }

    fn pack(version: &str, loader: &str, test: bool) -> Pack {
        Pack { version: version.into(), tag: format!("v{version}"), test, loader: loader.into(), ..Pack::default() }
    }

    fn index() -> Index {
        let mut combat = m("0.1.31", "0.1.33", false);
        combat.versions =
            vec![m("0.1.32-rc.1", "0.1.33", true), m("0.1.31", "0.1.33", false), m("0.1.30", "0.1.28", false)];
        Index {
            loader: "0.1.45".into(),
            packs: vec![
                pack("0.1.46-rc.1", "0.1.34", true),
                pack("0.1.45", "0.1.33", false),
                pack("0.1.44", "0.1.32", false),
            ],
            mods: vec![combat],
            ..Index::default()
        }
    }

    #[test]
    fn version_order() {
        let mut v = vec!["0.1.45", "0.1.9", "0.1.45-rc.2", "0.1.10", "0.1.45-rc.1", "v0.1.44", "0.1.45-rc.10"];
        v.sort_by(|a, b| compare(a, b));
        assert_eq!(v, ["0.1.9", "0.1.10", "v0.1.44", "0.1.45-rc.1", "0.1.45-rc.2", "0.1.45-rc.10", "0.1.45"]);
        assert_eq!(compare("0.1.33", "0.1.33.0"), Ordering::Equal);
    }

    #[test]
    fn latest_by_default() {
        let p = plan(&index(), &Settings::default(), None);
        assert_eq!(p.pack.unwrap().version, "0.1.45");
        assert_eq!(p.index.loader_tag(), "v0.1.45");
        assert_eq!(p.index.mods[0].version, "0.1.31"); // the newest stable one, never the test one
        assert_eq!(p.index.mods[0].versions.len(), 3);
    }

    #[test]
    fn test_versions_switch_takes_the_newest_test_ones() {
        let on = Settings { test_versions: true, ..Settings::default() };
        let p = plan(&index(), &on, None);
        assert_eq!(p.index.loader, "0.1.46");
        assert_eq!(p.index.loader_tag(), "v0.1.46-rc.1");
        assert_eq!(p.index.mods[0].version, "0.1.32-rc.1");
        // A test version the stable one already passed is not taken.
        let mut i = index();
        i.packs.insert(0, pack("0.1.45-rc.2", "0.1.33", true));
        i.packs.retain(|p| p.version != "0.1.46-rc.1");
        i.mods[0].versions[0] = m("0.1.31-rc.1", "0.1.33", true);
        let p = plan(&i, &on, None);
        assert_eq!(p.pack.unwrap().version, "0.1.45");
        assert_eq!(p.index.mods[0].version, "0.1.31");
    }

    #[test]
    fn a_mod_with_no_version_for_the_loader_stays_as_it_is() {
        let mut i = index();
        i.mods[0].versions.retain(|v| v.loader == "0.1.33");
        let p = plan(&i, &Settings::default(), Some("0.1.32"));
        let mut no_history = i.clone();
        no_history.packs.clear();
        let p2 = plan(&no_history, &Settings::default(), Some("0.1.32"));
        assert_eq!(p.index.mods[0].version, "0.1.31"); // the 0.1.45 pack carries Loader 0.1.33
        assert!(p2.index.mods.is_empty());
        assert_eq!(p2.blocked[0].needs_loader, "0.1.33");
    }

    #[test]
    fn without_pack_history_the_installed_loader_decides() {
        let mut i = index();
        i.packs.clear();
        let p = plan(&i, &Settings::default(), Some("0.1.32"));
        assert!(p.pack.is_none());
        assert_eq!(p.index.loader_tag(), "v0.1.45");
        assert_eq!(p.index.mods[0].version, "0.1.30");
        // Nothing known about the Loader: nothing is blocked.
        assert_eq!(plan(&i, &Settings::default(), None).index.mods[0].version, "0.1.31");
    }

    #[test]
    fn loader_minimum_and_index_without_history() {
        let v = m("1.0", "0.1.20", false);
        assert!(compatible(&v, Some("0.1.20")) && compatible(&v, Some("0.1.40")) && !compatible(&v, Some("0.1.19")));
        let only = m("1.0", "", false);
        assert_eq!(versions_of(&only).len(), 1);
    }

    #[test]
    fn packs_file() {
        let raw = serde_json::json!({"schema": 1, "packs": [
            {"version": "0.1.44", "tag": "v0.1.44", "bridge": "0.1.29", "loader": "0.1.32", "date": "2026-11-14",
             "changelog": {"pt_BR": "# x"}},
            {"version": "0.1.45-rc.1", "tag": "v0.1.45-rc.1", "test": true, "bridge": "0.1.30", "loader": "0.1.33"},
            {"version": "0.1.43", "tag": "../evil", "loader": "0.1.31"},
            {"version": "0.1.42", "tag": "v0.1.42"}
        ]});
        let p = parse_packs(&raw);
        assert_eq!(p.iter().map(|p| p.version.as_str()).collect::<Vec<_>>(), ["0.1.45-rc.1", "0.1.44"]);
        assert_eq!(p[0].base_version(), "0.1.45");
        assert_eq!(notes(&p[1].changelog).as_deref(), Some("# x"));
    }
}
