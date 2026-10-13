/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

import { useState } from 'react'

import { Account, headUrl } from '../api'

/** An account's avatar: the skin's head, or the name's initial when it cannot be loaded. */
export function Head({ account, size = 28 }: { account: Account; size?: number }) {
    // Offline accounts use the skin of whoever owns that nickname, if anyone does; otherwise the initial is shown.
    const [failed, setFailed] = useState(false)
    return (
        <div className="who__head" style={{ width: size, height: size }}>
            {failed ? (
                account.name.slice(0, 1).toUpperCase()
            ) : (
                <img src={headUrl(account.offline ? account.name : account.uuid, size * 2)} width={size} height={size} alt="" onError={() => setFailed(true)} />
            )}
        </div>
    )
}
