/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

/**
 * Browser preview outside Tauri (Vite in a regular browser): commands return sample data so the UI can be reviewed
 * without the app. None of this runs inside the app. Query flags (they combine, e.g. `?page=mods&notice`):
 *
 * - `?login`: the sign-in screen; `?code`: the sign-in screen with a device code already shown;
 * - `?page=mods|settings`: the page to open (Play by default);
 * - `?busy`: preparation in progress, with the progress bar;
 * - `?updates`: pending updates (the Loader, official mods and a third-party one);
 * - `?error`: the Play page showing an error, to check the "Details" toggle;
 * - `?crash`: the Play page after a crash;
 * - `?notice`: the one-time notice about a mod that became third-party;
 * - `?offline`: the offline-account form open, with the invalid-nickname error;
 * - `?gfx`: the Stein GFX toggle on.
 */
import type { AppState } from './api'

export const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
const q = new URLSearchParams(typeof location !== 'undefined' ? location.search : '')
export const previewPage = (q.get('page') as 'play' | 'mods' | 'settings' | null) ?? 'play'
export const previewBusy = q.has('busy')

const official = { thirdParty: false, sourceName: '', source: '' }
const extra = { thirdParty: true, sourceName: 'Mods do Fulano', source: 'https://github.com/fulano/mods/releases/latest/download/steinmods.json' }

const state: AppState = {
    version: '0.1.0',
    loader: '0.1.23',
    bridgeVersion: '0.1.12',
    loaderLibrary: '0.1.16',
    updates: q.has('updates')
        ? [
              { id: 'loader', name: 'Stein Loader', from: '0.1.23', to: '0.1.24', thirdParty: false },
              { id: 'hud', name: 'HUD', from: '0.1.7', to: '0.1.8', thirdParty: false },
              { id: 'connect', name: 'Connect', from: null, to: '0.1.6', thirdParty: false },
              { id: 'fulanomap', name: 'Mapa', from: '1.0.0', to: '1.1.0', thirdParty: true },
          ]
        : [],
    ready: true,
    portable: false,
    dir: 'C:\\Users\\voce\\AppData\\Roaming\\.steinclient',
    hasOwner: !(q.has('login') || q.has('code')),
    gfx: q.has('gfx'),
    accounts:
        q.has('login') || q.has('code')
            ? []
            : [
                  { uuid: '069a79f444e94726a5befca90e38aaf5', name: 'Notch', offline: false, verified: 0 },
                  { uuid: 'b50ad385829d3141a2167e7d7539ba7f', name: 'Teste_01', offline: true, verified: 0 },
              ],
    current: q.has('login') || q.has('code') ? null : '069a79f444e94726a5befca90e38aaf5',
    mods: [
        { id: 'hud', name: 'HUD', version: '0.1.11', description: 'HUD personalizável, minimapa e lista do Tab.', enabled: true, installedVersion: '0.1.10', ...official },
        { id: 'connect', name: 'Connect', version: '0.1.8', description: 'Contas, reconexão automática e proxy SOCKS5.', enabled: true, installedVersion: null, ...official },
        { id: 'keybind', name: 'Keybind', version: '0.1.1', description: 'Mensagens e comandos numa tecla.', enabled: false, installedVersion: '0.1.1', ...official },
        { id: 'fulanomap', name: 'Mapa', version: '1.1.0', description: 'Um mapa de exemplo de outro autor.', enabled: true, installedVersion: '1.0.0', ...extra },
    ],
    sources: [
        { url: extra.source, name: 'Mods do Fulano', author: 'Fulano' },
    ],
    notice: q.has('notice')
        ? 'Mapa agora é um mod de terceiros. Continua instalado e sendo atualizado: a fonte nova já foi adicionada em Mods de terceiros.'
        : null,
    closeOnPlay: true,
    news: { title: 'Junte-se ao Discord', text: 'Novas versões, suporte e dúvidas.', url: 'https://discord.gg/MEHE9b5bjj' },
    running: false,
}

export async function mockInvoke(cmd: string): Promise<unknown> {
    switch (cmd) {
        case 'app_state':
            return state
        case 'inspect_source':
            return {
                url: 'https://github.com/fulano/meu-mod/releases/latest/download/steinmods.json',
                host: 'github.com',
                name: 'Meu Mod',
                author: 'Fulano',
                homepage: 'https://github.com/fulano/meu-mod',
                mods: [{ id: 'meumod', name: 'Meu Mod', version: '1.0.0', description: 'Um mod de exemplo.', enabled: true, installedVersion: null, thirdParty: true, sourceName: 'Meu Mod', source: '' }],
            }
        case 'login_begin':
            return { userCode: 'K7Q2-M9XP', verificationUri: 'https://www.microsoft.com/link', expiresIn: 900 }
        case 'login_finish':
        case 'play':
        default:
            return undefined
    }
}

/** `?error`: the Play page already showing an error (to check the "Details" toggle). */
export const previewError = q.has('error')
    ? 'Sem conexão com os servidores de download. Verifique sua internet e tente novamente.\ncould not download https://resources.download.minecraft.net/3b/3b5d...: error sending request for url (https://resources.download.minecraft.net/3b/3b5d...)'
    : ''

/** `?crash`: the Play page after a crash. */
export const previewCrash = q.has('crash') ? -1 : null

/** `?offline`: the offline-account form open, showing the invalid-nickname error. */
export const previewOffline = q.has('offline')
export const previewNickError = q.has('offline') ? 'O nick deve ter de 3 a 16 caracteres, usando apenas letras, números e sublinhado (_).' : ''
