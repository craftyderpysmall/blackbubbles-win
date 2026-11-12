//! `Diagnose`: facts about the Mac's Apple databases, for debugging from a paired client
//! (`blackbubbles request '"Diagnose"'`). It runs inside bbd, so it sees exactly what bbd's
//! permissions allow. It reports structure and counts only: column names, flags, blob
//! lengths, folder titles and file paths. No note text, keys or contact details.

use std::fmt::Write;
use std::path::Path;

use rusqlite::types::ValueRef;
use rusqlite::{Connection, Row};

use crate::util;

pub fn report(notes_db: &Path, contacts: &crate::contacts::Contacts, handles: &[String], voice_dir: &Path) -> String {
    let mut out = String::new();
    section(&mut out, "Notes", |o| notes(o, notes_db));
    section(&mut out, "Contacts", |o| contacts_report(o, contacts, handles));
    section(&mut out, "Voice Memos", |o| voice(o, voice_dir));
    section(&mut out, "Automation permissions (TCC)", automation);
    out
}

type Res = Result<(), String>;

fn section(out: &mut String, title: &str, f: impl FnOnce(&mut String) -> Res) {
    let _ = writeln!(out, "== {title}");
    if let Err(e) = f(out) {
        let _ = writeln!(out, "  error: {e}");
    }
    out.push('\n');
}

fn open(path: &Path) -> Result<Connection, String> {
    util::open_readonly(path).map(|(c, _)| c).map_err(|e| e.message)
}

/// Non-null columns of a row, for the names given (or all when `only` is empty). Text is
/// shown only when `show_text`; numbers in full; blobs as their length.
fn describe(row: &Row, names: &[String], keep: impl Fn(&str) -> bool, show_text: bool) -> String {
    let mut parts = Vec::new();
    for (i, name) in names.iter().enumerate() {
        if !keep(name) {
            continue;
        }
        let Ok(value) = row.get_ref(i) else { continue };
        let shown = match value {
            ValueRef::Null => continue,
            ValueRef::Integer(n) => n.to_string(),
            ValueRef::Real(f) => format!("{f}"),
            ValueRef::Text(t) if show_text => format!("{:?}", String::from_utf8_lossy(t)),
            ValueRef::Text(t) => format!("<text {}>", t.len()),
            ValueRef::Blob(b) => format!("<blob {}>", b.len()),
        };
        parts.push(format!("{name}={shown}"));
    }
    parts.join(" ")
}

fn rows(out: &mut String, conn: &Connection, sql: &str, keep: impl Fn(&str) -> bool + Copy, show_text: bool) -> Res {
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let mut q = stmt.query([]).map_err(|e| e.to_string())?;
    let mut n = 0;
    while let Some(row) = q.next().map_err(|e| e.to_string())? {
        let _ = writeln!(out, "  {}", describe(row, &names, keep, show_text));
        n += 1;
    }
    if n == 0 {
        let _ = writeln!(out, "  (none)");
    }
    Ok(())
}

fn ent(conn: &Connection, name: &str) -> Option<i64> {
    conn.query_row("SELECT Z_ENT FROM Z_PRIMARYKEY WHERE Z_NAME = ?1", [name], |r| r.get(0)).ok()
}

