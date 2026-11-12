//! Gmail IMAP poller (port of linuxautofill's `imap_source`).
//!
//! Every 0.5 s: `EXAMINE INBOX` (read-only) and compare `UIDNEXT`. Measured by linuxautofill
//! at ~3 s behind Gmail, faster than IDLE. New mail is fetched with `BODY.PEEK[]<0.262144>`
//! so nothing is marked read and at most 256 KB is downloaded.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tracing::{info, warn};

use crate::config::GmailConfig;
use crate::{Incoming, Source};

const HOST: &str = "imap.gmail.com";
const POLL: Duration = Duration::from_millis(500);
const MAX_BACKOFF: Duration = Duration::from_secs(300);

type Tls = rustls::StreamOwned<rustls::ClientConnection, TcpStream>;

/// One untagged response; `{n}` literals are collected in order.
#[derive(Debug, Default)]
pub struct Item {
    pub line: String,
    pub literals: Vec<Vec<u8>>,
}

struct Imap<S: Read + Write> {
    stream: S,
    buf: Vec<u8>,
    tag: u32,
}

/// IMAP quoted string.
pub fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// `{123}` at the end of a line.
fn literal_len(line: &str) -> Option<usize> {
    let open = line.rfind('{')?;
    line[open + 1..].strip_suffix('}')?.parse().ok()
}

impl<S: Read + Write> Imap<S> {
    fn new(stream: S) -> Self {
        Self { stream, buf: Vec::new(), tag: 0 }
    }

    fn fill(&mut self) -> Result<()> {
        let mut chunk = [0u8; 16 * 1024];
        let n = self.stream.read(&mut chunk)?;
        if n == 0 {
            bail!("IMAP connection closed");
        }
        self.buf.extend_from_slice(&chunk[..n]);
        Ok(())
    }

    fn read_line(&mut self) -> Result<String> {
        loop {
            if let Some(pos) = self.buf.windows(2).position(|w| w == b"\r\n") {
                let line: Vec<u8> = self.buf.drain(..pos + 2).take(pos).collect();
                return Ok(String::from_utf8_lossy(&line).into_owned());
            }
            self.fill()?;
        }
    }

    fn read_exact(&mut self, n: usize) -> Result<Vec<u8>> {
        while self.buf.len() < n {
            self.fill()?;
        }
        Ok(self.buf.drain(..n).collect())
    }

    /// Read one logical response line, pulling in any literals it announces.
    fn read_item(&mut self) -> Result<Item> {
        let mut item = Item::default();
        loop {
            let line = self.read_line()?;
            item.line.push_str(&line);
            match literal_len(&line) {
                Some(n) => item.literals.push(self.read_exact(n)?),
                None => return Ok(item),
            }
        }
    }

    fn command(&mut self, command: &str) -> Result<Vec<Item>> {
        self.tag += 1;
        let tag = format!("A{}", self.tag);
        self.stream.write_all(format!("{tag} {command}\r\n").as_bytes())?;
        self.stream.flush()?;
        let mut items = Vec::new();
        loop {
            let item = self.read_item()?;
            if let Some(status) = item.line.strip_prefix(&format!("{tag} ")) {
                if status.starts_with("OK") {
                    return Ok(items);
                }
                let verb = command.split_whitespace().next().unwrap_or_default();
                bail!("IMAP {verb} failed: {status}");
            }
            items.push(item);
        }
    }
}

fn uidnext(items: &[Item]) -> Option<u32> {
    items.iter().find_map(|i| {
        let at = i.line.find("UIDNEXT ")?;
        i.line[at + 8..].split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
    })
}

/// `* 5 FETCH (UID 123 ...` → 123
fn fetch_uid(line: &str) -> Option<u32> {
    let at = line.find("UID ")?;
    line[at + 4..].split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
}

fn connect() -> Result<Tls> {
    let roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let addr = std::net::ToSocketAddrs::to_socket_addrs(&(HOST, 993))?.next().context("resolving imap.gmail.com")?;
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(30))?;
    tcp.set_read_timeout(Some(Duration::from_secs(60)))?;
    let conn = rustls::ClientConnection::new(Arc::new(config), HOST.try_into()?)?;
    Ok(rustls::StreamOwned::new(conn, tcp))
}

