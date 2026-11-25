// SPDX-License-Identifier: GPL-3.0-or-later
//! Local authenticated HTTP/JSON automation API (`127.0.0.1:<port>`, default
//! `53320`, off by default): lets local tools (such as the NectarLink PowerToys
//! Command Palette / Run extension, Raycast scripts, AutoHotkey, or CLI scripts)
//! list paired phones, send files/links/clipboard text, trigger Continuity
//! Camera, ring a phone, or read recent timeline and clipboard items.
//!
//! Security model:
//! - Strictly bound to loopback (`127.0.0.1`).
//! - Every request must carry `Authorization: Bearer <token>` matching the
//!   256-bit random hex secret in `<data_dir>/api-token.json` (verified in
//!   constant time).
//! - Any request carrying an `Origin` header is rejected with `403 Forbidden`
//!   so web pages opened in a browser cannot invoke the loopback API.

// Public facade: lifecycle and routing only. Domain code lives in local_api/.
use crate::settings::{DEFAULT_LOCAL_API_PORT, Settings};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
mod apps;
mod auth;
mod cache;
mod calls;
mod clipboard;
mod context;
mod deck;
mod devices;
mod http;
mod messages;
mod notifications;
mod pages;
mod photos;
mod previews;
mod privacy;
mod protocol;
mod sessions;
mod transfers;
// Preserve the pre-refactor Rust facade, including helpers used by integrations/tests.
#[allow(unused_imports)]
pub use auth::{
    ApiTokenFile, constant_time_eq, ensure_token_file, generate_token, read_token_file, remove_token_file,
    token_file_path, token_file_path_in, write_token_file,
};
pub(crate) use clipboard::{read_pc_clipboard, write_pc_clipboard};
use context::{try_node, with_state};
pub(crate) use http::{HttpRequest, bearer_token, read_http_request, write_json_response};
use nectarlink_core::DeviceId;
/// Default loopback TCP port (`127.0.0.1:53320`).
pub const DEFAULT_PORT: u16 = DEFAULT_LOCAL_API_PORT;

/// File name inside `<data_dir>` storing the active port and 256-bit bearer token.
pub const TOKEN_FILE_NAME: &str = "api-token.json";

struct Running {
    server: http::ServerHandle,
    data_dir: PathBuf,
    token: Arc<str>,
}

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);

/// Stops the running localhost API server, if any.
pub fn stop() {
    let mut running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
    set_private_access(false);
    stop_listener(&mut running);
}

// The low bit is the current switch; every transition changes the epoch.
// A disable/re-enable while a read is waiting must not revive that read.
static PRIVATE_ACCESS: AtomicU64 = AtomicU64::new(0);

fn set_private_access(allowed: bool) {
    let bit = u64::from(allowed);
    let mut old = PRIVATE_ACCESS.load(Ordering::Acquire);
    while old & 1 != bit {
        match PRIVATE_ACCESS.compare_exchange_weak(
            old,
            old.wrapping_add(1),
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => break,
            Err(current) => old = current,
        }
    }
}

fn private_epoch() -> Option<u64> {
    let state = PRIVATE_ACCESS.load(Ordering::Acquire);
    (state & 1 != 0).then_some(state)
}

fn stop_listener(running: &mut Option<Running>) {
    if let Some(active) = running.take() {
        active.server.cancel();
        // Withdraw the credentials before releasing the port.
        remove_token_file(&active.data_dir);
        drop(active);
    }
}

fn sync_listener(
    running: &mut Option<Running>,
    data_dir: &Path,
    port: Option<u16>,
) -> std::io::Result<Option<SocketAddr>> {
    let previous = port.and_then(|_| read_token_file(data_dir));
    if let (Some(port), Some(active), Some(previous)) = (port, running.as_ref(), previous.as_ref())
        && active.data_dir == data_dir
        && active.server.addr.port() == port
        && previous.port == port
        && active.token.as_ref() == previous.token
    {
        return Ok(None);
    }
    // Fail closed on an invalid transition, rather than leaving credentials
    // for an unbound port or leaving the old listener active.
    stop_listener(running);
    auth::clear_token_file(data_dir)?;
    let Some(port) = port else { return Ok(None) };
    if !privacy::ready() {
        return Err(std::io::Error::other("no secure random source for notification scopes"));
    }
    let token: Arc<str> = previous
        .map(|cfg| cfg.token)
        .or_else(generate_token)
        .ok_or_else(|| std::io::Error::other("no secure random source for the API token"))?
        .into();
    let socket = http::bind(port)?;
    let addr = socket.local_addr()?;
    write_token_file(data_dir, addr.port(), &token)?;
    *running =
        Some(Running { server: http::serve(socket, addr, token.clone()), data_dir: data_dir.into(), token });
    Ok(Some(addr))
}

