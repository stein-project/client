/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

import { useState } from 'react'

import { api, AppState, GfxApi } from '../api'
import { ErrorText } from '../components/ErrorText'
import { SectionHead } from '../components/SectionHead'
import { Icon } from '../ds/Icon'
import { previewDev } from '../preview'

/** The Settings page: game options, test versions, the About section and the developer options. */
export function SettingsPage({ state, refresh }: { state: AppState; refresh: () => void }) {
    return (
        <div className="page">
            <div className="page__head">
                <div className="eyebrow">IV · Ajustes</div>
                <h1 className="page__title">Ajustes</h1>
            </div>
            <GameSection state={state} refresh={refresh} />
            <TestSection state={state} refresh={refresh} />
            <AboutSection state={state} />
            <DevSection state={state} refresh={refresh} />
        </div>
    )
}

/**
 * "Versões de teste": the newest test versions (release candidates) of the Stein and of the mods, installed on their
 * own on the next Play. Off: back to the stable ones.
 */
function TestSection({ state, refresh }: { state: AppState; refresh: () => void }) {
    const [error, setError] = useState('')
    const toggle = async () => {
        setError('')
        try {
            await api.setTestVersions(!state.testVersions)
        } catch (err) {
            setError(String(err))
        }
        refresh()
    }
    return (
        <div className="section">
            <SectionHead n="II" label="Atualizações" />
            <div className="rows">
                <div className="row">
                    <div className="row__main">
                        <div className="row__title">
                            Versões de teste
                            {state.testPack && <span className="row__tag row__tag--test">Em uso: {state.testPack}</span>}
                        </div>
                        <div className="row__text">
                            Receba as versões novas do Stein e dos mods antes de todo mundo. Podem ter problemas.
                            Desligado, volta para as versões normais.
                        </div>
                    </div>
                    <button
                        className={'toggle' + (state.testVersions ? ' toggle--on' : '')}
                        onClick={toggle}
                        aria-label="Versões de teste"
                    />
                </div>
            </div>
            {error && <ErrorText error={error} />}
        </div>
    )
}

/** The game options and the folder shortcuts. */
function GameSection({ state, refresh }: { state: AppState; refresh: () => void }) {
    const closeOnPlay = async () => {
        await api.setCloseOnPlay(!state.closeOnPlay)
        refresh()
    }
    const gfx = async () => {
        await api.setGfx(!state.gfx)
        refresh()
    }
    const pickApi = async (value: GfxApi) => {
        if (value === state.gfxApi) return
        await api.setGfxApi(value)
        refresh()
    }
    return (
        <div className="section">
            <SectionHead n="I" label="Jogo" />
            <div className="rows">
                <div className="row">
                    <div className="row__main">
                        <div className="row__title">Fechar o launcher ao jogar</div>
                        <div className="row__text">Fecha quando o jogo abre e volta se o jogo der erro. Desligado, fica minimizado.</div>
                    </div>
                    <button className={'toggle' + (state.closeOnPlay ? ' toggle--on' : '')} onClick={closeOnPlay} />
                </div>
                <div className="row">
                    <div className="row__main">
                        <div className="row__title">
                            Stein GFX
                            <span className="row__tag">Novo</span>
                        </div>
                        <div className="row__text">O renderer novo do Stein. Desligado, usa o do jogo. Vale na próxima vez que o jogo abrir.</div>
                    </div>
                    <button className={'toggle' + (state.gfx ? ' toggle--on' : '')} onClick={gfx} aria-label="Stein GFX" />
                </div>
                <div className="row">
                    <div className="row__main">
                        <div className="row__title">
                            API gráfica
                        </div>
                        <div className="row__text">
                            {state.gfx
                                ? 'O Vulkan é o padrão e o mais rápido. Se não rodar aqui, o jogo usa o OpenGL. Vale na próxima vez que o jogo abrir.'
                                : 'Só com o Stein GFX ligado.'}
                        </div>
                    </div>
                    <div className="seg" role="radiogroup" aria-label="API gráfica">
                        {(['opengl', 'vulkan'] as const).map((value) => (
                            <button
                                key={value}
                                role="radio"
                                aria-checked={state.gfxApi === value}
                                className={'seg__opt' + (state.gfxApi === value ? ' seg__opt--on' : '')}
                                disabled={!state.gfx}
                                onClick={() => pickApi(value)}
                            >
                                {value === 'vulkan' ? 'Vulkan' : 'OpenGL'}
                            </button>
                        ))}
                    </div>
                </div>
            </div>
            <div className="actions">
                <button className="btn btn-ghost" onClick={() => api.openPath('game')}>
                    <Icon name="folder" size={14} stroke={1.4} />
                    Pasta do jogo
                </button>
                <button className="btn btn-ghost" onClick={() => api.openPath('screenshots')}>
                    <Icon name="cards" size={14} stroke={1.4} />
                    Capturas de tela
                </button>
                <button className="btn btn-ghost" onClick={() => api.openPath('logs')}>
                    <Icon name="list" size={14} stroke={1.4} />
                    Abrir logs
                </button>
            </div>
        </div>
    )
}

