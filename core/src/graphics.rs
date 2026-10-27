/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

//! The graphics API Stein GFX draws with (OpenGL or Vulkan), chosen in the launcher's settings. The game creates its
//! window for one API, so the choice is made before it opens: the launcher writes it to
//! `config/stein-graphics.properties` before each start, and the Stein Bridge reads it there (its video options only
//! show it).
//!
//! Vulkan is the default; OpenGL is only for machines where Vulkan cannot run. When a start with it dies before
//! showing the game, the Bridge leaves a mark in the same file: `vulkanStarting` still set (the game closed on the
//! way), or `vulkanCrashed` (the next start went back to OpenGL because of it). The launcher then switches its choice
//! to OpenGL, opens the game again with it and tells the player.

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::paths::Root;
use crate::settings::Settings;

pub const OPENGL: &str = "opengl";
pub const VULKAN: &str = "vulkan";

/// The key of the API in the file, and the Bridge's marks (`SteinBackend`).
const KEY: &str = "api";
const STARTING: &str = "vulkanStarting";
const CRASHED: &str = "vulkanCrashed";
/// Vulkan cannot run on this machine (no driver, a GPU or driver too old): the Bridge ran OpenGL and says why.
const UNSUPPORTED: &str = "vulkanUnsupported";

/// What the player reads in the launcher after Vulkan closed the game.
pub const CRASH_NOTICE: &str = "O jogo fechou ao abrir com o Vulkan neste computador, então o Stein Client abriu com \
                                o OpenGL. Você pode escolher o Vulkan de novo em Ajustes.";

fn file(root: &Root) -> PathBuf {
    root.dir().join("config").join("stein-graphics.properties")
}

/// The file's `key=value` lines, in order (comments and blank lines dropped).
fn read(root: &Root) -> Vec<(String, String)> {
    std::fs::read_to_string(file(root)).map(|text| parse(&text)).unwrap_or_default()
}

fn parse(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with('!'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

fn get<'a>(props: &'a [(String, String)], key: &str) -> Option<&'a str> {
    props.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn write(root: &Root, props: &[(String, String)]) -> Result<()> {
    let path = file(root);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut text = String::from("#Stein graphics API, written by the Stein Client before each start\n");
    for (k, v) in props {
        text.push_str(&format!("{k}={v}\n"));
    }
    std::fs::write(&path, text).with_context(|| format!("could not write {}", path.display()))
}

/// The API the game starts with: Vulkan only when chosen and Stein GFX is on (without it the game draws with its own
/// OpenGL pipeline).
pub fn api(settings: &Settings) -> &'static str {
    if settings.gfx_api == VULKAN && crate::launch::gfx(settings) {
        VULKAN
    } else {
        OPENGL
    }
}

/// What the player reads in the launcher after the game found that Vulkan cannot run on this computer.
pub const UNSUPPORTED_NOTICE: &str =
    "O Vulkan não roda neste computador (a placa de vídeo ou o driver não têm o que o \
                                      Stein precisa), então o Stein Client passou a abrir o jogo com o OpenGL. Se \
                                      você atualizar o driver de vídeo, pode escolher o Vulkan de novo em Ajustes.";

/// Switches the choice back to OpenGL, with the notice, when Vulkan was chosen. True if it switched.
fn fall_back(root: &Root, settings: &mut Settings, notice: &str) -> Result<bool> {
    if settings.gfx_api != VULKAN {
        return Ok(false);
    }
    settings.gfx_api = OPENGL.to_string();
    settings.notice = Some(notice.to_string());
    settings.save(root)?;
    Ok(true)
}

/// Before a start: the Bridge's mark of a start that went back to OpenGL switches the choice too, then the API is
/// written for the game. The other keys stay (a `vulkanStarting` still there is the Bridge's to handle).
pub fn apply(root: &Root, settings: &mut Settings) -> Result<()> {
    let mut props = read(root);
    if get(&props, CRASHED) == Some("true") {
        fall_back(root, settings, CRASH_NOTICE)?;
        props.retain(|(k, _)| k != CRASHED);
    }
    if get(&props, UNSUPPORTED).is_some() {
        fall_back(root, settings, UNSUPPORTED_NOTICE)?;
        props.retain(|(k, _)| k != UNSUPPORTED);
    }
    let api = api(settings);
    match props.iter_mut().find(|(k, _)| k == KEY) {
        Some(p) => p.1 = api.to_string(),
        None => props.insert(0, (KEY.to_string(), api.to_string())),
    }
    write(root, &props)
}

