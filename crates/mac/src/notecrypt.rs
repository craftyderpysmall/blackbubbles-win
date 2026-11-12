//! Password-protected Notes (the "separate password" kind), decrypted read-only.
//!
//! A locked note stores a salt, an iteration count and a wrapped key on the note, and an
//! IV and GCM tag next to its encrypted body:
//!
//! 1. key-encryption key = PBKDF2-HMAC-SHA256(password, salt, iterations), 16 bytes
//! 2. note key = AES key unwrap (RFC 3394) of the wrapped key with that KEK
//! 3. body = AES-128-GCM(note key, 16-byte IV, tag) → the usual gzipped protobuf
//!
//! A wrong password fails the key-unwrap integrity check in step 2.
//!
//! Account-level scheme (the owner's real database, 2026-11-06): the *account* row holds
//! the salt, 20000 iterations and a 24-byte `ZCRYPTOVERIFIER`; the note has iterations 0,
//! its own 16-byte salt, a 16-byte `ZCRYPTOWRAPPEDKEY`, and an IV and tag on *both* its own
//! row and its note-data row. Unwrapping the verifier with the password's KEK proves the
//! password (AES-KW integrity) and yields an account key. How the note key derives from it
//! isn't documented, so [`decrypt`] tries a matrix of constructions (key derivations ×
//! unwrap methods, including an AES-GCM-wrapped key using the note row's IV/tag) and accepts
//! only one whose AES-GCM tag verifies, so it never returns wrong plaintext. The winning
//! construction's label is logged, so it can be pinned down; `examples/note_probe.rs` runs
//! a much larger matrix offline against a `NoteCryptoDump`.

use aes::Aes128;
use aes::cipher::{BlockDecrypt, KeyInit, generic_array::GenericArray};
use aes_gcm::aead::{Aead, Payload, consts::U16};
use aes_gcm::{AesGcm, Nonce};
use sha2::Sha256;

type Aes128Gcm16 = AesGcm<Aes128, U16>;

const KW_IV: u64 = 0xA6A6_A6A6_A6A6_A6A6;

#[derive(Debug, PartialEq)]
pub enum UnlockError {
    /// Certain: the key-wrap integrity check failed.
    WrongPassword,
    /// The password is right, but no known construction decrypts this note.
    Unsupported,
    /// A GCM-wrapped note key authenticated (named construction), but the body didn't.
    KeyOnly(String),
    /// Locked with the device passcode: the key lives in Notes' Keychain, out of reach.
    DevicePasscode,
    Corrupt,
}

/// Password data stored on the Notes account (account-level scheme).
pub struct Account<'a> {
    pub salt: &'a [u8],
    pub iterations: u32,
    pub verifier: &'a [u8],
}

pub struct Locked<'a> {
    pub salt: &'a [u8],
    pub iterations: u32,
    pub wrapped_key: &'a [u8],
    /// IV/tag on the note's own row (may protect the wrapped key, not the body).
    pub key_iv: &'a [u8],
    pub key_tag: &'a [u8],
    /// IV/tag on the note-data row, next to the encrypted body.
    pub iv: &'a [u8],
    pub tag: &'a [u8],
    pub data: &'a [u8],
}

pub fn kek(password: &str, salt: &[u8], iterations: u32) -> [u8; 16] {
    let mut out = [0u8; 16];
    pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, iterations, &mut out);
    out
}

/// RFC 3394 AES key unwrap.
pub fn unwrap_key(kek: &[u8; 16], wrapped: &[u8]) -> Option<Vec<u8>> {
    unwrap_key_any(kek, wrapped)
}

/// RFC 3394 AES key unwrap with an AES-128 or AES-256 key-encryption key.
pub fn unwrap_key_any(kek: &[u8], wrapped: &[u8]) -> Option<Vec<u8>> {
    if wrapped.len() < 24 || wrapped.len() % 8 != 0 {
        return None;
    }
    let decrypt: Box<dyn Fn(&mut [u8; 16])> = match kek.len() {
        16 => {
            let c = Aes128::new(GenericArray::from_slice(kek));
            Box::new(move |b| c.decrypt_block(GenericArray::from_mut_slice(b)))
        }
        32 => {
            let c = aes::Aes256::new(GenericArray::from_slice(kek));
            Box::new(move |b| c.decrypt_block(GenericArray::from_mut_slice(b)))
        }
        _ => return None,
    };
    let n = wrapped.len() / 8 - 1;
    let mut a = u64::from_be_bytes(wrapped[..8].try_into().unwrap());
    let mut r: Vec<[u8; 8]> = wrapped[8..].chunks(8).map(|c| c.try_into().unwrap()).collect();
    for j in (0..6).rev() {
        for i in (1..=n).rev() {
            let t = (n * j + i) as u64;
            let mut block = [0u8; 16];
            block[..8].copy_from_slice(&(a ^ t).to_be_bytes());
            block[8..].copy_from_slice(&r[i - 1]);
            decrypt(&mut block);
            a = u64::from_be_bytes(block[..8].try_into().unwrap());
            r[i - 1].copy_from_slice(&block[8..]);
        }
    }
    (a == KW_IV).then(|| r.concat())
}

