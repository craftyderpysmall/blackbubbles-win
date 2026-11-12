//! The `bb://` URI scheme the webview loads media from.
//!
//! - `blob/<Kind>/<id>`: a server blob (photo, thumbnail, attachment, recording), fetched
//!   in chunks with `GetBlob` and kept in a byte-bounded LRU so scrolling back is instant.
//! - `map/<path>`: a file from OpenFreeMap (`tiles.openfreemap.org`: open vector tiles,
//!   styles, fonts and sprites, no API key). MapLibre's requests are rewritten to this
//!   route so the map works offline from a disk cache and sends a proper User-Agent.
//!
//! The frontend builds URLs with `convertFileSrc(path, "bb")`, which percent-encodes the
//! whole path, so it is decoded here before splitting.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use bb_proto::BlobKind;
use tauri::http::{Request, Response, StatusCode, header};
use tokio::sync::Semaphore;

use crate::conn::Client;

const CACHE_BYTES: usize = 160 * 1024 * 1024;
/// Thumbnails for a whole screen load in parallel, without flooding the connection.
const MAX_IN_FLIGHT: usize = 24;
/// Tile sets are versioned by date in their URLs, so tiles/fonts/sprites keep a week;
/// style and TileJSON documents (which point at the current version) keep a day.
const MAP_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 3600);
const MAP_DOC_MAX_AGE: Duration = Duration::from_secs(24 * 3600);
const MAP_HOST: &str = "https://tiles.openfreemap.org/";

pub type Blob = Arc<(Vec<u8>, String)>;

pub struct Blobs {
    entries: Mutex<Lru>,
    permits: Semaphore,
    tiles: Semaphore,
    http: reqwest::Client,
}

#[derive(Default)]
struct Lru {
    map: HashMap<(BlobKind, String), Blob>,
    order: VecDeque<(BlobKind, String)>,
    bytes: usize,
}

impl Lru {
    fn get(&mut self, key: &(BlobKind, String)) -> Option<Blob> {
        let hit = self.map.get(key).cloned()?;
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            let k = self.order.remove(pos).unwrap();
            self.order.push_back(k);
        }
        Some(hit)
    }

    fn insert(&mut self, key: (BlobKind, String), blob: Blob) {
        if blob.0.len() > CACHE_BYTES / 4 {
            return; // a huge video or file: don't evict a whole library for it
        }
        self.bytes += blob.0.len();
        if let Some(old) = self.map.insert(key.clone(), blob) {
            self.bytes -= old.0.len();
        } else {
            self.order.push_back(key);
        }
        while self.bytes > CACHE_BYTES {
            let Some(old) = self.order.pop_front() else { break };
            if let Some(b) = self.map.remove(&old) {
                self.bytes -= b.0.len();
            }
        }
    }
}

pub fn parse_kind(s: &str) -> Option<BlobKind> {
    Some(match s {
        "Attachment" => BlobKind::Attachment,
        "PhotoThumbnail" => BlobKind::PhotoThumbnail,
        "Photo" => BlobKind::Photo,
        "VoiceMemo" => BlobKind::VoiceMemo,
        _ => return None,
    })
}

pub fn sniff_mime(bytes: &[u8]) -> &'static str {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [0xFF, 0xD8, ..] => "image/jpeg",
        [b'G', b'I', b'F', ..] => "image/gif",
        [b'B', b'M', ..] => "image/bmp",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "image/webp",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'A', b'V', b'E', ..] => "audio/wav",
        [_, _, _, _, b'f', b't', b'y', b'p', ..] => "audio/mp4",
        _ => "application/octet-stream",
    }
}

