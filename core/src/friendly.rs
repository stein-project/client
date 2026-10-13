/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

//! The error message the player reads. The full error (the anyhow chain, with paths and URLs) goes on the second
//! line, which the UI shows under "Details", and to the log; the first line says what happened and what to do.

/// "Message for the player\ntechnical detail".
pub fn message(e: &anyhow::Error) -> String {
    format!("{}\n{e:#}", headline(e))
}

fn headline(e: &anyhow::Error) -> &'static str {
    for cause in e.chain() {
        if let Some(r) = cause.downcast_ref::<reqwest::Error>() {
            if r.is_timeout() || r.is_connect() || r.is_request() || r.is_body() || r.is_decode() {
                return "Sem conexão com os servidores de download. Verifique sua internet e tente novamente.";
            }
            if r.is_status() {
                return "Um servidor de download não respondeu como esperado. Tente novamente em alguns minutos.";
            }
        }
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            use std::io::ErrorKind::*;
            match io.kind() {
                PermissionDenied => {
                    return "Sem permissão para gravar os arquivos do jogo. Feche o Minecraft, se estiver aberto, e tente novamente.";
                }
                StorageFull => return "Não há espaço livre suficiente no disco.",
                _ => {}
            }
            // 32: ERROR_SHARING_VIOLATION (the file is open in another program, antivirus included).
            if io.raw_os_error() == Some(32) {
                return "Um arquivo do jogo está em uso por outro programa. Feche o Minecraft, se estiver aberto, e tente novamente.";
            }
        }
    }
    "Não foi possível preparar o jogo. Tente novamente; se o problema continuar, envie os logs (Ajustes › Abrir logs)."
}

/// Same as [`message`], also appending the full error to `logs/<file>` (launcher.log) for whoever helps.
pub fn logged(root: &crate::paths::Root, file: &str, e: &anyhow::Error) -> String {
    use std::io::Write;
    let _ = std::fs::create_dir_all(root.logs());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(root.logs().join(file)) {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let _ = writeln!(f, "[{secs}] Stein Client {}: {e:?}", crate::VERSION);
    }
    message(e)
}
