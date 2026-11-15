/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

import { ReactNode } from 'react'

/**
 * The little Markdown the changelogs use: headings, paragraphs, "-" lists (with continuation lines), **bold** and
 * `code`. Built as elements, never as HTML, so nothing in a changelog can run in the launcher; links show as their text.
 */
export function Markdown({ text }: { text: string }) {
    const blocks: ReactNode[] = []
    let para: string[] = []
    let list: string[] = []
    const flush = () => {
        if (para.length) blocks.push(<p key={blocks.length}>{inline(para.join(' '))}</p>)
        if (list.length)
            blocks.push(
                <ul key={blocks.length}>
                    {list.map((item, i) => (
                        <li key={i}>{inline(item)}</li>
                    ))}
                </ul>,
            )
        para = []
        list = []
    }
    for (const raw of text.split(/\r?\n/)) {
        const line = raw.trim()
        const heading = /^#{1,6}\s+(.*)$/.exec(line)
        if (!line) {
            flush()
        } else if (heading) {
            flush()
            blocks.push(<h3 key={blocks.length}>{inline(heading[1])}</h3>)
        } else if (/^[-*]\s+/.test(line)) {
            if (para.length) flush()
            list.push(line.replace(/^[-*]\s+/, ''))
        } else if (list.length && /^\s{2,}/.test(raw)) {
            list[list.length - 1] += ' ' + line
        } else {
            if (list.length) flush()
            para.push(line)
        }
    }
    flush()
    return <div className="md">{blocks}</div>
}

/** **bold**, `code` and [text](link) inside a line. */
function inline(s: string): ReactNode[] {
    const out: ReactNode[] = []
    const re = /\*\*([^*]+)\*\*|`([^`]+)`|\[([^\]]+)\]\([^)]*\)/g
    let last = 0
    let m: RegExpExecArray | null
    while ((m = re.exec(s))) {
        if (m.index > last) out.push(s.slice(last, m.index))
        if (m[1] !== undefined) out.push(<strong key={out.length}>{m[1]}</strong>)
        else if (m[2] !== undefined) out.push(<code key={out.length}>{m[2]}</code>)
        else out.push(m[3])
        last = re.lastIndex
    }
    if (last < s.length) out.push(s.slice(last))
    return out
}
