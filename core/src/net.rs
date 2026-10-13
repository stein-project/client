/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

//! Verified downloads. Every file first goes to a `.part` file next to the destination, hashed as it arrives; it only
//! becomes the real file if the hash matches. A drop halfway through never leaves a broken file in place of a good one.

use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use sha1::Digest;
use tokio::io::AsyncWriteExt;

/// The expected hash of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hash {
    Sha1(String),
    Sha256(String),
}

impl Hash {
    /// Is the file on disk already this one?
    pub fn matches_file(&self, path: &Path) -> bool {
        match std::fs::read(path) {
            Ok(bytes) => self.matches(&bytes),
            Err(_) => false,
        }
    }

    pub fn matches(&self, bytes: &[u8]) -> bool {
        match self {
            Hash::Sha1(h) => hex::encode(sha1::Sha1::digest(bytes)).eq_ignore_ascii_case(h),
            Hash::Sha256(h) => hex::encode(sha2::Sha256::digest(bytes)).eq_ignore_ascii_case(h),
        }
    }
}

/// The Stein Client HTTP client: HTTPS via rustls only, with a connect timeout.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("SteinClient/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        // A connection that stops sending data drops after 20 s and moves on to the next attempt (without this, a
        // single stalled connection held up the whole install until the overall limit). The overall limit is only
        // for what never finishes.
        .read_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(900))
        .build()
        .expect("HTTP client")
}

/// The whole body of a URL (JSON, lists). Three attempts.
pub async fn fetch(http: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let mut last = None;
    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(600 * attempt)).await;
        }
        match http.get(url).send().await.and_then(|r| r.error_for_status()) {
            Ok(resp) => match resp.bytes().await {
                Ok(b) => return Ok(b.to_vec()),
                Err(e) => last = Some(anyhow::Error::from(e)),
            },
            Err(e) => last = Some(anyhow::Error::from(e)),
        }
    }
    Err(last.unwrap()).with_context(|| format!("could not download {url}"))
}

/// Downloads `url` to `dest`, verifying `hash`. `on_bytes` receives how many bytes arrived in each chunk (for the
/// progress bar). Three attempts; the `.part` of a failed attempt is discarded.
pub async fn download(
    http: &reqwest::Client,
    url: &str,
    dest: &Path,
    hash: &Hash,
    on_bytes: Option<&(dyn Fn(u64) + Send + Sync)>,
) -> Result<()> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    // One .part per download, not per destination: two concurrent requests for the same file do not write over each
    // other (that was the "could not write" error on the first install).
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut part_name = dest.file_name().unwrap_or_default().to_os_string();
    part_name.push(format!(".{}-{n}.part", std::process::id()));
    let part = dest.with_file_name(part_name);
    let mut last = None;
    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(800 * attempt)).await;
        }
        match try_download(http, url, &part, hash, on_bytes).await {
            Ok(()) => {
                if let Err(e) = tokio::fs::rename(&part, dest).await {
                    let _ = tokio::fs::remove_file(&part).await;
                    // The right file is already there (another request wrote it, or it is open and matches): good
                    // enough.
                    if hash.matches_file(dest) {
                        return Ok(());
                    }
                    return Err(anyhow::Error::from(e)).with_context(|| format!("could not write {}", dest.display()));
                }
                return Ok(());
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&part).await;
                last = Some(e);
            }
        }
    }
    Err(last.unwrap()).with_context(|| format!("could not download {url}"))
}

async fn try_download(
    http: &reqwest::Client,
    url: &str,
    part: &Path,
    hash: &Hash,
    on_bytes: Option<&(dyn Fn(u64) + Send + Sync)>,
) -> Result<()> {
    let resp = http.get(url).send().await?.error_for_status()?;
    let mut file = tokio::fs::File::create(part).await?;
    let mut s1 = sha1::Sha1::new();
    let mut s256 = sha2::Sha256::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        match hash {
            Hash::Sha1(_) => s1.update(&chunk),
            Hash::Sha256(_) => s256.update(&chunk),
        }
        file.write_all(&chunk).await?;
        if let Some(f) = on_bytes {
            f(chunk.len() as u64);
        }
    }
    file.flush().await?;
    drop(file);
    let (got, want) = match hash {
        Hash::Sha1(h) => (hex::encode(s1.finalize()), h),
        Hash::Sha256(h) => (hex::encode(s256.finalize()), h),
    };
    if !got.eq_ignore_ascii_case(want) {
        bail!("file downloaded from {url} does not match (hash {got}, expected {want})");
    }
    Ok(())
}