fn notes(out: &mut String, db: &Path) -> Res {
    let conn = open(db)?;
    let cols = util::columns(&conn, "ZICCLOUDSYNCINGOBJECT");
    let interesting = |c: &str| ["CRYPTO", "PASSWORD", "AUTH", "SMART", "FOLDERTYPE", "PARENT", "MARKED", "DELETED", "HIDDEN", "ACCOUNT", "OWNER", "TITLE", "IDENTIFIER", "TYPE"].iter().any(|k| c.contains(k));
    let _ = writeln!(out, "  object columns of interest: {}", cols.iter().filter(|c| interesting(c)).cloned().collect::<Vec<_>>().join(", "));
    let _ = writeln!(out, "  note-data columns: {}", util::columns(&conn, "ZICNOTEDATA").join(", "));
    let entities: Vec<String> = conn
        .prepare("SELECT Z_NAME || '=' || Z_ENT FROM Z_PRIMARYKEY")
        .and_then(|mut s| s.query_map([], |r| r.get(0)).and_then(|r| r.collect()))
        .unwrap_or_default();
    let _ = writeln!(out, "  entities: {}", entities.join(", "));

    if let Some(folder) = ent(&conn, "ICFolder") {
        let _ = writeln!(out, "  -- folders (every non-null column; titles shown)");
        rows(out, &conn, &format!("SELECT * FROM ZICCLOUDSYNCINGOBJECT WHERE Z_ENT = {folder} ORDER BY Z_PK"), |c| c != "ZSERVERRECORDDATA" && c != "ZSERVERSHAREDATA", true)?;
    }
    if let Some(account) = ent(&conn, "ICAccount") {
        let _ = writeln!(out, "  -- accounts (crypto/password/auth columns; lengths only)");
        rows(
            out,
            &conn,
            &format!("SELECT * FROM ZICCLOUDSYNCINGOBJECT WHERE Z_ENT = {account}"),
            |c| c == "Z_PK" || c == "ZNAME" || ["CRYPTO", "PASSWORD", "AUTH", "ACCOUNTTYPE", "KEY"].iter().any(|k| c.contains(k)),
            false,
        )?;
    }
    if cols.iter().any(|c| c == "ZISPASSWORDPROTECTED") {
        let _ = writeln!(out, "  -- locked notes (crypto/account columns; lengths only)");
        rows(
            out,
            &conn,
            "SELECT * FROM ZICCLOUDSYNCINGOBJECT WHERE ZISPASSWORDPROTECTED = 1",
            |c| c == "Z_PK" || c == "Z_ENT" || ["CRYPTO", "PASSWORD", "AUTH", "ACCOUNT", "FOLDER", "KEY"].iter().any(|k| c.contains(k)),
            false,
        )?;
        let _ = writeln!(out, "  -- their note data (lengths only)");
        rows(
            out,
            &conn,
            "SELECT d.* FROM ZICNOTEDATA d JOIN ZICCLOUDSYNCINGOBJECT n ON n.Z_PK = d.ZNOTE WHERE n.ZISPASSWORDPROTECTED = 1",
            |_| true,
            false,
        )?;
    }
    Ok(())
}

fn contacts_report(out: &mut String, contacts: &crate::contacts::Contacts, handles: &[String]) -> Res {
    for db in contacts.databases() {
        match open(&db) {
            Ok(conn) => {
                let count = |t: &str| conn.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get::<_, i64>(0)).map_or_else(|e| e.to_string(), |n| n.to_string());
                let _ = writeln!(
                    out,
                    "  {}: records={} phones={} emails={}",
                    db.display(),
                    count("ZABCDRECORD"),
                    count("ZABCDPHONENUMBER"),
                    count("ZABCDEMAILADDRESS")
                );
            }
            Err(e) => {
                let _ = writeln!(out, "  {}: {e}", db.display());
            }
        }
    }
    let index = contacts.index();
    let phones: Vec<&String> = handles.iter().filter(|h| !h.contains('@')).collect();
    let matched = handles.iter().filter(|h| crate::contacts::key(h).is_some_and(|k| index.contains_key(&k))).count();
    // A looser rule (last 7 digits) that would match many more means a number-format issue.
    let short: std::collections::HashSet<String> = index.keys().filter(|k| !k.contains('@') && k.len() >= 7).map(|k| k[k.len() - 7..].to_string()).collect();
    let loose = phones
        .iter()
        .filter(|h| {
            let d: String = h.chars().filter(char::is_ascii_digit).collect();
            d.len() >= 7 && short.contains(&d[d.len() - 7..])
        })
        .count();
    let _ = writeln!(
        out,
        "  contact keys={} chat handles={} (phones {}) matched={} would-match-by-last-7-digits={}",
        index.len(),
        handles.len(),
        phones.len(),
        matched,
        loose
    );
    Ok(())
}

