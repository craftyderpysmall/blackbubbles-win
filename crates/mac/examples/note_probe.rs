//! Offline probe for locked-note encryption formats. Runs on Linux against a dump from bbd:
//!
//! ```sh
//! blackbubbles request '{"NoteCryptoDump":{"id":"x-coredata://…/ICNote/p112"}}' > dump.json
//! cargo run --release -p bb-mac --example note_probe -- dump.json   # then type the password
//! ```
//!
//! It checks the password against the account verifier, then tries a large matrix: key
//! derivations × every plausibly-sized blob in the dump as the wrapped key (including byte
//! strings found inside the CloudKit record archives) × every IV/tag pair, using AES-GCM,
//! RFC 3394, ECB and XOR. Only AES-GCM / AES-KW integrity checks count as hits, so it never
//! reports a false positive. It prints constructions and lengths only, never note text.

use std::collections::BTreeMap;
use std::io::BufRead;

use bb_mac::notecrypt::{self, Account, Locked};
use serde_json::Value;

type Blobs = BTreeMap<String, Vec<u8>>;

fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

fn collect(prefix: &str, row: &Value, out: &mut Blobs, ints: &mut BTreeMap<String, i64>) {
    let Some(obj) = row.as_object() else { return };
    for (k, v) in obj {
        let name = format!("{prefix}.{k}");
        if let Some(h) = v.get("hex").and_then(|h| h.as_str()) {
            let bytes = hex(h);
            if bytes.len() > 64 {
                embedded(&name, &bytes, out);
            }
            out.insert(name, bytes);
        } else if let Some(n) = v.as_i64() {
            ints.insert(name, n);
        }
    }
}

