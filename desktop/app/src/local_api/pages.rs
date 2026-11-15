// SPDX-License-Identifier: GPL-3.0-or-later
use super::context::{allowed, try_node};
use super::protocol::{Action, Query};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    sync::{Mutex, OnceLock},
    time::Duration,
};
type Reply = (u16, &'static str, Value);
fn fail(key: &str) -> Reply {
    (409, "Conflict", json!({"ok":false,"error":key}))
}
fn success(data: Value) -> Reply {
    (200, "OK", data)
}
fn valid_query(p: &Query) -> bool {
    matches!(p.page.as_str(), "messages" | "calls" | "photos" | "apps" | "deck")
        && p.thread.len() <= 1024
        && p.album.as_ref().is_none_or(|a| a.len() <= 256)
        && p.before_id.len() <= 256
        && p.offset <= 5000
        && p.search.len() <= 256
        && p.limit.is_none_or(|n| (1..=96).contains(&n))
}
pub(super) async fn read(body: &[u8]) -> Reply {
    let p: Query = match serde_json::from_slice(body) {
        Ok(p) => p,
        Err(_) => return fail("invalid_json"),
    };
    if !valid_query(&p) {
        return fail("invalid_page");
    }
    match tokio::time::timeout(Duration::from_secs(20), read_inner(p)).await {
        Ok(Ok(data)) => success(data),
        Ok(Err(_)) => fail("unavailable"),
        Err(_) => fail("timeout"),
    }
}
async fn read_inner(p: Query) -> Result<Value, nectarlink_core::Error> {
    use nectarlink_core::Error;
    if p.page == "deck" {
        return super::deck::read();
    }
    let node = try_node().ok_or(Error::Offline)?;
    let limit = p.limit.unwrap_or(5).min(if p.page == "calls" { 6 } else { 7 });
    match p.page.as_str() {
        "messages" => super::messages::read(p, node, limit).await,
        "calls" => super::calls::read(p, node, limit).await,
        "photos" => super::photos::read(p, node, limit).await,
        "apps" => super::apps::read(p, node, limit).await,
        _ => Err(Error::Unsupported),
    }
}
fn valid_action(p: &Action) -> bool {
    p.confirm
        && (16..=128).contains(&p.request_id.len())
        && p.request_id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && p.id.len() <= 1024
        && p.text.len() <= 4096
        && p.recipients.len() <= 10
        && p.recipients
            .iter()
            .all(|s| !s.trim().is_empty() && s.len() <= 128 && !s.chars().any(char::is_control))
        && match p.action.as_str() {
            "sms" => !p.text.trim().is_empty() && !p.recipients.is_empty(),
            "reply" => p.id.starts_with("chat:") && !p.text.trim().is_empty(),
            "dial" => {
                !p.text.is_empty()
                    && p.text.len() <= 128
                    && p.text.chars().all(|c| c.is_ascii_digit() || "+*# ()-".contains(c))
            }
            "photo" | "app" | "deck" | "answer" | "decline" | "silence" => !p.id.is_empty(),
            _ => false,
        }
}
// Retain identifiers even after failures/timeouts: no automatic retry or
// duplicate dispatch of uncertain, consequential operations. Fail closed at cap.
static REQUESTS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
static COMMAND: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
pub(super) async fn act(body: &[u8]) -> Reply {
    let p: Action = match serde_json::from_slice(body) {
        Ok(p) => p,
        Err(_) => return fail("invalid_json"),
    };
    if !valid_action(&p) {
        return fail("invalid_action");
    }
    {
        let mut seen = REQUESTS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
        if seen.contains(&p.request_id) {
            return fail("duplicate_request");
        }
        if seen.len() >= 10000 {
            return fail("request_limit_restart_required");
        }
        seen.insert(p.request_id.clone());
    }
    let Ok(_guard) = COMMAND.try_lock() else {
        return fail("busy");
    };
    match tokio::time::timeout(Duration::from_secs(20), act_inner(p)).await {
        Ok(Ok(data)) => success(data),
        Ok(Err(_)) => fail("action_failed_check_phone"),
        Err(_) => fail("uncertain_check_phone"),
    }
}
async fn act_inner(p: Action) -> Result<Value, nectarlink_core::Error> {
    use nectarlink_core::Error;
    let device = p.device;
    if p.action == "deck" {
        return super::deck::act(p).await;
    }
    let node = try_node().ok_or(Error::Offline)?;
    let feature = match p.action.as_str() {
        "sms" => "messages.sms",
        "reply" => "notifications.reply",
        "dial" => "calls.dial",
        "photo" => "files.recent_photos",
        "app" => "mirroring.app_windows",
        _ => "calls.control",
    };
    if !allowed(device, feature) {
        return Err(Error::Denied);
    }
    match p.action.as_str() {
        "sms" => super::messages::send(p, node).await,
        "reply" => super::messages::reply(p, node).await,
        "dial" => super::calls::dial(p, node).await,
        "photo" => super::photos::fetch(p, node).await,
        "app" => super::apps::launch(p, node).await,
        _ => super::calls::control(p, node).await,
    }
}
#[cfg(test)]
mod tests {
    use super::super::protocol::sized_window;
    use super::super::{cache::cached_list, deck::deck_view};
    use super::*;
    use nectarlink_core::DeviceId;
    fn window(items: Vec<Value>, offset: usize) -> Value {
        sized_window(items, offset, 5)
    }
    #[tokio::test]
    async fn readonly_lists_reuse_results_and_force_refresh_without_phone_actions() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let slot = OnceLock::new();
        let calls = AtomicUsize::new(0);
        let device = DeviceId([3; 32]);
        for force in [false, false, true, false] {
            let rows = cached_list(&slot, device, force, Duration::from_secs(60), async {
                calls.fetch_add(1, Ordering::Relaxed);
                Ok(vec![1, 2, 3])
            })
            .await
            .unwrap();
            assert_eq!(rows, vec![1, 2, 3]);
        }
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        for n in [4, 5] {
            cached_list(&slot, DeviceId([n; 32]), false, Duration::from_secs(60), async { Ok(vec![9]) })
                .await
                .unwrap();
        }
        assert_eq!(slot.get().unwrap().lock().await.len(), 2);
        let json = json!({"device":device.to_string(),"page":"messages","limit":7,"force":true});
        let q: Query = serde_json::from_value(json).unwrap();
        assert!(valid_query(&q) && q.force && q.limit == Some(7));
        let app_query: Query =
            serde_json::from_value(json!({"device":device.to_string(),"page":"apps","limit":15})).unwrap();
        assert!(valid_query(&app_query));
        for limit in [40, 44, 70, 75, 96] {
            let q: Query =
                serde_json::from_value(json!({"device":device.to_string(),"page":"apps","limit":limit}))
                    .unwrap();
            assert!(valid_query(&q));
            let window = sized_window((0..200).map(|i| json!({"id":i})).collect(), limit, limit);
            assert_eq!(window["items"].as_array().unwrap().len(), limit);
            assert_eq!(window["items"][0]["id"], limit);
            assert_eq!(window["more"], true);
        }
        for limit in [0, 97, 10000] {
            let q: Query =
                serde_json::from_value(json!({"device":device.to_string(),"page":"calls","limit":limit}))
                    .unwrap();
            assert!(!valid_query(&q));
        }
    }
    #[test]
    fn gallery_apps_and_complete_deck_have_explicit_bounds() {
        let rows = || (0..40).map(|i| json!({"id":i})).collect::<Vec<_>>();
        let gallery = sized_window(rows(), 0, 8);
        assert_eq!(gallery["items"].as_array().unwrap().len(), 8);
        assert_eq!(gallery["more"], true);
        let apps = sized_window(rows(), 12, 12);
        assert_eq!(apps["items"][0]["id"], 12);
        assert_eq!(apps["items"].as_array().unwrap().len(), 12);
        let dense_apps = sized_window(rows(), 15, 15);
        assert_eq!(dense_apps["items"][0]["id"], 15);
        assert_eq!(dense_apps["items"].as_array().unwrap().len(), 15);
        let deck = deck_view((0..192).map(|i| json!({"id":i})).collect());
        assert_eq!(deck["items"].as_array().unwrap().len(), 192);
        assert_eq!(deck["complete"], true);
        assert_eq!(deck["more"], false);
        assert_eq!(deck_view((0..250).map(|i| json!(i)).collect())["items"].as_array().unwrap().len(), 192);
    }
    #[test]
    fn display_ids_and_bounded_queries() {
        let device = DeviceId([7; 32]).to_string();
        for page in ["messages", "calls", "photos", "apps", "deck"] {
            let p: Query = serde_json::from_value(json!({"device":device,"page":page})).unwrap();
            assert!(valid_query(&p));
        }
        assert!(serde_json::from_value::<Query>(json!({"device":DeviceId([7;32]),"page":"calls"})).is_err());
        assert_eq!(window((0..12).map(|i| json!(i)).collect(), 5)["items"], json!([5, 6, 7, 8, 9]));
    }
    #[tokio::test]
    async fn malformed_actions_never_dispatch() {
        assert_eq!(act(br#"{}"#).await.2["error"], "invalid_json");
        let device = DeviceId([7; 32]).to_string();
        let body=serde_json::to_vec(&json!({"device":device,"action":"dial","request_id":"test-request-0001","confirm":false,"text":"123"})).unwrap();
        assert_eq!(act(&body).await.2["error"], "invalid_action");
        let mut p: Value = serde_json::from_slice(&body).unwrap();
        p["confirm"] = json!(true);
        let body = serde_json::to_vec(&p).unwrap();
        assert_eq!(act(&body).await.2["ok"], false);
        assert_eq!(act(&body).await.2["error"], "duplicate_request");
    }
}
