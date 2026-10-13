/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

import { useState } from 'react'

/**
 * An error from the Rust side: the first line is meant for the player; the rest (paths, URLs, the error chain) stays
 * collapsed under "Details" so it can be copied and sent to whoever is helping.
 */
export function ErrorText({ error, className = 'status status--err' }: { error: string; className?: string }) {
    const [open, setOpen] = useState(false)
    const cut = error.indexOf('\n')
    const head = cut < 0 ? error : error.slice(0, cut)
    const detail = cut < 0 ? '' : error.slice(cut + 1).trim()
    return (
        <div className={className}>
            {head}
            {detail && (
                <>
                    <button className="err__more" onClick={() => setOpen(!open)}>
                        {open ? 'Ocultar detalhes' : 'Detalhes'}
                    </button>
                    {open && <pre className="err__detail">{detail}</pre>}
                </>
            )}
        </div>
    )
}
