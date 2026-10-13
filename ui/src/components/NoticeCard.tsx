/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

import { api } from '../api'
import { Icon } from '../ds/Icon'

/** The one-time notice from the Rust side; dismissing it clears it for good and refreshes the state. */
export function NoticeCard({ notice, refresh }: { notice: string; refresh: () => void }) {
    return (
        <div className="card notice" role="status">
            <Icon name="check" size={18} stroke={1.4} />
            <div className="notice__text">{notice}</div>
            <button className="crash__close" onClick={() => api.dismissNotice().then(refresh)} aria-label="Fechar aviso">
                <Icon name="x" size={14} stroke={1.4} />
            </button>
        </div>
    )
}
