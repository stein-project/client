/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

//! Progress of a long-running step, for the UI (or the command line).

use serde::Serialize;

/// Where preparation stands: a named step and, when it can be counted, how much of it is done.
#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    /// The step, in a few words ("Baixando o Minecraft 1.8.9").
    pub step: String,
    /// How much of the step is done, from 0 to `total` (0/0 = not counted).
    pub done: u64,
    pub total: u64,
    /// An optional detail (the file, the speed).
    pub detail: String,
}

impl Progress {
    pub fn new(step: impl Into<String>) -> Self {
        Progress { step: step.into(), done: 0, total: 0, detail: String::new() }
    }

    pub fn count(step: impl Into<String>, done: u64, total: u64) -> Self {
        Progress { step: step.into(), done, total, detail: String::new() }
    }
}

/// Progress receiver. Cloneable and cheap to call: it is invoked many times during a download.
pub type Report = std::sync::Arc<dyn Fn(Progress) + Send + Sync>;
