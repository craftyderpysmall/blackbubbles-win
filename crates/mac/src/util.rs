use std::path::{Path, PathBuf};

use bb_proto::{ApiError, Response, Timestamp};
use rusqlite::{Connection, OpenFlags};

/// Seconds between the Unix epoch and Apple's 2001-01-01 epoch.
pub const APPLE_EPOCH_SECS: i64 = 978_307_200;

/// Messages stores nanoseconds since 2001 (older macOS: seconds).
pub fn apple_nanos_to_unix_ms(value: i64) -> Timestamp {
    let ms_since_2001 = if value.abs() > 1_000_000_000_000 { value / 1_000_000 } else { value * 1000 };
    ms_since_2001 + APPLE_EPOCH_SECS * 1000
}

pub fn unix_ms_to_apple_nanos(ms: Timestamp) -> i64 {
    (ms - APPLE_EPOCH_SECS * 1000) * 1_000_000
}

/// Core Data stores fractional seconds since 2001.
pub fn apple_secs_to_unix_ms(value: f64) -> Timestamp {
    ((value + APPLE_EPOCH_SECS as f64) * 1000.0) as Timestamp
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

pub fn expand_tilde(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None => PathBuf::from(path),
    }
}

/// Open one of Apple's SQLite stores read-only. Returns the connection and whether it is
/// a snapshot (see below), which callers must not cache.
///
/// Apple's stores use WAL mode. A read-only connection can't create the `-shm` side file,
/// so once the owning app quits (Voice Memos, Notes) and SQLite removes it, a normal
/// read-only open fails with "unable to open database file". Then we fall back to
/// `immutable=1`: still strictly read-only and touching nothing, but it reads the main
/// file as it is, so it's a snapshot that must be reopened to see changes.
pub fn open_readonly(path: &Path) -> Result<(Connection, bool), ApiError> {
    if !path.exists() {
        // With Full Disk Access missing, macOS hides the file entirely.
        return Err(ApiError::new(
            bb_proto::ErrorCode::NotFound,
            format!("{} doesn't exist (or bbd lacks Full Disk Access to see it).", path.display()),
        ));
    }
    let normal = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .and_then(|conn| conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(())).map(|_| conn));
    let err = match normal {
        Ok(conn) => return Ok((conn, false)),
        Err(err) => err,
    };
    let uri = format!("file:{}?mode=ro&immutable=1", percent_path(path));
    Connection::open_with_flags(&uri, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .and_then(|conn| conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(())).map(|_| conn))
        .map(|conn| (conn, true))
        .map_err(|_| {
            ApiError::new(
                bb_proto::ErrorCode::PermissionDenied,
                format!(
                    "cannot open {} ({err}). Grant bbd Full Disk Access in System Settings → Privacy & Security.",
                    path.display()
                ),
            )
        })
}

/// Percent-encode a path for an SQLite `file:` URI (spaces, `?`, `#`, `%`).
fn percent_path(path: &Path) -> String {
    let mut out = String::new();
    for c in path.to_string_lossy().chars() {
        match c {
            ' ' => out.push_str("%20"),
            '?' => out.push_str("%3f"),
            '#' => out.push_str("%23"),
            '%' => out.push_str("%25"),
            c => out.push(c),
        }
    }
    out
}

/// Columns of a table, for coping with schema differences between macOS versions.
pub fn columns(conn: &Connection, table: &str) -> Vec<String> {
    let Ok(mut stmt) = conn.prepare(&format!("PRAGMA table_info({table})")) else { return vec![] };
    stmt.query_map([], |row| row.get::<_, String>(1))
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
}

pub fn sql_err(err: rusqlite::Error) -> ApiError {
    ApiError::internal(format!("database error: {err}"))
}

/// Serve `len` bytes at `offset` from a file.
pub fn read_blob(path: &Path, offset: u64, len: u32, mime: Option<String>) -> Result<Response, ApiError> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).map_err(|e| ApiError::not_found(format!("{} ({e})", path.display())))?;
    let total = file.metadata().map_err(ApiError::internal)?.len();
    file.seek(SeekFrom::Start(offset.min(total))).map_err(ApiError::internal)?;
    let want = u64::from(len.min(bb_proto::MAX_BLOB_CHUNK)).min(total.saturating_sub(offset)) as usize;
    let mut data = vec![0u8; want];
    file.read_exact(&mut data).map_err(ApiError::internal)?;
    Ok(Response::Blob { data, total, mime_type: mime })
}

