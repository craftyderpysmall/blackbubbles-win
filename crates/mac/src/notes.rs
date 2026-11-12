//! Apple Notes via `NoteStore.sqlite` (read) and AppleScript (create/edit).
//!
//! Note bodies are gzip'd protobuf: `NoteStoreProto { 2: Document { 3: Note { 2: text } } }`.
//! IDs use the same `x-coredata://<store uuid>/ICNote/p<pk>` form as AppleScript, so the
//! client can pass them straight back for edits.

use std::io::Read;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use bb_proto::{ApiError, ErrorCode, Event, EventSink, Note, NoteFolder, NoteSummary, Response};
use rusqlite::{Connection, OptionalExtension};

use crate::util::{self, apple_secs_to_unix_ms, columns, escape_html, sql_err};

const OBJECTS: &str = "ZICCLOUDSYNCINGOBJECT";

pub struct Notes {
    db_path: PathBuf,
    conn: Mutex<Option<(Connection, Schema)>>,
}

struct Schema {
    uuid: String,
    note_ent: i64,
    folder_ent: i64,
    note_title: &'static str,
    folder_title: &'static str,
    modified: &'static str,
    folder_owner: Option<&'static str>,
    has_folder_type: bool,
    has_password: bool,
    has_deleted: bool,
    has_fetch_flag: bool,
}

