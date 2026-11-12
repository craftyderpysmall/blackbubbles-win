//! Connection to `bbd`: one background task owns the WebSocket, reconnects with backoff,
//! matches responses to requests and hands server events and status changes to hooks
//! (the webview bridge, the notifier, autofill).

use std::collections::HashMap;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use bb_proto::{ApiError, BlobKind, ClientFrame, ErrorCode, Event, PROTOCOL_VERSION, Request, Response, ServerFrame};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, oneshot};

use crate::config::ServerConfig;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "state", content = "detail")]
pub enum Status {
    NotPaired,
    Connecting,
    Connected { server_name: String },
    Disconnected(String),
}

type Reply = oneshot::Sender<Result<Response, ApiError>>;

/// Connect + TLS + WebSocket + Hello/Welcome.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
const PING_EVERY: Duration = Duration::from_secs(15);
/// No frame (not even a pong) for this long: the connection is dead.
const SILENCE_LIMIT: Duration = Duration::from_secs(45);
const SEND_TIMEOUT: Duration = Duration::from_secs(10);
/// Longest wait between reconnect attempts. It's a LAN, so retrying is cheap, and the
/// Mac waking up should show as connected within seconds.
const MAX_BACKOFF: Duration = Duration::from_secs(10);

struct Inner {
    status: Mutex<Status>,
    autofill: Mutex<Option<std_mpsc::Sender<bb_autofill::Incoming>>>,
    hooks: Mutex<Vec<EventHook>>,
    status_hooks: Mutex<Vec<StatusHook>>,
    /// Held while status hooks run, so they see changes in order and none fire after
    /// `close()` (a stale "Connecting" from a replaced client used to stick in the UI).
    status_delivery: Mutex<()>,
    closed: tokio::sync::watch::Sender<bool>,
}

pub type EventHook = Arc<dyn Fn(&Event) + Send + Sync>;
pub type StatusHook = Arc<dyn Fn(&Status) + Send + Sync>;

#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
    tx: Option<mpsc::UnboundedSender<(Request, Reply)>>,
    rt: tokio::runtime::Handle,
}

fn offline() -> ApiError {
    ApiError::new(ErrorCode::Internal, "not connected to the Mac")
}

impl Client {
    pub fn new(rt: tokio::runtime::Handle, server: Option<ServerConfig>) -> Self {
        let inner = Arc::new(Inner {
            status: Mutex::new(if server.is_some() { Status::Connecting } else { Status::NotPaired }),
            autofill: Mutex::default(),
            hooks: Mutex::default(),
            status_hooks: Mutex::default(),
            status_delivery: Mutex::default(),
            closed: tokio::sync::watch::channel(false).0,
        });
        let tx = server.map(|server| {
            let (tx, rx) = mpsc::unbounded_channel();
            rt.spawn(run(inner.clone(), server, rx));
            tx
        });
        Self { inner, tx, rt }
    }

    pub fn set_autofill(&self, tx: std_mpsc::Sender<bb_autofill::Incoming>) {
        *self.inner.autofill.lock().unwrap() = Some(tx);
    }

    /// Called for every server event.
    pub fn on_event(&self, hook: EventHook) {
        self.inner.hooks.lock().unwrap().push(hook);
    }

    /// Called with the current status now, then whenever it changes.
    pub fn on_status(&self, hook: StatusHook) {
        let _delivery = self.inner.status_delivery.lock().unwrap();
        self.inner.status_hooks.lock().unwrap().push(hook.clone());
        hook(&self.status());
    }

    /// Stop the connection task and the notifier's poll loop (after re-pairing).
    pub fn close(&self) {
        let _ = self.inner.closed.send(true);
        self.inner.hooks.lock().unwrap().clear();
        let _delivery = self.inner.status_delivery.lock().unwrap();
        self.inner.status_hooks.lock().unwrap().clear();
    }

    /// Resolves once [`close`](Self::close) has been called.
    pub async fn closed(&self) {
        let mut rx = self.inner.closed.subscribe();
        let _ = rx.wait_for(|closed| *closed).await;
    }

