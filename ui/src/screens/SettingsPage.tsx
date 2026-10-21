/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

import { useState } from 'react'

import { api, AppState } from '../api'
import { ErrorText } from '../components/ErrorText'
import { Head } from '../components/Head'
import { SectionHead } from '../components/SectionHead'
import { Icon } from '../ds/Icon'
import { previewDev, previewNickError, previewOffline } from '../preview'

/** The Settings page: accounts, game options, the About section and the developer options. */
export function SettingsPage({ state, refresh, onAdd }: { state: AppState; refresh: () => void; onAdd: () => void }) {
    return (
        <div className="page">
            <div className="page__head">
                <div className="eyebrow">III · Ajustes</div>
                <h1 className="page__title">Ajustes</h1>
            </div>
            <AccountsSection state={state} refresh={refresh} onAdd={onAdd} />
            <GameSection state={state} refresh={refresh} />
            <AboutSection state={state} />
            <DevSection state={state} refresh={refresh} />
        </div>
    )
}

/** The accounts: switch, remove, add a Microsoft account or an offline nick. */
function AccountsSection({ state, refresh, onAdd }: { state: AppState; refresh: () => void; onAdd: () => void }) {
    const [offlineForm, setOfflineForm] = useState(previewOffline)
    const [nick, setNick] = useState(previewOffline ? 'jogador teste' : '')
    const [nickError, setNickError] = useState(previewNickError)
    const addOffline = async (e: React.FormEvent) => {
        e.preventDefault()
        setNickError('')
        try {
            await api.addOfflineAccount(nick)
            setNick('')
            setOfflineForm(false)
            refresh()
        } catch (err) {
            setNickError(String(err))
        }
    }
    const select = async (uuid: string) => {
        await api.selectAccount(uuid)
        refresh()
    }
    const remove = async (uuid: string) => {
        await api.removeAccount(uuid)
        refresh()
    }
    return (
        <div className="section">
            <SectionHead n="I" label="Contas" />
            <div className="rows">
                {state.accounts.map((a) => (
                    <div className="row" key={a.uuid}>
                        <Head account={a} size={32} />
                        <div className="row__main">
                            <div className="row__title">
                                {a.name}
                                {a.offline && <span className="row__tag">Offline</span>}
                            </div>
                            <div className="row__text">
                                {a.uuid === state.current ? 'Em uso' : a.offline ? 'Conta offline' : 'Conta Microsoft'}
                            </div>
                        </div>
                        {a.uuid !== state.current && (
                            <button className="btn btn-ghost" onClick={() => select(a.uuid)}>
                                Usar
                            </button>
                        )}
                        <button className="btn btn-danger" onClick={() => remove(a.uuid)} aria-label="Remover">
                            <Icon name="logout" size={14} stroke={1.4} />
                        </button>
                    </div>
                ))}
            </div>
            {offlineForm ? (
                <OfflineForm
                    nick={nick}
                    setNick={setNick}
                    error={nickError}
                    onSubmit={addOffline}
                    onCancel={() => {
                        setOfflineForm(false)
                        setNickError('')
                    }}
                />
            ) : (
                <div className="actions">
                    <button className="btn btn-ghost" onClick={onAdd}>
                        <Icon name="plus" size={14} stroke={1.4} />
                        Adicionar conta Microsoft
                    </button>
                    {state.hasOwner && (
                        <button className="btn btn-ghost" onClick={() => setOfflineForm(true)}>
                            <Icon name="plus" size={14} stroke={1.4} />
                            Adicionar conta offline
                        </button>
                    )}
                </div>
            )}
            <p className="note">
                Contas offline servem para jogar sozinho e em servidores sem autenticação. Elas ficam disponíveis
                enquanto houver uma conta original no launcher.
            </p>
        </div>
    )
}

/** The form for a new offline account's nick. */
function OfflineForm({
    nick,
    setNick,
    error,
    onSubmit,
    onCancel,
}: {
    nick: string
    setNick: (nick: string) => void
    error: string
    onSubmit: (e: React.FormEvent) => void
    onCancel: () => void
}) {
    return (
        <form className="offline-form" onSubmit={onSubmit}>
            <input
                className="input"
                value={nick}
                onChange={(e) => setNick(e.target.value)}
                placeholder="Nick"
                maxLength={16}
                autoFocus
                spellCheck={false}
                aria-label="Nick da conta offline"
            />
            <button className="sl-btn sl-btn--accent" type="submit" disabled={nick.trim().length < 3}>
                Adicionar
            </button>
            <button
                className="btn btn-ghost"
                type="button"
                onClick={onCancel}
            >
                Cancelar
            </button>
            {error && <ErrorText error={error} />}
        </form>
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
    return (
        <div className="section">
            <SectionHead n="II" label="Jogo" />
            <div className="rows">
                <div className="row">
                    <div className="row__main">
                        <div className="row__title">Fechar o launcher ao jogar</div>
                        <div className="row__text">O launcher fecha quando o jogo abre e volta sozinho se o jogo for encerrado por um erro. Desligado, ele fica minimizado.</div>
                    </div>
                    <button className={'toggle' + (state.closeOnPlay ? ' toggle--on' : '')} onClick={closeOnPlay} />
                </div>
                <div className="row">
                    <div className="row__main">
                        <div className="row__title">
                            Stein GFX
                            <span className="row__tag">Novo</span>
                        </div>
                        <div className="row__text">O renderer novo do Stein. Desligado, o jogo abre com o renderer de sempre. Vale a partir da próxima abertura do jogo.</div>
                    </div>
                    <button className={'toggle' + (state.gfx ? ' toggle--on' : '')} onClick={gfx} aria-label="Stein GFX" />
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
                            ? 'Com o jogo aberto, mude os logs no menu do Stein Loader, na aba Registro.'
                            : 'Os logs ficam em logs/stein, um arquivo por assunto. São os mesmos interruptores da aba Registro do menu do Stein Loader no jogo: mudar aqui muda lá, e o contrário também.'}
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
                        Opções -D do Stein para testes, passadas ao jogo por último. Valem a partir da próxima abertura do
                        jogo. Deixe vazio se não souber para que servem.
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
