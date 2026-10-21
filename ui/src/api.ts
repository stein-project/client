/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

import { invoke as tauriInvoke } from '@tauri-apps/api/core'
import { listen as tauriListen, UnlistenFn } from '@tauri-apps/api/event'

import { inTauri, mockInvoke } from './preview'

const invoke = <T,>(cmd: string, args?: Record<string, unknown>): Promise<T> =>
    (inTauri ? tauriInvoke<T>(cmd, args) : (mockInvoke(cmd) as Promise<T>))
const listen = <T,>(event: string, f: (e: { payload: T }) => void): Promise<UnlistenFn> =>
    inTauri ? tauriListen<T>(event, f) : Promise.resolve(() => {})

/** Progress of a long-running step (mirrors stein-core's Progress). */
export type Progress = { step: string; done: number; total: number; detail: string }

export type Account = { uuid: string; name: string; offline: boolean; verified: number }
export type ModView = {
    id: string
    name: string
    version: string
    description: string
    enabled: boolean
    /** The version in the mods folder (null: not installed yet). */
    installedVersion: string | null
    /** Third-party mod (from a source the player added), and that source. */
    thirdParty: boolean
    sourceName: string
    source: string
}
/** Something the next Play installs (from is null), updates, or removes (to is null). */
export type Change = { id: string; name: string; from: string | null; to: string | null; thirdParty: boolean }
/** A third-party mod source the player added. */
export type Source = { url: string; name: string; author: string }
/** A source that has been read, shown for review before it is added. */
export type SourcePreview = { url: string; host: string; name: string; author: string; homepage: string; mods: ModView[] }
export type News = { title: string; text: string; url: string }

export type AppState = {
    version: string
    /** A genuine account exists; without one only the sign-in screen is shown (and offline accounts are locked). */
    hasOwner: boolean
    loader: string | null
    bridgeVersion: string | null
    loaderLibrary: string | null
    updates: Change[]
    ready: boolean
    portable: boolean
    dir: string
    accounts: Account[]
    current: string | null
    mods: ModView[]
    closeOnPlay: boolean
    news: News | null
    running: boolean
    /** Whether Stein GFX is enabled. */
    gfx: boolean
    /** Third-party mod sources. */
    sources: Source[]
    /** A one-time notice (e.g. a mod that moved to a third-party source). */
    notice: string | null
    /** Developer options: the Stein log topics (shared with the game's Log tab) and the extra Java arguments. */
    logTopics: LogTopic[]
    jvmArgs: string
    /** The project's addresses from the Stein registry; empty strings before the first index. */
    links: Links
}

/** The Discord invite and the player-head image service (`{uuid}` and `{size}` are filled in). */
export type Links = { discord: string; heads: string }

/** A Stein log topic, by the key the game stores it under. */
export type LogTopic = { key: string; on: boolean }

export type DeviceCode = { userCode: string; verificationUri: string; expiresIn: number }

export const api = {
    state: () => invoke<AppState>('app_state'),
    loadIndex: () => invoke<void>('load_index'),
    loginBegin: () => invoke<DeviceCode>('login_begin'),
    loginFinish: () => invoke<Account>('login_finish'),
    loginCancel: () => invoke<void>('login_cancel'),
    selectAccount: (uuid: string) => invoke<void>('select_account', { uuid }),
    addOfflineAccount: (name: string) => invoke<Account>('add_offline_account', { name }),
    removeAccount: (uuid: string) => invoke<void>('remove_account', { uuid }),
    setMod: (id: string, enabled: boolean) => invoke<void>('set_mod', { id, enabled }),
    inspectSource: (text: string) => invoke<SourcePreview>('inspect_source', { text }),
    addSource: (url: string) => invoke<void>('add_source', { url }),
    removeSource: (url: string) => invoke<void>('remove_source', { url }),
    dismissNotice: () => invoke<void>('dismiss_notice'),
    setCloseOnPlay: (value: boolean) => invoke<void>('set_close_on_play', { value }),
    setGfx: (value: boolean) => invoke<void>('set_gfx', { value }),
    setLogTopic: (key: string, value: boolean) => invoke<void>('set_log_topic', { key, value }),
    setJvmArgs: (value: string) => invoke<void>('set_jvm_args', { value }),
    play: (debug = false) => invoke<void>('play', { debug }),
    openCrashLog: () => invoke<void>('open_crash_log'),
    openPath: (kind: 'game' | 'mods' | 'logs' | 'steinlogs' | 'screenshots') => invoke<void>('open_path', { kind }),
    openUrl: (url: string) => invoke<void>('open_url', { url }),
}

export const onProgress = (f: (p: Progress) => void): Promise<UnlistenFn> =>
    listen<Progress>('progress', (e) => f(e.payload))

/** Payload is "running", "closed" or "crashed:<exit code>". */
export const onGame = (f: (s: string) => void): Promise<UnlistenFn> => listen<string>('game', (e) => f(e.payload))

/** The head image service when the registry has not said one yet. */
const DEFAULT_HEADS = 'https://mc-heads.net/avatar/{uuid}/{size}'
let heads = DEFAULT_HEADS

/** Uses the registry's head image service (https only, with `{uuid}`), or goes back to the built-in one. */
export const setHeads = (template: string | undefined) => {
    heads = template && template.startsWith('https://') && template.includes('{uuid}') ? template : DEFAULT_HEADS
}

/** The player's head rendered from their skin; offline, callers fall back to the name's initial. */
export const headUrl = (uuid: string, size = 64) =>
    heads.replace('{uuid}', encodeURIComponent(uuid)).replace('{size}', String(size))

/**
 * The launcher is about to update itself to the given version: it swaps its own executable and relaunches it. An
 * empty version means the update failed and the launcher carries on.
 */
export const onSelfUpdate = (f: (version: string) => void): Promise<UnlistenFn> =>
    listen<string>('self-update', (e) => f(e.payload))