    pub fn status(&self) -> Status {
        self.inner.status.lock().unwrap().clone()
    }

    pub async fn request(&self, req: Request) -> Result<Response, ApiError> {
        let tx = self.tx.as_ref().ok_or_else(offline)?;
        let (reply_tx, reply_rx) = oneshot::channel();
        tx.send((req, reply_tx)).map_err(|_| offline())?;
        reply_rx.await.map_err(|_| offline())?
    }

    /// Download a whole blob in chunks.
    pub async fn blob(&self, kind: BlobKind, id: String) -> Result<(Vec<u8>, Option<String>), ApiError> {
        const CHUNK: u32 = 1024 * 1024;
        let mut out = Vec::new();
        let mut mime = None;
        loop {
            let req = Request::GetBlob { kind, id: id.clone(), offset: out.len() as u64, len: CHUNK };
            match self.request(req).await? {
                Response::Blob { data, total, mime_type } => {
                    mime = mime.or(mime_type);
                    let done = data.is_empty() || out.len() as u64 + data.len() as u64 >= total;
                    out.extend_from_slice(&data);
                    if done {
                        return Ok((out, mime));
                    }
                }
                other => return Err(ApiError::internal(format!("unexpected reply {other:?}"))),
            }
        }
    }

    pub fn spawn<F: std::future::Future<Output = ()> + Send + 'static>(&self, f: F) {
        self.rt.spawn(f);
    }

}

impl Inner {
    fn set_status(&self, status: Status) {
        let _delivery = self.status_delivery.lock().unwrap();
        {
            let mut current = self.status.lock().unwrap();
            if *current == status {
                return;
            }
            *current = status.clone();
        }
        let hooks = self.status_hooks.lock().unwrap().clone();
        for hook in hooks {
            hook(&status);
        }
    }

    fn push_event(&self, event: Event) {
        if let Event::IncomingText { sender, text, .. } = &event
            && let Some(tx) = &*self.autofill.lock().unwrap()
        {
            let _ = tx.send(bb_autofill::Incoming {
                source: bb_autofill::Source::Sms,
                sender: sender.clone(),
                text: text.clone(),
                links: vec![],
            });
        }
        let hooks = self.hooks.lock().unwrap().clone();
        for hook in hooks {
            hook(&event);
        }
    }
}

async fn run(inner: Arc<Inner>, server: ServerConfig, rx: mpsc::UnboundedReceiver<(Request, Reply)>) {
    let mut closed = inner.closed.subscribe();
    tokio::select! {
        _ = closed.wait_for(|c| *c) => {}
        _ = connect_loop(&inner, server, rx) => {}
    }
}

