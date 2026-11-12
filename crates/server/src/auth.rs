//! Paired devices and one-time pairing codes, stored as JSON in the data directory.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// How long a pairing code from `bbd pair` stays valid.
pub const PAIR_CODE_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub name: String,
    /// SHA-256 of the token; the token itself is only ever held by the device.
    pub token_sha256: String,
    pub paired_at: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct PendingCode {
    code_sha256: String,
    expires_at: u64,
}

pub struct Store {
    dir: PathBuf,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn sha256_hex(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

/// Compare without leaking how many leading bytes matched.
fn constant_time_eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

impl Store {
    pub fn new(dir: &Path) -> Self {
        Self { dir: dir.to_path_buf() }
    }

    fn devices_path(&self) -> PathBuf {
        self.dir.join("devices.json")
    }

    fn code_path(&self) -> PathBuf {
        self.dir.join("pairing.json")
    }

    pub fn devices(&self) -> Vec<Device> {
        std::fs::read(self.devices_path())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn save_devices(&self, devices: &[Device]) -> Result<()> {
        bb_net::write_private(&self.devices_path(), &serde_json::to_vec_pretty(devices)?)
    }

    /// Returns the device name for a valid token.
    pub fn check_token(&self, token: &str) -> Option<String> {
        let hash = sha256_hex(token);
        self.devices().into_iter().find(|d| constant_time_eq(&d.token_sha256, &hash)).map(|d| d.name)
    }

    /// Create a new single-use pairing code, replacing any previous one.
    pub fn new_pairing_code(&self) -> Result<String> {
        const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
        let mut rng = rand::thread_rng();
        let raw: String = (0..10).map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char).collect();
        let code = format!("{}-{}", &raw[..5], &raw[5..]);
        let pending = PendingCode {
            code_sha256: sha256_hex(&code),
            expires_at: now() + PAIR_CODE_TTL.as_secs(),
        };
        std::fs::create_dir_all(&self.dir)?;
        bb_net::write_private(&self.code_path(), &serde_json::to_vec(&pending)?)?;
        Ok(code)
    }

    /// Consume a pairing code and register a device. Returns its new token.
    pub fn pair(&self, code: &str, device_name: &str) -> Option<String> {
        let pending: PendingCode = serde_json::from_slice(&std::fs::read(self.code_path()).ok()?).ok()?;
        let valid = pending.expires_at >= now()
            && constant_time_eq(&pending.code_sha256, &sha256_hex(code.trim().to_uppercase().as_str()));
        if !valid {
            return None;
        }
        // Single use: remove before handing out the token.
        std::fs::remove_file(self.code_path()).ok()?;
        let token = hex::encode(rand::thread_rng().r#gen::<[u8; 32]>());
        let mut devices = self.devices();
        let name = device_name.chars().filter(|c| !c.is_control()).take(64).collect::<String>();
        devices.push(Device { name, token_sha256: sha256_hex(&token), paired_at: now() });
        self.save_devices(&devices).ok()?;
        Some(token)
    }

    pub fn revoke(&self, name: &str) -> Result<usize> {
        let mut devices = self.devices();
        let before = devices.len();
        devices.retain(|d| d.name != name);
        self.save_devices(&devices)?;
        Ok(before - devices.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_code_is_single_use() {
        let dir = std::env::temp_dir().join(format!("bbd-auth-{}", std::process::id()));
        let store = Store::new(&dir);
        let code = store.new_pairing_code().unwrap();
        assert!(store.pair("WRONG-CODE0", "x").is_none());
        let token = store.pair(&code.to_lowercase(), "laptop").unwrap();
        assert_eq!(store.check_token(&token).as_deref(), Some("laptop"));
        assert!(store.pair(&code, "again").is_none());
        assert!(store.check_token("nope").is_none());
        assert_eq!(store.revoke("laptop").unwrap(), 1);
        assert!(store.check_token(&token).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
