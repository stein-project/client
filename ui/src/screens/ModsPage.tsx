/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

import { useState } from 'react'

import { api, AppState, ModView, Source, SourcePreview } from '../api'
import { ErrorText } from '../components/ErrorText'
import { NoticeCard } from '../components/NoticeCard'
import { SectionHead } from '../components/SectionHead'
import { Icon } from '../ds/Icon'

/** Whether the official mod index has loaded. */
export type IndexStatus = 'loading' | 'ok' | 'offline'

/** One mod with its version, pending change and on/off toggle. */
function ModRow({ m, onToggle }: { m: ModView; onToggle: (id: string, enabled: boolean) => void }) {
    return (
        <div className="row">
            <div className="row__main">
                <div className="row__title">
                    {m.name}
                    <span className="mono">v{m.installedVersion ?? m.version}</span>
                    {m.enabled && !m.installedVersion && <span className="row__tag">Será instalado</span>}
                    {m.enabled && m.installedVersion && m.installedVersion !== m.version && (
                        <span className="row__tag row__tag--update">Atualiza para v{m.version}</span>
                    )}
                    {!m.enabled && m.installedVersion && <span className="row__tag">Será removido</span>}
                </div>
                <div className="row__text">{m.description}</div>
            </div>
            <button
                className={'toggle' + (m.enabled ? ' toggle--on' : '')}
                onClick={() => onToggle(m.id, !m.enabled)}
                aria-label={m.enabled ? 'Desligar' : 'Ligar'}
            />
        </div>
    )
}

/** The host of a URL (what the safety warning shows). */
function hostOf(url: string): string {
    return url.split('://')[1]?.split('/')[0] ?? url
}

/** The Mods page: official mods, third-party sources, and the add-source flow with its safety warning. */
export function ModsPage({ state, refresh, index }: { state: AppState; refresh: () => void; index: IndexStatus }) {
    const toggle = async (id: string, enabled: boolean) => {
        await api.setMod(id, enabled)
        refresh()
    }
    const official = state.mods.filter((m) => !m.thirdParty)
    // Pasting a source: the raw text, the parsed source (for the warning) and whether a request is in flight.
    const [pasting, setPasting] = useState(false)
    const [text, setText] = useState('')
    const [preview, setPreview] = useState<SourcePreview | null>(null)
    const [working, setWorking] = useState(false)
    const [error, setError] = useState('')
    // Removing a source needs a second click to confirm.
    const [removing, setRemoving] = useState<string | null>(null)
    const closeForm = () => {
        setPasting(false)
        setText('')
        setPreview(null)
        setError('')
    }
    const run = async (f: () => Promise<void>) => {
        setError('')
        setWorking(true)
        try {
            await f()
        } catch (e) {
            setError(String(e))
        } finally {
            setWorking(false)
        }
    }
    const read = () => run(async () => setPreview(await api.inspectSource(text)))
    const add = () =>
        run(async () => {
            await api.addSource(preview!.url)
            closeForm()
            refresh()
        })
    const remove = (url: string) =>
        run(async () => {
            await api.removeSource(url)
            setRemoving(null)
            refresh()
        })
    return (
        <div className="page">
            <div className="page__head">
                <div className="eyebrow">II · Mods</div>
                <h1 className="page__title">Mods</h1>
                <p className="page__lead">Escolha os mods que quer usar. Eles são baixados e atualizados ao clicar em Jogar.</p>
            </div>

            <OfficialSection mods={official} index={index} onToggle={toggle} />

            <div className="section">
                <SectionHead n="II" label="Adições" />
                {state.notice && <NoticeCard notice={state.notice} refresh={refresh} />}
                {state.sources.length === 0 && !pasting && (
                    <p className="note">
                        Mods fora do padrão, atualizados como os outros: adicione a fonte com o link do índice de mods
                        (steinmods.json) que o autor publica. Alguns servidores proíbem certos mods: confira as regras
                        de onde você joga.
                    </p>
                )}
                {state.sources.map((s) => (
                    <SourceCard
                        key={s.url}
                        s={s}
                        mods={state.mods.filter((m) => m.thirdParty && m.source === s.url)}
                        index={index}
                        removing={removing === s.url}
                        working={working}
                        onAskRemove={() => setRemoving(s.url)}
                        onCancelRemove={() => setRemoving(null)}
                        onRemove={() => remove(s.url)}
                        onToggle={toggle}
                    />
                ))}

                {pasting && !preview && (
                    <SourceForm text={text} setText={setText} working={working} onRead={read} onCancel={closeForm} />
                )}

                {preview && <SourceWarning preview={preview} working={working} onAdd={add} onCancel={closeForm} />}

                {error && <ErrorText error={error} />}

                <div className="actions">
                    {!pasting && (
                        <button className="btn btn-ghost" onClick={() => setPasting(true)}>
                            <Icon name="plus" size={14} stroke={1.4} />
                            Adicionar fonte
                        </button>
                    )}
                    <button className="btn btn-ghost" onClick={() => api.openPath('mods')}>
                        <Icon name="folder" size={14} stroke={1.4} />
                        Pasta de mods
                    </button>
                </div>
                <p className="note">Um arquivo .steinmod colocado direto na pasta de mods também funciona, mas não é atualizado.</p>
            </div>
        </div>
    )
}

