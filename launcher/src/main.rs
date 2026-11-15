/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

//! SteinClient.exe: accounts, playing and mods. Downloading and installing live in `stein-core`; this crate only
//! holds the commands the UI calls and the events it listens to (`progress`, `game`, `self-update`).
//!
//! On startup the launcher updates itself (see [`update`]).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod update;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use stein_core::auth::{self, AuthError};
use stein_core::launch::Session;
use stein_core::paths::Root;
use stein_core::progress::{Progress, Report};
use stein_core::settings::Settings;
use tauri::{AppHandle, Emitter, Manager, State};

struct App {
    root: Root,
    /// The sign-in in progress (code already shown), and its cancel switch.
    pending: Mutex<Option<auth::Pending>>,
    cancel: Mutex<Option<Arc<AtomicBool>>>,
    running: AtomicBool,
    busy: AtomicBool,
    /// The file describing the last crash, for the "Open log" button.
    crash_log: Mutex<Option<std::path::PathBuf>>,
}

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModView {
    id: String,
    name: String,
    version: String,
    description: String,
    enabled: bool,
    /// The version in the mods folder (none: it is downloaded when the player clicks Play).
    installed_version: Option<String>,
    /// Third-party mod, plus the name and link of its source.
    third_party: bool,
    source_name: String,
    source: String,
    /// The version the next Play installs is a test one.
    test: bool,
    /// The next Play installs an older version than the one in the folder (test versions turned off).
    older: bool,
    /// No version works with the chosen Stein Loader: the Loader the mod asks for (the file stays as it is).
    needs_loader: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StateView {
    version: String,
    /// There is an original account (the launcher only gets past the sign-in screen with one; it unlocks offline
    /// accounts).
    has_owner: bool,
    loader: Option<String>,
    /// The installed Stein Loader libraries: Stein Bridge and Stein Loader (the in-game one).
    bridge_version: Option<String>,
    loader_library: Option<String>,
    /// What the next "Play" installs, updates or removes.
    updates: Vec<stein_core::stein::Change>,
    ready: bool,
    portable: bool,
    dir: String,
    accounts: Vec<auth::Account>,
    current: Option<String>,
    mods: Vec<ModView>,
    close_on_play: bool,
    news: Option<stein_core::stein::News>,
    running: bool,
    /// Stein GFX enabled.
    gfx: bool,
    /// The graphics API chosen for Stein GFX: "opengl" or "vulkan".
    gfx_api: String,
    /// The third-party mod sources that were added.
    sources: Vec<stein_core::sources::Source>,
    /// A one-time notice (the one about the migration to third-party mods).
    notice: Option<String>,
    /// Developer options: the Stein log topics (as the game's Log tab has them) and the extra Java arguments.
    log_topics: Vec<LogTopic>,
    jvm_args: String,
    /// The project's addresses from the registry (Discord invite, player-head images); empty before the first index.
    links: stein_core::stein::Links,
    /// The pack the next "Play" installs, when it is a test one (its version).
    test_pack: Option<String>,
    /// What the next "Play" runs, for the summary under the button.
    run: RunView,
    /// "Versões de teste".
    test_versions: bool,
}

/// What the next "Play" runs: the Stein version, the Java, the renderer and the memory.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RunView {
    /// The Stein version (the pack) the next Play installs or keeps; none before the registry was read.
    stein: Option<String>,
    /// It is a test version.
    test: bool,
    /// The game's Java ("25.0.4.1"); none before it is installed.
    java: Option<String>,
    /// Stein GFX (the new renderer) or the fixed pipeline, its fallback.
    gfx: bool,
    /// "vulkan" or "opengl" (the fixed pipeline is always OpenGL).
    api: String,
    /// The game's memory, in GB.
    memory_gb: u32,
}

/// A Stein log topic and whether it is on.
#[derive(Serialize)]
struct LogTopic {
    key: String,
    on: bool,
}

/// A third-party source that was read, for the player to review before adding it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourcePreview {
    url: String,
    /// The domain it comes from (what the security warning shows).
    host: String,
    name: String,
    author: String,
    homepage: String,
    mods: Vec<ModView>,
}

