/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

import { useEffect, useState } from 'react'

import { api, AppState, Change, ModWarning, News, onGame, onProgress, Progress, RunView } from '../api'
import { ErrorText } from '../components/ErrorText'
import { NoticeCard } from '../components/NoticeCard'
import { Logo } from '../ds/Brand'
import { Icon } from '../ds/Icon'
import { previewBusy, previewError, previewWarnings } from '../preview'

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
    // Mods the installed Loader may not run: the game stays closed until the player chooses.
    const [warnings, setWarnings] = useState<{ list: ModWarning[]; debug: boolean } | null>(
        previewWarnings.length ? { list: previewWarnings, debug: false } : null,
    )

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

    const play = async (debug = false, anyway = false) => {
        setError('')
        setWarnings(null)
        onDismissCrash()
        setBusy(true)
        try {
            const list = await api.play(debug, anyway)
            if (list.length > 0) setWarnings({ list, debug })
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
                <RunSummary run={state.run} />
                {error && <ErrorText error={error} />}
            </div>

            {warnings && !busy && !running && (
                <ModWarningsCard
                    warnings={warnings.list}
                    onPlay={() => play(warnings.debug, true)}
                    onCancel={() => setWarnings(null)}
                />
            )}

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

/** What the next Play runs, in one quiet line under the button: Stein, Java, graphics and memory. */
function RunSummary({ run }: { run: RunView }) {
    const graphics = run.gfx ? `Stein GFX · ${run.api === 'vulkan' ? 'Vulkan' : 'OpenGL'}` : 'Fixo · OpenGL'
    const items: [string, string][] = [
        ['Stein', run.stein ?? '—'],
        // The game always runs on the bundled Java 25.
        ['Java', run.java ? run.java.split('.')[0] : '25'],
        ['Gráficos', graphics],
        ['Memória', `${run.memoryGb} GB`],
    ]
    return (
        <dl className="runinfo">
            {items.map(([k, v]) => (
                <div key={k} className="runinfo__item">
                    <dt>{k}</dt>
                    <dd>
                        {v}
                        {k === 'Stein' && run.test && <span className="row__tag row__tag--test runinfo__tag">Teste</span>}
                    </dd>
                </div>
            ))}
        </dl>
    )
}

/** The logo, the tagline and the install-state and mod-count tags (the versions are in the summary under Play). */
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
                        {state.ready ? 'Pronto' : 'Não instalado'}
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
                    <div className="crash__title">O jogo fechou com um erro</div>
                    <div className="crash__body">
                        Se acontecer de novo, reinicie com os logs do Stein e mande o arquivo no Discord.
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

/** How many missing items each mod lists before "e mais N". */
const MISSING_SHOWN = 4

/**
 * The check before the game opens found mods the installed Stein Loader may not run. It only warns: the player plays
 * anyway or goes back to change a version.
 */
function ModWarningsCard({ warnings, onPlay, onCancel }: { warnings: ModWarning[]; onPlay: () => void; onCancel: () => void }) {
    const one = warnings.length === 1
    return (
        <div className="card crash" role="alert">
            <div className="crash__head">
                <Icon name="force" size={22} stroke={1.3} />
                <div className="crash__text">
                    <div className="crash__title">
                        {one ? `${warnings[0].name} pode não funcionar` : `${warnings.length} mods podem não funcionar`}
                    </div>
                    <div className="crash__body">
                        {one ? 'Foi feito' : 'Foram feitos'} para outra versão do Stein e {one ? 'pode' : 'podem'} falhar ou
                        fechar o jogo. Você pode jogar assim mesmo ou desligar em Mods.
                    </div>
                    <ul className="mwarn">
                        {warnings.map((w) => (
                            <li key={w.modId}>
                                <span className="mwarn__name">
                                    {w.name} <span className="mono">v{w.version}</span>
                                </span>
                                {w.reason && <span className="mwarn__reason">{w.reason}</span>}
                                {w.missing.length > 0 && (
                                    <span className="mwarn__missing mono">
                                        {w.missing.slice(0, MISSING_SHOWN).join(' · ')}
                                        {w.missing.length > MISSING_SHOWN && ` · e mais ${w.missing.length - MISSING_SHOWN}`}
                                    </span>
                                )}
                            </li>
                        ))}
                    </ul>
                </div>
            </div>
            <div className="actions">
                <button className="sl-btn sl-btn--accent" onClick={onPlay}>
                    <Icon name="play" size={14} stroke={1.5} />
                    Jogar assim mesmo
                </button>
                <button className="btn btn-ghost" onClick={onCancel}>
                    Voltar
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