/// RFC 3394 AES key wrap (only used to build test fixtures).
#[cfg(test)]
pub(crate) fn wrap_key(kek: &[u8; 16], key: &[u8]) -> Vec<u8> {
    use aes::cipher::BlockEncrypt;
    let cipher = Aes128::new(GenericArray::from_slice(kek));
    let n = key.len() / 8;
    let mut a = KW_IV;
    let mut r: Vec<[u8; 8]> = key.chunks(8).map(|c| c.try_into().unwrap()).collect();
    for j in 0..6 {
        for i in 1..=n {
            let mut block = [0u8; 16];
            block[..8].copy_from_slice(&a.to_be_bytes());
            block[8..].copy_from_slice(&r[i - 1]);
            cipher.encrypt_block(GenericArray::from_mut_slice(&mut block));
            a = u64::from_be_bytes(block[..8].try_into().unwrap()) ^ (n * j + i) as u64;
            r[i - 1].copy_from_slice(&block[8..]);
        }
    }
    let mut out = a.to_be_bytes().to_vec();
    out.extend(r.concat());
    out
}

pub fn gcm_open(key: &[u8], iv: &[u8], tag: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    if key.len() != 16 || iv.is_empty() || tag.len() != 16 {
        return None;
    }
    let mut sealed = data.to_vec();
    sealed.extend_from_slice(tag);
    let cipher = Aes128Gcm16::new(GenericArray::from_slice(key));
    match iv.len() {
        16 => cipher.decrypt(Nonce::<U16>::from_slice(iv), Payload { msg: &sealed, aad: &[] }).ok(),
        12 => {
            let cipher = aes_gcm::Aes128Gcm::new(GenericArray::from_slice(key));
            cipher.decrypt(aes_gcm::Nonce::from_slice(iv), Payload { msg: &sealed, aad: &[] }).ok()
        }
        _ => None,
    }
}

pub fn ecb(key: &[u8], block: &[u8], encrypt: bool) -> Option<Vec<u8>> {
    use aes::cipher::BlockEncrypt;
    if key.len() != 16 || block.len() != 16 {
        return None;
    }
    let cipher = Aes128::new(GenericArray::from_slice(key));
    let mut b = GenericArray::clone_from_slice(block);
    if encrypt { cipher.encrypt_block(&mut b) } else { cipher.decrypt_block(&mut b) }
    Some(b.to_vec())
}

/// The decrypted body blob (gzipped protobuf, same as an unlocked note's ZDATA).
pub fn decrypt(locked: &Locked, account: Option<&Account>, password: &str) -> Result<Vec<u8>, UnlockError> {
    decrypt_traced(locked, account, password).map(|(body, _)| body)
}

/// [`decrypt`], plus a label naming the construction that worked.
pub fn decrypt_traced(locked: &Locked, account: Option<&Account>, password: &str) -> Result<(Vec<u8>, String), UnlockError> {
    if locked.iv.len() != 16 || locked.tag.len() != 16 {
        return Err(UnlockError::Corrupt);
    }
    // Per-note scheme: the note has its own iterations and a 24-byte wrapped key.
    if locked.iterations > 0 && locked.wrapped_key.len() >= 24 {
        let kek = kek(password, locked.salt, locked.iterations);
        if let Some(key) = unwrap_key(&kek, locked.wrapped_key) {
            let body = gcm_open(&key, locked.iv, locked.tag, locked.data).ok_or(UnlockError::Corrupt)?;
            return Ok((body, "per-note".into()));
        }
        if account.is_none() {
            return Err(UnlockError::WrongPassword);
        }
    }
    // Account scheme: the verifier proves the password and yields the account key.
    let account = account.ok_or(UnlockError::Unsupported)?;
    let account_kek = kek(password, account.salt, account.iterations);
    let master = unwrap_key(&account_kek, account.verifier).ok_or(UnlockError::WrongPassword)?;
    let keks = account_keks(password, &account_kek, &master, account, locked);
    try_matrix(&keks, locked)
}