#[tauri::command]
fn app_state(app: State<'_, App>) -> StateView {
    let root = &app.root;
    let settings = Settings::load(root);
    let store = stein_core::accounts(root);
    let accounts = store.accounts();
    let current = settings
        .account
        .clone()
        .filter(|u| accounts.iter().any(|a| &a.uuid == u))
        .or_else(|| accounts.first().map(|a| a.uuid.clone()));
    let index = stein_core::stein::cached_index(root);
    let files = stein_core::stein::mod_files(root);
    let plan = index.as_ref().map(|i| stein_core::versions::plan_for(root, i, &settings));
    let updates = plan.as_ref().map(|p| stein_core::stein::pending(root, &p.index, &settings.mods)).unwrap_or_default();
    let mods = match (&index, &plan) {
        (Some(i), Some(p)) => i.mods.iter().map(|m| mod_view(m, p, &settings.mods, &files)).collect(),
        _ => Vec::new(),
    };
    let test_pack = plan.as_ref().and_then(|p| p.pack.as_ref()).filter(|p| p.test).map(|p| p.version.clone());
    let gfx = stein_core::launch::gfx(&settings);
    let run = RunView {
        stein: plan
            .as_ref()
            .and_then(|p| p.pack.as_ref().map(|p| p.version.clone()))
            .or_else(|| index.as_ref().map(|i| i.loader.clone())),
        test: test_pack.is_some(),
        java: stein_core::stein::java_version(root),
        gfx,
        api: if gfx { settings.gfx_api.clone() } else { stein_core::graphics::OPENGL.to_string() },
        memory_gb: stein_core::launch::ram_gb(root),
    };
    let components = stein_core::stein::components(root);
    let links = index.as_ref().map(|i| i.links.clone()).unwrap_or_default();
    StateView {
        // No choice made on the toggle: on, unless this machine says otherwise.
        gfx: stein_core::launch::gfx(&settings),
        gfx_api: settings.gfx_api.clone(),
        version: stein_core::VERSION.to_string(),
        bridge_version: components.as_ref().map(|c| c.0.clone()),
        loader_library: components.map(|c| c.1),
        updates,
        has_owner: accounts.iter().any(|a| !a.offline),
        loader: stein_core::stein::installed(root).map(|i| i.label()),
        ready: stein_core::ready(root),
        portable: root.is_portable(),
        dir: root.dir().to_string_lossy().to_string(),
        accounts,
        current,
        mods,
        close_on_play: settings.close_on_play,
        news: index.and_then(|i| i.news),
        running: app.running.load(Ordering::Relaxed),
        sources: stein_core::sources::third_party(root),
        notice: settings.notice.clone(),
        log_topics: stein_core::steinlog::topics(root).into_iter().map(|(key, on)| LogTopic { key, on }).collect(),
        jvm_args: settings.jvm_args.clone(),
        links,
        run,
        test_pack,
        test_versions: settings.test_versions,
    }
}

/// A mod of the index as the Mods page shows it: at the version the plan chose.
fn mod_view(
    m: &stein_core::stein::IndexMod,
    plan: &stein_core::versions::Plan,
    enabled: &[String],
    files: &std::collections::HashMap<String, String>,
) -> ModView {
    let chosen = plan.index.mods.iter().find(|c| c.id == m.id);
    let version = chosen.map(|c| c.version.clone()).unwrap_or_else(|| m.version.clone());
    let installed_version = files.get(&m.mod_id).cloned();
    let older = installed_version
        .as_deref()
        .is_some_and(|i| stein_core::versions::compare(i, &version) == std::cmp::Ordering::Greater);
    ModView {
        id: m.id.clone(),
        name: m.name.clone(),
        test: chosen.is_some_and(|c| c.test),
        older,
        version,
        description: m.description.clone(),
        enabled: enabled.contains(&m.id),
        installed_version,
        third_party: m.third_party,
        source_name: m.source_name.clone(),
        source: m.source.clone(),
        needs_loader: plan.blocked.iter().find(|b| b.id == m.id).map(|b| b.needs_loader.clone()),
    }
}

