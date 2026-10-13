/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

/** A section heading: Roman numeral, hairline rule and label. */
export function SectionHead({ n, label }: { n: string; label: string }) {
    return (
        <div className="sl-shead">
            <span className="sl-shead__n">{n}</span>
            <span className="sl-shead__rule" />
            <span className="sl-shead__label">{label}</span>
        </div>
    )
}
