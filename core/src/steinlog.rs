/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

//! The Stein log (logs/stein/, with network and performance) enabled for a single session: the "restart with Stein
//! logs" after a crash. The Loader stores the choice in config/steinloader.json (the `registro` key); the launcher
//! turns both topics on there before starting the game, saves what was there before, and restores it the next time
//! the game starts normally, so the player is not left with logging on forever without having asked for it.

use anyhow::Result;
use serde_json::{json, Map, Value};

use crate::paths::Root;

fn config(root: &Root) -> std::path::PathBuf {
    root.dir().join("config").join("steinloader.json")
}

fn saved(root: &Root) -> std::path::PathBuf {
    root.own().join("registro-anterior.json")
}

fn read(root: &Root) -> Map<String, Value> {
    std::fs::read(config(root))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

fn write(root: &Root, map: Map<String, Value>) -> Result<()> {
    let path = config(root);
    std::fs::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&Value::Object(map))?)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

/// Turns full logging on for the next start, saving what the player had.
pub fn enable_once(root: &Root) -> Result<()> {
    let mut map = read(root);
    if !saved(root).exists() {
        std::fs::create_dir_all(root.own())?;
        let before = map.get("registro").cloned().unwrap_or(Value::Null);
        std::fs::write(saved(root), serde_json::to_vec(&before)?)?;
    }
    map.insert("registro".into(), json!({ "rede": true, "desempenho": true }));
    write(root, map)
}

/// Restores logging to what it was before an [`enable_once`] (if there was one).
pub fn restore(root: &Root) -> Result<()> {
    let Ok(bytes) = std::fs::read(saved(root)) else { return Ok(()) };
    let before: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    let mut map = read(root);
    if before.is_null() {
        map.remove("registro");
    } else {
        map.insert("registro".into(), before);
    }
    write(root, map)?;
    std::fs::remove_file(saved(root))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> Root {
        let dir = std::env::temp_dir().join(format!("stein-steinlog-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Root::at(&dir)
    }

    #[test]
    fn enables_and_restores_previous() {
        let root = temp_root("previous");
        std::fs::create_dir_all(root.dir().join("config")).unwrap();
        std::fs::write(
            config(&root),
            r#"{"disabled":["replay"],"registro":{"rede":false,"desempenho":true},"panelKey":true}"#,
        )
        .unwrap();
        enable_once(&root).unwrap();
        enable_once(&root).unwrap(); // twice in a row does not lose the original
        let on = read(&root);
        assert_eq!(on["registro"], json!({ "rede": true, "desempenho": true }));
        assert_eq!(on["disabled"], json!(["replay"]));
        restore(&root).unwrap();
        let back = read(&root);
        assert_eq!(back["registro"], json!({ "rede": false, "desempenho": true }));
        assert_eq!(back["panelKey"], json!(true));
        assert!(!saved(&root).exists());
        restore(&root).unwrap(); // with nothing saved, leaves it alone
        assert_eq!(read(&root), back);
    }

    #[test]
    fn without_config_goes_back_to_no_logging_key() {
        let root = temp_root("none");
        enable_once(&root).unwrap();
        assert_eq!(read(&root)["registro"]["rede"], json!(true));
        restore(&root).unwrap();
        assert!(read(&root).get("registro").is_none());
    }
}