/** The official mods, or why the list is not there yet. */
function OfficialSection({
    mods,
    index,
    onToggle,
}: {
    mods: ModView[]
    index: IndexStatus
    onToggle: (id: string, enabled: boolean) => void
}) {
    return (
        <div className="section">
            <SectionHead n="I" label="Oficiais" />
            {mods.length === 0 ? (
                <p className="note">
                    {index === 'loading'
                        ? 'Carregando a lista de mods…'
                        : 'Não foi possível carregar a lista de mods. Verifique sua conexão e abra o launcher novamente.'}
                </p>
            ) : (
                <div className="rows">
                    {mods.map((m) => (
                        <ModRow key={m.id} m={m} onToggle={onToggle} />
                    ))}
                </div>
            )}
        </div>
    )
}

/** A third-party source: its name, author and host, the remove button (with confirmation) and its mods. */
function SourceCard({
    s,
    mods,
    index,
    removing,
    working,
    onAskRemove,
    onCancelRemove,
    onRemove,
    onToggle,
}: {
    s: Source
    mods: ModView[]
    index: IndexStatus
    removing: boolean
    working: boolean
    onAskRemove: () => void
    onCancelRemove: () => void
    onRemove: () => void
    onToggle: (id: string, enabled: boolean) => void
}) {
    // A single-mod source named after its mod: the mod row already shows the name, so don't repeat it.
    const same = mods.length === 1 && mods[0].name.toLowerCase() === s.name.toLowerCase()
    return (
        <div className="source">
            <div className="source__head">
                <div className="source__id">
                    {!same && <span className="source__name">{s.name || hostOf(s.url)}</span>}
                    {s.author && <span className="source__author">{same ? `${s.name} · de ${s.author}` : `de ${s.author}`}</span>}
                    <span className="source__host">{hostOf(s.url)}</span>
                </div>
                {removing ? (
                    <>
                        <button className="btn btn-danger" disabled={working} onClick={onRemove}>
                            Remover a fonte
                        </button>
                        <button className="btn btn-ghost" onClick={onCancelRemove}>
                            Cancelar
                        </button>
                    </>
                ) : (
                    <button className="btn btn-ghost" onClick={onAskRemove} aria-label="Remover a fonte">
                        <Icon name="trash" size={14} stroke={1.4} />
                    </button>
                )}
            </div>
            {removing && (
                <p className="note">Os mods desta fonte ficam na pasta de mods, mas deixam de ser atualizados.</p>
            )}
            {mods.length === 0 ? (
                <p className="note">{index === 'loading' ? 'Lendo a fonte…' : 'Não foi possível ler esta fonte agora.'}</p>
            ) : (
                <div className="rows">
                    {mods.map((m) => (
                        <ModRow key={m.id} m={m} onToggle={onToggle} />
                    ))}
                </div>
            )}
        </div>
    )
}

/** The box where the player pastes a source's link or JSON. */
function SourceForm({
    text,
    setText,
    working,
    onRead,
    onCancel,
}: {
    text: string
    setText: (text: string) => void
    working: boolean
    onRead: () => void
    onCancel: () => void
}) {
    return (
        <div className="source-form">
            <textarea
                className="input source-form__text"
                value={text}
                onChange={(e) => setText(e.target.value)}
                placeholder="https://github.com/autor/mod/releases/latest/download/steinmods.json"
                rows={3}
                autoFocus
                spellCheck={false}
                aria-label="Link ou JSON do índice de mods"
            />
            <div className="actions">
                <button className="sl-btn sl-btn--accent" disabled={working || !text.trim()} onClick={onRead}>
                    {working ? 'Lendo…' : 'Ler a fonte'}
                </button>
                <button className="btn btn-ghost" onClick={onCancel}>
                    Cancelar
                </button>
            </div>
        </div>
    )
}

/** The safety warning shown before a third-party source is added, with what it offers. */
function SourceWarning({
    preview,
    working,
    onAdd,
    onCancel,
}: {
    preview: SourcePreview
    working: boolean
    onAdd: () => void
    onCancel: () => void
}) {
    return (
        <div className="card crash warning" role="alert">
            <div className="crash__head">
                <Icon name="shield" size={22} stroke={1.3} />
                <div className="crash__text">
                    <div className="crash__title">Adicionar esta fonte?</div>
                    <div className="crash__body">
                        Estes mods não vêm com o Stein. Um mod roda com o mesmo acesso que o jogo tem ao seu
                        computador: ele pode ler seus arquivos e as contas salvas no Minecraft. Adicione só fontes de
                        autores em quem você confia. Alguns servidores proíbem certos mods, e a responsabilidade de
                        conferir as regras é sua.
                    </div>
                    <dl className="kv warning__kv">
                        <dt>Fonte</dt>
                        <dd>{preview.name || '—'}</dd>
                        <dt>Autor</dt>
                        <dd>{preview.author || 'não informado'}</dd>
                        <dt>De onde vem</dt>
                        <dd>{preview.host}</dd>
                        <dt>Mods</dt>
                        <dd>{preview.mods.map((m) => `${m.name} v${m.version}`).join(', ')}</dd>
                    </dl>
                </div>
            </div>
            <div className="actions">
                <button className="sl-btn sl-btn--accent" disabled={working} onClick={onAdd}>
                    {working ? 'Adicionando…' : 'Confio no autor, adicionar'}
                </button>
                {preview.homepage && (
                    <button className="btn btn-ghost" onClick={() => api.openUrl(preview.homepage)}>
                        Página do autor
                    </button>
                )}
                <button className="btn btn-ghost" onClick={onCancel}>
                    Cancelar
                </button>
            </div>
        </div>
    )
}
