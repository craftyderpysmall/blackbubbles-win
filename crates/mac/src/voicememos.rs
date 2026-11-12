//! Voice Memos via `CloudRecordings.db` and the audio files next to it.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bb_proto::{ApiError, Event, EventSink, Response, VoiceMemo};
use rusqlite::Connection;

use crate::util::{self, apple_secs_to_unix_ms, columns, sql_err};

pub struct VoiceMemos {
    dir: PathBuf,
    cache: PathBuf,
}

struct Row {
    id: String,
    title: String,
    created: i64,
    duration: f64,
    path: PathBuf,
}

impl VoiceMemos {
    pub fn new(dir: PathBuf, cache: PathBuf) -> Self {
        Self { dir, cache }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The recordings folder differs between macOS versions.
    pub fn default_dir() -> PathBuf {
        let home = util::home();
        let mut candidates = vec![
            home.join("Library/Group Containers/group.com.apple.VoiceMemos.shared/Recordings"),
            home.join("Library/Application Support/com.apple.voicememos/Recordings"),
        ];
        // Other group-container names Apple has used or may use.
        if let Ok(entries) = std::fs::read_dir(home.join("Library/Group Containers")) {
            candidates.extend(
                entries
                    .flatten()
                    .filter(|e| e.file_name().to_string_lossy().to_lowercase().contains("voicememos"))
                    .map(|e| e.path().join("Recordings")),
            );
        }
        candidates.iter().find(|p| p.join("CloudRecordings.db").exists()).unwrap_or(&candidates[0]).clone()
    }

    fn rows(&self) -> Result<Vec<Row>, ApiError> {
        let db = self.dir.join("CloudRecordings.db");
        if !db.exists() {
            return Err(ApiError::new(
                bb_proto::ErrorCode::NotFound,
                "No Voice Memos library on this Mac yet. Open the Voice Memos app on the Mac once (with Voice Memos on in iCloud settings) so it downloads your recordings.",
            ));
        }
        let (conn, _snapshot): (Connection, bool) = util::open_readonly(&db)?;
        let cols = columns(&conn, "ZCLOUDRECORDING");
        let has = |c: &str| cols.iter().any(|have| have == c);
        let title_expr = ["ZCUSTOMLABELFORSORTING", "ZCUSTOMLABEL", "ZENCRYPTEDTITLE"]
            .iter()
            .filter(|c| has(c))
            .map(|c| format!("NULLIF({c}, '')"))
            .chain(["NULL".to_string()])
            .collect::<Vec<_>>()
            .join(", ");
        let id_expr = if has("ZUNIQUEID") { "IFNULL(ZUNIQUEID, Z_PK)" } else { "Z_PK" };
        let evicted = if has("ZEVICTIONDATE") { "ZEVICTIONDATE IS NULL" } else { "1" };
        let sql = format!(
            "SELECT {id_expr}, COALESCE({title_expr}), ZDATE, ZDURATION, ZPATH FROM ZCLOUDRECORDING
             WHERE ZPATH IS NOT NULL AND {evicted} ORDER BY ZDATE DESC"
        );
        let mut stmt = conn.prepare(&sql).map_err(sql_err)?;
        let rows = stmt
            .query_map([], |r| {
                let id: rusqlite::types::Value = r.get(0)?;
                let path: String = r.get(4)?;
                let path = if Path::new(&path).is_absolute() { PathBuf::from(path) } else { self.dir.join(path) };
                Ok(Row {
                    id: match id {
                        rusqlite::types::Value::Text(t) => t,
                        rusqlite::types::Value::Integer(i) => i.to_string(),
                        other => format!("{other:?}"),
                    },
                    title: r.get::<_, Option<String>>(1)?.unwrap_or_else(|| "New Recording".into()),
                    created: apple_secs_to_unix_ms(r.get::<_, Option<f64>>(2)?.unwrap_or(0.0)),
                    duration: r.get::<_, Option<f64>>(3)?.unwrap_or(0.0),
                    path,
                })
            })
            .and_then(Iterator::collect)
            .map_err(sql_err)?;
        Ok(rows)
    }

    pub fn list(&self) -> Result<Response, ApiError> {
        let memos = self
            .rows()?
            .into_iter()
            .map(|r| VoiceMemo {
                bytes: std::fs::metadata(&r.path).map(|m| m.len()).unwrap_or(0),
                id: r.id,
                title: r.title,
                created: r.created,
                duration_secs: r.duration,
            })
            .collect();
        Ok(Response::VoiceMemos(memos))
    }

    pub fn blob(&self, id: &str, offset: u64, len: u32) -> Result<Response, ApiError> {
        let row = self.rows()?.into_iter().find(|r| r.id == id).ok_or_else(|| ApiError::not_found("recording"))?;
        let is_m4a = row.path.extension().is_some_and(|e| e.eq_ignore_ascii_case("m4a"));
        if is_m4a {
            return util::read_blob(&row.path, offset, len, Some("audio/mp4".into()));
        }
        // Newer recordings may be .qta; convert once to AAC .m4a, which clients can play.
        let converted = util::cached_conversion(&self.cache.join("voicememos"), id, "m4a", |out| {
            std::process::Command::new("/usr/bin/afconvert")
                .args(["-f", "m4af", "-d", "aac"])
                .arg(&row.path)
                .arg(out)
                .output()
                .map(|o| o.status.success())
        })?;
        util::read_blob(&converted, offset, len, Some("audio/mp4".into()))
    }

    pub fn watch(self: std::sync::Arc<Self>, sink: EventSink) {
        std::thread::spawn(move || {
            let db = self.dir.join("CloudRecordings.db");
            let mtime = || {
                let wal = db.with_extension("db-wal");
                [&db, &wal].iter().filter_map(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok()).max()
            };
            let mut last = mtime();
            loop {
                std::thread::sleep(Duration::from_secs(5));
                let now = mtime();
                if now != last {
                    last = now;
                    sink(Event::VoiceMemosChanged);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_recordings() {
        let dir = std::env::temp_dir().join(format!("bbmac-vm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _ = std::fs::remove_file(dir.join("CloudRecordings.db"));
        let conn = Connection::open(dir.join("CloudRecordings.db")).unwrap();
        conn.execute_batch(
            "CREATE TABLE ZCLOUDRECORDING (Z_PK INTEGER PRIMARY KEY, ZUNIQUEID TEXT, ZCUSTOMLABEL TEXT, ZENCRYPTEDTITLE TEXT,
                ZDATE REAL, ZDURATION REAL, ZPATH TEXT, ZEVICTIONDATE REAL);
             INSERT INTO ZCLOUDRECORDING VALUES (1, 'u1', '', 'Song idea', 700000000, 12.5, 'a.m4a', NULL);
             INSERT INTO ZCLOUDRECORDING VALUES (2, 'u2', 'Lecture', NULL, 700000100, 60, 'b.m4a', NULL);
             INSERT INTO ZCLOUDRECORDING VALUES (3, 'u3', 'In iCloud', NULL, 700000200, 5, 'c.m4a', 700000300);",
        )
        .unwrap();
        std::fs::write(dir.join("a.m4a"), b"fake audio").unwrap();
        let memos = VoiceMemos::new(dir.clone(), dir.join("cache"));
        let Response::VoiceMemos(list) = memos.list().unwrap() else { panic!() };
        assert_eq!(list.iter().map(|m| m.title.as_str()).collect::<Vec<_>>(), ["Lecture", "Song idea"]);
        assert_eq!(list[1].bytes, 10);
        let Response::Blob { data, total, .. } = memos.blob("u1", 5, 100).unwrap() else { panic!() };
        assert_eq!((data.as_slice(), total), (&b"audio"[..], 10));
        let _ = std::fs::remove_dir_all(dir);
    }
}
