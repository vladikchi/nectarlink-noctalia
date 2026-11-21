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
    net::SocketAddr,
    path::Path,
    sync::{Mutex, OnceLock},
};
use tokio::sync::oneshot;
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
mod protocol;
mod sessions;
mod transfers;
// Preserve the pre-refactor Rust facade, including helpers used by integrations/tests.
#[allow(unused_imports)]
pub use auth::{
    ApiTokenFile, constant_time_eq, ensure_token_file, generate_token, read_token_file, remove_token_file,
    token_file_path, token_file_path_in, write_token_file,
};
use http::HttpRequest;
pub use http::start_on_port;
/// Default loopback TCP port (`127.0.0.1:53320`).
pub const DEFAULT_PORT: u16 = DEFAULT_LOCAL_API_PORT;

/// File name inside `<data_dir>` storing the active port and 256-bit bearer token.
pub const TOKEN_FILE_NAME: &str = "api-token.json";

/// Handle to a running loopback API server instance.
#[derive(Debug)]
pub struct ServerHandle {
    addr: SocketAddr,
    port: u16,
    shutdown: Option<oneshot::Sender<()>>,
}

impl ServerHandle {
    /// Returns the bound loopback socket address (`127.0.0.1:<port>`).
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Stops the server loop.
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

static RUNNING: OnceLock<Mutex<Option<ServerHandle>>> = OnceLock::new();

fn running_slot() -> &'static Mutex<Option<ServerHandle>> {
    RUNNING.get_or_init(|| Mutex::new(None))
}

/// Synchronizes the running localhost API server with `desktop-settings.json`.
pub fn sync_from_settings() {
    let data_dir = crate::core_host::host().data_dir.clone();
    let settings = Settings::load(&data_dir);
    apply_settings(&data_dir, &settings);
}

/// Initializes the localhost API server from `settings` on startup.
pub fn init(settings: &Settings) {
    let data_dir = crate::core_host::host().data_dir.clone();
    apply_settings(&data_dir, settings);
}

/// Stops the running localhost API server, if any.
pub fn stop() {
    if let Ok(mut guard) = running_slot().lock() {
        *guard = None;
    }
}

fn apply_settings(data_dir: &Path, settings: &Settings) {
    let port = if settings.local_api_port == 0 { DEFAULT_PORT } else { settings.local_api_port };
    if !settings.local_api_enabled {
        stop();
        remove_token_file(data_dir);
        return;
    }
    if let Ok(guard) = running_slot().lock()
        && let Some(active) = guard.as_ref()
        && active.port == port
        && read_token_file(data_dir).is_some()
    {
        return;
    }
    let token_file = match ensure_token_file(data_dir, port) {
        Ok(tf) => tf,
        Err(e) => {
            tracing::warn!(error = %e, "can't write api-token.json");
            return;
        }
    };
    let dir = data_dir.to_path_buf();
    crate::core_host::spawn(async move {
        match start_on_port(port, token_file.token.clone()).await {
            Ok((addr, handle)) => {
                let _ = write_token_file(&dir, addr.port(), &token_file.token);
                if let Ok(mut guard) = running_slot().lock() {
                    *guard = Some(handle);
                }
                tracing::info!(%addr, "started local automation API");
            }
            Err(e) => {
                tracing::warn!(port, error = %e, "can't bind local automation API");
            }
        }
    });
}

async fn route_request(req: HttpRequest) -> (u16, &'static str, Value) {
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/v1/devices") => devices::handle_get_devices(),
        ("GET", "/v1/notifications") => notifications::handle_get_notifications(),
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
    use super::*;
    use nectarlink_core::DeviceId;
    use serde_json::json;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpStream,
    };

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
        for page in ["messages", "calls", "photos", "apps", "deck"] {
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

        // 6. Valid POST /v1/ring, POST /v1/camera, POST /v1/notify, POST /v1/send, GET /v1/timeline.
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
