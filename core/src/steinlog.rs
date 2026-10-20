/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

//! The Stein logs (logs/stein/: one file per topic — network, performance, item rendering). The Loader stores which
//! topics are on in config/steinloader.json (the `registro` key), and that file is the one place the choice lives: the
//! game's Log tab and the launcher's developer options both read and write it, so they always agree.
//!
//! The "restart with Stein logs" after a crash turns every topic on for a single session: the launcher saves what was
//! there before and restores it the next time the game starts normally, so the player is not left with logging on
//! forever without having asked for it.

use anyhow::Result;
use serde_json::{json, Map, Value};

use crate::paths::Root;

/// The topics, by the key the Loader stores them under (the Bridge's `SteinLog.Topic`).
pub const TOPICS: &[&str] = &["rede", "desempenho", "render"];

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
    map.insert("registro".into(), Value::Object(TOPICS.iter().map(|t| (t.to_string(), json!(true))).collect()));
    write(root, map)
}

/// Which topics are on, in [`TOPICS`] order (a missing one is off, as in the game).
pub fn topics(root: &Root) -> Vec<(String, bool)> {
    let map = read(root);
    let on = map.get("registro").and_then(Value::as_object);
    TOPICS
        .iter()
        .map(|t| (t.to_string(), on.and_then(|o| o.get(*t)).and_then(Value::as_bool).unwrap_or(false)))
        .collect()
}

/// Turns a topic on or off, as the game's Log tab does. A choice made here is the player's own: a pending restore from
/// a "restart with Stein logs" is dropped, or the next normal start would undo it.
pub fn set_topic(root: &Root, topic: &str, on: bool) -> Result<()> {
    anyhow::ensure!(TOPICS.contains(&topic), "unknown log topic: {topic}");
    let mut map = read(root);
    let mut registro = map.get("registro").and_then(Value::as_object).cloned().unwrap_or_default();
    registro.insert(topic.to_string(), json!(on));
    map.insert("registro".into(), Value::Object(registro));
    write(root, map)?;
    let _ = std::fs::remove_file(saved(root));
    Ok(())
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
        assert_eq!(on["registro"], json!({ "rede": true, "desempenho": true, "render": true }));
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

    #[test]
    fn topics_read_and_set_like_the_game() {
        let root = temp_root("topics");
        assert!(topics(&root).iter().all(|(_, on)| !on));
        std::fs::create_dir_all(root.dir().join("config")).unwrap();
        std::fs::write(config(&root), r#"{"disabled":["replay"],"registro":{"rede":true}}"#).unwrap();
        set_topic(&root, "render", true).unwrap();
        let t = topics(&root);
        assert_eq!(t, vec![("rede".into(), true), ("desempenho".into(), false), ("render".into(), true)]);
        assert_eq!(read(&root)["disabled"], json!(["replay"]));
        assert!(set_topic(&root, "nope", true).is_err());
    }

    #[test]
    fn a_choice_drops_the_pending_restore() {
        let root = temp_root("choice");
        enable_once(&root).unwrap();
        set_topic(&root, "rede", false).unwrap();
        restore(&root).unwrap(); // nothing saved any more: the choice stays
        assert_eq!(read(&root)["registro"]["rede"], json!(false));
        assert_eq!(read(&root)["registro"]["render"], json!(true));
    }
}