/// Byte strings inside a binary plist (NSKeyedArchiver'd CKRecord): marker 0x4N (N < 15)
/// or 0x4F 0x10 len. Only sizes that could be a key, IV, tag, salt or wrapped key.
fn embedded(name: &str, b: &[u8], out: &mut Blobs) {
    for i in 0..b.len() {
        let (len, start) = match b[i] {
            0x4C => (12, i + 1),
            0x4F if b.get(i + 1) == Some(&0x10) => (*b.get(i + 2).unwrap_or(&0) as usize, i + 3),
            _ => continue,
        };
        if [12, 16, 24, 32, 40].contains(&len) && start + len <= b.len() {
            out.insert(format!("{name}@{i}"), b[start..start + len].to_vec());
        }
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: note_probe dump.json (password on stdin)");
    let text = std::fs::read_to_string(&path).expect("read dump");
    // Accept either the bare dump or `blackbubbles request` output ({"Diagnostics": "<json>"}).
    let mut dump: Value = serde_json::from_str(&text).expect("json");
    if let Some(inner) = dump.get("Diagnostics").and_then(|d| d.as_str()) {
        dump = serde_json::from_str(inner).expect("inner json");
    }
    eprint!("Notes password: ");
    let mut password = String::new();
    std::io::stdin().lock().read_line(&mut password).unwrap();
    let password = password.trim_end_matches(['\n', '\r']);

    let mut blobs = Blobs::new();
    let mut ints = BTreeMap::new();
    collect("note", &dump["note"], &mut blobs, &mut ints);
    collect("data", &dump["note_data"], &mut blobs, &mut ints);
    for section in ["accounts", "account_data"] {
        for (pk, row) in dump[section].as_object().into_iter().flatten() {
            collect(&format!("{section}[{pk}]"), row, &mut blobs, &mut ints);
        }
    }
    let body = blobs.remove("data.ZDATA").expect("no note_data.ZDATA");
    if let Some(obj) = notecrypt::parse_v2(&body) {
        probe_v2(&obj, &blobs, password);
        return;
    }
    println!("body: {} bytes", body.len());
    for (k, v) in &blobs {
        if v.len() <= 64 {
            println!("  {k}: {} bytes", v.len());
        }
    }
    for (k, v) in &ints {
        if k.contains("CRYPTO") {
            println!("  {k} = {v}");
        }
    }

    // 1. Password check against every account verifier.
    let mut master = None;
    for (k, verifier) in blobs.iter().filter(|(k, _)| k.starts_with("accounts[") && k.ends_with(".ZCRYPTOVERIFIER")) {
        let acct = k.trim_end_matches(".ZCRYPTOVERIFIER");
        let salt = &blobs[&format!("{acct}.ZCRYPTOSALT")];
        let iters = ints[&format!("{acct}.ZCRYPTOITERATIONCOUNT")] as u32;
        let kek = notecrypt::kek(password, salt, iters);
        match notecrypt::unwrap_key(&kek, verifier) {
            Some(m) => {
                println!("password OK for {acct} (master key {} bytes)", m.len());
                master = Some((acct.to_string(), salt.clone(), iters, verifier.clone(), kek, m));
            }
            None => println!("password does NOT unwrap {acct}'s verifier"),
        }
    }
    let Some((_acct, acct_salt, acct_iters, verifier, acct_kek, master)) = master else {
        println!("No account verifier accepted the password; stopping.");
        return;
    };

    // 2. Key candidates.
    let empty = Vec::new();
    let note_salt = blobs.get("note.ZCRYPTOSALT").unwrap_or(&empty).clone();
    let wrapped_db = blobs.get("note.ZCRYPTOWRAPPEDKEY").unwrap_or(&empty).clone();
    let iv_data = blobs.get("data.ZCRYPTOINITIALIZATIONVECTOR").unwrap_or(&empty).clone();
    let tag_data = blobs.get("data.ZCRYPTOTAG").unwrap_or(&empty).clone();
    let account = Account { salt: &acct_salt, iterations: acct_iters, verifier: &verifier };
    let locked = Locked {
        salt: &note_salt,
        iterations: 0,
        wrapped_key: &wrapped_db,
        key_iv: blobs.get("note.ZCRYPTOINITIALIZATIONVECTOR").unwrap_or(&iv_data),
        key_tag: blobs.get("note.ZCRYPTOTAG").unwrap_or(&tag_data),
        iv: &iv_data,
        tag: &tag_data,
        data: &body,
    };
    match notecrypt::decrypt_traced(&locked, Some(&account), password) {
        Ok((plain, how)) => {
            println!("bbd's built-in matrix decrypts it: {how} ({} bytes)", plain.len());
        }
        Err(e) => println!("bbd's built-in matrix: {e:?}"),
    }

    let mut keks = notecrypt::account_keks(password, &acct_kek, &master, &account, &locked);
    let salts: Vec<(&String, &Vec<u8>)> = blobs.iter().filter(|(_, v)| v.len() == 16).collect();
    for (name, s) in &salts {
        for iters in [1u32, 1000, 10_000, acct_iters, 100_000] {
            keks.push((format!("pbkdf2(pw,{name},{iters})"), notecrypt::pbkdf16(password.as_bytes(), s, iters)));
        }
        keks.push((format!("hkdf(master,{name})"), notecrypt::hkdf16(s, &master, b"")));
        keks.push((format!("hkdf(master,info={name})"), notecrypt::hkdf16(b"", &master, s)));
        keks.push((format!("hmac(master,{name})"), notecrypt::hmac16(&master, s)));
        keks.push((format!("raw {name}"), s.to_vec()));
        if let Some(m) = <&[u8; 16]>::try_from(master.as_slice()).ok().and_then(|m| notecrypt::unwrap_key(m, s)) {
            keks.push((format!("kw(master,{name})"), m));
        }
    }
    keks.push(("sha256(pw)".into(), notecrypt::sha16(&[password.as_bytes()])));
    keks.retain(|(_, k)| k.len() == 16);
    println!("{} key candidates", keks.len());

    // 3. Note-key candidates: authenticated unwraps of every wrapped-sized blob.
    let ivs: Vec<(&String, &Vec<u8>)> = blobs.iter().filter(|(_, v)| v.len() == 16 || v.len() == 12).collect();
    let tags: Vec<(&String, &Vec<u8>)> = blobs.iter().filter(|(_, v)| v.len() == 16).collect();
    let wrapped: Vec<(&String, &Vec<u8>)> = blobs.iter().filter(|(_, v)| [16, 24, 32, 40].contains(&v.len())).collect();
    let mut note_keys: Vec<(String, Vec<u8>, bool)> = keks.iter().map(|(n, k)| (n.clone(), k.clone(), false)).collect();
    for (kn, k) in &keks {
        for (wn, w) in &wrapped {
            if let Some(nk) = <&[u8; 16]>::try_from(k.as_slice()).ok().and_then(|k| notecrypt::unwrap_key(k, w)) {
                println!("AUTH kw({kn}, {wn}) → {}-byte key", nk.len());
                note_keys.push((format!("kw({kn},{wn})"), nk, true));
            }
            if w.len() == 16 {
                for (ivn, iv) in &ivs {
                    for (tn, tag) in &tags {
                        if let Some(nk) = notecrypt::gcm_open(k, iv, tag, w) {
                            println!("AUTH gcm({kn}, ct={wn}, iv={ivn}, tag={tn}) → {}-byte key", nk.len());
                            note_keys.push((format!("gcm({kn},{wn},{ivn},{tn})"), nk, true));
                        }
                    }
                }
                for (m, nk) in [("ecb-dec", notecrypt::ecb(k, w, false)), ("ecb-enc", notecrypt::ecb(k, w, true))] {
                    if let Some(nk) = nk {
                        note_keys.push((format!("{m}({kn},{wn})"), nk, false));
                    }
                }
                note_keys.push((format!("xor({kn},{wn})"), k.iter().zip(w.iter()).map(|(a, b)| a ^ b).collect(), false));
            }
        }
    }
    println!("{} note-key candidates; trying the body…", note_keys.len());

    // 4. Body: every note key × IV/tag pairs from crypto columns.
    let body_ivs: Vec<_> = ivs.iter().filter(|(n, _)| n.contains("INITIALIZATIONVECTOR")).collect();
    let body_tags: Vec<_> = tags.iter().filter(|(n, _)| n.contains("TAG")).collect();
    let mut found = false;
    for (name, nk, auth) in &note_keys {
        for (ivn, iv) in &body_ivs {
            for (tn, tag) in &body_tags {
                if let Some(plain) = notecrypt::gcm_open(nk, iv, tag, &body) {
                    let decoded = bb_mac::notes::decode_body(&plain).map(|t| t.chars().count());
                    println!("BODY DECRYPTS: key={name} (key authenticated: {auth}) iv={ivn} tag={tn}; {} bytes, decoded text chars: {decoded:?}", plain.len());
                    found = true;
                }
            }
        }
    }
    if !found {
        println!("No construction decrypted the body.");
    }
}

/// What a decrypted blob looks like, without showing it.
fn kind(plain: &[u8]) -> String {
    let what = if plain.starts_with(&[0x1f, 0x8b]) {
        "gzip"
    } else if plain.starts_with(b"bplist") {
        "bplist"
    } else {
        "other"
    };
    let decoded = bb_mac::notes::decode_body(plain).map(|t| t.chars().count());
    format!("{} bytes, {what}, note text decodes: {}", plain.len(), decoded.map_or("no".into(), |n| format!("yes ({n} chars)")))
}

/// Cipher version 2 (ICCryptoEncryptionObject body).
fn probe_v2(obj: &notecrypt::V2Object, blobs: &Blobs, password: &str) {
    println!("body is a cipher-version-2 ICCryptoEncryptionObject: salt {} B, {} iterations, wrapped key {} B, metadata {} B, data {} B",
        obj.salt.len(), obj.iterations, obj.wrapped_key.len(), obj.metadata.len(), obj.data.len());
    if let Some((salt, iters, key)) = blobs.iter().find(|(k, _)| k.ends_with("ZCRYPTOPASSPHRASEVERIFIER")).and_then(|(_, v)| notecrypt::parse_v2_verifier(v)) {
        match notecrypt::v2_key(password, &salt, iters, &key) {
            Some((k, how)) => println!("account v2 verifier: password OK ({how}, {} B key)", k.len()),
            None => println!("account v2 verifier: password does NOT unwrap it"),
        }
    }
    let iv = blobs.get("data.ZCRYPTOINITIALIZATIONVECTOR").map(Vec::as_slice);
    let tag = blobs.get("data.ZCRYPTOTAG").map(Vec::as_slice);
    match notecrypt::decrypt_v2(obj, iv, tag, password) {
        Ok((plain, how)) => {
            println!("DECRYPTS with bbd's built-in v2 code: {how}; {}", kind(&plain));
            return;
        }
        Err(e) => println!("bbd's built-in v2 code: {e:?}"),
    }
    let Some((key, how)) = notecrypt::v2_key(password, &obj.salt, obj.iterations, &obj.wrapped_key) else {
        println!("The note's own wrapped key doesn't unwrap with this password; stopping.");
        return;
    };
    println!("note key unwrapped ({how}, {} B); searching body layouts…", key.len());
    let d = &obj.data;
    let mut keys: Vec<(&str, Vec<u8>)> = vec![("key", key.clone())];
    if key.len() == 32 {
        keys.push(("key[..16]", key[..16].to_vec()));
        keys.push(("key[16..]", key[16..].to_vec()));
    }
    let object_id = plist::Value::from_reader(std::io::Cursor::new(&obj.metadata))
        .ok()
        .and_then(|v| v.as_dictionary()?.get("objectIdentifier")?.as_string().map(str::to_owned))
        .unwrap_or_default();
    let unauth = obj.unauthenticated_metadata.clone();
    let mut both = obj.metadata.clone();
    both.extend_from_slice(&unauth);
    let aads: Vec<(&str, Vec<u8>)> = vec![
        ("metadata", obj.metadata.clone()),
        ("none", vec![]),
        ("objectIdentifier", object_id.clone().into_bytes()),
        ("objectIdentifier-lower", object_id.to_lowercase().into_bytes()),
        ("unauthenticatedMetadata", unauth.clone()),
        ("metadata+unauth", both),
        ("sha256(metadata)", notecrypt::sha16(&[&obj.metadata]).to_vec()),
    ];
    let mut layouts: Vec<(String, Vec<u8>, Vec<u8>, Vec<u8>)> = Vec::new(); // (name, iv, tag, ct)
    for n in [12usize, 16] {
        if d.len() > n + 16 {
            layouts.push((format!("nonce{n}|ct|tag"), d[..n].to_vec(), d[d.len() - 16..].to_vec(), d[n..d.len() - 16].to_vec()));
            layouts.push((format!("nonce{n}|tag|ct"), d[..n].to_vec(), d[n..n + 16].to_vec(), d[n + 16..].to_vec()));
        }
    }
    let iv12 = iv.map(|v| v[..12.min(v.len())].to_vec());
    for (ivn, iv) in [("db-iv", iv), ("db-iv[..12]", iv12.as_deref()), ("zero12", Some(&[0u8; 12][..])), ("zero16", Some(&[0u8; 16][..]))] {
        let Some(iv) = iv else { continue };
        if let Some(tag) = tag {
            layouts.push((format!("{ivn}, db-tag, ct"), iv.to_vec(), tag.to_vec(), d.clone()));
        }
        layouts.push((format!("{ivn}, ct|tag"), iv.to_vec(), d[d.len() - 16..].to_vec(), d[..d.len() - 16].to_vec()));
        layouts.push((format!("{ivn}, tag|ct"), iv.to_vec(), d[..16].to_vec(), d[16..].to_vec()));
    }
    let mut found = false;
    for (kn, k) in &keys {
        for (an, aad) in &aads {
            for (ln, iv, tg, ct) in &layouts {
                if let Some(plain) = notecrypt::gcm_any(k, iv, tg, ct, aad) {
                    println!("BODY DECRYPTS: AES-GCM {kn}, aad={an}, layout={ln}; {}", kind(&plain));
                    found = true;
                }
                if let Some(plain) = chacha(k, iv, tg, ct, aad) {
                    println!("BODY DECRYPTS: ChaCha20-Poly1305 {kn}, aad={an}, layout={ln}; {}", kind(&plain));
                    found = true;
                }
            }
        }
    }
    if !found {
        println!("No v2 body layout matched ({} tried).", keys.len() * aads.len() * layouts.len());
    }
}

/// ChaCha20-Poly1305 (CryptoKit's ChaChaPoly): 256-bit key, 12-byte nonce, 16-byte tag.
fn chacha(key: &[u8], nonce: &[u8], tag: &[u8], ct: &[u8], aad: &[u8]) -> Option<Vec<u8>> {
    use chacha20poly1305::aead::{Aead, KeyInit, Payload};
    if key.len() != 32 || nonce.len() != 12 || tag.len() != 16 {
        return None;
    }
    let cipher = chacha20poly1305::ChaCha20Poly1305::new_from_slice(key).ok()?;
    let mut sealed = ct.to_vec();
    sealed.extend_from_slice(tag);
    cipher.decrypt(chacha20poly1305::Nonce::from_slice(nonce), Payload { msg: &sealed, aad }).ok()
}
