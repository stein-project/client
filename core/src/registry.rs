/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

//! The Stein registry: the files the project publishes for its programs to read at run time (Stein-Project/registry,
//! served at [`DEFAULT_BASE`]), instead of baking them into a build. The launcher reads `stein.json` (links, the latest
//! Loader, news, the official mods enabled by default and their indexes), `mods/<id>.json` (each official mod's
//! index), `client.json` (its own update), `launch.json` and `compat.json` (the game's Java arguments)
//! and `migrations.json` (mods that changed source). `revoked.json` stays published for older launchers only.
//!
//! Every file comes with an ed25519 signature next to it (`<file>.sig`, the base64 of the raw signature over the file's
//! exact bytes) and is only believed when it matches [`PUBLIC_KEY`]: the registry decides what is downloaded and which
//! Java arguments the game gets, so a hijacked account or a tampered copy on the way must not be able to change that.
//! The verified copy is kept in `stein-client/registry/` and is what applies without internet, or when a new copy
//! arrives without a valid signature or with a `schema` this launcher does not know.
//!
//! `STEIN_REGISTRY` (environment) reads another address (a local copy, for tests); an empty value fetches nothing and
//! keeps the stored copies.

use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

use crate::net;
use crate::paths::Root;

/// Where the registry is published; the only address of it in the launcher.
pub const DEFAULT_BASE: &str = "https://stein-project.github.io/registry/";

/// The registry's public key (ed25519, the raw 32 bytes in base64). Changing it means a new launcher for everyone:
/// the private key that matches it signs every file.
pub const PUBLIC_KEY: &str = "exxXasq5SsodR84fDi9gVEs3oRfXXaPVgTn/eK47TUc=";

/// The file format this launcher understands (`"schema"` in each file; a file without it counts as this one).
pub const SCHEMA: u64 = 1;

/// Cap on a registry file: they are a handful of lines, and there is no reason to accept more.
const LIMIT: usize = 512 * 1024;

/// The registry address in use, always ending in `/`; `None` when `STEIN_REGISTRY` is set and empty (no fetching).
pub fn base() -> Option<String> {
    let base = std::env::var("STEIN_REGISTRY").unwrap_or_else(|_| DEFAULT_BASE.to_string());
    let base = base.trim();
    if base.is_empty() {
        return None;
    }
    Some(if base.ends_with('/') { base.to_string() } else { format!("{base}/") })
}

/// The registry file name behind a URL of the registry (`mods/hud.json`), or `None` for any other address. The
/// official address counts too when another one is in use: `stein.json` names files by the official address, and they
/// are read from the registry in use (a local copy, or the registry after a move).
pub fn file_of(url: &str) -> Option<String> {
    let base = base()?;
    let name = url.strip_prefix(&base).or_else(|| url.strip_prefix(DEFAULT_BASE))?;
    valid_name(name).then(|| name.to_string())
}

/// A file name the launcher accepts from the registry: plain `.json` paths, no way out of the cache folder.
fn valid_name(name: &str) -> bool {
    name.ends_with(".json")
        && !name.starts_with('/')
        && name.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
        })
}

fn cache_path(root: &Root, name: &str) -> PathBuf {
    name.split('/').fold(root.own().join("registry"), |p, part| p.join(part))
}

/// The stored copy of a file (verified when it was stored), or `None`.
pub fn cached(root: &Root, name: &str) -> Option<Vec<u8>> {
    if !valid_name(name) {
        return None;
    }
    std::fs::read(cache_path(root, name)).ok()
}

/// The stored copy, parsed.
pub fn cached_json(root: &Root, name: &str) -> Option<serde_json::Value> {
    serde_json::from_slice(&cached(root, name)?).ok()
}

/// Does the signature (base64 of the raw ed25519 signature) match the bytes and the registry's key?
pub fn verify(data: &[u8], signature: &str) -> bool {
    verify_with(PUBLIC_KEY, data, signature)
}

fn verify_with(public_key: &str, data: &[u8], signature: &str) -> bool {
    let b64 = base64::engine::general_purpose::STANDARD;
    let Ok(key) = b64.decode(public_key.trim()) else { return false };
    let Ok(key) = <[u8; 32]>::try_from(key.as_slice()) else { return false };
    let Ok(key) = VerifyingKey::from_bytes(&key) else { return false };
    let Ok(sig) = b64.decode(signature.trim()) else { return false };
    let Ok(sig) = Signature::from_slice(&sig) else { return false };
    key.verify(data, &sig).is_ok()
}

/// Downloads a file and its signature, checks both and stores the copy. Any failure (no network, a missing file, a bad
/// signature, a schema this launcher does not know) is an error and leaves the stored copy as it was.
pub async fn fetch(http: &reqwest::Client, root: &Root, name: &str) -> Result<Vec<u8>> {
    if !valid_name(name) {
        bail!("not a registry file: {name}");
    }
    let base = base().ok_or_else(|| anyhow!("the registry is turned off (STEIN_REGISTRY is empty)"))?;
    let url = format!("{base}{name}");
    let (body, sig) = futures_util::future::join(net::fetch(http, &url), net::fetch(http, &format!("{url}.sig"))).await;
    let (body, sig) = (body?, sig?);
    if body.len() > LIMIT {
        bail!("registry file {name} is too large ({} bytes)", body.len());
    }
    if !verify(&body, &String::from_utf8_lossy(&sig)) {
        let e = anyhow!("registry file {name} has no valid signature: ignored, keeping the stored copy");
        crate::friendly::logged(root, "launcher.log", &e);
        return Err(e);
    }
    check_schema(name, &body)?;
    let path = cache_path(root, name);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // A temporary file of its own: two fetches of the same file at once (the index and a refresh) never mix.
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("json.{}-{n}.tmp", std::process::id()));
    std::fs::write(&tmp, &body)?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(body)
}

