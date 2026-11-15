/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

//! The check before the game opens: does each mod in the folder still find in the installed Stein Loader everything it
//! uses? A mod declares the oldest Loader it needs, but it cannot know which newer Loader will break it, so the
//! launcher looks at the files themselves, after everything was downloaded.
//!
//! Two sources of warnings, and neither ever blocks: the player sees them and chooses whether to play anyway.
//! - **What the mod uses**: every class, method and field of the Loader (`dev/xavier/stein/loader/...`) a mod's classes
//!   refer to must exist in the installed Loader jar, with the same type. One that is missing means the mod fails when
//!   it reaches it. Constants and enum positions copied into the mod at build time are not seen here; the Loader's own
//!   build keeps those from changing (its ApiCheck).
//! - **Known breaks** (`breaks.json` of the registry): a change of behaviour that the files cannot show, written down
//!   when someone finds it ("Combat up to 0.1.29 misbehaves with Loader 0.1.35 or newer").

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths::Root;
use crate::versions::compare;

/// The registry file with the known breaks.
pub const BREAKS_FILE: &str = "breaks.json";

/// The Loader's classes: what a mod may refer to and the launcher checks.
const LOADER_PACKAGE: &str = "dev/xavier/stein/loader/";

/// Cap on what is read from one entry of a jar: a class or a manifest is far smaller.
const ENTRY_LIMIT: u64 = 16 * 1024 * 1024;

/// A mod the installed Loader may not run.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Warning {
    /// The id inside the `.steinmod`.
    pub mod_id: String,
    pub name: String,
    pub version: String,
    /// What the mod uses that the Loader does not have, readable ("Render.text(String, int, int)").
    pub missing: Vec<String>,
    /// The known break that names this version (in the player's language), if any.
    pub reason: Option<String>,
}

/// The mods in the folder the installed Loader may not run, with the Loader version. Nothing to say (or no Loader
/// installed yet): an empty list.
pub fn check(root: &Root) -> (Option<String>, Vec<Warning>) {
    let Some(loader) = crate::stein::components(root).map(|c| c.1) else { return (None, Vec::new()) };
    let jar = loader_jar(root, &loader);
    let api = loader_classes(&jar);
    let breaks = crate::registry::cached_json(root, BREAKS_FILE).map(|r| parse_breaks(&r)).unwrap_or_default();
    let mut out = Vec::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(root.mods())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("steinmod"))
        .collect();
    files.sort();
    for file in files {
        let Some(m) = read_mod(&file) else { continue };
        let missing = match &api {
            Some(api) => missing(api, &m.refs),
            None => Vec::new(),
        };
        let reason = breaks.iter().find(|b| b.applies(&m.id, &m.version, &loader)).map(Break::text);
        if !missing.is_empty() || reason.is_some() {
            out.push(Warning { mod_id: m.id, name: m.name, version: m.version, missing, reason });
        }
    }
    (Some(loader), out)
}

/// What identifies a set of warnings: the same mods at the same versions with the same Loader ask only once.
pub fn signature(loader: &str, warnings: &[Warning]) -> String {
    let mut parts: Vec<String> = warnings.iter().map(|w| format!("{}@{}", w.mod_id, w.version)).collect();
    parts.sort();
    format!("{loader}|{}", parts.join(","))
}

/// The Stein Loader library jar of a version, where the installer puts it.
fn loader_jar(root: &Root, version: &str) -> PathBuf {
    root.libraries()
        .join("dev")
        .join("xavier")
        .join("stein")
        .join("steinloader")
        .join(version)
        .join(format!("steinloader-{version}.jar"))
}

/// A known break from `breaks.json`: versions of a mod (inclusive bounds, each optional) that misbehave with a range of
/// Stein Loader versions.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Break {
    /// The id inside the `.steinmod`.
    #[serde(rename = "mod")]
    mod_id: String,
    mod_min: String,
    mod_max: String,
    loader_min: String,
    loader_max: String,
    /// What happens, by language ("pt_BR", "en_US", ...).
    reason: std::collections::BTreeMap<String, String>,
}

impl Break {
    fn applies(&self, mod_id: &str, version: &str, loader: &str) -> bool {
        let within = |v: &str, min: &str, max: &str| {
            (min.is_empty() || compare(v, min).is_ge()) && (max.is_empty() || compare(v, max).is_le())
        };
        self.mod_id == mod_id
            && within(version, &self.mod_min, &self.mod_max)
            && within(loader, &self.loader_min, &self.loader_max)
    }

    fn text(&self) -> String {
        crate::versions::notes(&self.reason).unwrap_or_default()
    }
}

