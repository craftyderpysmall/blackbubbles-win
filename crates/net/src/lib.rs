//! Transport shared by the BlackBubbles server and client.
//!
//! Security model: the server generates a self-signed certificate once. Clients learn its
//! SHA-256 fingerprint out of band (the pairing string printed by `bbd pair`) and pin it,
//! so no CA is involved and a man in the middle can't present a different certificate.
//! After TLS, the client proves itself with a device token (or a one-time pairing code).

use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{WebPkiSupportedAlgorithms, ring};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

pub use tokio_rustls;
pub use tokio_tungstenite;

pub const DEFAULT_PORT: u16 = 7878;
/// Attachments are sent in one frame, so allow large messages.
pub const MAX_MESSAGE_BYTES: usize = 256 * 1024 * 1024;
const SNI_NAME: &str = "blackbubbles";

/// SHA-256 of the server's DER certificate.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Fingerprint(pub [u8; 32]);

impl Fingerprint {
    pub fn of(cert_der: &[u8]) -> Self {
        Self(Sha256::digest(cert_der).into())
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&hex::encode(self.0))
    }
}

impl fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fingerprint({self})")
    }
}

impl std::str::FromStr for Fingerprint {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        let clean: String = s.chars().filter(|c| *c != ':').collect();
        let bytes = hex::decode(clean).context("fingerprint is not hex")?;
        Ok(Self(bytes.try_into().map_err(|_| anyhow!("fingerprint must be 32 bytes"))?))
    }
}

/// The server's long-lived certificate and key.
pub struct Identity {
    pub cert_der: Vec<u8>,
    pub key_der: Vec<u8>,
}

impl Identity {
    /// Load `cert.der`/`key.der` from `dir`, generating them on first run.
    pub fn load_or_create(dir: &Path) -> Result<Self> {
        let (cert_path, key_path) = (dir.join("cert.der"), dir.join("key.der"));
        if cert_path.exists() && key_path.exists() {
            return Ok(Self { cert_der: fs::read(cert_path)?, key_der: fs::read(key_path)? });
        }
        fs::create_dir_all(dir)?;
        let generated = rcgen::generate_simple_self_signed(vec![SNI_NAME.to_string()])?;
        let identity = Self {
            cert_der: generated.cert.der().to_vec(),
            key_der: generated.signing_key.serialize_der(),
        };
        write_private(&key_path, &identity.key_der)?;
        fs::write(cert_path, &identity.cert_der)?;
        Ok(identity)
    }

    pub fn fingerprint(&self) -> Fingerprint {
        Fingerprint::of(&self.cert_der)
    }

    pub fn acceptor(&self) -> Result<tokio_rustls::TlsAcceptor> {
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(ring::default_provider()))
            .with_safe_default_protocol_versions()?
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(self.cert_der.clone())],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key_der.clone())),
            )?;
        Ok(tokio_rustls::TlsAcceptor::from(Arc::new(config)))
    }
}

/// Write a file readable only by the current user.
pub fn write_private(path: &Path, data: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path).with_context(|| format!("writing {}", path.display()))?;
    // Lock the file down before the secret goes in.
    #[cfg(windows)]
    restrict_to_owner(path)?;
    file.write_all(data)?;
    Ok(())
}

/// Windows' "mode 600": a protected DACL (no inherited entries) granting full access to
/// the file's owner and SYSTEM only.
#[cfg(windows)]
pub fn restrict_to_owner(path: &Path) -> Result<()> {
    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
    use windows::Win32::Security::{DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SetFileSecurityW};
    use windows::core::{HSTRING, w};

    let mut sd = PSECURITY_DESCRIPTOR::default();
    // OW = the owner (whoever created the file), SY = LocalSystem, FA = full access.
    unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(w!("D:P(A;;FA;;;OW)(A;;FA;;;SY)"), SDDL_REVISION_1, &mut sd, None) }
        .context("building the file's access list")?;
    let set = unsafe {
        SetFileSecurityW(&HSTRING::from(path.as_os_str()), DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION, sd)
    };
    unsafe {
        let _ = LocalFree(Some(HLOCAL(sd.0)));
    }
    set.ok().with_context(|| format!("restricting access to {}", path.display()))
}

