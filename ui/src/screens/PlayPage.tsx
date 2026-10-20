/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

import { useEffect, useState } from 'react'

import { api, AppState, Change, News, onGame, onProgress, Progress } from '../api'
import { ErrorText } from '../components/ErrorText'
import { NoticeCard } from '../components/NoticeCard'
import { Logo } from '../ds/Brand'
import { Icon } from '../ds/Icon'
import { previewBusy, previewError } from '../preview'

/** The Play page: launch button, download progress, crash report, pending updates, notice and news. */
export function PlayPage({
    state,
    refresh,
    onLogin,
    crash,
    onDismissCrash,
}: {
    state: AppState
    refresh: () => void
    onLogin: () => void
    crash: number | null
    onDismissCrash: () => void
}) {
    const [progress, setProgress] = useState<Progress | null>(
        previewBusy ? { step: 'Baixando o Minecraft 1.8.9', done: 412, total: 770, detail: '' } : null,
    )
    const [busy, setBusy] = useState(previewBusy)
    const [error, setError] = useState(previewError)
    const [running, setRunning] = useState(state.running)

    useEffect(() => {
        const un1 = onProgress(setProgress)
        const un2 = onGame((s) => {
            setRunning(s === 'running')
            refresh()
        })
        return () => {
            un1.then((f) => f())
            un2.then((f) => f())
        }
    }, [refresh])

    const play = async (debug = false) => {
        setError('')
        onDismissCrash()
        setBusy(true)
        try {
            await api.play(debug)
        } catch (e) {
            const msg = String(e)
            if (msg === 'login') {
                onLogin()
            } else {
                setError(msg)
            }
        } finally {
            setBusy(false)
            setProgress(null)
            refresh()
        }
    }

    const pct = progress && progress.total > 0 ? Math.min(100, (progress.done / progress.total) * 100) : 0
    return (
        <div className="page">
            <Hero state={state} />

            <div className="play">
                <button className="sl-btn sl-btn--accent sl-btn--lg" onClick={() => play()} disabled={busy || running}>
                    <Icon name="play" size={16} stroke={1.5} />
                    {running ? 'Em jogo' : busy ? 'Preparando' : state.updates.length > 0 ? 'Atualizar e jogar' : 'Jogar'}
                </button>
                {busy && (
                    <>
                        <div className={'bar' + (progress && progress.total > 0 ? '' : ' bar--busy')}>
                            <div className="bar__fill" style={{ width: `${pct}%` }} />
                        </div>
                        <div className="status">
                            {progress?.step ?? 'Conferindo'}
                            {progress && progress.total > 0 && progress.total > 100000
                                ? ` · ${(progress.done / 1048576).toFixed(0)} de ${(progress.total / 1048576).toFixed(0)} MB`
                                : progress && progress.total > 0
                                ? ` · ${progress.done} de ${progress.total}`
                                : ''}
                        </div>
                    </>
                )}
                {!busy && running && <div className="status">O jogo está aberto.</div>}
                {error && <ErrorText error={error} />}
            </div>

            {crash !== null && !busy && !running && (
                <CrashCard
                    crash={crash}
                    state={state}
                    onRestart={() => play(true)}
                    onDismiss={onDismissCrash}
                    onError={setError}
                />
            )}

            {crash === null && !busy && state.updates.length > 0 && <UpdatesCard updates={state.updates} />}

            {crash === null && state.notice && <NoticeCard notice={state.notice} refresh={refresh} />}
            {crash === null && state.news && <NewsCard news={state.news} />}
        </div>
    )
}

/** The logo, the tagline and the Loader and mod-count tags. */
function Hero({ state }: { state: AppState }) {
    const enabled = state.mods.filter((m) => m.enabled).length
    return (
        <div className="hero">
            <Logo height={150} className="hero__logo" />
            <div>
                <div className="eyebrow">Minecraft 1.8.9</div>
                <p className="hero__lead">O jogo que você já conhece, com todo o poder do Stein Loader.</p>
                <div className="tags">
                    <span className="sl-tag sl-tag--pill">
                        <span className={'sl-tag__dot' + (state.ready ? ' sl-tag__dot--live' : '')} />
                        {state.loader ? `Stein Loader ${state.loader}` : 'Instalação pendente'}
                    </span>
                    <span className="sl-tag sl-tag--pill sl-tag--ritual">{enabled} {enabled === 1 ? 'mod ativo' : 'mods ativos'}</span>
                </div>
            </div>
        </div>
    )
}