/// Candidate keys for unwrapping (or directly decrypting) a note in the account scheme.
pub fn account_keks(password: &str, account_kek: &[u8; 16], master: &[u8], account: &Account, locked: &Locked) -> Vec<(String, Vec<u8>)> {
    let ns = locked.salt;
    let mut keks: Vec<(String, Vec<u8>)> = vec![
        ("master".into(), master.to_vec()),
        ("account-kek".into(), account_kek.to_vec()),
        ("pbkdf2(pw,note-salt,acct-iter)".into(), kek(password, ns, account.iterations).to_vec()),
        ("hkdf(master,note-salt)".into(), hkdf16(ns, master, b"")),
        ("hkdf(master,info=note-salt)".into(), hkdf16(b"", master, ns)),
        ("hkdf(acct-kek,note-salt)".into(), hkdf16(ns, account_kek, b"")),
        ("hmac(master,note-salt)".into(), hmac16(master, ns)),
        ("hmac(note-salt,master)".into(), hmac16(ns, master)),
        ("sha256(master|note-salt)".into(), sha16(&[master, ns])),
        ("pbkdf2(master,note-salt,acct-iter)".into(), pbkdf16(master, ns, account.iterations)),
        ("pbkdf2(master,note-salt,1)".into(), pbkdf16(master, ns, 1)),
    ];
    if master.len() == 32 {
        keks.push(("master[16..]".into(), master[16..].to_vec()));
        keks.push(("master[..16]".into(), master[..16].to_vec()));
    }
    keks.retain(|(_, k)| k.len() == 16);
    keks
}

/// Try every (key × unwrap method) for the note key, then the body. AES-GCM must
/// authenticate the body; a GCM-unwrapped key that authenticates but whose body doesn't is
/// reported as [`UnlockError::KeyOnly`].
pub fn try_matrix(keks: &[(String, Vec<u8>)], locked: &Locked) -> Result<(Vec<u8>, String), UnlockError> {
    let w = locked.wrapped_key;
    let mut bodies: Vec<(&[u8], &[u8], &str)> = vec![(locked.iv, locked.tag, "data-iv")];
    if (locked.key_iv, locked.key_tag) != (locked.iv, locked.tag) && locked.key_iv.len() == 16 && locked.key_tag.len() == 16 {
        bodies.push((locked.key_iv, locked.key_tag, "note-iv"));
    }
    let open_body = |key: &[u8], how: String| {
        bodies.iter().find_map(|(iv, tag, which)| gcm_open(key, iv, tag, locked.data).map(|b| (b, format!("{how} → body({which})"))))
    };
    let mut key_only = None;
    for (name, k) in keks {
        let mut note_keys: Vec<(String, Vec<u8>)> = vec![(format!("{name} (direct)"), k.clone())];
        if let Some(nk) = <&[u8; 16]>::try_from(k.as_slice()).ok().and_then(|k| unwrap_key(k, w)) {
            note_keys.push((format!("kw({name})"), nk));
        }
        // AES-GCM-wrapped key: the note row's IV/tag authenticate the 16-byte key.
        for (iv, tag, which) in [(locked.key_iv, locked.key_tag, "note-iv"), (locked.iv, locked.tag, "data-iv")] {
            if let Some(nk) = gcm_open(k, iv, tag, w) {
                let how = format!("gcm-wrap({name},{which})");
                if let Some(hit) = open_body(&nk, how.clone()) {
                    return Ok(hit);
                }
                key_only.get_or_insert(how);
            }
        }
        if let Some(nk) = ecb(k, w, false) {
            note_keys.push((format!("ecb-dec({name})"), nk));
        }
        if let Some(nk) = ecb(k, w, true) {
            note_keys.push((format!("ecb-enc({name})"), nk));
        }
        if w.len() == 16 {
            note_keys.push((format!("xor({name})"), k.iter().zip(w).map(|(a, b)| a ^ b).collect()));
        }
        for (how, nk) in note_keys {
            if let Some(hit) = open_body(&nk, how) {
                return Ok(hit);
            }
        }
    }
    Err(key_only.map(UnlockError::KeyOnly).unwrap_or(UnlockError::Unsupported))
}

