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
 * - `?page=mods|settings|changelog`: the page to open (Play by default);
 * - `?busy`: preparation in progress, with the progress bar;
 * - `?updates`: pending updates (the Loader, official mods and a third-party one);
 * - `?error`: the Play page showing an error, to check the "Details" toggle;
 * - `?crash`: the Play page after a crash;
 * - `?modwarn`: the Play page with the warning about mods the installed Loader may not run;
 * - `?notice`: the one-time notice about a mod that became third-party;
 * - `?offline`: the offline-account form open, with the invalid-nickname error;
 * - `?gfx`: the Stein GFX toggle on;
 * - `?opengl`: OpenGL chosen as the graphics API;
 * - `?dev`: the developer options open, with a log topic on and an extra Java argument;
 * - `?running`: the game open (the log topics locked in the launcher);
 * - `?tests`: test versions on, with a test pack and a test HUD;
 * - `?old`: test versions just turned off (Combat goes back to the stable version);
 * - `?fixed`: the fixed pipeline instead of Stein GFX.
 */
import type { AppState } from './api'

export const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
const q = new URLSearchParams(typeof location !== 'undefined' ? location.search : '')
export const previewPage = (q.get('page') as 'play' | 'mods' | 'settings' | 'changelog' | null) ?? 'play'
export const previewBusy = q.has('busy')

const official = { thirdParty: false, sourceName: '', source: '', test: false, older: false, needsLoader: null }
const extra = { thirdParty: true, sourceName: 'Mods do Fulano', source: 'https://github.com/fulano/mods/releases/latest/download/steinmods.json', test: false, older: false, needsLoader: null }
const tests = q.has('tests')
const old = q.has('old')
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
    gfxApi: q.has('opengl') ? 'opengl' : 'vulkan',
    accounts:
        q.has('login') || q.has('code')
            ? []
            : [
                  { uuid: '069a79f444e94726a5befca90e38aaf5', name: 'Notch', offline: false, verified: 0 },
                  { uuid: 'b50ad385829d3141a2167e7d7539ba7f', name: 'Teste_01', offline: true, verified: 0 },
              ],
    current: q.has('login') || q.has('code') ? null : '069a79f444e94726a5befca90e38aaf5',
    mods: [
        { id: 'hud', name: 'HUD', version: tests ? '0.1.18-rc.1' : '0.1.17', description: 'HUD personalizável, minimapa e lista do Tab.', enabled: true, installedVersion: '0.1.17', ...official, test: tests },
        { id: 'connect', name: 'Connect', version: '0.1.9', description: 'Contas, reconexão automática e proxy SOCKS5.', enabled: true, installedVersion: null, ...official },
        { id: 'keybind', name: 'Keybind', version: '0.1.3', description: 'Mensagens e comandos numa tecla.', enabled: false, installedVersion: '0.1.3', ...official },
        { id: 'fulanomap', name: 'Mapa', version: '1.1.0', description: 'Um mapa de exemplo de outro autor.', enabled: true, installedVersion: '1.0.0', ...extra },
        { id: 'steincombat', name: 'Combat', version: old ? '0.1.30' : '0.1.31', description: 'Módulos de PvP.', enabled: true, installedVersion: '0.1.31', ...extra, older: old },
    ],
    sources: [
        { url: extra.source, name: 'Mods do Fulano', author: 'Fulano' },
    ],
    notice: q.has('notice')
        ? 'Mapa agora é um mod de terceiros. Continua instalado e sendo atualizado: a fonte nova já foi adicionada em Mods de terceiros.'
        : null,
    closeOnPlay: true,
    news: { title: 'Junte-se ao Discord', text: 'Novas versões, suporte e dúvidas.', url: 'https://discord.gg/MEHE9b5bjj' },
    running: q.has('running'),
    logTopics: [
        { key: 'rede', on: false },
        { key: 'desempenho', on: false },
        { key: 'render', on: q.has('dev') },
    ],
    jvmArgs: q.has('dev') ? '-Dsteinbridge.gfx.gpuHeldItem=false' : '',
    links: { discord: 'https://discord.gg/MEHE9b5bjj', heads: 'https://mc-heads.net/avatar/{uuid}/{size}' },
    testPack: tests ? '0.1.45-rc.1' : null,
    testVersions: tests,
    run: {
        stein: tests ? '0.1.45-rc.1' : '0.1.44',
        test: tests,
        java: '25.0.4.1',
        gfx: !q.has('fixed'),
        api: q.has('opengl') || q.has('fixed') ? 'opengl' : 'vulkan',
        memoryGb: 4,
    },
}

/** `?dev`: the Settings page with the developer options open. */
export const previewDev = q.has('dev')

