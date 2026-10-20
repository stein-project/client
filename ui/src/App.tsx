/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

import { useCallback, useEffect, useState } from 'react'

import { Account, api, AppState, onGame, onSelfUpdate, setHeads } from './api'
import { Head } from './components/Head'
import { Window } from './components/Window'
import { Logo, Monogram } from './ds/Brand'
import { Icon, IconName } from './ds/Icon'
import { previewCrash, previewPage } from './preview'
import { Login } from './screens/Login'
import { IndexStatus, ModsPage } from './screens/ModsPage'
import { PlayPage } from './screens/PlayPage'
import { SettingsPage } from './screens/SettingsPage'

type Page = 'play' | 'mods' | 'settings'

/** The sidebar pages, numbered like the page headings (ModsPage "II", SettingsPage "III"). */
const NAV: { id: Page; label: string; idx: string; icon: IconName }[] = [
    { id: 'play', label: 'Jogar', idx: 'I', icon: 'play' },
    { id: 'mods', label: 'Mods', idx: 'II', icon: 'layers' },
    { id: 'settings', label: 'Ajustes', idx: 'III', icon: 'gear' },
]

export function App() {
    const [state, setState] = useState<AppState | null>(null)
    const [page, setPage] = useState<Page>(previewPage)
    const [adding, setAdding] = useState(false)
    const [index, setIndex] = useState<IndexStatus>('loading')
    // The last crash (the game's exit code), kept until the player dismisses the notice or plays again.
    const [crash, setCrash] = useState<number | null>(previewCrash)
    // A new launcher version: the launcher swaps its own executable and relaunches itself already updated.
    const [selfUpdate, setSelfUpdate] = useState<string | null>(null)
    useEffect(() => {
        // An empty version means the update failed and the launcher carries on with the current one.
        const un = onSelfUpdate((v) => setSelfUpdate(v || null))
        return () => {
            un.then((f) => f())
        }
    }, [])
    const refresh = useCallback(
        () =>
            api.state().then((s) => {
                setHeads(s.links?.heads)
                setState(s)
            }),
        [],
    )

    useEffect(() => {
        refresh()
        // Always fetch the latest official mod list: it is what lets the player pick mods before installing.
        api.loadIndex()
            .then(() => setIndex('ok'))
            .catch(() => setIndex('offline'))
            .finally(refresh)
        const un = onGame((s) => {
            if (s.startsWith('crashed')) {
                setCrash(Number(s.slice('crashed:'.length)) || -1)
                setAdding(false)
                setPage('play')
            } else if (s === 'running') {
                setCrash(null)
            }
        })
        return () => {
            un.then((f) => f())
        }
    }, [refresh])

    if (selfUpdate) {
        return (
            <Window version={state?.version ?? ''}>
                <div className="login">
                    <div className="login__card">
                        <Logo height={112} className="login__logo" />
                        <p className="login__lead login__update">Atualizando o Stein Client para a versão {selfUpdate}…</p>
                    </div>
                </div>
            </Window>
        )
    }
    if (!state) {
        return <Window version="" />
    }
    const current = state.accounts.find((a) => a.uuid === state.current) ?? null
    if (!state.hasOwner || !current || adding) {
        return (
            <Window version={state.version}>
                <Login
                    canCancel={state.hasOwner && !!current}
                    onDone={() => {
                        setAdding(false)
                        refresh()
                    }}
                    onCancel={() => setAdding(false)}
                />
            </Window>
        )
    }
    return (
        <Window version={state.version} portable={state.portable}>
            <div className="shell">
                <Sidebar page={page} setPage={setPage} current={current} />
                <main className="main">
                    {/* Always mounted (only hidden) so downloads and progress survive switching tabs and back. */}
                    <div style={{ display: page === 'play' ? 'contents' : 'none' }}>
                        <PlayPage
                            state={state}
                            refresh={refresh}
                            onLogin={() => setAdding(true)}
                            crash={crash}
                            onDismissCrash={() => setCrash(null)}
                        />
                    </div>
                    {page === 'mods' && <ModsPage state={state} refresh={refresh} index={index} />}
                    {page === 'settings' && <SettingsPage state={state} refresh={refresh} onAdd={() => setAdding(true)} />}
                </main>
            </div>
        </Window>
    )
}

/** The sidebar: wordmark, page navigation and the account in use. */
function Sidebar({ page, setPage, current }: { page: Page; setPage: (page: Page) => void; current: Account }) {
    return (
        <aside className="sidebar">
            <div className="wordmark">
                <Monogram size={42} />
                <div>
                    <div className="wordmark__name">Stein Client</div>
                    <div className="eyebrow wordmark__sub">Minecraft 1.8.9</div>
                </div>
            </div>
            <nav className="nav">
                {NAV.map((n) => (
                    <button
                        key={n.id}
                        className={'nav__item' + (page === n.id ? ' nav__item--active' : '')}
                        onClick={() => setPage(n.id)}
                    >
                        <Icon name={n.icon} size={18} stroke={1.3} />
                        <span className="nav__label">{n.label}</span>
                        <span className="nav__idx">{n.idx}</span>
                    </button>
                ))}
            </nav>
            <div className="who">
                <div className="who__row">
                    <Head account={current} />
                    <div style={{ minWidth: 0, lineHeight: 1.25 }}>
                        <div className="who__name">{current.name}</div>
                        <div className="who__kind">{current.offline ? 'Offline' : 'Microsoft'}</div>
                    </div>
                </div>
                <button className="btn btn-ghost" style={{ width: '100%' }} onClick={() => setPage('settings')}>
                    Contas
                </button>
            </div>
        </aside>
    )
}
