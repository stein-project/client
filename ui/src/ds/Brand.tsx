/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

import sl from '../assets/sl.png'
import steinClient from '../assets/stein-client.png'

/** The Stein Client wordmark (blackletter). Height is fixed; width follows the aspect ratio. */
export function Logo({ height, className = '' }: { height: number; className?: string }) {
    return <img className={'brand-logo ' + className} src={steinClient} height={height} alt="Stein Client" draggable={false} />
}

/** The SL monogram, the icon shared by the Stein family. */
export function Monogram({ size }: { size: number }) {
    return <img className="brand-sl" src={sl} width={size} height={size} alt="" draggable={false} />
}