const PACK_NOTES = `# Stein Loader 0.1.44: as opções de vídeo do Stein

As opções de vídeo do jogo deram lugar a uma tela nova do Stein, com cada opção explicada no seu idioma, opções de imagem novas, a escolha entre Vulkan e OpenGL dentro do jogo e céus personalizados dos pacotes de textura.

- **Tela de vídeo nova**: as opções ficam separadas por assunto (Geral, Imagem, Tela e HDR, Mundo, Texturas, Câmera e jogadores, Animações 1.7, Desempenho), cada uma com uma dica do que faz, inclusive as do próprio jogo. Os controles deslizantes aplicam quando você solta, e dá para ver a mudança na sua visão ou numa paisagem.
- **Opções de imagem**: suavização de bordas (as texturas continuam nítidas), nitidez, filtros de textura e anisotropia.
- **Resolução**: desenhe o mundo de 30% a 200% do tamanho da tela. Abaixo de 100% ganha FPS em placas fracas; acima, a imagem fica mais lisa.
- **HDR** (Vulkan, em monitores HDR), com calibração do brilho.
- **Vulkan ou OpenGL dentro do jogo**: a escolha fica na tela de vídeo e o Stein Client passa a respeitá-la.
- **Céu personalizado**: pacotes de textura que trazem céu próprio agora mostram esse céu.
- **Tremor ao tomar dano**: agora é uma opção.
- **Mais FPS com muitas entidades**: em lugares com muitos animais ou mobs, de 14% a 18% mais FPS, sem mudar a imagem.
- **Nomes**: um nome vazio sobre a cabeça não deixa mais uma caixinha preta.
- **Menus do Loader com o mesmo visual** da tela de vídeo nova.
- **Componentes**: Stein Bridge 0.1.29 e Stein Loader 0.1.32.

Para atualizar, abra o Stein Client ou o instalador novamente. Mods, opções e mundos continuam onde estão.`

const HUD_NOTES = `# HUD 0.1.17: minimapa e alvo em mods próprios

O HUD fica só com o que serve em qualquer servidor. O minimapa virou um mod separado, e o alvo foi para o Combat.

- **Minimapa à parte**: para continuar usando, adicione o mod Minimap no Stein Client.
- **Alvo no Combat**: o elemento do alvo agora vem com o Combat, no mesmo lugar da tela e com as mesmas opções.`

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
                mods: [{ id: 'meumod', name: 'Meu Mod', version: '1.0.0', description: 'Um mod de exemplo.', enabled: true, installedVersion: null, thirdParty: true, sourceName: 'Meu Mod', source: '', test: false, older: false, needsLoader: null }],
            }
        case 'changelog':
            return [
                { id: 'pack', name: 'Stein', version: '0.1.44', date: '2026-11-14', inUse: true, thirdParty: false, notes: PACK_NOTES },
                { id: 'hud', name: 'HUD', version: '0.1.17', date: '2026-11-03', inUse: !tests, thirdParty: false, notes: HUD_NOTES },
                { id: 'steincombat', name: 'Combat', version: '0.1.30', date: '2026-11-03', inUse: true, thirdParty: true, notes: '# Combat 0.1.30: só o clã é escudo\n\nAliados do clã sempre visíveis.\n\n- **Fantasmas** de mod a 8%.' },
                { id: 'pack', name: 'Stein', version: '0.1.43', date: '2026-11-03', inUse: false, thirdParty: false, notes: '# Stein Loader 0.1.43: clã no Combat\n\nAjustes para o Combat.\n\n- **Componentes**: Stein Bridge 0.1.28 e Stein Loader 0.1.31.' },
            ]
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

/** `?modwarn`: mods the installed Loader may not run. */
export const previewWarnings = q.has('modwarn')
    ? [
          {
              modId: 'steincombat',
              name: 'Combat',
              version: '0.1.31',
              missing: ['Entities.itemStack(Object)', 'Game.playSound(String, float)', 'Gameplay.stopSprinting()'],
              reason: null,
          },
          {
              modId: 'steinhud',
              name: 'HUD',
              version: '0.1.13',
              missing: ['Mesh', 'Mesh.Builder', 'Mesh.Format', 'Mesh.Primitive', 'Render.mesh(Mesh)'],
              reason: 'A lista do Tab fica vazia com este Loader.',
          },
      ]
    : []

/** `?crash`: the Play page after a crash. */
export const previewCrash = q.has('crash') ? -1 : null

/** `?offline`: the offline-account form open, showing the invalid-nickname error. */
export const previewOffline = q.has('offline')
export const previewNickError = q.has('offline') ? 'O nick deve ter de 3 a 16 caracteres, usando apenas letras, números e sublinhado (_).' : ''