fn reporter(handle: &AppHandle) -> Report {
    let h = handle.clone();
    Arc::new(move |p: Progress| {
        let _ = h.emit("progress", p);
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CodeView {
    user_code: String,
    verification_uri: String,
    expires_in: u64,
}

#[tauri::command]
async fn login_begin(app: State<'_, App>) -> Res<CodeView> {
    let (code, pending) = auth::begin().await.map_err(err)?;
    *app.cancel.lock().unwrap() = Some(pending.canceller());
    *app.pending.lock().unwrap() = Some(pending);
    Ok(CodeView { user_code: code.user_code, verification_uri: code.verification_uri, expires_in: code.expires_in })
}

#[tauri::command]
async fn login_finish(app: State<'_, App>) -> Res<auth::Account> {
    let pending = app.pending.lock().unwrap().take().ok_or("nenhuma entrada em andamento")?;
    let store = stein_core::accounts(&app.root);
    let creds = pending.finish(&store).await.map_err(err)?;
    let mut settings = Settings::load(&app.root);
    settings.account = Some(creds.uuid.clone());
    settings.save(&app.root).map_err(err)?;
    Ok(store.accounts().into_iter().find(|a| a.uuid == creds.uuid).ok_or("conta nao encontrada")?)
}

/// Creates an offline account (only with an original account in the launcher) and switches to it.
#[tauri::command]
fn add_offline_account(app: State<'_, App>, name: String) -> Res<auth::Account> {
    let store = stein_core::accounts(&app.root);
    let account = store.add_offline(&name).map_err(err)?;
    let mut settings = Settings::load(&app.root);
    settings.account = Some(account.uuid.clone());
    settings.save(&app.root).map_err(err)?;
    Ok(account)
}

#[tauri::command]
fn login_cancel(app: State<'_, App>) {
    if let Some(c) = app.cancel.lock().unwrap().take() {
        c.store(true, Ordering::Relaxed);
    }
}

#[tauri::command]
fn select_account(app: State<'_, App>, uuid: String) -> Res<()> {
    let mut s = Settings::load(&app.root);
    s.account = Some(uuid);
    s.save(&app.root).map_err(err)
}

#[tauri::command]
fn remove_account(app: State<'_, App>, uuid: String) -> Res<()> {
    stein_core::accounts(&app.root).remove(&uuid).map_err(err)?;
    let mut s = Settings::load(&app.root);
    if s.account.as_deref() == Some(uuid.as_str()) {
        s.account = None;
        s.save(&app.root).map_err(err)?;
    }
    Ok(())
}

/// Fetches the official mod list (and the news) and caches it: this is what lets the player pick mods before the
/// first install. Runs when the launcher opens; without internet, the cached list is kept.
#[tauri::command]
async fn load_index(app: State<'_, App>) -> Res<()> {
    let index = stein_core::stein::index(&stein_core::net::client(), &app.root).await.map_err(|e| format!("{e:#}"))?;
    stein_core::stein::remember_index(&app.root, &index);
    // The registry's migrations.json came with the index: a new migration runs now (its notice shows on refresh).
    let _ = stein_core::sources::migrate(&app.root);
    Ok(())
}

#[tauri::command]
fn set_mod(app: State<'_, App>, id: String, enabled: bool) -> Res<()> {
    let mut s = Settings::load(&app.root);
    s.mods.retain(|m| m != &id);
    if enabled {
        s.mods.push(id);
    }
    s.save(&app.root).map_err(err)
}

/// Reads the source the player pasted (the index link or its JSON) to show what it offers along with the security
/// warning. Nothing is saved yet.
#[tauri::command]
async fn inspect_source(app: State<'_, App>, text: String) -> Res<SourcePreview> {
    let root = &app.root;
    let url = stein_core::sources::url_from_paste(&text).map_err(err)?;
    let f = stein_core::sources::fetch(&stein_core::net::client(), &url, true).await.map_err(|e| format!("{e:#}"))?;
    let official = stein_core::stein::cached_index(root).map(|i| i.mods).unwrap_or_default();
    let mods: Vec<ModView> = f
        .mods
        .iter()
        .filter(|m| !official.iter().any(|o| !o.third_party && (o.mod_id == m.mod_id || o.id == m.id)))
        .map(|m| ModView {
            id: m.id.clone(),
            name: m.name.clone(),
            version: m.version.clone(),
            description: m.description.clone(),
            enabled: true,
            installed_version: None,
            third_party: true,
            source_name: m.source_name.clone(),
            source: m.source.clone(),
            test: false,
            older: false,
            needs_loader: None,
        })
        .collect();
    if mods.is_empty() {
        return Err("Essa fonte não traz nenhum mod que dê para instalar.".into());
    }
    let host = url.split("://").nth(1).and_then(|r| r.split('/').next()).unwrap_or("").to_string();
    Ok(SourcePreview { url: f.url, host, name: f.name, author: f.author, homepage: f.homepage, mods })
}

/// Adds the third-party source (after the warning) with its mods enabled, and reloads the mod list.
#[tauri::command]
async fn add_source(app: State<'_, App>, url: String) -> Res<()> {
    let root = &app.root;
    let http = stein_core::net::client();
    let f = stein_core::sources::fetch(&http, &url, true).await.map_err(|e| format!("{e:#}"))?;
    stein_core::sources::add(root, &f).map_err(err)?;
    if let Ok(index) = stein_core::stein::index(&http, root).await {
        stein_core::stein::remember_index(root, &index);
    }
    Ok(())
}

/// Removes the third-party source and its mod files (their settings stay).
#[tauri::command]
fn remove_source(app: State<'_, App>, url: String) -> Res<()> {
    stein_core::sources::remove(&app.root, &url).map_err(err)
}

/// "Versões de teste": the next Play installs the newest test versions (on) or goes back to the stable ones (off).
#[tauri::command]
fn set_test_versions(app: State<'_, App>, value: bool) -> Res<()> {
    let mut s = Settings::load(&app.root);
    s.test_versions = value;
    s.save(&app.root).map_err(err)
}

/// One release in the launcher's "Novidades" page: a version of the pack (`id` "pack") or of a mod, with its notes.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChangeView {
    id: String,
    name: String,
    version: String,
    date: String,
    /// The version the next Play installs (or keeps).
    in_use: bool,
    /// A mod the player added (an extra or another author's), not one that comes with the Stein: its own tab.
    third_party: bool,
    /// The notes in Markdown (pt_BR, else en_US).
    notes: String,
}

/// Every stable release the launcher knows with notes, newest first: the packs, each mod of the list and the extras not
/// added. Test versions never show.
#[tauri::command]
fn changelog(app: State<'_, App>) -> Vec<ChangeView> {
    let root = &app.root;
    let Some(index) = stein_core::stein::cached_index(root) else { return Vec::new() };
    let settings = Settings::load(root);
    let plan = stein_core::versions::plan_for(root, &index, &settings);
    let pack_in_use = plan.pack.as_ref().map(|p| p.version.clone());
    let mut out = Vec::new();
    for p in &index.packs {
        let in_use = pack_in_use.as_deref() == Some(p.version.as_str());
        // Test versions (release candidates) never show here: only what came out for everyone.
        if p.test {
            continue;
        }
        if let Some(notes) = stein_core::versions::notes(&p.changelog) {
            let (id, name) = ("pack".to_string(), "Stein".to_string());
            out.push(ChangeView {
                id,
                name,
                version: p.version.clone(),
                date: p.date.clone(),
                in_use,
                third_party: false,
                notes,
            });
        }
    }
    // The mods in the list, then the official extras not added (their notes only, none in use).
    for m in index.mods.iter().chain(&index.extras) {
        let chosen = plan.index.mods.iter().find(|c| c.id == m.id).map(|c| c.version.clone());
        for v in stein_core::versions::versions_of(m) {
            let in_use = chosen.as_deref() == Some(v.version.as_str());
            if v.test {
                continue;
            }
            if let Some(notes) = stein_core::versions::notes(&v.changelog) {
                let (id, name, third_party) = (m.id.clone(), m.name.clone(), m.third_party);
                let (version, date) = (v.version, v.date);
                out.push(ChangeView { id, name, version, date, in_use, third_party, notes });
            }
        }
    }
    // Newest first; on the same day the pack comes before the mods, then by name and higher versions first.
    out.sort_by(|a, b| {
        b.date
            .cmp(&a.date)
            .then_with(|| (b.id == "pack").cmp(&(a.id == "pack")))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| stein_core::versions::compare(&b.version, &a.version))
    });
    out
}

#[tauri::command]
fn dismiss_notice(app: State<'_, App>) -> Res<()> {
    let mut s = Settings::load(&app.root);
    s.notice = None;
    s.save(&app.root).map_err(err)
}

#[tauri::command]
fn set_close_on_play(app: State<'_, App>, value: bool) -> Res<()> {
    let mut s = Settings::load(&app.root);
    s.close_on_play = value;
    s.save(&app.root).map_err(err)
}

/// Stein GFX, the settings toggle: the launcher starts the game with `-Dsteinbridge.gfx`.
#[tauri::command]
fn set_gfx(app: State<'_, App>, value: bool) -> Res<()> {
    let mut s = Settings::load(&app.root);
    s.gfx = Some(value);
    if value {
        // Turning it back on retries it on this machine.
        s.gfx_unsupported = false;
    }
    s.save(&app.root).map_err(err)
}

/// The graphics API Stein GFX draws with (the settings choice): written for the game before each start, see
/// [`stein_core::graphics`].
#[tauri::command]
fn set_gfx_api(app: State<'_, App>, value: String) -> Res<()> {
    if value != stein_core::graphics::OPENGL && value != stein_core::graphics::VULKAN {
        return Err(format!("unknown graphics API: {value}"));
    }
    let mut s = Settings::load(&app.root);
    s.gfx_api = value;
    s.save(&app.root).map_err(err)
}

/// Developer options: a Stein log topic, in the same file the game's Log tab uses. Not while the game is open: the
/// game holds the choice then and writes it back when it changes.
#[tauri::command]
fn set_log_topic(app: State<'_, App>, key: String, value: bool) -> Res<()> {
    if app.running.load(Ordering::Relaxed) {
        return Err("Feche o jogo para mudar os logs por aqui, ou mude no menu do Stein Loader dentro do jogo.".into());
    }
    stein_core::steinlog::set_topic(&app.root, &key, value).map_err(err)
}

/// Developer options: the extra Java arguments, from the next start on. Arguments outside the allowed list
/// ([`stein_core::launch::registry_arg_allowed`]) are refused here, and dropped at start anyway.
#[tauri::command]
fn set_jvm_args(app: State<'_, App>, value: String) -> Res<()> {
    let rejected = stein_core::launch::rejected_dev_args(&value);
    if !rejected.is_empty() {
        return Err(format!(
            "Argumentos não permitidos: {}. Só valem -Xmx, -Xms, opções -XX: de memória e coleta de lixo, \
             --enable-native-access= e propriedades -D do Stein (stein., steinbridge., steinloader.).",
            rejected.join(" ")
        ));
    }
    let mut s = Settings::load(&app.root);
    s.jvm_args = value.split_whitespace().collect::<Vec<_>>().join(" ");
    s.save(&app.root).map_err(err)
}

/// Prepares (fast when already up to date), signs in with the account without a code and starts the game. The
/// launcher hides (the default) or minimises and keeps watching the game: if it exits normally, the launcher exits
/// too (or comes back, if it was only minimised); if the game crashes, the launcher comes back and shows what to do.
/// `debug`: with Stein logging enabled (offered after a crash).
#[tauri::command]
async fn play(
    handle: AppHandle,
    app: State<'_, App>,
    debug: Option<bool>,
    anyway: Option<bool>,
) -> Res<Vec<stein_core::modcheck::Warning>> {
    if app.running.load(Ordering::Relaxed) {
        return Err("O jogo já está aberto.".into());
    }
    if app.busy.swap(true, Ordering::SeqCst) {
        return Err("O jogo já está sendo preparado.".into());
    }
    let result = play_inner(&handle, &app, debug.unwrap_or(false), anyway.unwrap_or(false)).await;
    app.busy.store(false, Ordering::SeqCst);
    result
}

/// Prepares and opens the game. Mods the installed Loader may not run come back as warnings instead, with the game
/// closed, unless the player already chose to play with those same ones (`anyway`, or an earlier choice): it never
/// blocks, the player decides.
async fn play_inner(
    handle: &AppHandle,
    app: &App,
    debug: bool,
    anyway: bool,
) -> Res<Vec<stein_core::modcheck::Warning>> {
    let root = app.root.clone();
    let settings = Settings::load(&root);
    let store = stein_core::accounts(&root);
    let uuid = settings
        .account
        .clone()
        .or_else(|| store.accounts().first().map(|a| a.uuid.clone()))
        .ok_or("Entre com uma conta Microsoft primeiro.")?;
    let account =
        store.accounts().into_iter().find(|a| a.uuid == uuid).ok_or("Entre com uma conta Microsoft primeiro.")?;
    let report = reporter(handle);
    stein_core::prepare(&root, &report).await.map_err(|e| stein_core::friendly::logged(&root, "launcher.log", &e))?;
    report(Progress::new("Conferindo os mods"));
    if let (Some(loader), warnings) = stein_core::modcheck::check(&root) {
        if !warnings.is_empty() {
            let sig = stein_core::modcheck::signature(&loader, &warnings);
            if anyway {
                let mut s = Settings::load(&root);
                s.accepted_warnings = Some(sig);
                s.save(&root).map_err(err)?;
            } else if settings.accepted_warnings.as_deref() != Some(sig.as_str()) {
                for w in &warnings {
                    let line = format!(
                        "{} {} with Stein Loader {loader}: missing {:?}; known break: {:?}",
                        w.mod_id, w.version, w.missing, w.reason
                    );
                    stein_core::registry::note(&root, "launcher.log", &line);
                }
                return Ok(warnings);
            }
        }
    }
    let session = if account.offline {
        // An offline account only launches with an original account confirmed by Microsoft (or, without internet,
        // confirmed recently).
        report(Progress::new("Confirmando a conta original"));
        auth::verify_owner(&store).await.map_err(err)?;
        Session { name: account.name, uuid: account.uuid, access_token: "0".into(), offline: true }
    } else {
        report(Progress::new("Entrando com a conta"));
        let creds = match auth::refresh(&store, &uuid).await {
            Ok(c) => c,
            Err(AuthError::NeedsLogin) => return Err("login".into()),
            Err(e) => return Err(e.to_string()),
        };
        Session { name: creds.username, uuid: creds.uuid, access_token: creds.access_token, offline: false }
    };
    report(Progress::new("Abrindo o jogo"));
    let started = std::time::SystemTime::now();
    let mut child = if debug { stein_core::launch_debug(&root, &session) } else { stein_core::launch(&root, &session) }
        .map_err(|e| stein_core::friendly::logged(&root, "launcher.log", &e))?;
    app.running.store(true, Ordering::Relaxed);
    let _ = handle.emit("game", "running");
    let hidden = settings.close_on_play;
    if let Some(w) = handle.get_webview_window("main") {
        let _ = if hidden { w.hide() } else { w.minimize() };
    }
    let h = handle.clone();
    let retry_root = root.clone();
    std::thread::spawn(move || {
        let mut status = child.wait();
        // GPU without OpenGL 4.1: the game exits on startup with Stein GFX. The launcher records that and relaunches
        // once without it, so the player never sees the error.
        let failed = status.as_ref().ok().and_then(|s| s.code()) != Some(0);
        // Vulkan that closed the game before it showed: back to OpenGL, with a notice, and the game opens again with
        // it right away, so the player only sees the game.
        let vulkan_failed = stein_core::graphics::after_exit(&retry_root, failed).unwrap_or(false);
        if vulkan_failed {
            let again = if debug {
                stein_core::launch_debug(&retry_root, &session)
            } else {
                stein_core::launch(&retry_root, &session)
            };
            if let Ok(mut c) = again {
                status = c.wait();
            }
        } else if failed && stein_core::launch::gfx_unsupported_crash(&retry_root) {
            let mut s = Settings::load(&retry_root);
            if !s.gfx_unsupported {
                s.gfx_unsupported = true;
                let _ = s.save(&retry_root);
                let again = if debug {
                    stein_core::launch_debug(&retry_root, &session)
                } else {
                    stein_core::launch(&retry_root, &session)
                };
                if let Ok(mut c) = again {
                    status = c.wait();
                }
            }
        }
        let app = h.state::<App>();
        app.running.store(false, Ordering::Relaxed);
        let code = status.ok().and_then(|s| s.code()).unwrap_or(-1);
        if code == 0 {
            if hidden {
                h.exit(0);
                return;
            }
            let _ = h.emit("game", "closed");
        } else {
            *app.crash_log.lock().unwrap() = Some(crash_log(&app.root, started));
            let _ = h.emit("game", format!("crashed:{code}"));
        }
        if let Some(w) = h.get_webview_window("main") {
            let _ = w.show();
            let _ = w.unminimize();
            let _ = w.set_focus();
        }
    });
    Ok(Vec::new())
}

#[tauri::command]
fn open_path(app: State<'_, App>, kind: String) -> Res<()> {
    let root = &app.root;
    let path = match kind.as_str() {
        "mods" => root.mods(),
        "logs" => root.dir().join("logs"),
        "steinlogs" => root.dir().join("logs").join("stein"),
        "screenshots" => root.dir().join("screenshots"),
        _ => root.dir().to_path_buf(),
    };
    let _ = std::fs::create_dir_all(&path);
    open::that_detached(path).map_err(err)
}

/// The file that explains the crash: the game's crash report (crash-reports/) written after launch, or the session
/// log.
fn crash_log(root: &Root, started: std::time::SystemTime) -> std::path::PathBuf {
    let newest = std::fs::read_dir(root.dir().join("crash-reports"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .filter(|(t, _)| *t >= started)
        .max_by_key(|(t, _)| *t);
    match newest {
        Some((_, p)) => p,
        None => root.dir().join("logs").join("latest.log"),
    }
}

/// Opens the log of the last crash (the game's crash report, or the session log).
#[tauri::command]
fn open_crash_log(app: State<'_, App>) -> Res<()> {
    let path = app.crash_log.lock().unwrap().clone().unwrap_or_else(|| app.root.dir().join("logs").join("latest.log"));
    if !path.exists() {
        return Err("O arquivo de log não foi encontrado.".into());
    }
    open::that_detached(path).map_err(err)
}

#[tauri::command]
fn open_url(url: String) -> Res<()> {
    if !url.starts_with("https://") {
        return Err("so https".into());
    }
    open::that_detached(url).map_err(err)
}

fn main() {
    let root = Root::detect();
    // One-off: mods that left the official list become third-party mods (stein_core::sources::migrate).
    let _ = stein_core::sources::migrate(&root);
    let hops = update::hops();
    tauri::Builder::default()
        .manage(App {
            root: root.clone(),
            pending: Mutex::new(None),
            cancel: Mutex::new(None),
            running: AtomicBool::new(false),
            busy: AtomicBool::new(false),
            crash_log: Mutex::new(None),
        })
        .setup(move |app| {
            if hops < update::MAX_UPDATE_HOPS {
                update::self_update(app.handle().clone(), root.clone(), hops);
            }
            if hops > 0 {
                // Just updated itself: the old executable (.old) can be deleted as soon as its process exits.
                tauri::async_runtime::spawn(async {
                    for _ in 0..5 {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        if let Ok(exe) = std::env::current_exe() {
                            stein_core::update::clean_old(&exe);
                        }
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_state,
            load_index,
            add_offline_account,
            open_crash_log,
            login_begin,
            login_finish,
            login_cancel,
            select_account,
            remove_account,
            set_mod,
            inspect_source,
            add_source,
            remove_source,
            dismiss_notice,
            set_test_versions,
            changelog,
            set_close_on_play,
            set_gfx,
            set_gfx_api,
            set_log_topic,
            set_jvm_args,
            play,
            open_path,
            open_url
        ])
        .run(tauri::generate_context!())
        .expect("the Stein Client failed to start");
}
