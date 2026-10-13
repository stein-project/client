/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

import { useEffect, useState } from 'react'

import { api, DeviceCode } from '../api'
import { ErrorText } from '../components/ErrorText'
import { Logo } from '../ds/Brand'
import { inTauri } from '../preview'

/** The sign-in screen: Microsoft device-code login. */
export function Login({
    canCancel,
    onDone,
    onCancel,
}: {
    canCancel: boolean
    onDone: () => void
    onCancel: () => void
}) {
    const [code, setCode] = useState<DeviceCode | null>(null)
    const [error, setError] = useState('')
    const [waiting, setWaiting] = useState(false)
    useEffect(() => {
        if (!inTauri && location.search.includes('code')) start()
    }, [])

    const start = async () => {
        setError('')
        setWaiting(true)
        try {
            const c = await api.loginBegin()
            setCode(c)
            await api.loginFinish()
            onDone()
        } catch (e) {
            setError(String(e))
            setCode(null)
        } finally {
            setWaiting(false)
        }
    }
    const copyAndOpen = async () => {
        if (!code) return
        try {
            await navigator.clipboard.writeText(code.userCode)
        } catch {
            /* no clipboard access: the code is still on screen */
        }
        api.openUrl(code.verificationUri)
    }
    const cancel = () => {
        api.loginCancel()
        setCode(null)
        setWaiting(false)
        onCancel()
    }

    return (
        <div className="login">
            <div className="login__card">
                <Logo height={112} className="login__logo" />
                <div className="stage" key={code ? 'code' : 'start'}>
                    {!code ? (
                        <>
                            <div className="actions login__start">
                                <button className="sl-btn sl-btn--accent sl-btn--lg" onClick={start} disabled={waiting}>
                                    {waiting ? 'Aguarde…' : 'Entrar com a Microsoft'}
                                </button>
                                {canCancel && (
                                    <button className="btn btn-ghost" onClick={cancel}>
                                        Voltar
                                    </button>
                                )}
                            </div>
                        </>
                    ) : (
                        <>
                            <p className="login__lead">Acesse microsoft.com/link e informe o código abaixo:</p>
                            <div className="code">{code.userCode}</div>
                            <div className="actions">
                                <button className="sl-btn sl-btn--accent" onClick={copyAndOpen}>
                                    Copiar e abrir
                                </button>
                                <button className="btn btn-ghost" onClick={cancel}>
                                    Cancelar
                                </button>
                            </div>
                            <p className="note">
                                A Microsoft pedirá autorização para um aplicativo parceiro de login, o mesmo usado por
                                launchers independentes. O Stein Client não tem acesso à sua senha.
                            </p>
                        </>
                    )}
                </div>
                {error && <div style={{ marginTop: 18 }}><ErrorText error={error} /></div>}
            </div>
        </div>
    )
}
