/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

import React from 'react'

/**
 * Bespoke geometric stroke marks: the Stein design system avoids icon libraries
 * (no emoji, no Lucide). Every mark follows the same minimal-stroke
 * construction on a 24x24 grid.
 */
export type IconName =
    | 'check' | 'layers' | 'shield' | 'down'
    | 'play' | 'restart' | 'force' | 'plus' | 'gear' | 'trash' | 'x' | 'list' | 'cards' | 'star'
    // Stein Client additions (same construction: stroke only, 24x24)
    | 'folder' | 'minus' | 'logout' | 'maximize' | 'restore'

const dot = (cx: number, cy: number, r: number) => (
    <circle cx={cx} cy={cy} r={r} fill="currentColor" stroke="none" />
)

const SHAPES: Record<string, React.ReactNode> = {
    // ── metrics ──
    check: (<><circle cx={12} cy={12} r={8.5} /><path d="M8.4 12.4l2.6 2.6 4.6-5.2" /></>),
    // ── nav ──
    layers: (<><polygon points="12,3 21,8 12,13 3,8" /><path d="M3 13l9 5 9-5" /></>),
    shield: (<><path d="M12 3 20 6.5v5.5c0 5-3.6 7.5-8 8.5-4.4-1-8-3.5-8-8.5V6.5z" /></>),
    // ── event marks ──
    down: (<><path d="M12 5v12" /><path d="M7 12.5l5 5 5-5" /></>),
    // ── controls / ui ──
    play: (<><polygon points="8,5 19,12 8,19" /></>),
    restart: (<>
        <path d="M4.5 9.5a8 8 0 0 1 13.5-2.5" /><path d="M18.5 4v3h-3" />
        <path d="M19.5 14.5a8 8 0 0 1-13.5 2.5" /><path d="M5.5 20v-3h3" />
    </>),
    force: (<><circle cx={12} cy={12} r={8} /><path d="M12 8v5" />{dot(12, 16, 0.9)}</>),
    plus: (<><line x1={12} y1={5} x2={12} y2={19} /><line x1={5} y1={12} x2={19} y2={12} /></>),
    gear: (<>
        <circle cx={12} cy={12} r={3.2} />
        <path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.2 5.2l2.1 2.1M16.7 16.7l2.1 2.1M18.8 5.2l-2.1 2.1M5.2 18.8l2.1-2.1" />
    </>),
    trash: (<>
        <path d="M5 7h14" /><path d="M8 7V5h8v2" /><path d="M6.5 7l1 13h9l1-13" />
    </>),
    x: (<><line x1={6} y1={6} x2={18} y2={18} /><line x1={18} y1={6} x2={6} y2={18} /></>),
    list: (<>
        <line x1={9} y1={7} x2={20} y2={7} /><line x1={9} y1={12} x2={20} y2={12} />
        <line x1={9} y1={17} x2={20} y2={17} />{dot(4.5, 7, 1)}{dot(4.5, 12, 1)}{dot(4.5, 17, 1)}
    </>),
    cards: (<>
        <rect x={4} y={4} width={6.5} height={6.5} rx={1} />
        <rect x={13.5} y={4} width={6.5} height={6.5} rx={1} />
        <rect x={4} y={13.5} width={6.5} height={6.5} rx={1} />
        <rect x={13.5} y={13.5} width={6.5} height={6.5} rx={1} />
    </>),
    star: (<><polygon points="12,3 14.5,9 21,9.5 16,14 17.5,20.5 12,17 6.5,20.5 8,14 3,9.5 9.5,9" /></>),
    // ── Stein Client ──
    folder: (<><path d="M3.5 7V18.5h17V9H11L9 6.5H3.5z" /><line x1={3.5} y1={11} x2={20.5} y2={11} /></>),
    minus: (<><line x1={6} y1={12} x2={18} y2={12} /></>),
    maximize: (<><rect x={6.5} y={6.5} width={11} height={11} /></>),
    restore: (<><rect x={6} y={9} width={9} height={9} /><path d="M9 9V6h9v9h-3" /></>),
    logout: (<><path d="M14 5H5v14h9" /><path d="M10 12h10M17 9l3 3-3 3" /></>),
}

export function Icon({
    name,
    size = 18,
    stroke = 1.4,
    className,
    style,
}: {
    name: IconName
    size?: number
    stroke?: number
    className?: string
    style?: React.CSSProperties
}) {
    return (
        <svg
            width={size}
            height={size}
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={stroke}
            strokeLinecap="round"
            strokeLinejoin="round"
            className={className}
            style={style}
            aria-hidden="true"
        >
            {SHAPES[name] ?? dot(12, 12, 1.6)}
        </svg>
    )
}