fn session(account: &GmailConfig, tx: &Sender<Incoming>, last_uid: &mut Option<u32>, connected: &mut bool) -> Result<()> {
    let mut imap = Imap::new(connect()?);
    let greeting = imap.read_line()?;
    if !greeting.starts_with("* OK") {
        bail!("unexpected IMAP greeting");
    }
    let password: String = account.app_password.chars().filter(|c| !c.is_whitespace()).collect();
    imap.command(&format!("LOGIN {} {}", quote(&account.address), quote(&password)))
        .context("Gmail login failed (use an app password)")?;
    info!("autofill: watching {}", account.address);
    *connected = true;
    loop {
        let status = imap.command("EXAMINE INBOX")?;
        let next = uidnext(&status).context("server did not report UIDNEXT")?;
        match *last_uid {
            // First run: only mail arriving from now on.
            None => *last_uid = Some(next),
            Some(last) if next > last => {
                let items = imap.command(&format!("UID FETCH {last}:* (UID INTERNALDATE BODY.PEEK[]<0.262144>)"))?;
                let now = chrono::Utc::now().timestamp();
                for item in items {
                    // `n:*` returns the newest message even if it's older than n.
                    if fetch_uid(&item.line).is_none_or(|uid| uid < last) {
                        continue;
                    }
                    let Some(raw) = item.literals.first() else { continue };
                    if let Some(parsed) = crate::mail::parse_message(raw, now) {
                        let _ = tx.send(Incoming { source: Source::Mail, sender: parsed.sender, text: parsed.text, links: parsed.links });
                    }
                }
                *last_uid = Some(next);
            }
            Some(_) => {}
        }
        std::thread::sleep(POLL);
    }
}

/// Poll forever, reconnecting with exponential backoff. Mail that arrived while offline
/// is still picked up because the last seen UID is kept.
pub fn run(account: GmailConfig, tx: Sender<Incoming>) {
    let mut last_uid = None;
    let mut backoff = Duration::from_secs(1);
    loop {
        let mut connected = false;
        if let Err(err) = session(&account, &tx, &mut last_uid, &mut connected) {
            warn!("autofill: {}: {err:#}", account.address);
        }
        if connected {
            backoff = Duration::from_secs(1);
        }
        std::thread::sleep(backoff);
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake server: canned bytes in, everything written is discarded.
    struct Script(std::io::Cursor<Vec<u8>>);
    impl Read for Script {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.0.read(buf)
        }
    }
    impl Write for Script {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn parses_responses_with_literals() {
        let body = b"Subject: hi\r\n\r\nYour code is 1234\r\n";
        let mut script = b"* 3 EXISTS\r\n* OK [UIDNEXT 42] Predicted next UID\r\nA1 OK [READ-ONLY] done\r\n".to_vec();
        script.extend_from_slice(format!("* 3 FETCH (UID 41 INTERNALDATE \"x\" BODY[]<0> {{{}}}\r\n", body.len()).as_bytes());
        script.extend_from_slice(body);
        script.extend_from_slice(b")\r\nA2 OK done\r\nA3 NO [AUTHENTICATIONFAILED] bad\r\n");
        let mut imap = Imap::new(Script(std::io::Cursor::new(script)));

        assert_eq!(uidnext(&imap.command("EXAMINE INBOX").unwrap()), Some(42));
        let items = imap.command("UID FETCH 41:* (UID BODY.PEEK[])").unwrap();
        assert_eq!(fetch_uid(&items[0].line), Some(41));
        assert_eq!(items[0].literals[0], body);
        assert!(items[0].line.ends_with(')'));
        assert!(imap.command("LOGIN a b").unwrap_err().to_string().contains("AUTHENTICATIONFAILED"));
    }

    #[test]
    fn quotes_strings() {
        assert_eq!(quote(r#"a"b\c"#), r#""a\"b\\c""#);
    }
}