fn pick(cols: &[String], options: &[&'static str]) -> Option<&'static str> {
    options.iter().copied().find(|c| cols.iter().any(|have| have == c))
}

impl Schema {
    fn load(conn: &Connection) -> Result<Self, ApiError> {
        let ent = |name: &str| -> Result<i64, ApiError> {
            conn.query_row("SELECT Z_ENT FROM Z_PRIMARYKEY WHERE Z_NAME = ?1", [name], |r| r.get(0))
                .map_err(|e| ApiError::internal(format!("unrecognized Notes database ({name}: {e})")))
        };
        let cols = columns(conn, OBJECTS);
        let need = |options: &[&'static str]| {
            pick(&cols, options).ok_or_else(|| ApiError::internal(format!("Notes database lacks {options:?}")))
        };
        Ok(Self {
            uuid: conn.query_row("SELECT Z_UUID FROM Z_METADATA", [], |r| r.get(0)).map_err(sql_err)?,
            note_ent: ent("ICNote")?,
            folder_ent: ent("ICFolder")?,
            note_title: need(&["ZTITLE1", "ZTITLE"])?,
            folder_title: need(&["ZTITLE2", "ZTITLE"])?,
            modified: need(&["ZMODIFICATIONDATE1", "ZMODIFICATIONDATE"])?,
            folder_owner: pick(&cols, &["ZOWNER", "ZACCOUNT"]),
            has_folder_type: cols.iter().any(|c| c == "ZFOLDERTYPE"),
            has_password: cols.iter().any(|c| c == "ZISPASSWORDPROTECTED"),
            has_deleted: cols.iter().any(|c| c == "ZMARKEDFORDELETION"),
            has_fetch_flag: cols.iter().any(|c| c == "ZNEEDSINITIALFETCHFROMCLOUD"),
        })
    }

    fn id(&self, kind: &str, pk: i64) -> String {
        format!("x-coredata://{}/{kind}/p{pk}", self.uuid)
    }

    fn not_deleted(&self, alias: &str) -> String {
        if self.has_deleted { format!("IFNULL({alias}.ZMARKEDFORDELETION, 0) = 0") } else { "1".into() }
    }

    /// Hides iCloud placeholders the Mac never downloaded (the owner's phantom "E" folder
    /// had ZNEEDSINITIALFETCHFROMCLOUD = 1). Notes.app doesn't show these either.
    fn fetched(&self, alias: &str) -> String {
        if self.has_fetch_flag { format!("IFNULL({alias}.ZNEEDSINITIALFETCHFROMCLOUD, 0) = 0") } else { "1".into() }
    }

    fn not_trash(&self, alias: &str) -> String {
        if self.has_folder_type { format!("IFNULL({alias}.ZFOLDERTYPE, 0) != 1") } else { "1".into() }
    }
}

/// `x-coredata://…/ICNote/p42` → 42
fn pk_from_id(id: &str) -> Option<i64> {
    id.rsplit_once("/p").and_then(|(_, n)| n.parse().ok())
}

// --- minimal protobuf reading -------------------------------------------------------

fn varint(buf: &[u8], pos: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *buf.get(*pos)?;
        *pos += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

/// The first length-delimited field `number` in a message.
fn field(buf: &[u8], number: u64) -> Option<&[u8]> {
    let mut pos = 0;
    while pos < buf.len() {
        let key = varint(buf, &mut pos)?;
        match key & 7 {
            0 => {
                varint(buf, &mut pos)?;
            }
            1 => pos += 8,
            2 => {
                let len = varint(buf, &mut pos)? as usize;
                let data = buf.get(pos..pos.checked_add(len)?)?;
                pos += len;
                if key >> 3 == number {
                    return Some(data);
                }
            }
            5 => pos += 4,
            _ => return None,
        }
    }
    None
}

pub fn decode_body(zdata: &[u8]) -> Option<String> {
    let mut raw = Vec::new();
    // Gzipped protobuf as stored; a decrypted body may also come un-gzipped.
    if flate2::read::GzDecoder::new(zdata).read_to_end(&mut raw).is_err() {
        raw = zdata.to_vec();
    }
    let text = field(field(field(&raw, 2)?, 3)?, 2)?;
    Some(String::from_utf8_lossy(text).replace('\u{fffc}', ""))
}

/// Plain text → the HTML body Notes expects. The first line becomes the title.
pub fn text_to_html(text: &str) -> String {
    text.lines()
        .map(|line| if line.is_empty() { "<div><br></div>".to_string() } else { format!("<div>{}</div>", escape_html(line)) })
        .collect()
}

fn self_get_unlocked(conn: &Connection, pk: i64, summary: NoteSummary) -> Result<Response, ApiError> {
    let data: Option<Vec<u8>> =
        conn.query_row("SELECT ZDATA FROM ZICNOTEDATA WHERE ZNOTE = ?1", [pk], |r| r.get(0)).optional().map_err(sql_err)?.flatten();
    Ok(Response::Note(Note { summary, body: data.as_deref().and_then(decode_body).unwrap_or_default() }))
}

impl Notes {
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path, conn: Mutex::new(None) }
    }

    pub fn default_path() -> PathBuf {
        util::home().join("Library/Group Containers/group.com.apple.notes/NoteStore.sqlite")
    }

    pub fn db_path(&self) -> &std::path::Path {
        &self.db_path
    }

    fn with_conn<T>(&self, f: impl FnOnce(&Connection, &Schema) -> Result<T, ApiError>) -> Result<T, ApiError> {
        let mut guard = self.conn.lock().unwrap();
        if guard.is_none() {
            let (conn, snapshot) = util::open_readonly(&self.db_path)?;
            let schema = Schema::load(&conn)?;
            if snapshot {
                // Notes isn't running; a snapshot must not be cached (it wouldn't see edits).
                return f(&conn, &schema);
            }
            *guard = Some((conn, schema));
        }
        let (conn, schema) = guard.as_ref().unwrap();
        let result = f(conn, schema);
        if result.is_err() {
            *guard = None;
        }
        result
    }

    pub fn folders(&self) -> Result<Response, ApiError> {
        self.with_conn(|conn, s| {
            let account = match s.folder_owner {
                Some(owner) => format!("(SELECT a.ZNAME FROM {OBJECTS} a WHERE a.Z_PK = f.{owner})"),
                None => "NULL".into(),
            };
            let sql = format!(
                "SELECT f.Z_PK, f.{title}, {account},
                    (SELECT COUNT(*) FROM {OBJECTS} n WHERE n.Z_ENT = {note} AND n.ZFOLDER = f.Z_PK AND {nd} AND {nf})
                 FROM {OBJECTS} f WHERE f.Z_ENT = {folder} AND {fd} AND {nt} AND {ff} ORDER BY f.{title} COLLATE NOCASE",
                title = s.folder_title,
                note = s.note_ent,
                folder = s.folder_ent,
                nd = s.not_deleted("n"),
                fd = s.not_deleted("f"),
                nt = s.not_trash("f"),
                ff = s.fetched("f"),
                nf = s.fetched("n"),
            );
            let mut stmt = conn.prepare_cached(&sql).map_err(sql_err)?;
            let folders = stmt
                .query_map([], |r| {
                    Ok(NoteFolder {
                        id: s.id("ICFolder", r.get(0)?),
                        name: r.get::<_, Option<String>>(1)?.unwrap_or_else(|| "Notes".into()),
                        account: r.get::<_, Option<String>>(2)?.unwrap_or_else(|| "On My Mac".into()),
                        count: r.get(3)?,
                    })
                })
                .and_then(Iterator::collect)
                .map_err(sql_err)?;
            Ok(Response::NoteFolders(folders))
        })
    }

    fn summary_sql(s: &Schema, filter: &str) -> String {
        format!(
            "SELECT n.Z_PK, n.{title}, n.ZSNIPPET, n.ZFOLDER, n.{modified}, {locked}
             FROM {OBJECTS} n LEFT JOIN {OBJECTS} f ON f.Z_PK = n.ZFOLDER
             WHERE n.Z_ENT = {note} AND {nd} AND {nt} AND {nf} AND n.ZFOLDER IS NOT NULL AND {filter}
             ORDER BY n.{modified} DESC",
            title = s.note_title,
            modified = s.modified,
            locked = if s.has_password { "IFNULL(n.ZISPASSWORDPROTECTED, 0)" } else { "0" },
            note = s.note_ent,
            nd = s.not_deleted("n"),
            nt = s.not_trash("f"),
            nf = s.fetched("n"),
        )
    }

    fn summary(s: &Schema, r: &rusqlite::Row) -> rusqlite::Result<(NoteSummary, bool)> {
        let locked: i64 = r.get(5)?;
        Ok((
            NoteSummary {
                id: s.id("ICNote", r.get(0)?),
                folder_id: r.get::<_, Option<i64>>(3)?.map(|pk| s.id("ICFolder", pk)).unwrap_or_default(),
                title: r.get::<_, Option<String>>(1)?.unwrap_or_else(|| "New Note".into()),
                snippet: if locked != 0 { "Locked".into() } else { r.get::<_, Option<String>>(2)?.unwrap_or_default() },
                modified: apple_secs_to_unix_ms(r.get::<_, Option<f64>>(4)?.unwrap_or(0.0)),
                locked: locked != 0,
            },
            locked != 0,
        ))
    }

    pub fn list(&self, folder_id: Option<&str>) -> Result<Response, ApiError> {
        self.with_conn(|conn, s| {
            let folder = folder_id.map(|id| pk_from_id(id).ok_or_else(|| ApiError::not_found("folder"))).transpose()?;
            let mut stmt = conn.prepare_cached(&Self::summary_sql(s, "(?1 IS NULL OR n.ZFOLDER = ?1)")).map_err(sql_err)?;
            let notes = stmt
                .query_map([folder], |r| Self::summary(s, r).map(|(n, _)| n))
                .and_then(Iterator::collect)
                .map_err(sql_err)?;
            Ok(Response::Notes(notes))
        })
    }

    pub fn get(&self, id: &str) -> Result<Response, ApiError> {
        let pk = pk_from_id(id).ok_or_else(|| ApiError::not_found("note"))?;
        self.with_conn(|conn, s| {
            let (summary, locked) = conn
                .query_row(&Self::summary_sql(s, "n.Z_PK = ?1"), [pk], |r| Self::summary(s, r))
                .optional()
                .map_err(sql_err)?
                .ok_or_else(|| ApiError::not_found("note"))?;
            let body = if locked {
                "🔒 This note is locked. Unlock it in Notes on your Mac.".to_string()
            } else {
                let data: Option<Vec<u8>> = conn
                    .query_row("SELECT ZDATA FROM ZICNOTEDATA WHERE ZNOTE = ?1", [pk], |r| r.get(0))
                    .optional()
                    .map_err(sql_err)?
                    .flatten();
                data.as_deref().and_then(decode_body).unwrap_or_default()
            };
            Ok(Response::Note(Note { summary, body }))
        })
    }

    /// Locked notes are encrypted with the user's Notes password; never overwrite one
    /// with the placeholder text the client was shown.
    pub fn ensure_unlocked(&self, id: &str) -> Result<(), ApiError> {
        match self.get(id)? {
            Response::Note(note) if note.summary.locked => {
                Err(ApiError::new(ErrorCode::PermissionDenied, "This note is locked. Unlock it in Notes on your Mac to edit it."))
            }
            _ => Ok(()),
        }
    }

    /// Decrypt a note locked with a Notes password. Read-only: Notes' AppleScript can't
    /// edit locked notes, and we never write Apple's database.
    pub fn unlock(&self, id: &str, password: &str) -> Result<Response, ApiError> {
        let pk = pk_from_id(id).ok_or_else(|| ApiError::not_found("note"))?;
        self.with_conn(|conn, s| {
            let (summary, locked) = conn
                .query_row(&Self::summary_sql(s, "n.Z_PK = ?1"), [pk], |r| Self::summary(s, r))
                .optional()
                .map_err(sql_err)?
                .ok_or_else(|| ApiError::not_found("note"))?;
            if !locked {
                return self_get_unlocked(conn, pk, summary);
            }
            let obj = columns(conn, OBJECTS);
            let data_cols = columns(conn, "ZICNOTEDATA");
            let o = |c: &str| if obj.iter().any(|h| h == c) { format!("n.{c}") } else { "NULL".into() };
            let d = |c: &str| if data_cols.iter().any(|h| h == c) { format!("d.{c}") } else { "NULL".into() };
            let sql = format!(
                "SELECT {salt}, {iter}, {wrapped}, COALESCE({div}, {oiv}), COALESCE({dtag}, {otag}), d.ZDATA,
                        COALESCE({oiv}, {div}), COALESCE({otag}, {dtag})
                 FROM {OBJECTS} n LEFT JOIN ZICNOTEDATA d ON d.ZNOTE = n.Z_PK WHERE n.Z_PK = ?1",
                salt = o("ZCRYPTOSALT"),
                iter = o("ZCRYPTOITERATIONCOUNT"),
                wrapped = o("ZCRYPTOWRAPPEDKEY"),
                div = d("ZCRYPTOINITIALIZATIONVECTOR"),
                oiv = o("ZCRYPTOINITIALIZATIONVECTOR"),
                dtag = d("ZCRYPTOTAG"),
                otag = o("ZCRYPTOTAG"),
            );
            type Blob = Option<Vec<u8>>;
            let ((salt, iterations, wrapped, iv, tag, data), (key_iv, key_tag)): ((Blob, Option<i64>, Blob, Blob, Blob, Blob), (Blob, Blob)) = conn
                .query_row(&sql, [pk], |r| Ok(((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?), (r.get(6)?, r.get(7)?))))
                .map_err(sql_err)?;
            let missing = || ApiError::unsupported("This note's encryption isn't readable here (it may be locked with your device passcode). Open it on your iPhone or Mac.");
            // Cipher version 2: the body is an ICCryptoEncryptionObject carrying its own salt
            // and wrapped key (see notecrypt).
            if let Some(obj) = data.as_deref().and_then(crate::notecrypt::parse_v2) {
                let (plain, how) = crate::notecrypt::decrypt_v2(&obj, iv.as_deref(), tag.as_deref(), password).map_err(|e| match e {
                    crate::notecrypt::UnlockError::WrongPassword => ApiError::new(ErrorCode::PermissionDenied, "Wrong password."),
                    crate::notecrypt::UnlockError::DevicePasscode => ApiError::unsupported(
                        "This note is locked with your iPhone or Mac passcode. Its key is kept in Apple's keychain, which only the Notes app can use, so open it on your iPhone or Mac.",
                    ),
                    crate::notecrypt::UnlockError::KeyOnly(how) => {
                        eprintln!("notes: unlock {id}: {how}, but no body layout decrypted");
                        ApiError::unsupported(
                            "Your password is correct and the note's key unlocked, but its text didn't decrypt. BlackBubbles can't read this note yet.",
                        )
                    }
                    _ => missing(),
                })?;
                eprintln!("notes: unlock {id}: decrypted via {how}");
                let body = decode_body(&plain).ok_or_else(|| {
                    eprintln!("notes: unlock {id}: decrypted {} bytes but they aren't a note body (starts {:02x?})", plain.len(), &plain[..plain.len().min(4)]);
                    ApiError::unsupported("The note decrypted, but its contents are in a format BlackBubbles can't read yet.")
                })?;
                return Ok(Response::Note(Note { summary, body }));
            }
            let (Some(salt), Some(iterations), Some(wrapped), Some(iv), Some(tag), Some(data)) = (salt, iterations, wrapped, iv, tag, data) else {
                return Err(missing());
            };
            // The note row's own IV/tag may differ from the note-data row's (they can belong
            // to the wrapped key rather than the body).
            let (key_iv, key_tag) = (key_iv.unwrap_or_else(|| iv.clone()), key_tag.unwrap_or_else(|| tag.clone()));
            let locked = crate::notecrypt::Locked {
                salt: &salt,
                iterations: iterations.clamp(0, 10_000_000) as u32,
                wrapped_key: &wrapped,
                key_iv: &key_iv,
                key_tag: &key_tag,
                iv: &iv,
                tag: &tag,
                data: &data,
            };
            // The account (via the note's folder) holds the password salt/iterations/verifier
            // in the account-level scheme.
            let account_row: Option<(Blob, Option<i64>, Blob)> = match s.folder_owner {
                Some(owner) if obj.iter().any(|c| c == "ZCRYPTOVERIFIER") => conn
                    .query_row(
                        &format!(
                            "SELECT a.ZCRYPTOSALT, a.ZCRYPTOITERATIONCOUNT, a.ZCRYPTOVERIFIER FROM {OBJECTS} n
                             JOIN {OBJECTS} f ON f.Z_PK = n.ZFOLDER JOIN {OBJECTS} a ON a.Z_PK = f.{owner} WHERE n.Z_PK = ?1"
                        ),
                        [pk],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )
                    .optional()
                    .map_err(sql_err)?,
                _ => None,
            };
            let account = match &account_row {
                Some((Some(salt), Some(iterations), Some(verifier))) if *iterations > 0 => Some(crate::notecrypt::Account {
                    salt,
                    iterations: (*iterations).min(10_000_000) as u32,
                    verifier,
                }),
                _ => None,
            };
            let (plain, how) = crate::notecrypt::decrypt_traced(&locked, account.as_ref(), password).map_err(|e| match e {
                crate::notecrypt::UnlockError::WrongPassword => ApiError::new(ErrorCode::PermissionDenied, "Wrong password."),
                crate::notecrypt::UnlockError::Unsupported => {
                    eprintln!("notes: unlock {id}: password verified, no known construction decrypts the note");
                    ApiError::unsupported(
                        "Your password is correct, but this note uses an encryption format BlackBubbles can't read yet. Open it on your iPhone or Mac.",
                    )
                }
                crate::notecrypt::UnlockError::KeyOnly(how) => {
                    eprintln!("notes: unlock {id}: note key unwrapped ({how}) but the body didn't decrypt");
                    ApiError::unsupported(
                        "Your password is correct and the note's key unlocked, but its text didn't decrypt. BlackBubbles can't read this note yet.",
                    )
                }
                crate::notecrypt::UnlockError::Corrupt | crate::notecrypt::UnlockError::DevicePasscode => missing(),
            })?;
            eprintln!("notes: unlock {id}: decrypted via {how}");
            let body = decode_body(&plain).ok_or_else(missing)?;
            Ok(Response::Note(Note { summary, body }))
        })
    }

    /// `NoteCryptoDump`: every stored field of a *locked* note, its note-data row, the
    /// accounts it points at and their account-data rows, as JSON with blobs in hex. The body
    /// stays encrypted; this is for working out a format offline (`examples/note_probe.rs`).
    pub fn crypto_dump(&self, id: &str) -> Result<Response, ApiError> {
        let pk = pk_from_id(id).ok_or_else(|| ApiError::not_found("note"))?;
        self.with_conn(|conn, s| {
            let (_, locked) = conn
                .query_row(&Self::summary_sql(s, "n.Z_PK = ?1"), [pk], |r| Self::summary(s, r))
                .optional()
                .map_err(sql_err)?
                .ok_or_else(|| ApiError::not_found("note"))?;
            if !locked {
                return Err(ApiError::new(ErrorCode::BadRequest, "Only locked notes can be dumped."));
            }
            let note = row_json(conn, &format!("SELECT * FROM {OBJECTS} WHERE Z_PK = ?1"), pk)?;
            let note_data = row_json(conn, "SELECT * FROM ZICNOTEDATA WHERE ZNOTE = ?1", pk)?;
            // Accounts: every ZACCOUNT* the note points at, plus its folder's owner.
            let mut account_pks: Vec<i64> = note
                .as_object()
                .into_iter()
                .flatten()
                .filter(|(k, _)| k.starts_with("ZACCOUNT") && !k.starts_with("ZACCOUNTNAME"))
                .filter_map(|(_, v)| v.as_i64())
                .collect();
            if let (Some(owner), Some(folder)) = (s.folder_owner, note.get("ZFOLDER").and_then(|v| v.as_i64()))
                && let Ok(a) = conn.query_row(&format!("SELECT {owner} FROM {OBJECTS} WHERE Z_PK = ?1"), [folder], |r| r.get::<_, i64>(0))
            {
                account_pks.push(a);
            }
            account_pks.sort();
            account_pks.dedup();
            let mut accounts = serde_json::Map::new();
            let mut account_data = serde_json::Map::new();
            for a in account_pks {
                let row = row_json(conn, &format!("SELECT * FROM {OBJECTS} WHERE Z_PK = ?1"), a)?;
                if let Some(d) = row.get("ZACCOUNTDATA").and_then(|v| v.as_i64()) {
                    account_data.insert(d.to_string(), row_json(conn, &format!("SELECT * FROM {OBJECTS} WHERE Z_PK = ?1"), d)?);
                }
                accounts.insert(a.to_string(), row);
            }
            let dump = serde_json::json!({
                "note_id": id,
                "note": note,
                "note_data": note_data,
                "accounts": accounts,
                "account_data": account_data,
            });
            Ok(Response::Diagnostics(serde_json::to_string_pretty(&dump).map_err(|e| ApiError::internal(e.to_string()))?))
        })
    }

    /// Move a note to Recently Deleted. AppleScript returns before Notes.app has saved its
    /// database, so wait (up to 10 s) until the note has left the lists; otherwise the
    /// client's refetch sees it still there.
    pub fn delete(&self, id: &str) -> Result<Response, ApiError> {
        let pk = pk_from_id(id).ok_or_else(|| ApiError::not_found("note"))?;
        util::osascript(
            r#"on run argv
                tell application "Notes" to delete note id (item 1 of argv)
            end run"#,
            &[id],
        )?;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            let listed = self.with_conn(|conn, s| {
                conn.query_row(&Self::summary_sql(s, "n.Z_PK = ?1"), [pk], |_| Ok(()))
                    .optional()
                    .map(|r| r.is_some())
                    .map_err(sql_err)
            });
            if !listed.unwrap_or(false) {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        Ok(Response::Ok)
    }

    pub fn create(folder_id: Option<&str>, body: &str) -> Result<Response, ApiError> {
        let html = text_to_html(body);
        let mut args = vec![html.as_str()];
        args.extend(folder_id);
        let id = util::osascript(
            r#"on run argv
                tell application "Notes"
                    if (count of argv) > 1 then
                        set n to make new note at folder id (item 2 of argv) with properties {body:(item 1 of argv)}
                    else
                        set n to make new note with properties {body:(item 1 of argv)}
                    end if
                    return id of n
                end tell
            end run"#,
            &args,
        )?;
        Ok(Response::Created { id })
    }

    pub fn update(id: &str, body: &str) -> Result<Response, ApiError> {
        util::osascript(
            r#"on run argv
                tell application "Notes" to set body of note id (item 1 of argv) to (item 2 of argv)
            end run"#,
            &[id, &text_to_html(body)],
        )?;
        Ok(Response::Ok)
    }

    /// Emit `NotesChanged` when the newest modification date moves.
    /// Poll a fingerprint of every Notes object: row count, the sum of Core Data's per-row
    /// save counter (`Z_OPT`, bumped on *any* change, including a move to Recently Deleted,
    /// which leaves the modification date alone) and the newest modification date.
    pub fn watch(self: std::sync::Arc<Self>, sink: EventSink) {
        std::thread::spawn(move || {
            let mut last: Option<(i64, i64, f64)> = None;
            loop {
                std::thread::sleep(Duration::from_secs(2));
                if let Ok(now) = self.fingerprint() {
                    if last.is_some_and(|l| l != now) {
                        sink(Event::NotesChanged);
                    }
                    last = Some(now);
                }
            }
        });
    }

    fn fingerprint(&self) -> Result<(i64, i64, f64), ApiError> {
        self.with_conn(|conn, s| {
            conn.query_row(
                &format!("SELECT COUNT(*), IFNULL(SUM(Z_OPT), 0), IFNULL(MAX({}), 0) FROM {OBJECTS}", s.modified),
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(sql_err)
        })
    }
}

/// One row as a JSON object of its non-null columns; blobs as `{"hex": …}`.
fn row_json(conn: &Connection, sql: &str, pk: i64) -> Result<serde_json::Value, ApiError> {
    use rusqlite::types::ValueRef;
    let mut stmt = conn.prepare(sql).map_err(sql_err)?;
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let mut rows = stmt.query([pk]).map_err(sql_err)?;
    let mut out = serde_json::Map::new();
    if let Some(row) = rows.next().map_err(sql_err)? {
        for (i, name) in names.iter().enumerate() {
            let value = match row.get_ref(i).map_err(sql_err)? {
                ValueRef::Null => continue,
                ValueRef::Integer(n) => n.into(),
                ValueRef::Real(f) => f.into(),
                ValueRef::Text(t) => String::from_utf8_lossy(t).into(),
                ValueRef::Blob(b) => serde_json::json!({ "hex": b.iter().map(|x| format!("{x:02x}")).collect::<String>() }),
            };
            out.insert(name.clone(), value);
        }
    }
    Ok(out.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn proto_field(number: u64, data: &[u8]) -> Vec<u8> {
        let mut out = vec![((number << 3) | 2) as u8];
        let mut len = data.len();
        loop {
            let byte = (len & 0x7f) as u8;
            len >>= 7;
            out.push(if len > 0 { byte | 0x80 } else { byte });
            if len == 0 {
                break;
            }
        }
        out.extend_from_slice(data);
        out
    }

    pub fn note_blob(text: &str) -> Vec<u8> {
        let mut note = vec![0x08, 0x00]; // varint field 1 to exercise skipping
        note.extend(proto_field(2, text.as_bytes()));
        let doc = [vec![0x08, 0x01], proto_field(3, &note)].concat();
        let root = proto_field(2, &doc);
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&root).unwrap();
        gz.finish().unwrap()
    }

    #[test]
    fn decodes_note_body() {
        let long = format!("Groceries\n{}", "line\n".repeat(100));
        assert_eq!(decode_body(&note_blob(&long)).as_deref(), Some(long.as_str()));
    }

    #[test]
    fn reads_folders_notes_and_bodies() {
        let dir = std::env::temp_dir().join(format!("bbmac-notes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("NoteStore.sqlite");
        let _ = std::fs::remove_file(&db);
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE Z_PRIMARYKEY (Z_ENT INTEGER, Z_NAME TEXT);
             INSERT INTO Z_PRIMARYKEY VALUES (5, 'ICFolder'), (12, 'ICNote'), (14, 'ICAccount');
             CREATE TABLE Z_METADATA (Z_UUID TEXT);
             INSERT INTO Z_METADATA VALUES ('UUID-1');
             CREATE TABLE {OBJECTS} (Z_PK INTEGER PRIMARY KEY, Z_ENT INTEGER, ZTITLE1 TEXT, ZTITLE2 TEXT, ZSNIPPET TEXT,
                 ZFOLDER INTEGER, ZMODIFICATIONDATE1 REAL, ZMARKEDFORDELETION INTEGER, ZISPASSWORDPROTECTED INTEGER,
                 ZFOLDERTYPE INTEGER, ZOWNER INTEGER, ZNAME TEXT, ZNEEDSINITIALFETCHFROMCLOUD INTEGER);
             INSERT INTO {OBJECTS} (Z_PK, Z_ENT, ZNAME) VALUES (1, 14, 'iCloud');
             INSERT INTO {OBJECTS} (Z_PK, Z_ENT, ZTITLE2, ZOWNER, ZFOLDERTYPE) VALUES (2, 5, 'Notes', 1, 0), (3, 5, 'Recently Deleted', 1, 1);
             INSERT INTO {OBJECTS} (Z_PK, Z_ENT, ZTITLE1, ZSNIPPET, ZFOLDER, ZMODIFICATIONDATE1) VALUES
                 (10, 12, 'Groceries', 'milk', 2, 700000000.0),
                 (11, 12, 'Old', 'gone', 3, 700000001.0);
             INSERT INTO {OBJECTS} (Z_PK, Z_ENT, ZTITLE1, ZFOLDER, ZMODIFICATIONDATE1, ZISPASSWORDPROTECTED) VALUES
                 (12, 12, 'Secret', 2, 600000000.0, 1);
             -- From the owner's real Mac: a folder never downloaded from iCloud ('E'), and a
             -- note with no folder and date 0 (shown as Dec 31 2000). Notes.app hides both.
             INSERT INTO {OBJECTS} (Z_PK, Z_ENT, ZTITLE2, ZOWNER, ZFOLDERTYPE, ZNEEDSINITIALFETCHFROMCLOUD) VALUES (4, 5, 'E', 1, 0, 1);
             INSERT INTO {OBJECTS} (Z_PK, Z_ENT, ZTITLE1, ZFOLDER, ZMODIFICATIONDATE1) VALUES (13, 12, 'New Note', NULL, 0.0);
             CREATE TABLE ZICNOTEDATA (Z_PK INTEGER PRIMARY KEY, ZNOTE INTEGER, ZDATA BLOB);"
        ))
        .unwrap();
        conn.execute("INSERT INTO ZICNOTEDATA (ZNOTE, ZDATA) VALUES (10, ?1)", [note_blob("Groceries\nmilk")]).unwrap();
        drop(conn);

        let notes = Notes::new(db);
        let Response::NoteFolders(folders) = notes.folders().unwrap() else { panic!() };
        assert_eq!(folders.len(), 1, "trash and the never-downloaded folder are hidden");
        assert_eq!((folders[0].name.as_str(), folders[0].account.as_str(), folders[0].count), ("Notes", "iCloud", 2));

        let Response::Notes(list) = notes.list(Some(&folders[0].id)).unwrap() else { panic!() };
        assert_eq!(list.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(), ["Groceries", "Secret"]);
        let Response::Notes(all) = notes.list(None).unwrap() else { panic!() };
        assert!(all.iter().all(|n| n.title != "New Note"), "a note without a folder is hidden");
        assert_eq!(list[0].id, "x-coredata://UUID-1/ICNote/p10");

        let Response::Note(note) = notes.get(&list[0].id).unwrap() else { panic!() };
        assert_eq!(note.body, "Groceries\nmilk");
        let Response::Note(locked) = notes.get(&list[1].id).unwrap() else { panic!() };
        assert!(locked.body.contains("locked"));
        assert!(locked.summary.locked && !list[0].locked);
        assert!(notes.ensure_unlocked(&list[0].id).is_ok());
        assert_eq!(notes.ensure_unlocked(&list[1].id).unwrap_err().code, ErrorCode::PermissionDenied);

        // Encrypt a body for the locked note the way Notes does, then unlock it.
        let (wrapped, iv, tag, data) = crate::notecrypt::encrypt_for_test("pw", b"0123456789abcdef", 2000, &note_blob("Secret\nthe code is 1234"));
        let conn = Connection::open(&notes.db_path).unwrap();
        conn.execute_batch(
            "ALTER TABLE ZICCLOUDSYNCINGOBJECT ADD COLUMN ZCRYPTOSALT BLOB;
             ALTER TABLE ZICCLOUDSYNCINGOBJECT ADD COLUMN ZCRYPTOITERATIONCOUNT INTEGER;
             ALTER TABLE ZICCLOUDSYNCINGOBJECT ADD COLUMN ZCRYPTOWRAPPEDKEY BLOB;
             ALTER TABLE ZICNOTEDATA ADD COLUMN ZCRYPTOINITIALIZATIONVECTOR BLOB;
             ALTER TABLE ZICNOTEDATA ADD COLUMN ZCRYPTOTAG BLOB;",
        )
        .unwrap();
        conn.execute(
            "UPDATE ZICCLOUDSYNCINGOBJECT SET ZCRYPTOSALT = ?1, ZCRYPTOITERATIONCOUNT = 2000, ZCRYPTOWRAPPEDKEY = ?2 WHERE Z_PK = 12",
            rusqlite::params![b"0123456789abcdef".to_vec(), wrapped],
        )
        .unwrap();
        conn.execute("INSERT INTO ZICNOTEDATA (ZNOTE, ZDATA, ZCRYPTOINITIALIZATIONVECTOR, ZCRYPTOTAG) VALUES (12, ?1, ?2, ?3)", rusqlite::params![data, iv, tag])
            .unwrap();
        drop(conn);
        let notes = Notes::new(notes.db_path.clone()); // fresh connection sees the new columns
        let Response::Note(open) = notes.unlock(&list[1].id, "pw").unwrap() else { panic!() };
        assert_eq!(open.body, "Secret\nthe code is 1234");
        assert_eq!(notes.unlock(&list[1].id, "nope").unwrap_err().code, ErrorCode::PermissionDenied);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn html_body_escapes() {
        assert_eq!(text_to_html("a<b\n\nc"), "<div>a&lt;b</div><div><br></div><div>c</div>");
    }
}