fn voice(out: &mut String, dir: &Path) -> Res {
    let home = util::home();
    let _ = writeln!(out, "  using: {}", dir.display());
    for candidate in [
        "Library/Group Containers/group.com.apple.VoiceMemos.shared/Recordings",
        "Library/Application Support/com.apple.voicememos/Recordings",
        "Library/Containers/com.apple.VoiceMemos/Data/Library/Application Support/Recordings",
    ] {
        let path = home.join(candidate);
        let files = std::fs::read_dir(&path).map(|d| d.count().to_string()).unwrap_or_else(|e| e.to_string());
        let _ = writeln!(out, "  {candidate}: {files} entries");
    }
    if let Ok(entries) = std::fs::read_dir(home.join("Library/Group Containers")) {
        let names: Vec<String> = entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.to_lowercase().contains("voice"))
            .collect();
        let _ = writeln!(out, "  group containers mentioning voice: {names:?}");
    }
    let db = dir.join("CloudRecordings.db");
    if db.exists() {
        let conn = open(&db)?;
        let count = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0)).map_or_else(|e| e.to_string(), |n| n.to_string());
        let _ = writeln!(
            out,
            "  recordings={} with-path={}",
            count("SELECT COUNT(*) FROM ZCLOUDRECORDING"),
            count("SELECT COUNT(*) FROM ZCLOUDRECORDING WHERE ZPATH IS NOT NULL")
        );
    }
    Ok(())
}

fn automation(out: &mut String) -> Res {
    let db = util::home().join("Library/Application Support/com.apple.TCC/TCC.db");
    let conn = open(&db)?;
    rows(
        out,
        &conn,
        "SELECT service, client, indirect_object_identifier, auth_value FROM access
         WHERE client LIKE '%bbd%' OR client LIKE '%craftyderpysmall%'",
        |_| true,
        true,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn reports_without_panicking() {
        let dir = std::env::temp_dir().join(format!("bb-diag-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("NoteStore.sqlite");
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE Z_PRIMARYKEY (Z_ENT INTEGER, Z_NAME TEXT);
             INSERT INTO Z_PRIMARYKEY VALUES (5, 'ICFolder'), (12, 'ICNote'), (14, 'ICAccount');
             CREATE TABLE ZICCLOUDSYNCINGOBJECT (Z_PK INTEGER PRIMARY KEY, Z_ENT INTEGER, ZTITLE2 TEXT, ZFOLDERTYPE INTEGER,
                 ZISPASSWORDPROTECTED INTEGER, ZCRYPTOSALT BLOB, ZNAME TEXT, ZSERVERRECORDDATA BLOB);
             INSERT INTO ZICCLOUDSYNCINGOBJECT VALUES (78, 5, 'E', 3, NULL, NULL, NULL, x'00'), (14, 14, NULL, NULL, NULL, x'0102', 'iCloud', NULL),
                 (90, 12, NULL, NULL, 1, x'010203', NULL, NULL);
             CREATE TABLE ZICNOTEDATA (Z_PK INTEGER PRIMARY KEY, ZNOTE INTEGER, ZDATA BLOB);
             INSERT INTO ZICNOTEDATA VALUES (1, 90, x'0011');",
        )
        .unwrap();
        drop(conn);
        let contacts = crate::contacts::Contacts::new(dir.join("missing-addressbook"));
        let text = super::report(&db, &contacts, &["+15550001".into()], &dir.join("no-voice"));
        assert!(text.contains("ZTITLE2=\"E\" ZFOLDERTYPE=3"), "{text}");
        assert!(text.contains("ZCRYPTOSALT=<blob 3>"), "{text}");
        assert!(!text.contains("ZSERVERRECORDDATA"), "{text}");
        assert!(text.contains("== Automation permissions (TCC)"), "{text}");
        let _ = std::fs::remove_dir_all(dir);
    }
}