async fn connect_loop(inner: &Arc<Inner>, server: ServerConfig, mut rx: mpsc::UnboundedReceiver<(Request, Reply)>) {
    let mut backoff = Duration::from_secs(1);
    loop {
        inner.set_status(Status::Connecting);
        let mut connected = false;
        let result = session(inner, &server, &mut rx, &mut connected).await;
        match result {
            Ok(()) => return, // the client was dropped
            Err(err) => {
                let message = format!("{err:#}");
                inner.set_status(Status::Disconnected(message))
            }
        }
        if connected {
            backoff = Duration::from_secs(1);
        }
        // While waiting, fail requests fast instead of letting the UI hang.
        let sleep = tokio::time::sleep(backoff);
        tokio::pin!(sleep);
        loop {
            tokio::select! {
                _ = &mut sleep => break,
                next = rx.recv() => match next {
                    Some((_, reply)) => { let _ = reply.send(Err(offline())); }
                    None => return,
                },
            }
        }
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

async fn session(
    inner: &Inner,
    server: &ServerConfig,
    rx: &mut mpsc::UnboundedReceiver<(Request, Reply)>,
    connected: &mut bool,
) -> Result<()> {
    let pin = server.fingerprint.parse()?;
    // The whole handshake shares one deadline. Before, only the TCP/TLS part had one, so a
    // connection that died before `Welcome` left the status on "Connecting" forever.
    let (ws, server_name) = tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
        let mut ws = bb_net::connect(&server.host, server.port, pin).await?;
        bb_net::send(&mut ws, &ClientFrame::Hello { protocol: PROTOCOL_VERSION, device_token: server.token.clone() })
            .await?;
        match bb_net::recv::<_, ServerFrame>(&mut ws).await? {
            ServerFrame::Welcome { server_name, .. } => Ok((ws, server_name)),
            ServerFrame::Rejected(err) => bail!("{}", err.message),
            other => bail!("unexpected greeting {other:?}"),
        }
    })
    .await
    .map_err(|_| anyhow!("timed out connecting to {}", server.host))??;
    inner.set_status(Status::Connected { server_name });
    *connected = true;

    let (mut sink, mut stream) = ws.split();
    let mut pending: HashMap<u64, Reply> = HashMap::new();
    let mut next_id = 0u64;
    let mut ping = tokio::time::interval(PING_EVERY);
    // Anything from the server (a pong included) proves the link is alive. Without this a
    // connection that dies silently (the Mac sleeping, Wi-Fi dropping) only fails when the
    // kernel gives up on retransmitting, which takes about 15 minutes.
    let mut last_heard = tokio::time::Instant::now();
    loop {
        tokio::select! {
            next = rx.recv() => {
                let Some((req, reply)) = next else { return Ok(()) };
                next_id += 1;
                pending.insert(next_id, reply);
                let message = bb_net::to_message(&ClientFrame::Request { id: next_id, req })?;
                tokio::time::timeout(SEND_TIMEOUT, sink.send(message))
                    .await
                    .map_err(|_| anyhow!("the Mac stopped responding"))??;
            }
            message = stream.next() => {
                let message = message.ok_or_else(|| anyhow!("server closed the connection"))??;
                last_heard = tokio::time::Instant::now();
                match bb_net::from_message::<ServerFrame>(&message)? {
                    Some(ServerFrame::Response { id, res }) => {
                        if let Some(reply) = pending.remove(&id) {
                            let _ = reply.send(res);
                        }
                    }
                    Some(ServerFrame::Event(event)) => inner.push_event(event),
                    Some(ServerFrame::Rejected(err)) => bail!("{}", err.message),
                    _ => {}
                }
            }
            _ = ping.tick() => {
                if last_heard.elapsed() > SILENCE_LIMIT {
                    bail!("the Mac stopped responding");
                }
                let message = bb_net::tokio_tungstenite::tungstenite::Message::Ping(Vec::new().into());
                tokio::time::timeout(SEND_TIMEOUT, sink.send(message))
                    .await
                    .map_err(|_| anyhow!("the Mac stopped responding"))??;
            }
        }
    }
}

/// Pair with a server using a `blackbubbles://` string from `bbd pair`.
pub async fn pair(uri: &str, device_name: &str) -> Result<ServerConfig> {
    let uri: bb_net::PairUri = uri.parse()?;
    let code = uri.code.clone().ok_or_else(|| anyhow!("pairing string has no code; run `bbd pair` again"))?;
    let mut ws = bb_net::connect(&uri.host, uri.port, uri.fingerprint).await?;
    bb_net::send(&mut ws, &ClientFrame::Pair { protocol: PROTOCOL_VERSION, code, device_name: device_name.into() }).await?;
    let token = match bb_net::recv::<_, ServerFrame>(&mut ws).await? {
        ServerFrame::Paired { device_token } => device_token,
        ServerFrame::Rejected(err) => bail!("{}", err.message),
        other => bail!("unexpected reply {other:?}"),
    };
    Ok(ServerConfig { host: uri.host, port: uri.port, fingerprint: uri.fingerprint.to_string(), token })
}

pub fn device_name() -> String {
    let host = host_name();
    if host.is_empty() { "BlackBubbles (Windows)".to_string() } else { format!("{host} (Windows)") }
}

/// The machine's name as Windows shows it.
pub fn host_name() -> String {
    std::env::var("COMPUTERNAME").map(|s| s.trim().to_string()).unwrap_or_default()
}
