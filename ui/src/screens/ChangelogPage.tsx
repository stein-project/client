/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

import { useEffect, useMemo, useState } from 'react'

import { api, AppState, ChangeView } from '../api'
import { Markdown } from '../components/Markdown'

/** The project's changelog on the site, for everything the launcher does not list (older releases, the installer). */
const SITE_CHANGELOG = 'https://stein-project.github.io/pt/changelog/'

/**
 * The "Novidades" page: the notes of every version of the Stein and of the mods the launcher knows, newest first, like
 * the changelog on the site, in two tabs: what comes with the Stein (the pack and the base mods) and the additions the
 * player made (the extras and other authors' mods).
 */
export function ChangelogPage({ state }: { state: AppState }) {
    const [entries, setEntries] = useState<ChangeView[] | null>(null)
    const [tab, setTab] = useState<Tab>('stein')
    const [filter, setFilter] = useState<string>('all')
    useEffect(() => {
        let live = true
        api.changelog()
            .then((e) => live && setEntries(e))
            .catch(() => live && setEntries([]))
        return () => {
            live = false
        }
        // The list follows the versions in use ("Em uso").
    }, [state])
    const inTab = (entries ?? []).filter((e) => e.thirdParty === (tab === 'extras'))
    const hasExtras = (entries ?? []).some((e) => e.thirdParty)
    const components = useMemo(() => {
        const seen = new Map<string, string>()
        for (const e of inTab) if (!seen.has(e.id)) seen.set(e.id, e.name)
        // The Stein first, then the mods by name.
        return [...seen].sort(([a, an], [b, bn]) => (a === 'pack' ? -1 : b === 'pack' ? 1 : an.localeCompare(bn)))
    }, [entries, tab]) // eslint-disable-line react-hooks/exhaustive-deps
    const shown = inTab.filter((e) => filter === 'all' || e.id === filter)
    const pick = (t: Tab) => {
        setTab(t)
        setFilter('all')
    }
    return (
        <div className="page">
            <div className="page__head clog__head">
                <div>
                    <div className="eyebrow">III · Novidades</div>
                    <h1 className="page__title">Novidades</h1>
                    <p className="page__lead">O que mudou em cada versão do Stein e dos mods.</p>
                </div>
                <button className="btn btn-ghost" onClick={() => api.openUrl(SITE_CHANGELOG)}>
                    Ver no site
                </button>
            </div>
            {entries === null ? (
                <p className="note">Carregando…</p>
            ) : entries.length === 0 ? (
                <p className="note">As novidades aparecem quando houver conexão.</p>
            ) : (
                <>
                    <div className="clog__tabs" role="tablist">
                        <TabButton on={tab === 'stein'} onClick={() => pick('stein')} label="Stein" />
                        {hasExtras && <TabButton on={tab === 'extras'} onClick={() => pick('extras')} label="Adições" />}
                    </div>
                    {tab === 'extras' && (
                        <p className="note clog__note">
                            Extras do Stein e mods de outros autores.
                        </p>
                    )}
                    <div className="clog__filters" role="toolbar" aria-label="Filtrar">
                        <Filter on={filter === 'all'} onClick={() => setFilter('all')} label="Todos" />
                        {components.map(([id, name]) => (
                            <Filter key={id} on={filter === id} onClick={() => setFilter(id)} label={name} />
                        ))}
                    </div>
                    <ol className="clog">
                        {shown.map((e) => (
                            <Entry key={entryId(e.id, e.version)} e={e} />
                        ))}
                    </ol>
                </>
            )}
        </div>
    )
}

type Tab = 'stein' | 'extras'

function TabButton({ on, onClick, label }: { on: boolean; onClick: () => void; label: string }) {
    return (
        <button className={'clog__tab' + (on ? ' clog__tab--on' : '')} role="tab" aria-selected={on} onClick={onClick}>
            {label}
        </button>
    )
}

function Filter({ on, onClick, label }: { on: boolean; onClick: () => void; label: string }) {
    return (
        <button className={'clog__filter' + (on ? ' clog__filter--on' : '')} aria-pressed={on} onClick={onClick}>
            {label}
        </button>
    )
}

function Entry({ e }: { e: ChangeView }) {
    const { title, body } = split(e.notes)
    return (
        <li className="clog__entry" id={entryId(e.id, e.version)}>
            <div className="clog__meta">
                <span className="clog__tag">{e.name}</span>
                <span className="clog__ver">{e.version}</span>
                {e.date && <span>{date(e.date)}</span>}
                <span className="clog__badges">
                    {e.inUse && <span className="row__tag clog__badge">Em uso</span>}
                </span>
            </div>
            <div className="clog__body">
                {title && <h2 className="clog__title">{title}</h2>}
                <Markdown text={body} />
            </div>
        </li>
    )
}

const entryId = (id: string, version: string) => `clog-${id}-${version}`

/** "2026-11-14" as "14/11/2026". */
function date(d: string): string {
    const [y, m, day] = d.split('-')
    return y && m && day ? `${day}/${m}/${y}` : d
}

/**
 * The file's own "# HUD 0.1.17: minimap and target..." line becomes the entry's title, without the name and version
 * the side column already shows.
 */
function split(notes: string): { title: string; body: string } {
    const lines = notes.replace(/^﻿/, '').split(/\r?\n/)
    const first = lines.findIndex((l) => l.trim() !== '')
    const heading = first >= 0 ? /^#\s+(.*)$/.exec(lines[first].trim()) : null
    if (!heading) return { title: '', body: notes }
    const text = heading[1]
    const colon = text.indexOf(': ')
    const title = colon >= 0 ? text.slice(colon + 2) : text
    return { title: title.charAt(0).toUpperCase() + title.slice(1), body: lines.slice(first + 1).join('\n') }
}