/** What to do after a crash: restart with the Stein logs, open the log, or ask on Discord. */
function CrashCard({
    crash,
    state,
    onRestart,
    onDismiss,
    onError,
}: {
    crash: number
    state: AppState
    onRestart: () => void
    onDismiss: () => void
    onError: (error: string) => void
}) {
    return (
        <div className="card crash" role="alert">
            <div className="crash__head">
                <Icon name="force" size={22} stroke={1.3} />
                <div className="crash__text">
                    <div className="crash__title">O jogo foi encerrado inesperadamente</div>
                    <div className="crash__body">
                        Se o problema se repetir, reinicie com os logs do Stein ativos e envie o arquivo de log
                        no Discord do Stein Labs. Assim a equipe consegue encontrar a causa.
                    </div>
                    <div className="crash__code">Código de saída {crash}</div>
                </div>
                <button className="crash__close" onClick={onDismiss} aria-label="Fechar aviso">
                    <Icon name="x" size={14} stroke={1.4} />
                </button>
            </div>
            <div className="actions">
                <button className="sl-btn sl-btn--accent" onClick={onRestart}>
                    <Icon name="restart" size={14} stroke={1.5} />
                    Reiniciar com logs do Stein
                </button>
                <button className="btn btn-ghost" onClick={() => api.openCrashLog().catch((e) => onError(String(e)))}>
                    <Icon name="list" size={14} stroke={1.4} />
                    Abrir log
                </button>
                <button className="btn btn-ghost" onClick={() => api.openUrl(discordUrl(state))}>
                    Discord do Stein Labs
                </button>
            </div>
        </div>
    )
}

/** What the next Play installs, updates or removes. */
function UpdatesCard({ updates }: { updates: Change[] }) {
    return (
        <div className="card updates">
            <div className="updates__head">
                <Icon name="down" size={18} stroke={1.4} />
                <div>
                    <div className="updates__title">Atualizações disponíveis</div>
                    <div className="updates__text">Serão instaladas ao clicar em Atualizar e jogar.</div>
                </div>
            </div>
            <ul className="updates__list">
                {updates.map((u) => (
                    <li key={u.id}>
                        <span className="updates__name">
                            {u.name}
                            {u.thirdParty && <span className="row__tag">terceiros</span>}
                        </span>
                        <span className="updates__change">
                            {u.to === null
                                ? 'será removido'
                                : u.from === null
                                  ? `será instalado · v${u.to}`
                                  : `v${u.from} → v${u.to}`}
                        </span>
                    </li>
                ))}
            </ul>
        </div>
    )
}

/** The news item from the official index. */
function NewsCard({ news }: { news: News }) {
    return (
        <div className="card news">
            <Icon name="star" size={20} stroke={1.3} style={{ color: 'var(--gold)' }} />
            <div className="news__text">
                <div className="news__title">{news.title}</div>
                <div className="news__body">{news.text}</div>
            </div>
            {news.url && (
                <button className="btn btn-ghost" onClick={() => api.openUrl(news.url)}>
                    Abrir
                </button>
            )}
        </div>
    )
}

/** The Stein Labs Discord: the registry's invite, then the news link when it is a Discord invite, then the built-in one. */
function discordUrl(state: AppState): string {
    const isDiscord = (url: string) => /^https:\/\/(discord\.gg|discord\.com)\//.test(url)
    const fromRegistry = state.links?.discord ?? ''
    const fromNews = state.news?.url ?? ''
    return isDiscord(fromRegistry) ? fromRegistry : isDiscord(fromNews) ? fromNews : 'https://discord.gg/MEHE9b5bjj'
}