fn parse_breaks(raw: &serde_json::Value) -> Vec<Break> {
    let Some(list) = raw["breaks"].as_array() else { return Vec::new() };
    list.iter()
        .filter_map(|b| serde_json::from_value::<Break>(b.clone()).ok())
        .filter(|b| !b.mod_id.is_empty())
        .collect()
}

/// A class of the Loader: its parents and its members ("name:descriptor").
#[derive(Debug, Default)]
struct ClassInfo {
    parents: Vec<String>,
    members: HashSet<String>,
}

/// What a class refers to in the Loader's package: classes, and members as (owner, name, descriptor).
#[derive(Debug, Default)]
struct Refs {
    classes: HashSet<String>,
    members: HashSet<(String, String, String)>,
}

struct ModFile {
    id: String,
    name: String,
    version: String,
    refs: Refs,
}

/// The classes of the Loader package in the jar; `None` when the jar cannot be read (then only the known breaks count).
fn loader_classes(jar: &Path) -> Option<HashMap<String, ClassInfo>> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(jar).ok()?).ok()?;
    let mut out = HashMap::new();
    for i in 0..zip.len() {
        let Ok(mut e) = zip.by_index(i) else { continue };
        if !e.name().starts_with(LOADER_PACKAGE) || !e.name().ends_with(".class") {
            continue;
        }
        let Some(bytes) = read_entry(&mut e) else { continue };
        if let Some(c) = parse_class(&bytes) {
            out.insert(c.name.clone(), ClassInfo { parents: c.parents, members: c.members });
        }
    }
    (!out.is_empty()).then_some(out)
}

/// The id, name and version of a `.steinmod` and what its classes use of the Loader.
fn read_mod(file: &Path) -> Option<ModFile> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(file).ok()?).ok()?;
    let manifest: serde_json::Value = {
        let mut e = zip.by_name("steinmod.json").ok()?;
        serde_json::from_slice(&read_entry(&mut e)?).ok()?
    };
    let id = manifest["id"].as_str()?.to_string();
    let version = manifest["version"].as_str().unwrap_or("").to_string();
    let name = manifest["name"].as_str().filter(|n| !n.is_empty()).unwrap_or(&id).to_string();
    let mut refs = Refs::default();
    for i in 0..zip.len() {
        let Ok(mut e) = zip.by_index(i) else { continue };
        if !e.name().ends_with(".class") {
            continue;
        }
        if let Some(c) = read_entry(&mut e).and_then(|b| parse_class(&b)) {
            refs.classes.extend(c.refs.classes);
            refs.members.extend(c.refs.members);
        }
    }
    Some(ModFile { id, name, version, refs })
}

fn read_entry(e: &mut impl Read) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    e.take(ENTRY_LIMIT).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

