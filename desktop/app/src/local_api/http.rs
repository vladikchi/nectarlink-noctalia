// SPDX-License-Identifier: GPL-3.0-or-later
use super::{auth::constant_time_eq, route_request};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    future::poll_fn,
    io,
    net::SocketAddr,
    sync::{Arc, Mutex},
    task::Poll,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpSocket, TcpStream},
    sync::watch,
    task::JoinSet,
    time::Instant,
};
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 256 * 1024;
const MAX_CONNECTIONS: usize = 32;
const IO_TIMEOUT: Duration = Duration::from_secs(5);
const HANDLER_TIMEOUT: Duration = super::pages::PAGE_TIMEOUT.saturating_add(Duration::from_secs(1));

#[derive(Debug)]
pub(super) struct ServerHandle {
    pub(super) addr: SocketAddr,
    listener: Arc<Mutex<Option<TcpListener>>>,
    stop: watch::Sender<bool>,
}

impl ServerHandle {
    pub(super) fn cancel(&self) {
        self.stop.send_if_modified(|stopped| {
            let changed = !*stopped;
            *stopped = true;
            changed
        });
    }

    pub(super) fn stop(&mut self) {
        self.cancel();
        // Drop the listening socket now, not on the next runtime tick. A
        // subsequent enable can bind the same port without waiting for tasks.
        self.listener.lock().unwrap_or_else(|e| e.into_inner()).take();
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Binds a numeric loopback address synchronously; requires a Tokio I/O context.
pub(super) fn bind(port: u16) -> io::Result<TcpListener> {
    let socket = TcpSocket::new_v4()?;
    #[cfg(unix)]
    socket.set_reuseaddr(true)?;
    socket.bind(SocketAddr::from(([127, 0, 0, 1], port)))?;
    socket.listen(128)
}

pub(super) fn serve(listener: TcpListener, addr: SocketAddr, token: Arc<str>) -> ServerHandle {
    let listener = Arc::new(Mutex::new(Some(listener)));
    let accepting = listener.clone();
    let (stop, mut stopped) = watch::channel(false);
    tokio::spawn(async move {
        let mut clients = JoinSet::new();
        loop {
            tokio::select! {
                biased;
                _ = cancelled(&mut stopped) => break,
                _ = clients.join_next(), if !clients.is_empty() => {},
                accepted = poll_fn(|cx| {
                    let guard = accepting.lock().unwrap_or_else(|e| e.into_inner());
                    match guard.as_ref() {
                        Some(socket) => socket.poll_accept(cx).map(Some),
                        None => Poll::Ready(None),
                    }
                }), if clients.len() < MAX_CONNECTIONS => {
                    let Some(accepted) = accepted else { break };
                    let Ok((stream, _)) = accepted else {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    };
                    let token = token.clone();
                    let mut stopping = stopped.clone();
                    clients.spawn(async move {
                        tokio::select! {
                            biased;
                            _ = cancelled(&mut stopping) => {},
                            _ = handle_connection(stream, &token) => {},
                        }
                    });
                }
            }
        }
        // Dropping the set aborts any remaining work. Per-client cancellation
        // also prevents a request waiting for input from starting on shutdown.
    });
    ServerHandle { addr, listener, stop }
}

async fn cancelled(stop: &mut watch::Receiver<bool>) {
    let _ = stop.wait_for(|stopped| *stopped).await;
}

#[cfg(test)]
pub(super) async fn start_on_port(port: u16, token: String) -> io::Result<(SocketAddr, ServerHandle)> {
    let listener = bind(port)?;
    let addr = listener.local_addr()?;
    Ok((addr, serve(listener, addr, token.into())))
}

#[derive(Debug)]
pub(crate) struct HttpRequest {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) query: HashMap<String, String>,
    pub(crate) headers: HashMap<String, String>,
    pub(crate) body: Vec<u8>,
}

struct RequestHead {
    method: String,
    target: String,
    headers: HashMap<String, String>,
    buffered_body: Vec<u8>,
}

pub(crate) fn bearer_token(value: &str) -> Option<&str> {
    let value = value.trim();
    value.get(..7)?.eq_ignore_ascii_case("bearer ").then(|| value[7..].trim())
}

fn invalid_request() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid HTTP request")
}