/// Applies the current preferences at startup or on the UI thread. The
/// background settings writer must not reapply an older snapshot to the API.
pub fn apply_settings(settings: &Settings) {
    let host = crate::core_host::host();
    let mut running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
    let port = settings.local_api_enabled.then_some(if settings.local_api_port == 0 {
        DEFAULT_PORT
    } else {
        settings.local_api_port
    });
    let private = port.is_some() && settings.local_api_private;
    if !private {
        set_private_access(false);
    }
    // A numeric bind and token publication run under the same controller lock.
    // There is no detached startup that could commit after a later disable.
    let _runtime = host.runtime.enter();
    match sync_listener(&mut running, &host.data_dir, port) {
        Ok(addr) => {
            set_private_access(private);
            if let Some(addr) = addr {
                tracing::info!(%addr, "started local automation API");
            }
        }
        Err(e) => {
            set_private_access(false);
            tracing::warn!(?port, error = %e, "can't start local automation API");
        }
    }
}

const SMS_THREADS: u32 = 60;
const SMS_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

fn not_allowed() -> (u16, &'static str, Value) {
    (403, "Forbidden", json!({"ok": false, "error": "not_allowed"}))
}

/// The paired device `?device=` names (an ID, an ID's start, or part of its
/// name); `Err` when it names none. No `?device=` is `Ok(None)`: all devices.
fn device_filter(query: &HashMap<String, String>) -> Result<Option<DeviceId>, ()> {
    let Some(q) = query.get("device").map(|s| s.trim()).filter(|s| !s.is_empty()) else { return Ok(None) };
    if let Ok(id) = q.parse::<DeviceId>() {
        return Ok(Some(id));
    }
    let lower = q.to_ascii_lowercase();
    with_state(|s| {
        s.devices
            .iter()
            .find(|d| {
                d.id.to_string().starts_with(&lower) || d.info.name.to_ascii_lowercase().contains(&lower)
            })
            .map(|d| d.id)
    })
    .flatten()
    .map(Some)
    .ok_or(())
}

fn unknown_device() -> (u16, &'static str, Value) {
    (404, "Not Found", json!({ "ok": false, "error": "unknown_device" }))
}