fn hmac_full(key: &[u8], parts: &[&[u8]]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("hmac takes any key length");
    for p in parts {
        mac.update(p);
    }
    mac.finalize().into_bytes().to_vec()
}

pub fn hmac16(key: &[u8], msg: &[u8]) -> Vec<u8> {
    hmac_full(key, &[msg])[..16].to_vec()
}

/// HKDF-SHA256 (RFC 5869), 16-byte output.
pub fn hkdf16(salt: &[u8], ikm: &[u8], info: &[u8]) -> Vec<u8> {
    let salt = if salt.is_empty() { &[0u8; 32][..] } else { salt };
    let prk = hmac_full(salt, &[ikm]);
    hmac_full(&prk, &[info, &[1]])[..16].to_vec()
}

pub fn sha16(parts: &[&[u8]]) -> Vec<u8> {
    use sha2::Digest;
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize()[..16].to_vec()
}

pub fn pbkdf16(secret: &[u8], salt: &[u8], iterations: u32) -> Vec<u8> {
    let mut out = [0u8; 16];
    pbkdf2::pbkdf2_hmac::<Sha256>(secret, salt, iterations.max(1), &mut out);
    out.to_vec()
}

// --- Cipher version 2 (`ICCryptoEncryptionObject`) -------------------------------------
//
// Found in the owner's real database (2026-11-07). The note-data `ZDATA` of a locked note is
// not raw ciphertext but an NSKeyedArchiver'd `ICCryptoEncryptionObject`:
//
// - `unauthenticatedMetadata`: plist {passphraseSalt (32 B), passphraseIterationCount, passphraseHint}
// - `metadata`: plist {cipherVersion: 2, objectIdentifier: the note's ZIDENTIFIER}
// - `wrappedEncryptionKey`: 40 B, an RFC 3394-wrapped 256-bit key
// - `encryptedData`
//
// The account's `ICAccountData.ZCRYPTOPASSPHRASEVERIFIER` has the same shape (same salt, its
// own wrapped key). The 16-byte salt/wrapped key in the note's columns are legacy leftovers.
// Decryption: KEK = PBKDF2-SHA256(password, passphraseSalt, iterations); key = AES-KW unwrap
// (integrity-checked, so a wrong password is certain); then AES-GCM over `encryptedData`.
// Body: AES-256-GCM over `encryptedData` = ciphertext | IV (32 B) | tag (16 B), with the raw
// `metadata` bytes as additional data; the plaintext is the usual gzipped protobuf. Apple
// calls this "Cipher v1 Neo" (cipherVersion 2). Layout from 6over3/NotesKit's
// docs/encryption-research.md (confirmed by memrynote/memry's importer); the KEK/DEK test
// vector below is from that document. Other layouts are still tried, each authenticated.
// If `metadata` has an `accountKeyIdentifier`, the note key is wrapped by a Keychain key
// (device-passcode mode, "V2") and can't be opened outside Notes.

/// The parts of an `ICCryptoEncryptionObject`.
#[derive(Debug, Clone)]
pub struct V2Object {
    pub salt: Vec<u8>,
    pub iterations: u32,
    pub wrapped_key: Vec<u8>,
    /// The raw `metadata` plist bytes (probably the GCM additional data).
    pub metadata: Vec<u8>,
    /// The raw `unauthenticatedMetadata` plist bytes.
    pub unauthenticated_metadata: Vec<u8>,
    /// `metadata` names an `accountKeyIdentifier`: the key is wrapped by a Keychain key
    /// (device-passcode mode), not by the password.
    pub account_key: bool,
    pub data: Vec<u8>,
}