/** What each Stein log topic is, by the key the game stores it under. */
const LOG_TOPICS: Record<string, { title: string; text: string }> = {
    rede: {
        title: 'Rede e pacotes',
        text: 'Pacotes recebidos e enviados, por tipo, a cada segundo. Para quedas de conexão e travamentos.',
    },
    desempenho: {
        title: 'Desempenho',
        text: 'Quadros, ticks, memória e coleta de lixo, a cada segundo. Para quedas de FPS e engasgos.',
    },
    render: {
        title: 'Desenho dos itens',
        text: 'A placa de vídeo, as opções e como cada item é montado e desenhado. Para itens que aparecem errados.',
    },
}

const DEV_OPEN_KEY = 'stein.devOptions'

/**
 * Developer options, closed by default: the Stein log topics (the same switches as the Log tab of the Stein Loader
 * menu in the game, kept in the same file) and extra Java arguments for the game.
 */
function DevSection({ state, refresh }: { state: AppState; refresh: () => void }) {
    const [open, setOpen] = useState(() => previewDev || localStorage.getItem(DEV_OPEN_KEY) === '1')
    const [args, setArgs] = useState(state.jvmArgs)
    const [error, setError] = useState('')
    const toggleOpen = () => {
        localStorage.setItem(DEV_OPEN_KEY, open ? '0' : '1')
        setOpen(!open)
    }
    const topic = async (key: string, on: boolean) => {
        setError('')
        try {
            await api.setLogTopic(key, on)
        } catch (err) {
            setError(String(err))
        }
        refresh()
    }
    const saveArgs = async () => {
        setError('')
        try {
            await api.setJvmArgs(args)
        } catch (err) {
            setError(String(err))
        }
        refresh()
    }
    return (
        <div className="section">
            <SectionHead n="IV" label="Desenvolvedor" />
            {!open ? (
                <div className="actions">
                    <button className="btn btn-ghost" onClick={toggleOpen}>
                        Mostrar opções de desenvolvedor
                    </button>
                </div>
            ) : (
                <>
                    <div className="rows">
                        {state.logTopics.map((t) => (
                            <div className="row" key={t.key}>
                                <div className="row__main">
                                    <div className="row__title">{LOG_TOPICS[t.key]?.title ?? t.key}</div>
                                    <div className="row__text">{LOG_TOPICS[t.key]?.text ?? ''}</div>
                                </div>
                                <button
                                    className={'toggle' + (t.on ? ' toggle--on' : '')}
                                    onClick={() => topic(t.key, !t.on)}
                                    disabled={state.running}
                                    aria-label={LOG_TOPICS[t.key]?.title ?? t.key}
                                />
                            </div>
                        ))}
                    </div>
                    <p className="note">
                        {state.running
                            ? 'Com o jogo aberto, mude no menu do Stein, aba Registro.'
                            : 'Ficam em logs/stein, um arquivo por assunto. São os mesmos da aba Registro no jogo.'}
                    </p>
                    <div className="dev-args">
                        <div className="row__title">Argumentos da JVM</div>
                        <textarea
                            className="input source-form__text"
                            value={args}
                            onChange={(e) => setArgs(e.target.value)}
                            placeholder="-Dsteinbridge.gfx.gpuHeldItem=false"
                            rows={2}
                            spellCheck={false}
                            aria-label="Argumentos extras da JVM"
                        />
                    </div>
                    <p className="note">
                        Opções -D para testes. Valem na próxima vez que o jogo abrir. Deixe vazio se não souber.
                    </p>
                    {error && <ErrorText error={error} />}
                    <div className="actions">
                        <button className="sl-btn sl-btn--accent" onClick={saveArgs} disabled={args === state.jvmArgs}>
                            Salvar argumentos
                        </button>
                        <button className="btn btn-ghost" onClick={() => api.openPath('steinlogs')}>
                            <Icon name="folder" size={14} stroke={1.4} />
                            Logs do Stein
                        </button>
                        <button className="btn btn-ghost" onClick={toggleOpen}>
                            Ocultar
                        </button>
                    </div>
                </>
            )}
        </div>
    )
}

/** Versions and the folder in use. */
function AboutSection({ state }: { state: AppState }) {
    return (
        <div className="section">
            <SectionHead n="III" label="Sobre" />
            <dl className="kv">
                <dt>Stein Client</dt>
                <dd>v{state.version}</dd>
                <dt>Stein Loader</dt>
                <dd>{state.loader ?? 'não instalado'}</dd>
                {state.bridgeVersion && state.loaderLibrary && (
                    <>
                        <dt>Componentes</dt>
                        <dd>
                            Stein Bridge {state.bridgeVersion} · Loader {state.loaderLibrary}
                        </dd>
                    </>
                )}
                <dt>Pasta</dt>
                <dd>
                    {state.dir}
                    {state.portable ? ' (portátil)' : ''}
                </dd>
            </dl>
        </div>
    )
}
