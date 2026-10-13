/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

import { getCurrentWindow } from '@tauri-apps/api/window'
import { useEffect, useState } from 'react'

import { Monogram } from '../ds/Brand'
import { Icon } from '../ds/Icon'
import { inTauri } from '../preview'

/** The frameless window: a custom title bar (drag, minimise, maximise, close) around the content. */
export function Window({
    version,
    portable,
    children,
}: {
    version: string
    portable?: boolean
    children?: React.ReactNode
}) {
    const w = inTauri ? getCurrentWindow() : null
    const [maximized, setMaximized] = useState(false)
    useEffect(() => {
        if (!w) return
        const sync = () => w.isMaximized().then(setMaximized)
        sync()
        const un = w.onResized(sync)
        return () => {
            un.then((f) => f())
        }
    }, [])
    return (
        <div className="window">
            <div className="titlebar">
                <span className="titlebar__title" data-tauri-drag-region>
                    <Monogram size={15} />
                    Stein Client{version ? ` · v${version}` : ''}
                    {portable ? ' · portátil' : ''}
                </span>
                <div className="titlebar__drag" data-tauri-drag-region onDoubleClick={() => w?.toggleMaximize()} />
                <button className="titlebar__btn" onClick={() => w?.minimize()} aria-label="Minimizar">
                    <Icon name="minus" size={14} stroke={1.4} />
                </button>
                <button
                    className="titlebar__btn"
                    onClick={() => w?.toggleMaximize()}
                    aria-label={maximized ? 'Restaurar' : 'Maximizar'}
                >
                    <Icon name={maximized ? 'restore' : 'maximize'} size={14} stroke={1.4} />
                </button>
                <button className="titlebar__btn titlebar__btn--close" onClick={() => w?.close()} aria-label="Fechar">
                    <Icon name="x" size={14} stroke={1.4} />
                </button>
            </div>
            <div className="window__body">{children}</div>
        </div>
    )
}
