// SPDX-License-Identifier: GPL-3.0-or-later
use super::context::{resolve_target_device, try_node};
use serde::Deserialize;
use serde_json::{Value, json};
#[cfg(test)]
use std::sync::Mutex;
#[cfg(test)]
pub(super) static TEST_CLIPBOARD: Mutex<String> = Mutex::new(String::new());

pub(super) fn read_pc_clipboard() -> String {
    #[cfg(test)]
    {
        return TEST_CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner()).clone();
    }
    #[cfg(not(test))]
    {
        match crate::win::clipboard::read() {
            crate::win::clipboard::Clip::Text { text, .. } => text,
            _ => String::new(),
        }
    }
}

pub(super) fn write_pc_clipboard(text: &str) -> Result<(), String> {
    #[cfg(test)]
    {
        *TEST_CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner()) = text.to_owned();
        Ok(())
    }
    #[cfg(not(test))]
    {
        crate::win::clipboard::write(text)
    }
}

pub(super) fn handle_get_clipboard() -> (u16, &'static str, Value) {
    let text = read_pc_clipboard();
    let history = try_node()
        .map(|node| {
            node.clipboard_history(None)
                .into_iter()
                .take(20)
                .map(|item| {
                    let kind = match item.kind {
                        nectarlink_core::ClipboardItemKind::Text => "text",
                        nectarlink_core::ClipboardItemKind::Image => "image",
                    };
                    json!({
                        "id": item.id,
                        "kind": kind,
                        "preview": item.text,
                        "deviceName": item.device_name,
                        "incoming": item.incoming,
                        "timestamp": item.timestamp,
                        "pinned": item.pinned,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    (200, "OK", json!({ "ok": true, "text": text, "history": history }))
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct ClipboardPayload {
    text: String,
    #[serde(default)]
    device: Option<String>,
}

pub(super) async fn handle_post_clipboard(body: &[u8]) -> (u16, &'static str, Value) {
    let payload: ClipboardPayload = match serde_json::from_slice(body) {
        Ok(p) => p,
        Err(_) => return (400, "Bad Request", json!({ "ok": false, "error": "invalid_json" })),
    };
    if let Err(e) = write_pc_clipboard(&payload.text) {
        return (500, "Internal Server Error", json!({ "ok": false, "error": e }));
    }
    if let (Some(device), Some(node)) = (resolve_target_device(payload.device.as_deref()), try_node()) {
        let _ = node.send_clipboard(device, payload.text).await;
    }
    (200, "OK", json!({ "ok": true }))
}
