// SPDX-License-Identifier: GPL-3.0-or-later
use super::context::{resolve_target_device, try_node};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
#[derive(Debug, Default, Deserialize)]
pub(super) struct SendPayload {
    #[serde(default)]
    device: Option<String>,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

pub(super) async fn handle_post_send(body: &[u8]) -> (u16, &'static str, Value) {
    let payload: SendPayload = match serde_json::from_slice(body) {
        Ok(p) => p,
        Err(_) => return (400, "Bad Request", json!({ "ok": false, "error": "invalid_json" })),
    };
    if payload.paths.is_empty()
        && payload.text.as_deref().unwrap_or("").trim().is_empty()
        && payload.url.as_deref().unwrap_or("").trim().is_empty()
    {
        return (400, "Bad Request", json!({ "ok": false, "error": "empty_send_payload" }));
    }

    let target = resolve_target_device(payload.device.as_deref());
    if let (Some(device), Some(node)) = (target, try_node()) {
        if !payload.paths.is_empty() {
            let paths: Vec<PathBuf> = payload.paths.iter().map(PathBuf::from).collect();
            if let Ok(Ok(outgoing)) =
                tokio::task::spawn_blocking(move || nectarlink_core::outgoing_paths(&paths)).await
                && !outgoing.is_empty()
            {
                let _ = node.send_files(device, outgoing).await;
            }
        }
        if let Some(url) = payload.url.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            crate::links::send_to(device, url.to_owned());
        }
        if let Some(text) = payload.text.as_deref().filter(|s| !s.is_empty()) {
            let _ = node.send_clipboard(device, text.to_owned()).await;
        }
    }

    (200, "OK", json!({ "ok": true }))
}
