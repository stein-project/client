/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

//! The game's output in `logs/stein-client/game.log`, kept across starts: before the game starts again, the previous
//! one goes into `logs/stein-client/<date>-<n>.zip`, as the game archives its own latest.log.
//!
//! Before, every start rewrote game.log. A player whose game froze, who closed it and opened it again, had already
//! lost the one session that mattered (15/11/2026). The launcher's own files (launcher.log, installer.log) go along
//! when they are big, so they cannot grow without end either.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::paths::Root;

/// The newest archives kept; older ones are deleted.
const KEEP: usize = 200;

/// The launcher's append-only files are archived (and started over) past this size.
const LAUNCHER_LOG_LIMIT: u64 = 1 << 20;

/// Files of the launcher's own that go into the archive when they pass [`LAUNCHER_LOG_LIMIT`].
const LAUNCHER_LOGS: &[&str] = &["launcher.log", "installer.log"];

/// Archives the previous game.log (and a launcher log that grew too big) before the game starts again. Returns the
/// archive written, if any.
pub fn archive_previous(root: &Root) -> Result<Option<PathBuf>> {
    let dir = root.logs();
    std::fs::create_dir_all(&dir)?;
    let mut files: Vec<PathBuf> = Vec::new();
    let game = dir.join("game.log");
    if non_empty(&game) {
        files.push(game);
    }
    for name in LAUNCHER_LOGS {
        let f = dir.join(name);
        if std::fs::metadata(&f).map(|m| m.len() > LAUNCHER_LOG_LIMIT).unwrap_or(false) {
            files.push(f);
        }
    }
    if files.is_empty() {
        return Ok(None);
    }
    let newest = files.iter().filter_map(|f| f.metadata().and_then(|m| m.modified()).ok()).max();
    let day = newest.map(chrono::DateTime::<chrono::Local>::from).unwrap_or_else(chrono::Local::now);
    let zip = free_name(&dir, &day.format("%Y-%m-%d").to_string()).context("no free archive name")?;
    write_zip(&zip, &files).with_context(|| format!("archiving into {}", zip.display()))?;
    for f in &files {
        // A file still held (a game from the same folder that is still running) stays; its copy is in the zip.
        let _ = std::fs::remove_file(f);
    }
    prune(&dir);
    Ok(Some(zip))
}

fn non_empty(f: &Path) -> bool {
    std::fs::metadata(f).map(|m| m.len() > 0).unwrap_or(false)
}

/// `<day>-<n>.zip` with the first n not taken.
fn free_name(dir: &Path, day: &str) -> Option<PathBuf> {
    (1..10_000).map(|n| dir.join(format!("{day}-{n}.zip"))).find(|p| !p.exists())
}

fn write_zip(zip: &Path, files: &[PathBuf]) -> Result<()> {
    let tmp = zip.with_extension("zip.tmp");
    {
        let mut w = zip::ZipWriter::new(std::fs::File::create(&tmp)?);
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for f in files {
            let name = f.file_name().and_then(|n| n.to_str()).unwrap_or("log").to_string();
            w.start_file(name, options)?;
            let mut src = std::fs::File::open(f)?;
            std::io::copy(&mut src, &mut w)?;
        }
        w.finish()?.flush()?;
    }
    std::fs::rename(&tmp, zip)?;
    Ok(())
}

/// Deletes the oldest archives beyond [`KEEP`].
fn prune(dir: &Path) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    let mut zips: Vec<(std::time::SystemTime, PathBuf)> = read
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| is_archive(p))
        .filter_map(|p| Some((p.metadata().and_then(|m| m.modified()).ok()?, p)))
        .collect();
    if zips.len() <= KEEP {
        return;
    }
    zips.sort();
    for (_, p) in &zips[..zips.len() - KEEP] {
        let _ = std::fs::remove_file(p);
    }
}

/// `YYYY-MM-DD-N.zip`.
fn is_archive(p: &Path) -> bool {
    let Some(name) = p.file_name().and_then(|n| n.to_str()) else { return false };
    let Some(stem) = name.strip_suffix(".zip") else { return false };
    let b = stem.as_bytes();
    b.len() >= 12
        && b[..10].iter().enumerate().all(|(i, c)| if i == 4 || i == 7 { *c == b'-' } else { c.is_ascii_digit() })
        && b[10] == b'-'
        && b[11..].iter().all(u8::is_ascii_digit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(name: &str) -> Root {
        let dir = std::env::temp_dir().join(format!("stein-gamelog-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Root::at(dir)
    }

    #[test]
    fn previous_game_log_goes_into_a_zip() {
        let root = root("zip");
        std::fs::create_dir_all(root.logs()).unwrap();
        std::fs::write(root.logs().join("game.log"), "first session\n").unwrap();
        let zip = archive_previous(&root).unwrap().expect("an archive");
        assert!(!root.logs().join("game.log").exists());
        let mut a = zip::ZipArchive::new(std::fs::File::open(&zip).unwrap()).unwrap();
        let mut text = String::new();
        std::io::Read::read_to_string(&mut a.by_name("game.log").unwrap(), &mut text).unwrap();
        assert_eq!(text, "first session\n");
        // the second start takes the next number
        std::fs::write(root.logs().join("game.log"), "second\n").unwrap();
        let second = archive_previous(&root).unwrap().unwrap();
        assert_ne!(zip, second);
        assert!(second.file_name().unwrap().to_str().unwrap().ends_with("-2.zip"));
        // nothing to archive: no zip
        assert!(archive_previous(&root).unwrap().is_none());
        let _ = std::fs::remove_dir_all(root.dir());
    }

    #[test]
    fn small_launcher_logs_stay() {
        let root = root("small");
        std::fs::create_dir_all(root.logs()).unwrap();
        std::fs::write(root.logs().join("launcher.log"), "a line\n").unwrap();
        assert!(archive_previous(&root).unwrap().is_none());
        assert!(root.logs().join("launcher.log").exists());
        let _ = std::fs::remove_dir_all(root.dir());
    }

    #[test]
    fn archive_names() {
        assert!(is_archive(Path::new("2026-11-15-3.zip")));
        assert!(is_archive(Path::new("2026-11-15-12.zip")));
        assert!(!is_archive(Path::new("game.zip")));
        assert!(!is_archive(Path::new("2026-11-15.zip")));
    }
}
