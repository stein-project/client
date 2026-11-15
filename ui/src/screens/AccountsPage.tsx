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
import { Icon } from '../ds/Icon'
import { previewNickError, previewOffline } from '../preview'

/** The Accounts page (from the account at the bottom of the sidebar): who plays, and adding or removing an account. */
export function AccountsPage({ state, refresh, onAdd }: { state: AppState; refresh: () => void; onAdd: () => void }) {
    return (
        <div className="page">
            <div className="page__head">
                <div className="eyebrow">Contas</div>
                <h1 className="page__title">Contas</h1>
                <p className="page__lead">A conta em uso é a que entra no jogo.</p>
            </div>
            <AccountsSection state={state} refresh={refresh} onAdd={onAdd} />
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
                Contas offline servem para jogar sozinho e em servidores sem autenticação. Precisam de uma conta
                original no launcher.
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