/// A JSON file whose `schema` is newer than this launcher's is not used: its fields may mean something else.
fn check_schema(name: &str, body: &[u8]) -> Result<()> {
    let json: serde_json::Value =
        serde_json::from_slice(body).map_err(|_| anyhow!("registry file {name} is not valid JSON"))?;
    match json.get("schema").and_then(serde_json::Value::as_u64) {
        Some(s) if s != SCHEMA => bail!("registry file {name} has schema {s}, this launcher knows {SCHEMA}"),
        _ => Ok(()),
    }
}

/// The file from the registry, or the stored copy when the registry cannot be read now.
pub async fn load(http: &reqwest::Client, root: &Root, name: &str) -> Result<Vec<u8>> {
    match fetch(http, root, name).await {
        Ok(b) => Ok(b),
        Err(e) => cached(root, name).ok_or(e),
    }
}

/// [`load`], parsed.
pub async fn load_json(http: &reqwest::Client, root: &Root, name: &str) -> Result<serde_json::Value> {
    Ok(serde_json::from_slice(&load(http, root, name).await?)?)
}

/// The files the launcher only reads from its stored copy (when the game starts, at the launcher's start): refreshed
/// together, each one on its own, ignoring the ones that cannot be read now.
pub const AUXILIARY: &[&str] = &["launch.json", "compat.json", "migrations.json"];

/// Refreshes [`AUXILIARY`]; returns how many arrived.
pub async fn refresh_auxiliary(http: &reqwest::Client, root: &Root) -> usize {
    let got = futures_util::future::join_all(AUXILIARY.iter().map(|n| fetch(http, root, n))).await;
    got.iter().filter(|r| r.is_ok()).count()
}

/// Appends a line to one of the launcher's logs (`logs/stein-client/<file>`).
pub fn note(root: &Root, file: &str, line: &str) {
    use std::io::Write;
    let _ = std::fs::create_dir_all(root.logs());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(root.logs().join(file)) {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let _ = writeln!(f, "[{secs}] Stein Client {}: {line}", crate::VERSION);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    /// Signed with the registry's real private key by `openssl pkeyutl -sign -rawin`, as `tools/sign.sh` does: proves
    /// the launcher reads what the registry's tooling writes.
    const VECTOR: &[u8] = b"stein registry test vector\n";
    const VECTOR_SIG: &str = "TJCF6Pjr9CPiQhvXMAf5198V18S+FHJ/PRg5W8Uam7mlmq3ZolLaq+uZBo6GA437FOpekUajFvppKhpIOx1MDQ==";

    fn b64(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    #[test]
    fn real_key_signature_from_openssl() {
        assert!(verify(VECTOR, VECTOR_SIG));
        assert!(verify(VECTOR, &format!("  {VECTOR_SIG}\n"))); // a trailing newline in the .sig file is fine
    }

    #[test]
    fn tampered_data_or_signature_is_refused() {
        assert!(!verify(b"stein registry test vector!\n", VECTOR_SIG));
        let mut sig = base64::engine::general_purpose::STANDARD.decode(VECTOR_SIG).unwrap();
        sig[10] ^= 1;
        assert!(!verify(VECTOR, &b64(&sig)));
        assert!(!verify(VECTOR, "not base64 !"));
        assert!(!verify(VECTOR, ""));
    }

    #[test]
    fn another_key_is_refused() {
        let other = SigningKey::from_bytes(&[7u8; 32]);
        let sig = b64(&other.sign(VECTOR).to_bytes());
        assert!(verify_with(&b64(other.verifying_key().as_bytes()), VECTOR, &sig));
        assert!(!verify(VECTOR, &sig)); // valid signature, but not by the registry's key
    }

    #[test]
    fn names_stay_inside_the_cache() {
        for good in ["stein.json", "mods/hud.json", "a-b_c.json"] {
            assert!(valid_name(good), "{good}");
        }
        for bad in ["../x.json", "mods/../../x.json", "/x.json", "x.txt", "mods//x.json", "a b.json", "x\\y.json"] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn newer_schema_is_not_used() {
        assert!(check_schema("x.json", br#"{"schema": 1}"#).is_ok());
        assert!(check_schema("x.json", br#"{"version": "1"}"#).is_ok());
        assert!(check_schema("x.json", br#"{"schema": 2}"#).is_err());
        assert!(check_schema("x.json", b"not json").is_err());
    }

    #[test]
    fn registry_urls() {
        if std::env::var("STEIN_REGISTRY").is_ok() {
            return; // depends on the default address
        }
        assert_eq!(file_of(&format!("{DEFAULT_BASE}mods/hud.json")).as_deref(), Some("mods/hud.json"));
        assert_eq!(file_of("https://github.com/x/y/releases/latest/download/steinmods.json"), None);
        assert_eq!(file_of(&format!("{DEFAULT_BASE}../x.json")), None);
    }
}