/// Parse a cipher-version-2 body. `None` if `zdata` isn't one.
pub fn parse_v2(zdata: &[u8]) -> Option<V2Object> {
    use plist::Value;
    if !zdata.starts_with(b"bplist") {
        return None;
    }
    let top = Value::from_reader(std::io::Cursor::new(zdata)).ok()?;
    let top = top.as_dictionary()?;
    let objects = top.get("$objects")?.as_array()?;
    let deref = |v: &Value| -> Option<Value> {
        match v {
            Value::Uid(u) => objects.get(u.get() as usize).cloned(),
            other => Some(other.clone()),
        }
    };
    let root = deref(top.get("$top")?.as_dictionary()?.get("root")?)?;
    let root = root.as_dictionary()?;
    let data = |key: &str| deref(root.get(key)?).and_then(|v| v.as_data().map(<[u8]>::to_vec));
    let unauth_raw = data("unauthenticatedMetadata")?;
    let unauth = Value::from_reader(std::io::Cursor::new(&unauth_raw)).ok()?;
    let unauth = unauth.as_dictionary()?;
    Some(V2Object {
        salt: unauth.get("passphraseSalt")?.as_data()?.to_vec(),
        iterations: u32::try_from(unauth.get("passphraseIterationCount")?.as_unsigned_integer()?).ok()?,
        wrapped_key: data("wrappedEncryptionKey")?,
        metadata: data("metadata").unwrap_or_default(),
        unauthenticated_metadata: unauth_raw.clone(),
        account_key: data("metadata")
            .and_then(|m| Value::from_reader(std::io::Cursor::new(m)).ok())
            .and_then(|m| m.as_dictionary().map(|d| d.contains_key("accountKeyIdentifier")))
            .unwrap_or(false),
        data: data("encryptedData")?,
    })
}

/// The account's passphrase verifier (`ICAccountData.ZCRYPTOPASSPHRASEVERIFIER`): a small
/// protobuf (field 1 = date, field 2 → field 5 = plist {key, authenticatedData, metadata}).
/// Returns (salt, iterations, wrapped key).
pub fn parse_v2_verifier(blob: &[u8]) -> Option<(Vec<u8>, u32, Vec<u8>)> {
    use plist::Value;
    let start = blob.windows(8).position(|w| w == b"bplist00")?;
    let top = Value::from_reader(std::io::Cursor::new(&blob[start..])).ok()?;
    let top = top.as_dictionary()?;
    let meta = top.get("metadata")?.as_data()?;
    let meta = Value::from_reader(std::io::Cursor::new(meta)).ok()?;
    let meta = meta.as_dictionary()?;
    Some((
        meta.get("passphraseSalt")?.as_data()?.to_vec(),
        u32::try_from(meta.get("passphraseIterationCount")?.as_unsigned_integer()?).ok()?,
        top.get("key")?.as_data()?.to_vec(),
    ))
}

/// AES-GCM with a 128/256-bit key, 12/16-byte nonce and optional additional data.
pub fn gcm_any(key: &[u8], iv: &[u8], tag: &[u8], data: &[u8], aad: &[u8]) -> Option<Vec<u8>> {
    use aes_gcm::aead::consts::U12;
    if tag.len() != 16 {
        return None;
    }
    let mut sealed = data.to_vec();
    sealed.extend_from_slice(tag);
    let payload = Payload { msg: &sealed, aad };
    macro_rules! open {
        ($aes:ty, $n:ty) => {
            AesGcm::<$aes, $n>::new(GenericArray::from_slice(key)).decrypt(GenericArray::from_slice(iv), payload).ok()
        };
    }
    use aes_gcm::aead::consts::U32;
    match (key.len(), iv.len()) {
        (16, 12) => open!(Aes128, U12),
        (16, 16) => open!(Aes128, U16),
        (16, 32) => open!(Aes128, U32),
        (32, 12) => open!(aes::Aes256, U12),
        (32, 16) => open!(aes::Aes256, U16),
        // "Cipher v1 Neo": CryptoKit accepts 32-byte GCM nonces.
        (32, 32) => open!(aes::Aes256, U32),
        _ => None,
    }
}

/// Unwrap a v2 key: PBKDF2-SHA256 with a 32- or 16-byte output, then AES-KW.
pub fn v2_key(password: &str, salt: &[u8], iterations: u32, wrapped: &[u8]) -> Option<(Vec<u8>, &'static str)> {
    [(32usize, "kek256"), (16, "kek128")].into_iter().find_map(|(len, label)| {
        let mut kek = vec![0u8; len];
        pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, iterations.max(1), &mut kek);
        unwrap_key_any(&kek, wrapped).map(|k| (k, label))
    })
}

