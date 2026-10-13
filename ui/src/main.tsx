/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

import React from 'react'
import ReactDOM from 'react-dom/client'

import { App } from './App'
import './app.css'

// The browser's right-click menu makes no sense in an app window.
window.addEventListener('contextmenu', (e) => e.preventDefault())

ReactDOM.createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
        <App />
    </React.StrictMode>,
)