impl Blobs {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("BlackBubbles/", env!("CARGO_PKG_VERSION"), " (+https://github.com/craftyderpysmall/blackbubbles)"))
            .timeout(Duration::from_secs(20))
            .build()
            .expect("http client");
        Self { entries: Mutex::default(), permits: Semaphore::new(MAX_IN_FLIGHT), tiles: Semaphore::new(6), http }
    }

    /// A whole blob, from the cache or the server.
    pub async fn get(&self, client: &Client, kind: BlobKind, id: &str) -> Result<Blob, String> {
        let key = (kind, id.to_string());
        if let Some(hit) = self.entries.lock().unwrap().get(&key) {
            return Ok(hit);
        }
        let _permit = self.permits.acquire().await.map_err(|e| e.to_string())?;
        // Another request may have filled it while we waited.
        if let Some(hit) = self.entries.lock().unwrap().get(&key) {
            return Ok(hit);
        }
        let (bytes, mime) = client.blob(kind, id.to_string()).await.map_err(|e| e.message)?;
        let mime = mime.filter(|m| !m.is_empty()).unwrap_or_else(|| sniff_mime(&bytes).to_string());
        let blob: Blob = Arc::new((bytes, mime));
        self.entries.lock().unwrap().insert(key, blob.clone());
        Ok(blob)
    }

    /// A file from OpenFreeMap, from the disk cache when fresh enough.
    async fn map_file(&self, path: &str) -> Result<Vec<u8>, String> {
        let safe = !path.is_empty()
            && !path.split('/').any(|seg| seg.is_empty() || seg == "." || seg == "..")
            && path.chars().all(|c| c.is_ascii_alphanumeric() || " _.,-/".contains(c));
        if !safe {
            return Err("bad map path".into());
        }
        let max_age = if path.ends_with(".json") || !path.contains('.') { MAP_DOC_MAX_AGE } else { MAP_MAX_AGE };
        let cached = map_dir().map(|d| d.join(path.replace(' ', "_")));
        if let Some(file) = &cached
            && let Ok(meta) = std::fs::metadata(file)
            && meta.modified().ok().and_then(|m| SystemTime::now().duration_since(m).ok()).is_some_and(|age| age < max_age)
            && let Ok(bytes) = tokio::fs::read(file).await
        {
            return Ok(bytes);
        }
        let _permit = self.tiles.acquire().await.map_err(|e| e.to_string())?;
        let encoded: Vec<String> = path.split('/').map(|seg| percent_encoding::utf8_percent_encode(seg, percent_encoding::NON_ALPHANUMERIC).to_string()).collect();
        let url = format!("{MAP_HOST}{}", encoded.join("/").replace("%2E", ".").replace("%2D", "-").replace("%5F", "_"));
        let fetched = async { self.http.get(&url).send().await?.error_for_status()?.bytes().await }.await;
        match fetched {
            Ok(bytes) => {
                if let Some(file) = &cached {
                    let _ = std::fs::create_dir_all(file.parent().unwrap());
                    let _ = std::fs::write(file, &bytes);
                }
                Ok(bytes.to_vec())
            }
            // Offline: a stale copy beats a blank map.
            Err(err) => match &cached {
                Some(file) if file.exists() => tokio::fs::read(file).await.map_err(|e| e.to_string()),
                _ => Err(err.to_string()),
            },
        }
    }
}

fn map_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "craftyderpysmall", "BlackBubbles").map(|d| d.cache_dir().join("map"))
}

fn map_mime(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("pbf" | "mvt") => "application/x-protobuf",
        Some("png") => "image/png",
        Some("json") => "application/json",
        _ if !path.contains('.') => "application/json",
        _ => "application/octet-stream",
    }
}

fn respond(status: StatusCode, mime: &str, body: Vec<u8>, cache: bool) -> Response<Vec<u8>> {
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*");
    if cache {
        builder = builder.header(header::CACHE_CONTROL, "private, max-age=86400, immutable");
    }
    builder.body(body).unwrap()
}

fn error(status: StatusCode, message: impl Into<String>) -> Response<Vec<u8>> {
    respond(status, "text/plain", message.into().into_bytes(), false)
}

/// Serve one `bb://` request.
pub async fn handle(blobs: &Blobs, client: Client, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let raw = request.uri().path().trim_start_matches('/');
    let path = percent_encoding::percent_decode_str(raw).decode_utf8_lossy().into_owned();
    let mut parts = path.splitn(3, '/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some("blob"), Some(kind), Some(id)) => {
            let Some(kind) = parse_kind(kind) else { return error(StatusCode::NOT_FOUND, "unknown blob kind") };
            match blobs.get(&client, kind, id).await {
                Ok(blob) => respond(StatusCode::OK, &blob.1, blob.0.clone(), true),
                Err(err) => error(StatusCode::BAD_GATEWAY, err),
            }
        }
        (Some("map"), Some(first), rest) => {
            let file = match rest {
                Some(rest) => format!("{first}/{rest}"),
                None => first.to_string(),
            };
            match blobs.map_file(&file).await {
                Ok(bytes) => {
                    let gzipped = bytes.starts_with(&[0x1f, 0x8b]);
                    let mut res = respond(StatusCode::OK, map_mime(&file), bytes, false);
                    if gzipped {
                        res.headers_mut().insert(header::CONTENT_ENCODING, header::HeaderValue::from_static("gzip"));
                    }
                    res
                }
                Err(err) => error(StatusCode::BAD_GATEWAY, err),
            }
        }
        _ => error(StatusCode::NOT_FOUND, "not found"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_common_types() {
        assert_eq!(sniff_mime(b"\x89PNG\r\n"), "image/png");
        assert_eq!(sniff_mime(b"\xFF\xD8\xFF\xE0"), "image/jpeg");
        assert_eq!(sniff_mime(b"RIFF\0\0\0\0WAVEfmt "), "audio/wav");
        assert_eq!(sniff_mime(b"\0\0\0\x20ftypM4A "), "audio/mp4");
        assert_eq!(sniff_mime(b"BM\0\0"), "image/bmp");
    }

    #[test]
    fn lru_is_bounded_by_bytes() {
        let mut lru = Lru::default();
        let chunk = CACHE_BYTES / 5;
        for i in 0..8 {
            lru.insert((BlobKind::Photo, i.to_string()), Arc::new((vec![0; chunk], String::new())));
        }
        assert!(lru.bytes <= CACHE_BYTES);
        assert!(lru.get(&(BlobKind::Photo, "0".into())).is_none());
        assert!(lru.get(&(BlobKind::Photo, "7".into())).is_some());
    }
}