#[derive(Debug)]
struct PinnedVerifier {
    pin: Fingerprint,
    algorithms: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if Fingerprint::of(end_entity.as_ref()) == self.pin {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "server certificate does not match the paired fingerprint".into(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

pub fn connector(pin: Fingerprint) -> Result<tokio_rustls::TlsConnector> {
    let provider = ring::default_provider();
    let verifier = PinnedVerifier { pin, algorithms: provider.signature_verification_algorithms };
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(provider))
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    Ok(tokio_rustls::TlsConnector::from(Arc::new(config)))
}

fn ws_config() -> WebSocketConfig {
    WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_MESSAGE_BYTES))
}

pub type ClientStream = WebSocketStream<tokio_rustls::client::TlsStream<TcpStream>>;
pub type ServerStream = WebSocketStream<tokio_rustls::server::TlsStream<TcpStream>>;

/// Open a pinned TLS WebSocket to the server.
pub async fn connect(host: &str, port: u16, pin: Fingerprint) -> Result<ClientStream> {
    let tcp = connect_tcp(host, port).await?;
    tcp.set_nodelay(true)?;
    let tls = connector(pin)?.connect(ServerName::try_from(SNI_NAME)?, tcp).await?;
    let (ws, _) = tokio_tungstenite::client_async_with_config(
        format!("ws://{SNI_NAME}/v{}", bb_proto::PROTOCOL_VERSION),
        tls,
        Some(ws_config()),
    )
    .await?;
    Ok(ws)
}

/// How long one address may take to accept a TCP connection.
const ADDRESS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);
const ATTEMPT_STAGGER: std::time::Duration = std::time::Duration::from_millis(250);

/// Connect to every address `host` resolves to at once and keep the first that answers.
///
/// mDNS often returns an IPv6 address first (the owner's Mac: a ULA that `bbd` didn't
/// listen on). Trying addresses one at a time, as `TcpStream::connect` does, lets one
/// dead address use up the caller's whole timeout before the working one is tried.
async fn connect_tcp(host: &str, port: u16) -> Result<TcpStream> {
    let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .with_context(|| format!("resolving {host}"))?
        .collect();
    if addrs.is_empty() {
        bail!("{host} has no addresses");
    }
    // Staggered like Happy Eyeballs (RFC 8305): the next address starts only if the earlier
    // ones haven't connected yet, so normally just one connection is opened.
    let attempts = addrs.into_iter().enumerate().map(|(i, addr)| {
        Box::pin(async move {
            tokio::time::sleep(ATTEMPT_STAGGER * i as u32).await;
            match tokio::time::timeout(ADDRESS_TIMEOUT, TcpStream::connect(addr)).await {
                Ok(Ok(tcp)) => Ok(tcp),
                Ok(Err(err)) => Err(anyhow!("{addr}: {err}")),
                Err(_) => Err(anyhow!("{addr}: timed out")),
            }
        })
    });
    let (tcp, _) = futures_util::future::select_ok(attempts)
        .await
        .with_context(|| format!("connecting to {host}:{port}"))?;
    Ok(tcp)
}

/// Complete TLS and the WebSocket handshake for an accepted TCP connection.
pub async fn accept(tcp: TcpStream, acceptor: &tokio_rustls::TlsAcceptor) -> Result<ServerStream> {
    tcp.set_nodelay(true)?;
    let tls = acceptor.accept(tcp).await?;
    Ok(tokio_tungstenite::accept_async_with_config(tls, Some(ws_config())).await?)
}

pub fn to_message<T: Serialize>(value: &T) -> Result<Message> {
    Ok(Message::Binary(bb_proto::encode(value)?.into()))
}

/// Decode a binary frame. Returns `Ok(None)` for control frames that carry no data.
pub fn from_message<T: DeserializeOwned>(message: &Message) -> Result<Option<T>> {
    match message {
        Message::Binary(bytes) => Ok(Some(bb_proto::decode(bytes)?)),
        Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => Ok(None),
        Message::Text(_) => bail!("unexpected text frame"),
        Message::Close(_) => bail!("connection closed"),
    }
}

/// Read the next data frame.
pub async fn recv<S, T>(ws: &mut WebSocketStream<S>) -> Result<T>
where
    S: AsyncRead + AsyncWrite + Unpin,
    T: DeserializeOwned,
{
    use futures_util::StreamExt;
    loop {
        let message = ws.next().await.ok_or_else(|| anyhow!("connection closed"))??;
        if let Some(value) = from_message(&message)? {
            return Ok(value);
        }
    }
}

pub async fn send<S, T>(ws: &mut WebSocketStream<S>, value: &T) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
    T: Serialize,
{
    use futures_util::SinkExt;
    ws.send(to_message(value)?).await?;
    Ok(())
}