/// Decrypt a cipher-version-2 note. `iv`/`tag` are the note-data row's columns, if any.
pub fn decrypt_v2(obj: &V2Object, iv: Option<&[u8]>, tag: Option<&[u8]>, password: &str) -> Result<(Vec<u8>, String), UnlockError> {
    if obj.account_key {
        // Keyed to the device passcode via the Data Protection Keychain, which only Notes
        // itself may read; the password only checks the account verifier.
        return Err(UnlockError::DevicePasscode);
    }
    let (key, kek) = v2_key(password, &obj.salt, obj.iterations, &obj.wrapped_key).ok_or(UnlockError::WrongPassword)?;
    let d = &obj.data;
    // The documented layout: ciphertext | IV (32 B) | tag (16 B), AAD = the raw metadata.
    if d.len() > 48
        && let Some(p) = gcm_any(&key, &d[d.len() - 48..d.len() - 16], &d[d.len() - 16..], &d[..d.len() - 48], &obj.metadata)
    {
        return Ok((p, format!("v1-neo {kek} gcm(ct|iv32|tag, aad=metadata)")));
    }
    let none: &[u8] = &[];
    let aads: [(&[u8], &str); 2] = [(&obj.metadata, "aad=metadata"), (none, "no-aad")];
    for (aad, aad_name) in aads {
        // IV and tag from the database columns.
        if let (Some(iv), Some(tag)) = (iv, tag)
            && let Some(p) = gcm_any(&key, iv, tag, d, aad)
        {
            return Ok((p, format!("v2 {kek} gcm(db iv/tag, {aad_name})")));
        }
        // Nonce in front, tag at the end (CryptoKit's "combined" layout and friends).
        for n in [12usize, 16] {
            if d.len() > n + 16
                && let Some(p) = gcm_any(&key, &d[..n], &d[d.len() - 16..], &d[n..d.len() - 16], aad)
            {
                return Ok((p, format!("v2 {kek} gcm(nonce{n}|ct|tag, {aad_name})")));
            }
        }
        // Column IV, tag appended to the data.
        if let Some(iv) = iv
            && d.len() > 16
            && let Some(p) = gcm_any(&key, iv, &d[d.len() - 16..], &d[..d.len() - 16], aad)
        {
            return Ok((p, format!("v2 {kek} gcm(db iv, ct|tag, {aad_name})")));
        }
    }
    Err(UnlockError::KeyOnly(format!("v2 {kek}: key unwrapped ({} bytes)", key.len())))
}