/// `GET /v1/messages`: conversations, newest first, with unread counts: SMS
/// and MMS from each connected phone, and chats from notifications (WhatsApp,
/// Signal...) kept on this PC.
async fn handle_get_messages(query: &HashMap<String, String>) -> (u16, &'static str, Value) {
    let Some(epoch) = private_epoch() else { return not_allowed() };
    let Ok(device) = device_filter(query) else { return unknown_device() };
    let devices = with_state(|s| {
        s.devices
            .iter()
            .filter(|d| device.is_none_or(|id| d.id == id))
            .map(|d| {
                (
                    d.id,
                    d.info.os.eq_ignore_ascii_case("android")
                        && context::feature_access(s, d.id, "messages.sms") == Some(true),
                )
            })
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
    let mut batches = Vec::new();
    if let Some(node) = try_node() {
        for (id, ask_phone) in devices {
            let sms = if ask_phone {
                // Preserve the legacy partial-list behavior for unavailable SMS.
                tokio::time::timeout(SMS_WAIT, node.sms_threads(id, SMS_THREADS))
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            if private_epoch() != Some(epoch) {
                return not_allowed();
            }
            batches.push((id, node.chat_threads(id), sms));
        }
    }
    // Apply the current visibility rules after all phone I/O, and count only
    // rows that may actually be shown. Removed phones cannot return late rows.
    let (revision, mut threads, unread_total) = with_state(|state| {
        let mut rows = Vec::new();
        let mut unread_total = 0u64;
        for (id, chats, sms) in batches {
            if state.device(&id).is_none() {
                continue;
            }
            append_legacy_chats(state, id, chats, &mut rows, &mut unread_total);
            if context::feature_access(state, id, "messages.sms") == Some(true) {
                for t in sms {
                    unread_total += u64::from(t.unread);
                    rows.push((
                        t.date,
                        json!({
                            "id": t.id,
                            "deviceId": id.to_string(),
                            "kind": "sms",
                            "app": "",
                            "appName": "SMS",
                            "title": crate::messages::title_of(&t),
                            "group": t.addresses.len() > 1,
                            "lastMessage": t.snippet,
                            "unread": t.unread,
                            "timestamp": t.date / 1000,
                        }),
                    ));
                }
            }
        }
        (Some(state.notification_visibility_revision), rows, unread_total)
    })
    .unwrap_or_default();
    threads.sort_by_key(|t| std::cmp::Reverse(t.0));
    let threads = threads.into_iter().map(|(_, t)| t).collect::<Vec<_>>();
    if private_epoch() != Some(epoch) {
        return not_allowed();
    }
    if revision != privacy::revision() {
        return (409, "Conflict", json!({"ok":false,"error":"notification_rules_changed"}));
    }
    (200, "OK", json!({ "ok": true, "unreadTotal": unread_total, "threads": threads }))
}

fn append_legacy_chats(
    state: &crate::state::AppState,
    device: DeviceId,
    chats: Vec<nectarlink_core::ChatThreadRecord>,
    rows: &mut Vec<(i64, Value)>,
    unread_total: &mut u64,
) {
    for t in chats.into_iter().filter(|t| privacy::visible(state, &t.app)) {
        *unread_total += u64::from(t.unread);
        rows.push((
            t.updated_ms,
            json!({
                "id":t.thread_id,"deviceId":device.to_string(),"kind":"chat","app":t.app,
                "appName":if t.app_name.trim().is_empty() { &t.app } else { &t.app_name },
                "title":t.title,"group":t.is_group,"lastMessage":t.snippet,
                "unread":t.unread,"timestamp":t.updated_ms / 1000,
            }),
        ));
    }
}

async fn route_request(req: HttpRequest) -> (u16, &'static str, Value) {
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/v1/devices") => devices::handle_get_devices(),
        ("GET", "/v1/notifications") => {
            let Some(epoch) = private_epoch() else { return not_allowed() };
            let reply = notifications::handle_get_notifications(&req.query);
            if private_epoch() == Some(epoch) { reply } else { not_allowed() }
        }
        ("GET", "/v1/messages") => handle_get_messages(&req.query).await,
        ("GET", "/v1/sessions") => sessions::get(),
        ("POST", "/v1/page") => pages::read(&req.body).await,
        ("POST", "/v1/page-action") => pages::act(&req.body).await,
        ("POST", "/v1/session") => sessions::set(&req.body).await,
        ("POST", "/v1/type") => sessions::type_text(&req.body).await,
        ("POST", "/v1/send") => transfers::handle_post_send(&req.body).await,
        ("GET", "/v1/clipboard") => clipboard::handle_get_clipboard(),
        ("POST", "/v1/clipboard") => clipboard::handle_post_clipboard(&req.body).await,
        ("POST", "/v1/notify") => notifications::handle_post_notify(&req.body).await,
        ("POST", "/v1/ring") => devices::handle_post_ring(&req.body).await,
        ("POST", "/v1/phone-toggle") => devices::handle_post_phone_toggle(&req.body).await,
        ("POST", "/v1/play-on-phone") => sessions::handle_post_play_on_phone(&req.body),
        ("POST", "/v1/camera") => sessions::handle_post_camera(&req.body),
        ("GET", "/v1/timeline") => notifications::handle_get_timeline(&req.query),
        _ => (404, "Not Found", json!({ "ok": false, "error": "not_found" })),
    }
}

#[cfg(test)]
mod tests {
    use super::http::start_on_port;
    use super::*;
    use nectarlink_core::DeviceId;
    use serde_json::json;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpStream,
    };

    #[tokio::test]
    async fn listener_publication_and_shutdown_follow_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let dir = dir.path();
        let mut running = None;
        let addr = sync_listener(&mut running, dir, Some(0)).unwrap().unwrap();
        assert!(addr.ip().is_loopback());
        assert_eq!(read_token_file(dir).unwrap().port, addr.port());
        assert!(sync_listener(&mut running, dir, Some(addr.port())).unwrap().is_none());
        let mut pending = TcpStream::connect(addr).await.unwrap();
        sync_listener(&mut running, dir, None).unwrap();
        assert!(running.is_none() && read_token_file(dir).is_none());
        let closed = tokio::time::timeout(std::time::Duration::from_secs(1), pending.read_u8()).await;
        assert!(matches!(closed, Ok(Err(ref e)) if matches!(
            e.kind(), std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::UnexpectedEof
        )));

        let occupied = http::bind(addr.port()).unwrap(); // Immediate rebind after stop.
        assert!(sync_listener(&mut running, dir, Some(addr.port())).is_err());
        assert!(running.is_none() && read_token_file(dir).is_none());
        drop(occupied);
        sync_listener(&mut running, dir, Some(addr.port())).unwrap();
        stop_listener(&mut running);
        assert!(read_token_file(dir).is_none());
    }

    #[test]
    fn legacy_messages_count_only_visible_chats() {
        use crate::state::{AppRule, AppState};
        let device = DeviceId([7; 32]);
        let mut state = AppState::default();
        state.set_app_rule("org.example.hidden", AppRule::Hidden);
        state.set_app_rule("org.example.quiet", AppRule::Quiet);
        let fixture = |app: &str, unread| nectarlink_core::ChatThreadRecord {
            peer: device,
            thread_id: format!("chat:{app}:fixture"),
            app: app.to_owned(),
            app_name: String::new(),
            notification_key: String::new(),
            reply_action_id: None,
            title: "Test conversation".into(),
            is_group: false,
            snippet: "Test text".into(),
            last_sender: None,
            updated_ms: 2_000,
            unread,
            active: false,
            avatar: None,
            app_icon: None,
        };
        let (mut rows, mut unread) = (Vec::new(), 0);
        append_legacy_chats(
            &state,
            device,
            vec![fixture("org.example.quiet", 2), fixture("org.example.hidden", 5)],
            &mut rows,
            &mut unread,
        );
        assert_eq!(unread, 2);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1["app"], "org.example.quiet");
        assert_eq!(rows[0].1["timestamp"], 2);
    }

    #[tokio::test]
    async fn plugin_wire_payloads_reach_session_and_typing_validation_over_http() {
        let token = generate_token().unwrap();
        let (addr, mut server) = start_on_port(0, token.clone()).await.unwrap();
        let id = DeviceId([7; 32]).to_string();
        for kind in ["screen", "keyboard", "pc_audio", "phone_audio", "watch", "webcam"] {
            let body = json!({"device":id,"kind":kind,"on":true}).to_string();
            let (code,data)=raw_http_call(addr,&format!("POST /v1/session HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{body}",body.len())).await;
            assert_eq!(code, 409, "display ID must parse, even without a running core");
            assert_eq!(data["error"], "no_device");
        }
        let body = json!({"device":id,"text":"Проверка 🦀"}).to_string();
        let (code,data)=raw_http_call(addr,&format!("POST /v1/type HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{body}",body.len())).await;
        assert_eq!(code, 409);
        assert_eq!(data["error"], "unavailable");
        server.stop();
    }

    async fn raw_http_call(addr: SocketAddr, raw_request: &str) -> (u16, Value) {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream.write_all(raw_request.as_bytes()).await.unwrap();
        stream.flush().await.unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).await.unwrap();
        let (head, body) = resp.split_once("\r\n\r\n").unwrap();
        let status_line = head.lines().next().unwrap();
        let code: u16 = status_line.split_whitespace().nth(1).unwrap().parse().unwrap();
        let json: Value = serde_json::from_str(body).unwrap();
        (code, json)
    }

    #[tokio::test]
    async fn all_post_routes_preserve_malformed_json_contract_without_dispatch() {
        let token = generate_token().unwrap();
        let (addr, mut server) = start_on_port(0, token.clone()).await.unwrap();
        for route in [
            "send",
            "clipboard",
            "notify",
            "ring",
            "phone-toggle",
            "play-on-phone",
            "camera",
            "session",
            "type",
            "page",
            "page-action",
        ] {
            let request = format!(
                "POST /v1/{route} HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\nContent-Length: 1\r\n\r\n{{"
            );
            let (status, body) = raw_http_call(addr, &request).await;
            assert_eq!(status, if matches!(route, "page" | "page-action") { 409 } else { 400 }, "{route}");
            assert_eq!(body["error"], "invalid_json", "{route}");
        }
        let (status, body) = raw_http_call(
            addr,
            &format!("GET /v1/unknown HTTP/1.1\r\nAuthorization: Bearer {token}\r\n\r\n"),
        )
        .await;
        assert_eq!(status, 404);
        assert_eq!(body["error"], "not_found");
        server.stop();
    }

    #[tokio::test]
    async fn sidebar_routes_accept_display_ids_without_real_actions() {
        let token = generate_token().unwrap();
        let (addr, mut handle) = start_on_port(0, token.clone()).await.unwrap();
        for page in ["calls", "photos", "apps", "deck"] {
            let payload = json!({"device":DeviceId([7;32]).to_string(),"page":page}).to_string();
            let (code, body) = raw_http_call(addr,&format!("POST /v1/page HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{payload}",payload.len())).await;
            assert_eq!(code, if page == "deck" { 200 } else { 409 });
            assert_ne!(body["error"], "invalid_json");
        }
        for action in ["sms", "reply", "dial", "photo", "app", "deck", "answer", "decline", "silence"] {
            let payload=json!({"device":DeviceId([7;32]).to_string(),"action":action,"request_id":"http-test-request-0001","confirm":false}).to_string();
            let (code, body) = raw_http_call(addr,&format!("POST /v1/page-action HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{payload}",payload.len())).await;
            assert_eq!(code, 409);
            assert_eq!(body["error"], "invalid_action");
        }
        handle.stop();
    }

    #[tokio::test]
    async fn enforces_bearer_auth_blocks_origin_header_and_serves_v1_routes() {
        let token = generate_token().expect("secure random");
        let (addr, mut handle) = start_on_port(0, token.clone()).await.unwrap();

        // 1. Missing Authorization header -> 401 Unauthorized.
        let (code, body) = raw_http_call(addr, "GET /v1/devices HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").await;
        assert_eq!(code, 401);
        assert_eq!(body["ok"], false);
        assert_eq!(body["error"], "unauthorized");

        // 2. Wrong Bearer token -> 401 Unauthorized.
        let (code, body) = raw_http_call(
            addr,
            "GET /v1/devices HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer deadbeef\r\n\r\n",
        )
        .await;
        assert_eq!(code, 401);
        assert_eq!(body["error"], "unauthorized");

        // 3. Request with Origin header (even with valid token) -> 403 Forbidden.
        let (code, body) = raw_http_call(
            addr,
            &format!(
                "GET /v1/devices HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: https://evil.example\r\nAuthorization: Bearer {token}\r\n\r\n"
            ),
        )
        .await;
        assert_eq!(code, 403);
        assert_eq!(body["ok"], false);
        assert_eq!(body["error"], "forbidden");

        // 4. Valid GET /v1/devices -> 200 OK.
        let (code, body) = raw_http_call(
            addr,
            &format!("GET /v1/devices HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\n\r\n"),
        )
        .await;
        assert_eq!(code, 200);
        assert_eq!(body["ok"], true);
        assert!(body["devices"].is_array());

        // 5. Valid POST /v1/clipboard and GET /v1/clipboard -> 200 OK.
        let clip_body = r#"{"text":"hello from local api"}"#;
        let (code, body) = raw_http_call(
            addr,
            &format!(
                "POST /v1/clipboard HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{clip_body}",
                clip_body.len()
            ),
        )
        .await;
        assert_eq!(code, 200);
        assert_eq!(body["ok"], true);

        let (code, body) = raw_http_call(
            addr,
            &format!(
                "GET /v1/clipboard HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\n\r\n"
            ),
        )
        .await;
        assert_eq!(code, 200);
        assert_eq!(body["ok"], true);
        assert_eq!(body["text"], "hello from local api");

        // 6. Notifications and messages stay closed until their own switch is on.
        for path in ["/v1/notifications", "/v1/messages"] {
            let (code, body) = raw_http_call(
                addr,
                &format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\n\r\n"),
            )
            .await;
            assert_eq!(code, 403, "{path}");
            assert_eq!(body["error"], "not_allowed", "{path}");
        }
        let page = json!({"device": DeviceId([7; 32]).to_string(), "page": "messages"}).to_string();
        let request = format!(
            "POST /v1/page HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{page}",
            page.len()
        );
        let (code, body) = raw_http_call(addr, &request).await;
        assert_eq!(code, 403);
        assert_eq!(body["error"], "not_allowed");
        // With it on, they answer (nothing paired in tests), and a device that
        // isn't paired is an error, not every device.
        set_private_access(true);
        let get = |path: &str| {
            format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\n\r\n")
        };
        let (code, body) = raw_http_call(addr, &get("/v1/notifications?limit=5")).await;
        assert_eq!(code, 200);
        assert_eq!(body["notifications"], json!([]));
        let (code, body) = raw_http_call(addr, &get("/v1/messages")).await;
        assert_eq!(code, 200);
        assert_eq!(body["unreadTotal"], 0);
        assert_eq!(body["threads"], json!([]));
        let (code, body) = raw_http_call(addr, &request).await;
        assert_eq!(code, 409); // The route reached the device check, with no paired core in tests.
        assert_eq!(body["error"], "unavailable");
        let (code, body) = raw_http_call(addr, &get("/v1/messages?device=nobody")).await;
        assert_eq!(code, 404);
        assert_eq!(body["error"], "unknown_device");
        let in_flight_epoch = private_epoch();
        set_private_access(false);
        assert!(private_epoch().is_none());
        set_private_access(true);
        assert_ne!(private_epoch(), in_flight_epoch, "an off/on cycle invalidates earlier private reads");
        set_private_access(false);

        // 7. Valid POST /v1/ring, POST /v1/camera, POST /v1/notify, POST /v1/send, GET /v1/timeline.
        let (code, _) = raw_http_call(
            addr,
            &format!(
                "POST /v1/ring HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\nContent-Length: 2\r\n\r\n{{}}"
            ),
        )
        .await;
        assert_eq!(code, 409); // No core/device in this isolated API test.

        let cam_body = r#"{"mode":"scan"}"#;
        let (code, body) = raw_http_call(
            addr,
            &format!(
                "POST /v1/camera HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{cam_body}",
                cam_body.len()
            ),
        )
        .await;
        assert_eq!(code, 409);
        assert_eq!(body["error"], "no_device");

        for (path, payload) in [
            ("ring", r#"{"device":"not-a-phone"}"#),
            ("camera", r#"{"device":"not-a-phone"}"#),
            ("notify", r#"{"device":"not-a-phone","title":"Test"}"#),
            ("send", r#"{"device":"not-a-phone","url":"https://nectarlink.dev"}"#),
            ("clipboard", r#"{"device":"not-a-phone","text":"Test"}"#),
        ] {
            let request = format!(
                "POST /v1/{path} HTTP/1.1\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{payload}",
                payload.len()
            );
            let (status, body) = raw_http_call(addr, &request).await;
            assert_eq!(status, 409, "{path}");
            assert_eq!(body["error"], "no_device", "{path}");
        }

        let notify_body = r#"{"title":"Build finished","body":"0 errors"}"#;
        let (code, _) = raw_http_call(
            addr,
            &format!(
                "POST /v1/notify HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{notify_body}",
                notify_body.len()
            ),
        )
        .await;
        assert_eq!(code, 200);

        let send_body = r#"{"url":"https://nectarlink.dev"}"#;
        let (code, _) = raw_http_call(
            addr,
            &format!(
                "POST /v1/send HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{send_body}",
                send_body.len()
            ),
        )
        .await;
        assert_eq!(code, 200);

        let (code, body) = raw_http_call(
            addr,
            &format!(
                "GET /v1/timeline?limit=10&q=test HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\n\r\n"
            ),
        )
        .await;
        assert_eq!(code, 200);
        assert_eq!(body["ok"], true);
        assert!(body["items"].is_array());

        handle.stop();
    }
}