async fn read_head(stream: &mut TcpStream) -> io::Result<Option<RequestHead>> {
    let mut buf = Vec::with_capacity(2048);
    let mut chunk = [0u8; 2048];
    let mut checked = 0;
    let header_end = loop {
        if let Some(pos) = buf[checked..].windows(4).position(|w| w == b"\r\n\r\n") {
            break checked + pos;
        }
        if buf.len() >= MAX_HEADER_BYTES {
            return Err(invalid_request());
        }
        checked = buf.len().saturating_sub(3);
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return if buf.is_empty() { Ok(None) } else { Err(invalid_request()) };
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    if header_end + 4 > MAX_HEADER_BYTES {
        return Err(invalid_request());
    }
    let header_text = std::str::from_utf8(&buf[..header_end]).map_err(|_| invalid_request())?;
    let mut lines = header_text.split("\r\n");
    let mut parts = lines.next().unwrap_or_default().split_ascii_whitespace();
    let method = parts.next().ok_or_else(invalid_request)?.to_ascii_uppercase();
    let target = parts.next().ok_or_else(invalid_request)?.to_owned();
    if !matches!(parts.next(), Some("HTTP/1.0" | "HTTP/1.1")) || parts.next().is_some() {
        return Err(invalid_request());
    }

    let mut headers = HashMap::new();
    for line in lines {
        let (key, value) = line.split_once(':').ok_or_else(invalid_request)?;
        if key.is_empty()
            || !key.bytes().all(is_header_name_byte)
            || value.bytes().any(|b| b < 32 && b != b'\t' || b == 127)
        {
            return Err(invalid_request());
        }
        let key = key.to_ascii_lowercase();
        if matches!(key.as_str(), "authorization" | "content-length" | "host") && headers.contains_key(&key) {
            return Err(invalid_request());
        }
        headers.insert(key, value.trim().to_owned());
    }

    Ok(Some(RequestHead { method, target, headers, buffered_body: buf.split_off(header_end + 4) }))
}

fn is_header_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

fn body_length(headers: &HashMap<String, String>) -> io::Result<usize> {
    if headers.contains_key("transfer-encoding") {
        return Err(invalid_request());
    }
    match headers.get("content-length") {
        None => Ok(0),
        Some(raw) if !raw.is_empty() && raw.bytes().all(|b| b.is_ascii_digit()) => {
            raw.parse::<usize>().ok().filter(|&len| len <= MAX_BODY_BYTES).ok_or_else(invalid_request)
        }
        _ => Err(invalid_request()),
    }
}

async fn read_body(stream: &mut TcpStream, head: RequestHead) -> io::Result<HttpRequest> {
    let RequestHead { method, target, headers, buffered_body: mut body } = head;
    let content_len = body_length(&headers)?;
    if body.len() < content_len {
        let present = body.len();
        body.resize(content_len, 0);
        stream.read_exact(&mut body[present..]).await?;
    } else {
        body.truncate(content_len);
    }
    let (path, query) = parse_target(&target);
    Ok(HttpRequest { method, path, query, headers, body })
}

/// The agent gateway shares the bounded parser; it has its own access checks.
pub(crate) async fn read_http_request(stream: &mut TcpStream) -> io::Result<Option<HttpRequest>> {
    let Some(head) = read_head(stream).await? else { return Ok(None) };
    read_body(stream, head).await.map(Some)
}

pub(super) fn parse_target(target: &str) -> (String, HashMap<String, String>) {
    let mut query = HashMap::new();
    let (path, qs) = match target.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (target, None),
    };
    if let Some(qs) = qs {
        for pair in qs.split('&') {
            if pair.is_empty() {
                continue;
            }
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            query.insert(percent_decode(k), percent_decode(v));
        }
    }
    (path.to_owned(), query)
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (from_hex(bytes[i + 1]), from_hex(bytes[i + 2])) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        } else if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn from_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

pub(crate) async fn write_json_response(
    stream: &mut TcpStream,
    status_code: u16,
    status_text: &str,
    body: &Value,
) -> std::io::Result<()> {
    tokio::time::timeout(IO_TIMEOUT, write_response(stream, status_code, status_text, body)).await?
}

async fn write_response(
    stream: &mut TcpStream,
    status_code: u16,
    status_text: &str,
    body: &Value,
) -> std::io::Result<()> {
    let payload = serde_json::to_vec(body).unwrap_or_else(|_| b"{}".to_vec());
    let header = format!(
        "HTTP/1.1 {status_code} {status_text}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        payload.len()
    );
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(&payload).await?;
    stream.flush().await
}

async fn handle_connection(mut stream: TcpStream, expected_token: &str) -> std::io::Result<()> {
    let deadline = Instant::now() + IO_TIMEOUT;
    let head = match tokio::time::timeout_at(deadline, read_head(&mut stream)).await? {
        Ok(Some(head)) => head,
        Ok(None) => return Ok(()),
        Err(e) if e.kind() == io::ErrorKind::InvalidData => return bad_request(&mut stream).await,
        Err(e) => return Err(e),
    };

    // 1. Reject browser cross-origin requests carrying an `Origin` header.
    if head.headers.contains_key("origin") {
        return write_json_response(
            &mut stream,
            403,
            "Forbidden",
            &json!({ "ok": false, "error": "forbidden" }),
        )
        .await;
    }

    // 2. Verify `Authorization: Bearer <token>` in constant time.
    let provided = head.headers.get("authorization").and_then(|v| bearer_token(v)).unwrap_or("");
    if provided.is_empty() || !constant_time_eq(provided, expected_token) {
        return write_json_response(
            &mut stream,
            401,
            "Unauthorized",
            &json!({ "ok": false, "error": "unauthorized" }),
        )
        .await;
    }

    // No body allocation or route parsing before authentication. Don't
    // dispatch a body with invalid framing or without all declared bytes.
    let req = match tokio::time::timeout_at(deadline, read_body(&mut stream, head)).await? {
        Ok(req) => req,
        Err(e) if matches!(e.kind(), io::ErrorKind::InvalidData | io::ErrorKind::UnexpectedEof) => {
            return bad_request(&mut stream).await;
        }
        Err(e) => return Err(e),
    };
    let (status, reason, payload) = tokio::time::timeout(HANDLER_TIMEOUT, route_request(req))
        .await
        .unwrap_or_else(|_| (504, "Gateway Timeout", json!({ "ok": false, "error": "timeout" })));
    write_json_response(&mut stream, status, reason, &payload).await
}

async fn bad_request(stream: &mut TcpStream) -> io::Result<()> {
    write_json_response(stream, 400, "Bad Request", &json!({"ok":false,"error":"invalid_request"})).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_decoding_contract_is_unchanged() {
        let (path, query) = parse_target("/v1/timeline?limit=10&q=some+text%2B&empty=&raw=%ZZ");
        assert_eq!(path, "/v1/timeline");
        assert_eq!(query["limit"], "10");
        assert_eq!(query["q"], "some text+");
        assert_eq!(query["empty"], "");
        assert_eq!(query["raw"], "%ZZ");
    }
}