/// Run an AppleScript with arguments passed as `argv` (never interpolated into the source).
/// How long an AppleScript may run. Longer almost always means macOS is showing a prompt
/// on the Mac's screen (e.g. "bbd wants to control Notes") that nobody can see from here.
const OSASCRIPT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub fn osascript(script: &str, args: &[&str]) -> Result<String, ApiError> {
    run_with_timeout(
        std::process::Command::new("/usr/bin/osascript").arg("-e").arg(script).args(args),
        OSASCRIPT_TIMEOUT,
    )
    .map_err(|e| match e {
        RunError::TimedOut => ApiError::internal(
            "The Mac didn't answer within 30 seconds. macOS is probably asking for permission on the Mac's screen \
             (\"bbd wants to control …\"): click OK there, or allow it in System Settings → Privacy & Security → Automation → bbd.",
        ),
        RunError::Io(e) => ApiError::internal(format!("osascript: {e}")),
    })
    .and_then(|output| osascript_result(&output))
}

pub(crate) enum RunError {
    TimedOut,
    Io(std::io::Error),
}

/// Run a command, killing it if it outlives `timeout`.
pub(crate) fn run_with_timeout(cmd: &mut std::process::Command, timeout: std::time::Duration) -> Result<std::process::Output, RunError> {
    use std::process::Stdio;
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(RunError::Io)?;
    let start = std::time::Instant::now();
    loop {
        if child.try_wait().map_err(RunError::Io)?.is_some() {
            return child.wait_with_output().map_err(RunError::Io);
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(RunError::TimedOut);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn osascript_result(output: &std::process::Output) -> Result<String, ApiError> {
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(ApiError::internal(format!(
            "AppleScript failed: {}. Allow bbd to control the app in System Settings → Privacy & Security → Automation.",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

/// Convert a file with a macOS tool into the cache, reusing earlier conversions.
pub fn cached_conversion(cache: &Path, key: &str, ext: &str, convert: impl FnOnce(&Path) -> std::io::Result<bool>) -> Result<PathBuf, ApiError> {
    let safe: String = key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    let out = cache.join(format!("{safe}.{ext}"));
    if out.exists() {
        return Ok(out);
    }
    std::fs::create_dir_all(cache).map_err(ApiError::internal)?;
    let tmp = cache.join(format!("{safe}.tmp.{ext}"));
    match convert(&tmp) {
        Ok(true) => {
            std::fs::rename(&tmp, &out).map_err(ApiError::internal)?;
            Ok(out)
        }
        Ok(false) => Err(ApiError::internal("conversion failed")),
        Err(e) => Err(ApiError::internal(format!("conversion failed: {e}"))),
    }
}

pub fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    #[test]
    fn commands_time_out() {
        let slow = super::run_with_timeout(std::process::Command::new("sleep").arg("5"), std::time::Duration::from_millis(300));
        assert!(matches!(slow, Err(super::RunError::TimedOut)));
        let fast = super::run_with_timeout(std::process::Command::new("echo").arg("hi"), std::time::Duration::from_secs(5));
        assert_eq!(String::from_utf8_lossy(&fast.ok().unwrap().stdout).trim(), "hi");
    }

    #[test]
    fn opens_wal_database_without_shm() {
        let dir = std::env::temp_dir().join(format!("bb util wal {}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("Cloud Recordings.db");
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE t (x); INSERT INTO t VALUES (42);").unwrap();
        }
        // The state after the owning app quits: no side files. (SQLite removes them on
        // close on Linux; Apple's SQLite may keep them, so remove them explicitly.)
        for side in ["Cloud Recordings.db-wal", "Cloud Recordings.db-shm"] {
            let _ = std::fs::remove_file(dir.join(side));
        }
        // Make the folder read-only so SQLite can't create -shm (as for a read-only open).
        let mut perms = std::fs::metadata(&dir).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o555);
        std::fs::set_permissions(&dir, perms).unwrap();
        let (conn, snapshot) = super::open_readonly(&db).unwrap();
        let x: i64 = conn.query_row("SELECT x FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(x, 42);
        let mut perms = std::fs::metadata(&dir).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
        std::fs::set_permissions(&dir, perms).unwrap();
        assert!(snapshot, "the plain read-only open should have failed (that was the bug)");
        assert_eq!(super::open_readonly(&dir.join("missing.db")).unwrap_err().code, bb_proto::ErrorCode::NotFound);
        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;

    #[test]
    fn apple_time_conversions() {
        // 2023-01-01T00:00:00Z
        let unix_ms = 1_672_531_200_000;
        let nanos = unix_ms_to_apple_nanos(unix_ms);
        assert_eq!(apple_nanos_to_unix_ms(nanos), unix_ms);
        assert_eq!(apple_nanos_to_unix_ms(nanos / 1_000_000_000), unix_ms);
        assert_eq!(apple_secs_to_unix_ms(694_224_000.0), unix_ms);
    }
}
