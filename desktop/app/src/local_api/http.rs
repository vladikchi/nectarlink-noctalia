// SPDX-License-Identifier: GPL-3.0-or-later
use super::{ServerHandle, auth::constant_time_eq, route_request};
use serde_json::{Value, json};
use std::{collections::HashMap, net::SocketAddr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};
const MAX_REQUEST_BYTES: usize = 256 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(5);
/// Starts the localhost HTTP/JSON server on `127.0.0.1:<port>` (pass `0` in
/// tests for an OS-assigned ephemeral port).
pub async fn start_on_port(port: u16, token: String) -> std::io::Result<(SocketAddr, ServerHandle)> {
    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    let addr = listener.local_addr()?;
    let actual_port = addr.port();
    let (tx, mut rx) = oneshot::channel::<()>();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut rx => break,
                accepted = listener.accept() => {
                    let Ok((stream, _peer)) = accepted else { continue };
                    let expected_token = token.clone();
                    tokio::spawn(async move {
                        let _ = tokio::time::timeout(
                            IO_TIMEOUT,
                            handle_connection(stream, &expected_token),
                        )
                        .await;
                    });
                }
            }
        }
    });

    Ok((addr, ServerHandle { addr, port: actual_port, shutdown: Some(tx) }))
}

#[derive(Debug)]
pub(super) struct HttpRequest {
    pub(super) method: String,
    pub(super) path: String,
    pub(super) query: HashMap<String, String>,
    pub(super) headers: HashMap<String, String>,
    pub(super) body: Vec<u8>,
}

async fn read_http_request(stream: &mut TcpStream) -> std::io::Result<Option<HttpRequest>> {
    let mut buf = Vec::with_capacity(2048);
    let mut chunk = [0u8; 2048];
    let header_end = loop {
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
        if buf.len() >= MAX_REQUEST_BYTES {
            return Ok(None);
        }
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..n]);
    };

    let header_bytes = &buf[..header_end];
    let header_text = String::from_utf8_lossy(header_bytes);
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_ascii_uppercase();
    let raw_target = parts.next().unwrap_or("/");
    if method.is_empty() {
        return Ok(None);
    }

    let (path, query) = parse_target(raw_target);
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_owned());
        }
    }

    let content_len = headers
        .get("content-length")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0)
        .min(MAX_REQUEST_BYTES);

    let body_start = header_end + 4;
    let mut body = Vec::with_capacity(content_len);
    if buf.len() > body_start {
        let available = (buf.len() - body_start).min(content_len);
        body.extend_from_slice(&buf[body_start..body_start + available]);
    }
    while body.len() < content_len {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        let take = n.min(content_len - body.len());
        body.extend_from_slice(&chunk[..take]);
    }

    Ok(Some(HttpRequest { method, path, query, headers, body }))
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

async fn write_json_response(
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
    let Some(req) = read_http_request(&mut stream).await? else {
        return Ok(());
    };

    // 1. Reject browser cross-origin requests carrying an `Origin` header.
    if req.headers.contains_key("origin") {
        return write_json_response(
            &mut stream,
            403,
            "Forbidden",
            &json!({ "ok": false, "error": "forbidden" }),
        )
        .await;
    }

    // 2. Verify `Authorization: Bearer <token>` in constant time.
    let provided = req
        .headers
        .get("authorization")
        .and_then(|v| {
            let trimmed = v.trim();
            if trimmed.len() >= 7 && trimmed[..7].eq_ignore_ascii_case("bearer ") {
                Some(trimmed[7..].trim())
            } else {
                None
            }
        })
        .unwrap_or("");
    if provided.is_empty() || !constant_time_eq(provided, expected_token) {
        return write_json_response(
            &mut stream,
            401,
            "Unauthorized",
            &json!({ "ok": false, "error": "unauthorized" }),
        )
        .await;
    }

    let (status, reason, payload) = route_request(req).await;
    write_json_response(&mut stream, status, reason, &payload).await
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
