//! iMessage / SMS via `~/Library/Messages/chat.db` (read) and AppleScript (send).

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use bb_proto::{ApiError, Attachment, Chat, Event, EventSink, GroupEvent, Message, Response, Service, Timestamp, UnreadCount};
use rusqlite::{Connection, OptionalExtension, params};

use crate::typedstream;
use crate::util::{self, apple_nanos_to_unix_ms, sql_err, unix_ms_to_apple_nanos};

pub struct Messages {
    db_path: PathBuf,
    cache: PathBuf,
    conn: Mutex<Option<Connection>>,
}

fn service(name: Option<&str>) -> Service {
    match name.unwrap_or_default() {
        "SMS" => Service::Sms,
        "RCS" => Service::Rcs,
        _ => Service::IMessage,
    }
}

const MESSAGE_COLUMNS: &str = "m.ROWID, m.guid, m.text, m.attributedBody, m.is_from_me, m.date, h.id, m.service, cmj.chat_id, \
    IFNULL(m.item_type, 0), IFNULL(m.group_action_type, 0), (SELECT oh.id FROM handle oh WHERE oh.ROWID = m.other_handle), m.group_title";

/// Rows worth showing: ordinary messages and the group events we can describe. Other
/// non-zero item types (location sharing, FaceTime, "kept" audio…) carry no text.
const SHOWN: &str = "m.associated_message_type = 0 AND (IFNULL(m.item_type, 0) = 0 OR m.item_type IN (1, 2, 3))";

/// Decode a group event from `item_type` / `group_action_type` (as Messages stores them).
pub(crate) fn group_event(item_type: i64, action: i64, other: Option<String>, title: Option<String>) -> Option<GroupEvent> {
    match (item_type, action) {
        (1, 0) => Some(GroupEvent::Added { who: other }),
        (1, 1) => Some(GroupEvent::Removed { who: other }),
        (2, _) => Some(GroupEvent::Renamed { name: title.filter(|t| !t.is_empty()) }),
        (3, 0) => Some(GroupEvent::Left),
        (3, 1) => Some(GroupEvent::PhotoChanged),
        (3, 2) => Some(GroupEvent::PhotoRemoved),
        _ => None,
    }
}

impl Messages {
    pub fn new(db_path: PathBuf, cache: PathBuf) -> Self {
        Self { db_path, cache, conn: Mutex::new(None) }
    }

    pub fn default_path() -> PathBuf {
        util::home().join("Library/Messages/chat.db")
    }

    fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> Result<T, ApiError>) -> Result<T, ApiError> {
        let mut guard = self.conn.lock().unwrap();
        if guard.is_none() {
            let (conn, snapshot) = util::open_readonly(&self.db_path)?;
            if snapshot {
                return f(&conn);
            }
            *guard = Some(conn);
        }
        f(guard.as_ref().unwrap())
    }

    fn chat_guid(conn: &Connection, chat_rowid: i64) -> rusqlite::Result<String> {
        conn.query_row("SELECT guid FROM chat WHERE ROWID = ?1", [chat_rowid], |r| r.get(0))
    }

    fn row_to_message(conn: &Connection, row: &rusqlite::Row) -> rusqlite::Result<(i64, Message, Service)> {
        let rowid: i64 = row.get(0)?;
        let text: Option<String> = row.get(2)?;
        let body: Option<Vec<u8>> = row.get(3)?;
        let text = text
            .and_then(|t| typedstream::clean(&t))
            .or_else(|| body.as_deref().and_then(typedstream::extract_text).and_then(|t| typedstream::clean(&t)));
        let is_from_me: bool = row.get::<_, i64>(4)? != 0;
        let chat_rowid: i64 = row.get(8)?;
        let event = group_event(row.get(9)?, row.get(10)?, row.get(11)?, row.get(12)?);
        let text = if event.is_some() { None } else { text };
        let message = Message {
            guid: row.get(1)?,
            chat_guid: Self::chat_guid(conn, chat_rowid)?,
            sender: if is_from_me { None } else { row.get(6)? },
            is_from_me,
            text,
            date: apple_nanos_to_unix_ms(row.get(5)?),
            attachments: Self::attachments(conn, rowid)?,
            group_event: event,
        };
        Ok((rowid, message, service(row.get::<_, Option<String>>(7)?.as_deref())))
    }

    fn attachments(conn: &Connection, message_rowid: i64) -> rusqlite::Result<Vec<Attachment>> {
        let mut stmt = conn.prepare_cached(
            "SELECT a.guid, a.mime_type, a.transfer_name, a.total_bytes FROM message_attachment_join maj
             JOIN attachment a ON a.ROWID = maj.attachment_id WHERE maj.message_id = ?1",
        )?;
        stmt.query_map([message_rowid], |r| {
            Ok(Attachment {
                guid: r.get(0)?,
                mime_type: r.get(1)?,
                file_name: r.get(2)?,
                total_bytes: r.get::<_, Option<i64>>(3)?.unwrap_or(0).max(0) as u64,
            })
        })?
        .collect()
    }

    /// Every phone number / email Messages knows.
    pub fn handles(&self) -> Result<Vec<String>, ApiError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare_cached("SELECT DISTINCT id FROM handle WHERE id IS NOT NULL").map_err(sql_err)?;
            stmt.query_map([], |r| r.get(0)).and_then(Iterator::collect).map_err(sql_err)
        })
    }

    pub fn list_chats(&self, limit: u32, before: Option<Timestamp>) -> Result<Response, ApiError> {
        self.with_conn(|conn| {
            // macOS 11+ keeps an indexed copy of the date in the join table; using it avoids
            // touching every message row, which matters on large histories.
            let has_join_date = util::columns(conn, "chat_message_join").iter().any(|c| c == "message_date");
            // Your own address for the chat: `last_addressed_handle` (newer macOS) or
            // `account_login` ("E:me@icloud.com" / "P:+15551234567").
            let chat_cols = util::columns(conn, "chat");
            let mine = ["last_addressed_handle", "account_login"]
                .iter()
                .filter(|c| chat_cols.iter().any(|have| have == *c))
                .map(|c| format!("NULLIF(c.{c}, '')"))
                .chain(["NULL".to_string()])
                .collect::<Vec<_>>()
                .join(", ");
            let sql = if has_join_date {
                format!("SELECT c.ROWID, c.guid, c.display_name, c.chat_identifier, c.service_name, MAX(cmj.message_date) AS last, COALESCE({mine})
                 FROM chat c JOIN chat_message_join cmj ON cmj.chat_id = c.ROWID
                 GROUP BY c.ROWID HAVING ?1 IS NULL OR last < ?1
                 ORDER BY last DESC LIMIT ?2")
            } else {
                format!("SELECT c.ROWID, c.guid, c.display_name, c.chat_identifier, c.service_name, MAX(m.date) AS last, COALESCE({mine})
                 FROM chat c JOIN chat_message_join cmj ON cmj.chat_id = c.ROWID
                 JOIN message m ON m.ROWID = cmj.message_id
                 GROUP BY c.ROWID HAVING ?1 IS NULL OR last < ?1
                 ORDER BY last DESC LIMIT ?2")
            };
            let mut stmt = conn.prepare(&sql).map_err(sql_err)?;
            let before = before.map(unix_ms_to_apple_nanos);
            let rows: Vec<(i64, String, Option<String>, Option<String>, Option<String>, Option<String>)> = stmt
                .query_map(params![before, limit], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(6)?)))
                .and_then(Iterator::collect)
                .map_err(sql_err)?;
            let unread: std::collections::HashMap<String, u32> =
                Self::unread(conn)?.into_iter().map(|u| (u.chat_guid, u.count)).collect();
            let mut chats = Vec::with_capacity(rows.len());
            for (rowid, guid, display_name, identifier, service_name, mine) in rows {
                let mut handles = conn
                    .prepare_cached(
                        "SELECT h.id FROM chat_handle_join chj JOIN handle h ON h.ROWID = chj.handle_id WHERE chj.chat_id = ?1",
                    )
                    .map_err(sql_err)?;
                let mut participants: Vec<String> =
                    handles.query_map([rowid], |r| r.get(0)).and_then(Iterator::collect).map_err(sql_err)?;
                if participants.is_empty()
                    && let Some(id) = identifier.clone().filter(|i| !i.is_empty())
                {
                    participants.push(id);
                }
                let last_message = conn
                    .prepare_cached(&format!(
                        "SELECT {MESSAGE_COLUMNS} FROM chat_message_join cmj JOIN message m ON m.ROWID = cmj.message_id
                         LEFT JOIN handle h ON h.ROWID = m.handle_id
                         WHERE cmj.chat_id = ?1 AND {SHOWN} ORDER BY m.date DESC LIMIT 1"
                    ))
                    .map_err(sql_err)?
                    .query_row([rowid], |r| Self::row_to_message(conn, r))
                    .optional()
                    .map_err(sql_err)?
                    .map(|(_, m, _)| m);
                // Only a real (group) name. Never fall back to the number: clients put the
                // contact name there, and a number in display_name hid it.
                let display_name = display_name.filter(|n| !n.trim().is_empty());
                let my_address = mine.map(|m| m.strip_prefix("E:").or_else(|| m.strip_prefix("P:")).unwrap_or(&m).to_string());
                let unread = unread.get(&guid).copied().unwrap_or(0);
                chats.push(Chat { guid, display_name, participants, service: service(service_name.as_deref()), last_message, my_address, unread });
            }
            Ok(Response::Chats(chats))
        })
    }

    /// Unread incoming messages per chat, as Messages counts them: `is_read = 0`, not from
    /// me, a visible message (no tapbacks or group events). Sorted by chat for comparison.
    fn unread(conn: &Connection) -> Result<Vec<UnreadCount>, ApiError> {
        conn.prepare_cached(
            "SELECT c.guid, COUNT(*) FROM message m JOIN chat_message_join cmj ON cmj.message_id = m.ROWID
             JOIN chat c ON c.ROWID = cmj.chat_id
             WHERE m.is_read = 0 AND m.is_from_me = 0 AND IFNULL(m.item_type, 0) = 0 AND m.associated_message_type = 0
             GROUP BY c.guid ORDER BY c.guid",
        )
        .map_err(sql_err)?
        .query_map([], |r| Ok(UnreadCount { chat_guid: r.get(0)?, count: r.get(1)? }))
        .and_then(Iterator::collect)
        .map_err(sql_err)
    }

    pub fn list_messages(&self, chat_guid: &str, limit: u32, before: Option<Timestamp>) -> Result<Response, ApiError> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare_cached(&format!(
                    "SELECT {MESSAGE_COLUMNS} FROM chat c JOIN chat_message_join cmj ON cmj.chat_id = c.ROWID
                     JOIN message m ON m.ROWID = cmj.message_id LEFT JOIN handle h ON h.ROWID = m.handle_id
                     WHERE c.guid = ?1 AND (?2 IS NULL OR m.date < ?2) AND {SHOWN}
                     ORDER BY m.date DESC LIMIT ?3"
                ))
                .map_err(sql_err)?;
            let before = before.map(unix_ms_to_apple_nanos);
            let mut messages: Vec<Message> = stmt
                .query_map(params![chat_guid, before, limit], |r| Self::row_to_message(conn, r).map(|(_, m, _)| m))
                .and_then(Iterator::collect)
                .map_err(sql_err)?;
            messages.reverse();
            Ok(Response::Messages(messages))
        })
    }

    pub fn attachment_blob(&self, guid: &str, offset: u64, len: u32) -> Result<Response, ApiError> {
        let (path, mime): (Option<String>, Option<String>) = self.with_conn(|conn| {
            conn.query_row("SELECT filename, mime_type FROM attachment WHERE guid = ?1", [guid], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()
                .map_err(sql_err)?
                .ok_or_else(|| ApiError::not_found("attachment"))
        })?;
        let path = util::expand_tilde(&path.ok_or_else(|| ApiError::not_found("attachment file"))?);
        let is_heic = mime.as_deref().is_some_and(|m| m.contains("heic") || m.contains("heif"))
            || path.extension().is_some_and(|e| e.eq_ignore_ascii_case("heic"));
        if is_heic {
            // Clients can't decode HEIC; convert once with the built-in `sips`.
            let jpeg = util::cached_conversion(&self.cache.join("attachments"), guid, "jpg", |out| {
                std::process::Command::new("/usr/bin/sips")
                    .args(["-s", "format", "jpeg", "-s", "formatOptions", "85"])
                    .arg(&path)
                    .arg("--out")
                    .arg(out)
                    .output()
                    .map(|o| o.status.success())
            })?;
            return util::read_blob(&jpeg, offset, len, Some("image/jpeg".into()));
        }
        util::read_blob(&path, offset, len, mime)
    }

    pub fn send_text(chat_guid: &str, text: &str) -> Result<Response, ApiError> {
        util::osascript(
            r#"on run argv
                tell application "Messages"
                    send (item 2 of argv) to chat id (item 1 of argv)
                end tell
            end run"#,
            &[chat_guid, text],
        )?;
        Ok(Response::Ok)
    }

    pub fn send_attachment(chat_guid: &str, file_name: &str, data: &[u8]) -> Result<Response, ApiError> {
        // Messages can only send files from places it may read, like ~/Pictures.
        let dir = util::home().join("Pictures/BlackBubbles");
        std::fs::create_dir_all(&dir).map_err(ApiError::internal)?;
        let safe: String = Path::new(file_name)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "attachment".into())
            .chars()
            .filter(|c| !c.is_control() && *c != '/')
            .collect();
        let path = dir.join(format!("{}-{safe}", crate::now_ms()));
        std::fs::write(&path, data).map_err(ApiError::internal)?;
        util::osascript(
            r#"on run argv
                set theFile to (POSIX file (item 2 of argv)) as alias
                tell application "Messages"
                    send theFile to chat id (item 1 of argv)
                end tell
            end run"#,
            &[chat_guid, &path.to_string_lossy()],
        )?;
        Ok(Response::Ok)
    }

    /// Poll for new rows and push them as events. Polling MAX(ROWID) is a cheap index lookup.
    pub fn watch(self: std::sync::Arc<Self>, sink: EventSink) {
        std::thread::spawn(move || {
            let mut last: Option<i64> = None;
            let mut last_unread: Option<Vec<UnreadCount>> = None;
            let mut tick: u64 = 0;
            loop {
                std::thread::sleep(Duration::from_millis(500));
                tick += 1;
                let result = self.with_conn(|conn| {
                    // Read state changes without new rows (read on the iPhone), so poll the
                    // unread counts too: every 2 s, an indexed query over unread rows only.
                    if tick.is_multiple_of(4) {
                        let unread = Self::unread(conn)?;
                        if last_unread.as_ref().is_some_and(|l| *l != unread) {
                            sink(Event::UnreadChanged(unread.clone()));
                        }
                        last_unread = Some(unread);
                    }
                    let max: i64 = conn.query_row("SELECT IFNULL(MAX(ROWID), 0) FROM message", [], |r| r.get(0)).map_err(sql_err)?;
                    let Some(prev) = last else {
                        last = Some(max);
                        return Ok(());
                    };
                    if max <= prev {
                        return Ok(());
                    }
                    let mut stmt = conn
                        .prepare_cached(&format!(
                            "SELECT {MESSAGE_COLUMNS} FROM message m JOIN chat_message_join cmj ON cmj.message_id = m.ROWID
                             LEFT JOIN handle h ON h.ROWID = m.handle_id
                             WHERE m.ROWID > ?1 AND {SHOWN} ORDER BY m.ROWID"
                        ))
                        .map_err(sql_err)?;
                    let rows: Vec<(i64, Message, Service)> =
                        stmt.query_map([prev], |r| Self::row_to_message(conn, r)).and_then(Iterator::collect).map_err(sql_err)?;
                    for (_, message, service) in rows {
                        if let (false, Some(text)) = (message.is_from_me, &message.text) {
                            sink(Event::IncomingText {
                                sender: message.sender.clone().unwrap_or_default(),
                                text: text.clone(),
                                service,
                            });
                        }
                        sink(Event::NewMessage(message));
                    }
                    last = Some(max);
                    Ok(())
                });
                if let Err(err) = result {
                    eprintln!("messages watcher: {err}");
                    *self.conn.lock().unwrap() = None;
                    std::thread::sleep(Duration::from_secs(5));
                }
            }
        });
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn fixture(path: &Path) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE chat (ROWID INTEGER PRIMARY KEY, guid TEXT, display_name TEXT, chat_identifier TEXT, service_name TEXT, account_login TEXT);
             CREATE TABLE handle (ROWID INTEGER PRIMARY KEY, id TEXT);
             CREATE TABLE chat_handle_join (chat_id INTEGER, handle_id INTEGER);
             CREATE TABLE message (ROWID INTEGER PRIMARY KEY, guid TEXT, text TEXT, attributedBody BLOB, is_from_me INTEGER,
                 date INTEGER, handle_id INTEGER, service TEXT, associated_message_type INTEGER DEFAULT 0);
             CREATE TABLE chat_message_join (chat_id INTEGER, message_id INTEGER);
             CREATE TABLE attachment (ROWID INTEGER PRIMARY KEY, guid TEXT, filename TEXT, mime_type TEXT, transfer_name TEXT, total_bytes INTEGER);
             CREATE TABLE message_attachment_join (message_id INTEGER, attachment_id INTEGER);
             INSERT INTO chat VALUES (1, 'iMessage;-;+15550001', '', '+15550001', 'iMessage', 'P:+15559999');
             INSERT INTO chat VALUES (2, 'SMS;-;55512', NULL, '55512', 'SMS', 'E:me@icloud.com');
             INSERT INTO handle VALUES (1, '+15550001'), (2, '55512');
             INSERT INTO chat_handle_join VALUES (1, 1), (2, 2);
             INSERT INTO message VALUES (1, 'g1', 'hello', NULL, 0, 700000000000000000, 1, 'iMessage', 0);
             INSERT INTO message VALUES (2, 'g2', 'hi back', NULL, 1, 700000001000000000, 0, 'iMessage', 0);
             INSERT INTO message VALUES (3, 'g3', NULL, NULL, 0, 700000002000000000, 1, 'iMessage', 2000);
             INSERT INTO message VALUES (4, 'g4', 'Your code is 482913', NULL, 0, 700000003000000000, 2, 'SMS', 0);
             INSERT INTO chat_message_join VALUES (1, 1), (1, 2), (1, 3), (2, 4);
             INSERT INTO attachment VALUES (1, 'a1', '~/x.jpg', 'image/jpeg', 'x.jpg', 1234);
             INSERT INTO message_attachment_join VALUES (2, 1);
             ALTER TABLE message ADD COLUMN item_type INTEGER DEFAULT 0;
             ALTER TABLE message ADD COLUMN group_action_type INTEGER DEFAULT 0;
             ALTER TABLE message ADD COLUMN other_handle INTEGER DEFAULT 0;
             ALTER TABLE message ADD COLUMN group_title TEXT;
             ALTER TABLE message ADD COLUMN is_read INTEGER DEFAULT 1;
             UPDATE message SET is_read = 0 WHERE ROWID IN (3, 4);",
        )
        .unwrap();
    }

    #[test]
    fn group_events() {
        let dir = std::env::temp_dir().join(format!("bbmac-msg3-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("chat.db");
        let _ = std::fs::remove_file(&db);
        fixture(&db);
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "INSERT INTO chat VALUES (3, 'iMessage;+;chat99', 'Trip', 'chat99', 'iMessage', 'P:+15559999');
             INSERT INTO handle VALUES (3, '+15550003');
             INSERT INTO chat_handle_join VALUES (3, 1), (3, 3);
             INSERT INTO message (ROWID, guid, text, is_from_me, date, handle_id, service, item_type, group_action_type, other_handle, group_title) VALUES
               (10, 'e1', NULL, 1, 700000010000000000, 0, 'iMessage', 2, 0, 0, 'Weekend trip'),
               (11, 'e2', NULL, 1, 700000011000000000, 0, 'iMessage', 1, 0, 3, NULL),
               (12, 'e3', NULL, 0, 700000012000000000, 1, 'iMessage', 3, 0, 0, NULL),
               (13, 'e4', NULL, 0, 700000013000000000, 3, 'iMessage', 4, 0, 0, NULL),
               (14, 'e5', 'hey all', 0, 700000014000000000, 3, 'iMessage', 0, 0, 0, NULL),
               (15, 'e6', NULL, 1, 700000015000000000, 0, 'iMessage', 3, 0, 0, NULL);
             INSERT INTO chat_message_join VALUES (3, 10), (3, 11), (3, 12), (3, 13), (3, 14), (3, 15);",
        )
        .unwrap();
        drop(conn);
        let messages = Messages::new(db, dir.clone());
        let Response::Messages(list) = messages.list_messages("iMessage;+;chat99", 20, None).unwrap() else { panic!() };
        let events: Vec<_> = list.iter().map(|m| (m.guid.as_str(), m.is_from_me, m.sender.as_deref(), m.group_event.clone())).collect();
        assert_eq!(
            events,
            [
                ("e1", true, None, Some(GroupEvent::Renamed { name: Some("Weekend trip".into()) })),
                ("e2", true, None, Some(GroupEvent::Added { who: Some("+15550003".into()) })),
                ("e3", false, Some("+15550001"), Some(GroupEvent::Left)),
                // e4 (item_type 4, location sharing) has nothing to show and is skipped.
                ("e5", false, Some("+15550003"), None),
                ("e6", true, None, Some(GroupEvent::Left)),
            ]
        );
        assert_eq!(group_event(3, 1, None, None), Some(GroupEvent::PhotoChanged));
        assert_eq!(group_event(1, 1, Some("x".into()), None), Some(GroupEvent::Removed { who: Some("x".into()) }));
        assert_eq!(group_event(6, 0, None, None), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn uses_join_table_dates_when_present() {
        let dir = std::env::temp_dir().join(format!("bbmac-msg2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("chat.db");
        let _ = std::fs::remove_file(&db);
        fixture(&db);
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "ALTER TABLE chat_message_join ADD COLUMN message_date INTEGER;
             UPDATE chat_message_join SET message_date = (SELECT date FROM message WHERE ROWID = message_id);",
        )
        .unwrap();
        drop(conn);
        let Response::Chats(chats) = Messages::new(db, dir.clone()).list_chats(10, None).unwrap() else { panic!() };
        assert_eq!(chats.iter().map(|c| c.guid.as_str()).collect::<Vec<_>>(), ["SMS;-;55512", "iMessage;-;+15550001"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn lists_chats_and_messages() {
        let dir = std::env::temp_dir().join(format!("bbmac-msg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("chat.db");
        let _ = std::fs::remove_file(&db);
        fixture(&db);
        let messages = Messages::new(db, dir.clone());

        let Response::Chats(chats) = messages.list_chats(10, None).unwrap() else { panic!() };
        assert_eq!(chats.len(), 2);
        assert_eq!(chats[0].guid, "SMS;-;55512");
        assert_eq!(chats[0].service, Service::Sms);
        assert_eq!(chats[1].display_name, None, "an unnamed chat has no display_name, so contact names show");
        assert_eq!(chats[1].participants, ["+15550001"]);
        assert_eq!(chats[1].my_address.as_deref(), Some("+15559999"));
        assert_eq!(chats[0].my_address.as_deref(), Some("me@icloud.com"));
        // Unread: the code text counts; the unread tapback (associated_message_type 2000) doesn't.
        assert_eq!((chats[0].unread, chats[1].unread), (1, 0));
        // The tapback (associated_message_type 2000) is not the last message.
        assert_eq!(chats[1].last_message.as_ref().unwrap().text.as_deref(), Some("hi back"));

        let Response::Messages(list) = messages.list_messages("iMessage;-;+15550001", 10, None).unwrap() else { panic!() };
        assert_eq!(list.iter().map(|m| m.guid.as_str()).collect::<Vec<_>>(), ["g1", "g2"]);
        assert_eq!(list[0].sender.as_deref(), Some("+15550001"));
        assert!(list[1].is_from_me);
        assert_eq!(list[1].attachments[0].file_name.as_deref(), Some("x.jpg"));

        let before = list[1].date;
        let Response::Messages(older) = messages.list_messages("iMessage;-;+15550001", 10, Some(before)).unwrap() else { panic!() };
        assert_eq!(older.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