/// What a mod uses that the Loader does not have, readable and sorted.
fn missing(api: &HashMap<String, ClassInfo>, refs: &Refs) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for class in &refs.classes {
        if !api.contains_key(class) {
            out.push(short_class(class));
        }
    }
    for (owner, name, desc) in &refs.members {
        // A missing class was already listed.
        if api.contains_key(owner) && !has_member(api, owner, &format!("{name}:{desc}"), &mut HashSet::new()) {
            out.push(readable_member(owner, name, desc));
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The member in the class or in one of its parents. `Object` and `Enum`, the parents of nearly every Loader class, are
/// known by name; any other parent outside the Loader (Java's, the game's) cannot be checked here, so it counts as
/// having it.
fn has_member(api: &HashMap<String, ClassInfo>, class: &str, key: &str, seen: &mut HashSet<String>) -> bool {
    if !seen.insert(class.to_string()) {
        return false;
    }
    let name = key.split(':').next().unwrap_or(key);
    match class {
        "java/lang/Object" => return OBJECT_METHODS.contains(&name),
        "java/lang/Enum" => return ENUM_METHODS.contains(&name) || OBJECT_METHODS.contains(&name),
        _ => {}
    }
    let Some(c) = api.get(class) else { return true };
    c.members.contains(key) || c.parents.iter().any(|p| has_member(api, p, key, seen))
}

/// What every class gets from `java.lang.Object`.
const OBJECT_METHODS: &[&str] =
    &["<init>", "equals", "hashCode", "toString", "getClass", "notify", "notifyAll", "wait", "clone", "finalize"];

/// What every enum gets from `java.lang.Enum`.
const ENUM_METHODS: &[&str] = &["name", "ordinal", "compareTo", "getDeclaringClass", "describeConstable", "valueOf"];

/// "dev/xavier/stein/loader/api/Render$Text" as "Render.Text".
fn short_class(class: &str) -> String {
    class.rsplit('/').next().unwrap_or(class).replace('$', ".")
}

/// A member as the player could find it in the mod's code: "Render.text(String, int, int)" or "Render.WHITE".
fn readable_member(owner: &str, name: &str, desc: &str) -> String {
    let owner = short_class(owner);
    let Some(params) = desc.strip_prefix('(').and_then(|d| d.split_once(')')).map(|(p, _)| p) else {
        return format!("{owner}.{name}");
    };
    let mut types = Vec::new();
    let mut rest = params;
    while !rest.is_empty() {
        let (t, after) = java_type(rest);
        types.push(t);
        rest = after;
    }
    let name = if name == "<init>" { "new".to_string() } else { name.to_string() };
    format!("{owner}.{name}({})", types.join(", "))
}

/// The first type of a descriptor, readable, and what follows it.
fn java_type(d: &str) -> (String, &str) {
    let mut dims = 0;
    let mut rest = d;
    while let Some(r) = rest.strip_prefix('[') {
        dims += 1;
        rest = r;
    }
    let (base, after) = match rest.as_bytes().first() {
        Some(b'L') => match rest.find(';') {
            Some(end) => (short_class(&rest[1..end]), &rest[end + 1..]),
            None => (rest.to_string(), ""),
        },
        Some(c) => {
            let t = match c {
                b'Z' => "boolean",
                b'B' => "byte",
                b'C' => "char",
                b'S' => "short",
                b'I' => "int",
                b'J' => "long",
                b'F' => "float",
                b'D' => "double",
                b'V' => "void",
                _ => "?",
            };
            (t.to_string(), &rest[1..])
        }
        None => (String::new(), ""),
    };
    (format!("{base}{}", "[]".repeat(dims)), after)
}

/// A class file read just enough for the check: its name, parents and members, and what it refers to in the Loader.
struct Class {
    name: String,
    parents: Vec<String>,
    members: HashSet<String>,
    refs: Refs,
}

enum Cp {
    Utf8(String),
    Class(u16),
    Ref(u16, u16),
    NameType(u16, u16),
    Other,
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn bytes(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.b.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }
    fn u1(&mut self) -> Option<u8> {
        Some(self.bytes(1)?[0])
    }
    fn u2(&mut self) -> Option<u16> {
        let s = self.bytes(2)?;
        Some(u16::from_be_bytes([s[0], s[1]]))
    }
    fn u4(&mut self) -> Option<u32> {
        let s = self.bytes(4)?;
        Some(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    }
}

fn parse_class(b: &[u8]) -> Option<Class> {
    let mut r = Reader { b, at: 0 };
    if r.u4()? != 0xCAFE_BABE {
        return None;
    }
    r.u2()?;
    r.u2()?;
    let count = r.u2()? as usize;
    let mut cp: Vec<Cp> = Vec::with_capacity(count);
    cp.push(Cp::Other);
    while cp.len() < count {
        let tag = r.u1()?;
        let entry = match tag {
            1 => {
                let n = r.u2()? as usize;
                Cp::Utf8(String::from_utf8_lossy(r.bytes(n)?).into_owned())
            }
            7 => Cp::Class(r.u2()?),
            9..=11 => Cp::Ref(r.u2()?, r.u2()?),
            12 => Cp::NameType(r.u2()?, r.u2()?),
            3 | 4 => {
                r.u4()?;
                Cp::Other
            }
            5 | 6 => {
                r.bytes(8)?;
                // Long and double take two slots.
                cp.push(Cp::Other);
                Cp::Other
            }
            8 | 16 | 19 | 20 => {
                r.u2()?;
                Cp::Other
            }
            15 => {
                r.u1()?;
                r.u2()?;
                Cp::Other
            }
            17 | 18 => {
                r.u4()?;
                Cp::Other
            }
            _ => return None,
        };
        cp.push(entry);
    }
    let utf = |i: u16| match cp.get(i as usize) {
        Some(Cp::Utf8(s)) => Some(s.as_str()),
        _ => None,
    };
    let class_name = |i: u16| match cp.get(i as usize) {
        Some(Cp::Class(n)) => utf(*n).map(element_class),
        _ => None,
    };
    r.u2()?;
    let name = class_name(r.u2()?)?.to_string();
    let mut parents = Vec::new();
    if let Some(s) = class_name(r.u2()?) {
        parents.push(s.to_string());
    }
    for _ in 0..r.u2()? {
        if let Some(i) = class_name(r.u2()?) {
            parents.push(i.to_string());
        }
    }
    let mut members = HashSet::new();
    for _ in 0..2 {
        for _ in 0..r.u2()? {
            r.u2()?;
            let (n, d) = (r.u2()?, r.u2()?);
            if let (Some(n), Some(d)) = (utf(n), utf(d)) {
                members.insert(format!("{n}:{d}"));
            }
            for _ in 0..r.u2()? {
                r.u2()?;
                let len = r.u4()? as usize;
                r.bytes(len)?;
            }
        }
    }
    let mut refs = Refs::default();
    for e in &cp {
        match e {
            Cp::Class(n) => {
                if let Some(c) = utf(*n).map(element_class) {
                    if c.starts_with(LOADER_PACKAGE) && c != name {
                        refs.classes.insert(c.to_string());
                    }
                }
            }
            Cp::Ref(owner, nt) => {
                let Some(owner) = class_name(*owner) else { continue };
                if !owner.starts_with(LOADER_PACKAGE) || owner == name {
                    continue;
                }
                if let Some(Cp::NameType(n, d)) = cp.get(*nt as usize) {
                    if let (Some(n), Some(d)) = (utf(*n), utf(*d)) {
                        refs.members.insert((owner.to_string(), n.to_string(), d.to_string()));
                    }
                }
            }
            _ => {}
        }
    }
    Some(Class { name, parents, members, refs })
}

/// The class an array type is made of ("[Lx/Y;" is "x/Y"); a plain class name as is.
fn element_class(n: &str) -> &str {
    let n = n.trim_start_matches('[');
    n.strip_prefix('L').and_then(|s| s.strip_suffix(';')).unwrap_or(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api() -> HashMap<String, ClassInfo> {
        let mut api = HashMap::new();
        let render = ClassInfo {
            parents: vec!["java/lang/Object".into()],
            members: ["text:(Ljava/lang/String;II)V".to_string(), "<init>:()V".to_string()].into(),
        };
        api.insert("dev/xavier/stein/loader/api/Render".to_string(), render);
        let sub = ClassInfo { parents: vec!["dev/xavier/stein/loader/api/Render".into()], members: HashSet::new() };
        api.insert("dev/xavier/stein/loader/api/Sub".to_string(), sub);
        api
    }

    #[test]
    fn finds_what_is_missing() {
        let mut refs = Refs::default();
        refs.classes.insert("dev/xavier/stein/loader/api/Render".into());
        refs.classes.insert("dev/xavier/stein/loader/api/Gone".into());
        let m = |o: &str, n: &str, d: &str| (format!("dev/xavier/stein/loader/api/{o}"), n.into(), d.into());
        refs.members.insert(m("Render", "text", "(Ljava/lang/String;II)V"));
        // Inherited from a Loader parent: fine; from Java's Object: cannot be checked, fine.
        refs.members.insert(m("Sub", "text", "(Ljava/lang/String;II)V"));
        refs.members.insert(m("Sub", "hashCode", "()I"));
        // The same name with another type, and a member of a class that is gone (listed once, as the class).
        refs.members.insert(m("Render", "text", "(Ljava/lang/String;FF)V"));
        refs.members.insert(m("Gone", "x", "I"));
        assert_eq!(missing(&api(), &refs), ["Gone", "Render.text(String, float, float)"]);
    }

    #[test]
    fn readable_members() {
        let o = "dev/xavier/stein/loader/api/Mesh$Builder";
        assert_eq!(readable_member(o, "<init>", "([I[[Ljava/util/List;J)V"), "Mesh.Builder.new(int[], List[][], long)");
        assert_eq!(readable_member(o, "WHITE", "I"), "Mesh.Builder.WHITE");
    }

    #[test]
    fn known_breaks() {
        let raw = serde_json::json!({"schema": 1, "breaks": [
            {"mod": "steincombat", "modMax": "0.1.29", "loaderMin": "0.1.35", "reason": {"pt_BR": "x"}},
            {"modMax": "9"}
        ]});
        let b = parse_breaks(&raw);
        assert_eq!(b.len(), 1);
        assert!(b[0].applies("steincombat", "0.1.29", "0.1.35"));
        assert!(!b[0].applies("steincombat", "0.1.30", "0.1.35"));
        assert!(!b[0].applies("steincombat", "0.1.29", "0.1.34"));
        assert!(!b[0].applies("steinhud", "0.1.1", "0.1.40"));
        assert_eq!(b[0].text(), "x");
    }

    #[test]
    fn same_warnings_same_signature() {
        let w = |id: &str, v: &str| Warning {
            mod_id: id.into(),
            name: id.into(),
            version: v.into(),
            missing: vec![],
            reason: None,
        };
        let a = signature("0.1.32", &[w("b", "1"), w("a", "2")]);
        assert_eq!(a, signature("0.1.32", &[w("a", "2"), w("b", "1")]));
        assert_ne!(a, signature("0.1.33", &[w("a", "2"), w("b", "1")]));
        assert_ne!(a, signature("0.1.32", &[w("a", "3"), w("b", "1")]));
    }
}
