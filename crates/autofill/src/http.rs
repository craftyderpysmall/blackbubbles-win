//! `POST /message` for an iPhone Shortcuts "Message" automation.
//!
//! Same contract as linuxautofill: `Authorization: Bearer <token>`, body is JSON
//! `{"text": ..., "sender": ...}` or plain text, at most 16 KiB. Replies 204 on success,
//! else 404/401/411/413/400. Links in texts are ignored ("verify" texts are a common scam).

use std::io::Read;
use std::sync::mpsc::Sender;

use anyhow::Result;
use tiny_http::{Method, Request, Response, Server};
use tracing::{info, warn};

use crate::{Incoming, Source};

pub const MAX_BODY: usize = 16 * 1024;

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Parse a request body into (sender, text).
pub fn parse_body(body: &[u8]) -> Option<(String, String)> {
    let text = std::str::from_utf8(body).ok()?;
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text)
        && value.is_object()
    {
        let message = value.get("text")?.as_str()?.to_string();
        let sender = value.get("sender").and_then(|s| s.as_str()).unwrap_or("iPhone").to_string();
        return Some((sender, message));
    }
    (!text.trim().is_empty()).then(|| ("iPhone".to_string(), text.to_string()))
}

fn status_for(request: &mut Request, token: &str, tx: &Sender<Incoming>) -> u16 {
    if request.method() != &Method::Post || request.url() != "/message" {
        return 404;
    }
    let auth = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"))
        .map(|h| h.value.as_str().to_string())
        .unwrap_or_default();
    let presented = auth.strip_prefix("Bearer ").unwrap_or_default();
    if !constant_time_eq(presented.as_bytes(), token.as_bytes()) {
        return 401;
    }
    let Some(len) = request.body_length() else { return 411 };
    if len > MAX_BODY {
        return 413;
    }
    let mut body = Vec::with_capacity(len);
    if request.as_reader().take(MAX_BODY as u64 + 1).read_to_end(&mut body).is_err() || body.len() > MAX_BODY {
        return 400;
    }
    match parse_body(&body) {
        Some((sender, text)) => {
            let _ = tx.send(Incoming { source: Source::Sms, sender, text, links: vec![] });
            204
        }
        None => 400,
    }
}

pub fn run(host: &str, port: u16, token: String, tx: Sender<Incoming>) -> Result<()> {
    if token.len() < 16 {
        anyhow::bail!("autofill.http.token must be at least 16 characters");
    }
    let server = Server::http((host, port)).map_err(|e| anyhow::anyhow!("binding {host}:{port}: {e}"))?;
    info!("autofill HTTP endpoint on {host}:{port}/message");
    let token = std::sync::Arc::new(token);
    for mut request in server.incoming_requests() {
        let (token, tx) = (token.clone(), tx.clone());
        // One thread per request so a slow client can't stall the endpoint.
        std::thread::spawn(move || {
            let peer = request.remote_addr().map(|a| a.ip().to_string()).unwrap_or_default();
            let status = status_for(&mut request, &token, &tx);
            if status != 204 {
                warn!("autofill HTTP: rejected request from {peer} ({status})");
            }
            let _ = request.respond(Response::empty(status));
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_and_plain_bodies() {
        assert_eq!(parse_body(br#"{"text":"Your code is 1234","sender":"Bank"}"#), Some(("Bank".into(), "Your code is 1234".into())));
        assert_eq!(parse_body(br#"{"text":"x"}"#).unwrap().0, "iPhone");
        assert_eq!(parse_body(b"plain code 1234").unwrap().1, "plain code 1234");
        assert_eq!(parse_body(br#"{"nottext":1}"#), None);
        assert_eq!(parse_body(b"  "), None);
    }
}