/// Everything a client needs to pair: `blackbubbles://HOST:PORT/?fp=HEX&code=CODE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairUri {
    pub host: String,
    pub port: u16,
    pub fingerprint: Fingerprint,
    pub code: Option<String>,
}

impl fmt::Display for PairUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let host = if self.host.contains(':') { format!("[{}]", self.host) } else { self.host.clone() };
        write!(f, "blackbubbles://{host}:{}/?fp={}", self.port, self.fingerprint)?;
        if let Some(code) = &self.code {
            write!(f, "&code={code}")?;
        }
        Ok(())
    }
}

impl std::str::FromStr for PairUri {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        let url = url::Url::parse(s.trim()).context("invalid pairing string")?;
        if url.scheme() != "blackbubbles" {
            bail!("pairing string must start with blackbubbles://");
        }
        let host = url.host_str().ok_or_else(|| anyhow!("pairing string has no host"))?;
        let host = host.trim_start_matches('[').trim_end_matches(']').to_string();
        let mut fingerprint = None;
        let mut code = None;
        for (key, value) in url.query_pairs() {
            match &*key {
                "fp" => fingerprint = Some(value.parse()?),
                "code" => code = Some(value.into_owned()),
                _ => {}
            }
        }
        Ok(Self {
            host,
            port: url.port().unwrap_or(DEFAULT_PORT),
            fingerprint: fingerprint.ok_or_else(|| anyhow!("pairing string has no fp="))?,
            code,
        })
    }
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn private_files_are_owner_only() {
        let path = std::env::temp_dir().join(format!("bb-net-private-{}", std::process::id()));
        super::write_private(&path, b"secret").unwrap();
        let out = std::process::Command::new("icacls").arg(&path).output().unwrap();
        let acl = String::from_utf8_lossy(&out.stdout).to_string();
        let _ = std::fs::remove_file(&path);
        assert!(acl.contains(r"NT AUTHORITY\SYSTEM"), "{acl}");
        for group in ["Everyone", r"BUILTIN\Users", "Authenticated Users"] {
            assert!(!acl.contains(group), "{group} can read it: {acl}");
        }
    }

    use super::*;
    use bb_proto::{ClientFrame, ServerFrame};

    #[test]
    fn pair_uri_round_trips() {
        let uri = PairUri {
            host: "mac.tail1234.ts.net".into(),
            port: 7878,
            fingerprint: Fingerprint([7; 32]),
            code: Some("ABCD-1234".into()),
        };
        assert_eq!(uri.to_string().parse::<PairUri>().unwrap(), uri);
        let v6 = PairUri { host: "fd7a::1".into(), code: None, ..uri };
        assert_eq!(v6.to_string().parse::<PairUri>().unwrap(), v6);
    }

    #[tokio::test]
    async fn pinned_tls_round_trip_and_rejects_wrong_pin() {
        let dir = std::env::temp_dir().join(format!("bbnet-test-{}", std::process::id()));
        let identity = Identity::load_or_create(&dir).unwrap();
        let acceptor = identity.acceptor().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let (tcp, _) = listener.accept().await.unwrap();
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    let Ok(mut ws) = accept(tcp, &acceptor).await else { return };
                    let _: ClientFrame = recv(&mut ws).await.unwrap();
                    send(&mut ws, &ServerFrame::Paired { device_token: "t".into() }).await.unwrap();
                });
            }
        });

        let mut ws = connect("127.0.0.1", port, identity.fingerprint()).await.unwrap();
        send(&mut ws, &ClientFrame::Pair { protocol: 1, code: "c".into(), device_name: "d".into() })
            .await
            .unwrap();
        let reply: ServerFrame = recv(&mut ws).await.unwrap();
        assert_eq!(reply, ServerFrame::Paired { device_token: "t".into() });

        assert!(connect("127.0.0.1", port, Fingerprint([0; 32])).await.is_err());
        let _ = fs::remove_dir_all(dir);
    }

    /// `localhost` usually resolves to `::1` first; with only IPv4 listening the client must
    /// still get through (the owner's Mac: mDNS gave an IPv6 address bbd didn't serve).
    #[tokio::test]
    async fn falls_back_to_an_address_that_listens() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { while listener.accept().await.is_ok() {} });
        let tcp = tokio::time::timeout(std::time::Duration::from_secs(5), connect_tcp("localhost", port))
            .await
            .expect("fallback took too long")
            .unwrap();
        assert!(tcp.peer_addr().unwrap().ip().is_loopback());
    }
}