/// Encrypt a body the way Notes does (test fixtures only). Returns (wrapped key, IV, tag, data).
#[cfg(test)]
pub(crate) fn encrypt_for_test(password: &str, salt: &[u8], iterations: u32, body: &[u8]) -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
    let note_key = [7u8; 16];
    let iv = [9u8; 16];
    let wrapped = wrap_key(&kek(password, salt, iterations), &note_key);
    let cipher = Aes128Gcm16::new(GenericArray::from_slice(&note_key));
    let mut sealed = cipher.encrypt(Nonce::<U16>::from_slice(&iv), body).unwrap();
    let tag = sealed.split_off(sealed.len() - 16);
    (wrapped, iv.to_vec(), tag, sealed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3394_test_vector() {
        // RFC 3394 §4.1: 128-bit key data with a 128-bit KEK.
        let kek: [u8; 16] = hex("000102030405060708090A0B0C0D0E0F").try_into().unwrap();
        let wrapped = hex("1FA68B0A8112B447AEF34BD8FB5A7B829D3E862371D2CFE5");
        assert_eq!(unwrap_key(&kek, &wrapped).unwrap(), hex("00112233445566778899AABBCCDDEEFF"));
        assert_eq!(wrap_key(&kek, &hex("00112233445566778899AABBCCDDEEFF")), wrapped);
    }

    #[test]
    fn round_trip_and_wrong_password() {
        let (wrapped, iv, tag, data) = encrypt_for_test("hunter2", b"saltsaltsaltsalt", 1000, b"secret body");
        let locked = Locked { salt: b"saltsaltsaltsalt", iterations: 1000, wrapped_key: &wrapped, key_iv: &iv, key_tag: &tag, iv: &iv, tag: &tag, data: &data };
        assert_eq!(decrypt(&locked, None, "hunter2").unwrap(), b"secret body");
        assert_eq!(decrypt(&locked, None, "wrong").unwrap_err(), UnlockError::WrongPassword);
    }

    /// The owner's layout: account salt/iterations/24-byte verifier; note iterations 0 and
    /// a 16-byte key (here the note key encrypted with the account key, one candidate).
    #[test]
    fn account_scheme() {
        let account_key = [3u8; 16];
        let note_key = [7u8; 16];
        let verifier = wrap_key(&kek("pw", b"accountsaltsalt!", 500), &account_key);
        let wrapped16 = ecb(&account_key, &note_key, true).unwrap();
        let iv = [9u8; 16];
        let cipher = Aes128Gcm16::new(GenericArray::from_slice(&note_key));
        let mut sealed = cipher.encrypt(Nonce::<U16>::from_slice(&iv), b"locked body".as_slice()).unwrap();
        let tag = sealed.split_off(sealed.len() - 16);
        let note = Locked { salt: b"notesaltnotesalt", iterations: 0, wrapped_key: &wrapped16, key_iv: &iv, key_tag: &tag, iv: &iv, tag: &tag, data: &sealed };
        let account = Account { salt: b"accountsaltsalt!", iterations: 500, verifier: &verifier };
        assert_eq!(decrypt(&note, Some(&account), "pw").unwrap(), b"locked body");
        assert_eq!(decrypt(&note, Some(&account), "nope").unwrap_err(), UnlockError::WrongPassword);
        // Right password but an unknown construction: says so instead of "wrong password".
        let garbage = [0u8; 16];
        let odd = Locked { wrapped_key: &garbage, ..note };
        assert_eq!(decrypt(&odd, Some(&account), "pw").unwrap_err(), UnlockError::Unsupported);
    }

    fn gcm_seal(key: &[u8], iv: &[u8], body: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let cipher = Aes128Gcm16::new(GenericArray::from_slice(key));
        let mut sealed = cipher.encrypt(Nonce::<U16>::from_slice(iv), body).unwrap();
        let tag = sealed.split_off(sealed.len() - 16);
        (sealed, tag)
    }

    /// The note key sealed with AES-GCM under the account key, using the note row's IV/tag;
    /// the body under the note key with the note-data row's IV/tag.
    #[test]
    fn account_scheme_gcm_wrapped_key() {
        let account_key = [3u8; 16];
        let note_key = [7u8; 16];
        let verifier = wrap_key(&kek("pw", b"accountsaltsalt!", 500), &account_key);
        let (key_iv, data_iv) = ([1u8; 16], [2u8; 16]);
        let (wrapped, key_tag) = gcm_seal(&account_key, &key_iv, &note_key);
        let (data, tag) = gcm_seal(&note_key, &data_iv, b"gcm body");
        let note = Locked { salt: b"notesaltnotesalt", iterations: 0, wrapped_key: &wrapped, key_iv: &key_iv, key_tag: &key_tag, iv: &data_iv, tag: &tag, data: &data };
        let account = Account { salt: b"accountsaltsalt!", iterations: 500, verifier: &verifier };
        let (body, how) = decrypt_traced(&note, Some(&account), "pw").unwrap();
        assert_eq!((body.as_slice(), how.as_str()), (b"gcm body".as_slice(), "gcm-wrap(master,note-iv) → body(data-iv)"));
        // Key authenticates but the body is damaged: says so distinctly.
        let mut bad = data.clone();
        bad[0] ^= 1;
        let broken = Locked { data: &bad, ..note };
        assert!(matches!(decrypt(&broken, Some(&account), "pw").unwrap_err(), UnlockError::KeyOnly(_)));
    }

    fn wrap_key_256(kek: &[u8], key: &[u8]) -> Vec<u8> {
        use aes::cipher::BlockEncrypt;
        let cipher = aes::Aes256::new(GenericArray::from_slice(kek));
        let n = key.len() / 8;
        let mut a = KW_IV;
        let mut r: Vec<[u8; 8]> = key.chunks(8).map(|c| c.try_into().unwrap()).collect();
        for j in 0..6 {
            for i in 1..=n {
                let mut block = [0u8; 16];
                block[..8].copy_from_slice(&a.to_be_bytes());
                block[8..].copy_from_slice(&r[i - 1]);
                cipher.encrypt_block(GenericArray::from_mut_slice(&mut block));
                a = u64::from_be_bytes(block[..8].try_into().unwrap()) ^ (n * j + i) as u64;
                r[i - 1].copy_from_slice(&block[8..]);
            }
        }
        let mut out = a.to_be_bytes().to_vec();
        out.extend(r.concat());
        out
    }

    fn bplist(v: plist::Value) -> Vec<u8> {
        let mut out = Vec::new();
        v.to_writer_binary(&mut out).unwrap();
        out
    }

    /// The owner's cipher-version-2 layout: an NSKeyedArchiver'd ICCryptoEncryptionObject.
    #[test]
    fn cipher_v2_object() {
        use plist::{Dictionary, Uid, Value};
        let salt = [4u8; 32];
        let mut kek = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<Sha256>(b"pw", &salt, 50, &mut kek);
        let key = [8u8; 32];
        let wrapped = wrap_key_256(&kek, &key);
        let metadata = bplist(Value::Dictionary(Dictionary::from_iter([
            ("cipherVersion".to_string(), Value::Integer(2.into())),
            ("objectIdentifier".to_string(), Value::String("NOTE-ID".into())),
        ])));
        let unauth = bplist(Value::Dictionary(Dictionary::from_iter([
            ("passphraseSalt".to_string(), Value::Data(salt.to_vec())),
            ("passphraseIterationCount".to_string(), Value::Integer(50.into())),
            ("passphraseHint".to_string(), Value::String(String::new())),
        ])));
        // ciphertext | IV (32) | tag (16), as Notes stores it.
        let iv = [5u8; 32];
        let cipher = AesGcm::<aes::Aes256, aes_gcm::aead::consts::U32>::new(GenericArray::from_slice(&key));
        let mut sealed = cipher.encrypt(GenericArray::from_slice(&iv), Payload { msg: b"v2 body", aad: &metadata }).unwrap();
        let tag = sealed.split_off(sealed.len() - 16);
        sealed.extend_from_slice(&iv);
        sealed.extend_from_slice(&tag);
        let root = Value::Dictionary(Dictionary::from_iter([
            ("metadata".to_string(), Value::Uid(Uid::new(3))),
            ("unauthenticatedMetadata".to_string(), Value::Uid(Uid::new(2))),
            ("wrappedEncryptionKey".to_string(), Value::Uid(Uid::new(4))),
            ("encryptedData".to_string(), Value::Uid(Uid::new(5))),
        ]));
        let archive = bplist(Value::Dictionary(Dictionary::from_iter([
            ("$archiver".to_string(), Value::String("NSKeyedArchiver".into())),
            ("$top".to_string(), Value::Dictionary(Dictionary::from_iter([("root".to_string(), Value::Uid(Uid::new(1)))]))),
            (
                "$objects".to_string(),
                Value::Array(vec![
                    Value::String("$null".into()),
                    root,
                    Value::Data(unauth),
                    Value::Data(metadata),
                    Value::Data(wrapped),
                    Value::Data(sealed),
                ]),
            ),
        ])));
        let obj = parse_v2(&archive).expect("parses");
        assert_eq!((obj.salt.len(), obj.iterations, obj.wrapped_key.len()), (32, 50, 40));
        let (body, how) = decrypt_v2(&obj, None, None, "pw").unwrap();
        assert_eq!((body.as_slice(), how.as_str()), (b"v2 body".as_slice(), "v1-neo kek256 gcm(ct|iv32|tag, aad=metadata)"));
        assert_eq!(decrypt_v2(&obj, None, None, "nope").unwrap_err(), UnlockError::WrongPassword);
        let passcode = V2Object { account_key: true, ..obj };
        assert_eq!(decrypt_v2(&passcode, None, None, "pw").unwrap_err(), UnlockError::DevicePasscode);
        assert!(parse_v2(b"not a plist").is_none());
    }

    /// KEK and key unwrap from NotesKit's published v1-Neo test note ("Needleinahaystack").
    #[test]
    fn v1_neo_published_vector() {
        let salt = hex("1faf405b502ebdb2796842e6217fa4ca274b69ac1efae128bb09dfb04989c96d");
        let wrapped = hex("3e7cc8825082cd508d4f51328a287b4df4c3763eb20bacd6748adc956d44d82071f34b4039c90fe5");
        let mut kek = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<Sha256>(b"jjkhehe", &salt, 20000, &mut kek);
        assert_eq!(kek.to_vec(), hex("66c2ed1de7e3e94b63a39048edbb540b375b364ce9ffedce93d7dee4f930d576"));
        let (dek, how) = v2_key("jjkhehe", &salt, 20000, &wrapped).unwrap();
        assert_eq!((dek, how), (hex("e1d12f49ace8816f7334da221f0f7f149a266d6386de576b4d59a395d78afba2"), "kek256"));
    }

    #[test]
    fn hkdf_rfc5869_case1() {
        let okm = hkdf16(&hex("000102030405060708090a0b0c"), &hex("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b"), &hex("f0f1f2f3f4f5f6f7f8f9"));
        assert_eq!(okm, hex("3cb25f25faacd57a90434f64d0362f2a"));
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }
}
