//! Contact names for chat handles, from the AddressBook databases.
//!
//! chat.db only stores phone numbers and emails. Messages.app looks names up in Contacts;
//! we do the same by reading `AddressBook-v22.abcddb` (the local one plus one per synced
//! account under `Sources/`). Full Disk Access covers these files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bb_proto::{ApiError, ContactName, Response};
use rusqlite::Connection;

use crate::util::{self, columns};

pub struct Contacts {
    root: PathBuf,
}

/// The key a phone number or email is matched by: lowercase email, or the last ten digits
/// of a phone number (so "+1 (707) 339-5660" matches "7073395660"). Short codes keep all.
pub(crate) fn key(handle: &str) -> Option<String> {
    let handle = handle.trim();
    if handle.contains('@') {
        return Some(handle.to_lowercase());
    }
    let digits: String = handle.chars().filter(char::is_ascii_digit).collect();
    match digits.len() {
        0 => None,
        n if n > 10 => Some(digits[n - 10..].to_string()),
        _ => Some(digits),
    }
}

fn display_name(first: Option<String>, last: Option<String>, nick: Option<String>, org: Option<String>) -> Option<String> {
    let clean = |s: Option<String>| s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let full = [clean(first), clean(last)].into_iter().flatten().collect::<Vec<_>>().join(" ");
    Some(full).filter(|f| !f.is_empty()).or_else(|| clean(nick)).or_else(|| clean(org))
}

impl Contacts {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn default_root() -> PathBuf {
        util::home().join("Library/Application Support/AddressBook")
    }

    pub(crate) fn databases(&self) -> Vec<PathBuf> {
        let mut dbs = vec![self.root.join("AddressBook-v22.abcddb")];
        if let Ok(sources) = std::fs::read_dir(self.root.join("Sources")) {
            dbs.extend(sources.flatten().map(|s| s.path().join("AddressBook-v22.abcddb")));
        }
        dbs.retain(|p| p.exists());
        dbs
    }

    /// Phone/email key → name, across every address book.
    pub fn index(&self) -> HashMap<String, String> {
        let mut index = HashMap::new();
        for db in self.databases() {
            if let Err(err) = read_db(&db, &mut index) {
                eprintln!("contacts: {}: {err}", db.display());
            }
        }
        index
    }

    /// Names for the given chat handles (handles without a contact are left out).
    pub fn names(&self, handles: &[String]) -> Result<Response, ApiError> {
        let index = self.index();
        let names = handles
            .iter()
            .filter_map(|h| Some(ContactName { handle: h.clone(), name: index.get(&key(h)?)?.clone() }))
            .collect();
        Ok(Response::Contacts(names))
    }
}

fn read_db(path: &Path, index: &mut HashMap<String, String>) -> Result<(), ApiError> {
    let (conn, _snapshot): (Connection, bool) = util::open_readonly(path)?;
    let record = columns(&conn, "ZABCDRECORD");
    let col = |c: &str| if record.iter().any(|have| have == c) { format!("r.{c}") } else { "NULL".into() };
    let name_cols = format!("{}, {}, {}, {}", col("ZFIRSTNAME"), col("ZLASTNAME"), col("ZNICKNAME"), col("ZORGANIZATION"));
    let email_col = if columns(&conn, "ZABCDEMAILADDRESS").iter().any(|c| c == "ZADDRESSNORMALIZED") {
        "IFNULL(e.ZADDRESSNORMALIZED, e.ZADDRESS)"
    } else {
        "e.ZADDRESS"
    };
    let queries = [
        format!("SELECT p.ZFULLNUMBER, {name_cols} FROM ZABCDPHONENUMBER p JOIN ZABCDRECORD r ON r.Z_PK = p.ZOWNER"),
        format!("SELECT {email_col}, {name_cols} FROM ZABCDEMAILADDRESS e JOIN ZABCDRECORD r ON r.Z_PK = e.ZOWNER"),
    ];
    for sql in queries {
        let Ok(mut stmt) = conn.prepare(&sql) else { continue };
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, Option<String>>(0)?, display_name(r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))))
            .map_err(util::sql_err)?;
        for (handle, name) in rows.flatten() {
            if let (Some(k), Some(name)) = (handle.as_deref().and_then(key), name) {
                index.entry(k).or_insert(name);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_match_formats() {
        assert_eq!(key("+1 (707) 339-5660"), key("7073395660"));
        assert_eq!(key("+17073395660").as_deref(), Some("7073395660"));
        assert_eq!(key("Me@iCloud.com").as_deref(), Some("me@icloud.com"));
        assert_eq!(key("55512").as_deref(), Some("55512"));
        assert_eq!(key("no digits"), None);
    }

    #[test]
    fn names_from_address_books() {
        let root = std::env::temp_dir().join(format!("bb-contacts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let source = root.join("Sources/ABC-123");
        std::fs::create_dir_all(&source).unwrap();
        let schema = "CREATE TABLE ZABCDRECORD (Z_PK INTEGER PRIMARY KEY, ZFIRSTNAME TEXT, ZLASTNAME TEXT, ZNICKNAME TEXT, ZORGANIZATION TEXT);
             CREATE TABLE ZABCDPHONENUMBER (Z_PK INTEGER PRIMARY KEY, ZOWNER INTEGER, ZFULLNUMBER TEXT);
             CREATE TABLE ZABCDEMAILADDRESS (Z_PK INTEGER PRIMARY KEY, ZOWNER INTEGER, ZADDRESS TEXT, ZADDRESSNORMALIZED TEXT);";
        let local = Connection::open(root.join("AddressBook-v22.abcddb")).unwrap();
        local.execute_batch(schema).unwrap();
        local
            .execute_batch(
                "INSERT INTO ZABCDRECORD VALUES (1, 'Anthony', 'Rossi', NULL, NULL), (2, NULL, NULL, NULL, 'Dentist Co');
                 INSERT INTO ZABCDPHONENUMBER VALUES (1, 1, '(707) 555-0101'), (2, 2, '+1 707 555 0199');",
            )
            .unwrap();
        let synced = Connection::open(source.join("AddressBook-v22.abcddb")).unwrap();
        synced.execute_batch(schema).unwrap();
        synced
            .execute_batch(
                "INSERT INTO ZABCDRECORD VALUES (1, 'Sam', NULL, NULL, NULL);
                 INSERT INTO ZABCDEMAILADDRESS VALUES (1, 1, 'Sam@Example.com', 'sam@example.com');",
            )
            .unwrap();
        drop((local, synced));

        let handles = ["+17075550101", "sam@example.com", "+17075550199", "+19995550000"].map(String::from);
        let Response::Contacts(names) = Contacts::new(root.clone()).names(&handles).unwrap() else { panic!() };
        let got: Vec<(&str, &str)> = names.iter().map(|c| (c.handle.as_str(), c.name.as_str())).collect();
        assert_eq!(got, [("+17075550101", "Anthony Rossi"), ("sam@example.com", "Sam"), ("+17075550199", "Dentist Co")]);
        let _ = std::fs::remove_dir_all(root);
    }
}