/// After the game exits: a start with Vulkan that never got through its first seconds (the mark is still there).
/// When the game failed, it was Vulkan that closed it: the choice goes back to OpenGL, with the notice. When it
/// closed normally (the player quit right away), the mark only goes. True if it switched.
pub fn after_exit(root: &Root, failed: bool) -> Result<bool> {
    let mut props = read(root);
    if get(&props, UNSUPPORTED).is_some() {
        // the game ran with OpenGL because Vulkan cannot run here: the choice follows now, so the settings show it
        let mut settings = Settings::load(root);
        fall_back(root, &mut settings, UNSUPPORTED_NOTICE)?;
        props.retain(|(k, _)| k != UNSUPPORTED);
        write(root, &props)?;
        props = read(root);
    }
    if get(&props, STARTING) != Some("true") {
        return Ok(false);
    }
    props.retain(|(k, _)| k != STARTING);
    let mut switched = false;
    if failed {
        let mut settings = Settings::load(root);
        switched = fall_back(root, &mut settings, CRASH_NOTICE)?;
        if let Some(p) = props.iter_mut().find(|(k, _)| k == KEY) {
            p.1 = OPENGL.to_string();
        }
    }
    write(root, &props)?;
    Ok(switched)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An empty game folder of its own for each test.
    fn root(name: &str) -> Root {
        let dir = std::env::temp_dir().join(format!("stein-graphics-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = Root::at(&dir);
        std::fs::create_dir_all(root.dir().join("config")).unwrap();
        std::fs::create_dir_all(root.own()).unwrap();
        root
    }

    fn vulkan(root: &Root) -> Settings {
        let mut s = Settings::default();
        s.gfx_api = VULKAN.to_string();
        s.save(root).unwrap();
        s
    }

    #[test]
    fn parses_java_properties() {
        let p = parse("#Stein graphics API\n#Sat Oct 24 17:03:01 GMT-03:00 2026\napi=vulkan\nvulkanStarting=true\n");
        assert_eq!(get(&p, KEY), Some("vulkan"));
        assert_eq!(get(&p, STARTING), Some("true"));
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn writes_the_choice_and_keeps_the_mark() {
        let root = root("keep");
        std::fs::write(file(&root), "api=opengl\nvulkanStarting=true\n").unwrap();
        let mut s = vulkan(&root);
        apply(&root, &mut s).unwrap();
        let p = read(&root);
        assert_eq!(get(&p, KEY), Some(VULKAN));
        assert_eq!(get(&p, STARTING), Some("true"));
    }

    #[test]
    fn vulkan_unsupported_switches_to_opengl() {
        let root = root("unsupported");
        vulkan(&root);
        std::fs::write(file(&root), "api=opengl\nvulkanUnsupported=old_driver\n").unwrap();
        // after the game closed: the choice follows at once, without a relaunch
        assert!(!after_exit(&root, false).unwrap());
        let s = Settings::load(&root);
        assert_eq!(s.gfx_api, OPENGL);
        assert_eq!(s.notice.as_deref(), Some(UNSUPPORTED_NOTICE));
        assert_eq!(get(&read(&root), UNSUPPORTED), None);
        // and if the launcher missed it, the next start's apply does the same
        let mut s = vulkan(&root);
        std::fs::write(file(&root), "api=opengl\nvulkanUnsupported=no_driver\n").unwrap();
        apply(&root, &mut s).unwrap();
        assert_eq!(s.gfx_api, OPENGL);
        assert_eq!(get(&read(&root), KEY), Some(OPENGL));
    }

    #[test]
    fn vulkan_by_default() {
        let root = root("default");
        let mut s = Settings::default();
        apply(&root, &mut s).unwrap();
        assert_eq!(get(&read(&root), KEY), Some(VULKAN));
    }

    #[test]
    fn without_stein_gfx_it_is_opengl() {
        let root = root("nogfx");
        let mut s = vulkan(&root);
        s.gfx = Some(false);
        apply(&root, &mut s).unwrap();
        assert_eq!(get(&read(&root), KEY), Some(OPENGL));
    }

    #[test]
    fn the_bridge_fallback_switches_the_choice() {
        let root = root("bridge");
        std::fs::write(file(&root), "api=opengl\nvulkanCrashed=true\n").unwrap();
        let mut s = vulkan(&root);
        apply(&root, &mut s).unwrap();
        assert_eq!(s.gfx_api, OPENGL);
        assert_eq!(Settings::load(&root).notice.as_deref(), Some(CRASH_NOTICE));
        let p = read(&root);
        assert_eq!(get(&p, KEY), Some(OPENGL));
        assert_eq!(get(&p, CRASHED), None);
    }

    #[test]
    fn a_crash_on_the_way_switches_a_quick_quit_does_not() {
        let root = root("exit");
        std::fs::create_dir_all(root.dir().join("config")).unwrap();
        vulkan(&root);
        std::fs::write(file(&root), "api=vulkan\nvulkanStarting=true\n").unwrap();
        assert!(!after_exit(&root, false).unwrap());
        assert_eq!(Settings::load(&root).gfx_api, VULKAN);
        assert_eq!(get(&read(&root), STARTING), None);

        std::fs::write(file(&root), "api=vulkan\nvulkanStarting=true\n").unwrap();
        assert!(after_exit(&root, true).unwrap());
        assert_eq!(Settings::load(&root).gfx_api, OPENGL);
        let p = read(&root);
        assert_eq!(get(&p, KEY), Some(OPENGL));
        assert_eq!(get(&p, STARTING), None);
    }
}
