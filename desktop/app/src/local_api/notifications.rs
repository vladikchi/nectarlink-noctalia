// SPDX-License-Identifier: GPL-3.0-or-later
use super::context::{resolve_target_device, try_node, with_state};
use nectarlink_core::TaskNotify;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};
#[derive(Debug, Default, Deserialize)]
pub(super) struct NotifyPayload {
    title: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    device: Option<String>,
}

pub(super) async fn handle_post_notify(body: &[u8]) -> (u16, &'static str, Value) {
    let payload: NotifyPayload = match serde_json::from_slice(body) {
        Ok(p) => p,
        Err(_) => return (400, "Bad Request", json!({ "ok": false, "error": "invalid_json" })),
    };
    if payload.title.trim().is_empty() {
        return (400, "Bad Request", json!({ "ok": false, "error": "missing_title" }));
    }
    let full_title = match payload.body.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(b) => format!("{}: {b}", payload.title.trim()),
        None => payload.title.trim().to_owned(),
    };
    let task = TaskNotify {
        id: format!("api-{}", SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis())),
        title: full_title,
        active: false,
        elapsed_ms: 0,
        exit_code: Some(0),
    };
    if let Some(node) = try_node() {
        let target = resolve_target_device(payload.device.as_deref());
        let _ = node.task_notify(target, task).await;
    }
    (200, "OK", json!({ "ok": true }))
}

pub(super) fn handle_get_notifications() -> (u16, &'static str, Value) {
    let items = with_state(|s| {
        let mut visible = s
            .notifications
            .iter()
            .filter(|view| s.app_rule(&view.notification.app) != crate::state::AppRule::Hidden)
            .collect::<Vec<_>>();
        visible.sort_by_key(|view| std::cmp::Reverse(view.notification.when));
        visible
            .into_iter()
            .take(100)
            .map(|view| {
                let n = &view.notification;
                json!({
                    "deviceId": view.device.to_string(),
                    "key": n.key,
                    "app": n.app,
                    "appName": n.app_name,
                    "title": n.title,
                    "text": n.text,
                    "timestamp": n.when,
                    "ongoing": n.ongoing,
                })
            })
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
    (200, "OK", json!({ "ok": true, "notifications": items }))
}

pub(super) fn handle_get_timeline(query: &HashMap<String, String>) -> (u16, &'static str, Value) {
    let limit = query
        .get("limit")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(20)
        .clamp(1, nectarlink_core::MAX_TIMELINE_PAGE_LIMIT);
    let search = query.get("q").map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());

    let items = try_node()
        .and_then(|node| {
            node.timeline_page(&nectarlink_core::TimelineQuery {
                kind: None,
                device: None,
                search,
                offset: 0,
                limit,
            })
            .ok()
        })
        .map(|page| {
            page.entries
                .into_iter()
                .map(|entry| {
                    json!({
                        "id": entry.id,
                        "kind": entry.kind.as_str(),
                        "direction": if entry.incoming { "incoming" } else { "outgoing" },
                        "deviceId": entry.device_id.to_string(),
                        "timestamp": entry.timestamp,
                        "title": entry.title,
                        "detail": entry.detail,
                        "target": entry.target,
                        "sizeBytes": entry.size_bytes,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    (200, "OK", json!({ "ok": true, "items": items }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notifications_without_core_are_an_empty_live_feed() {
        let (code, _, body) = handle_get_notifications();
        assert_eq!(code, 200);
        assert_eq!(body["ok"], true);
        assert_eq!(body["notifications"], json!([]));
    }
}
