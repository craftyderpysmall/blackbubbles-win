//! Autofill settings: the `[autofill]` table of `~/.config/blackbubbles/config.toml`.
//!
//! Compatible with linuxautofill's config: `clear_after`, `extra_keywords`, `[http]`, and
//! `[[gmail]]` (a single `[gmail]` table also works).

use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub enabled: bool,
    /// Seconds until the clipboard is restored and the notification expires.
    pub clear_after: u64,
    /// Extra literal words that mark a code, e.g. `["clave", "código"]`.
    pub extra_keywords: Vec<String>,
    /// Take codes from iMessage/SMS seen by the Mac server.
    pub from_server: bool,
    /// Show a click-to-paste notification. Off: codes are copied silently.
    pub notify: bool,
    /// iPhone Shortcut endpoint (works even when the Mac is offline).
    pub http: Option<HttpConfig>,
    #[serde(deserialize_with = "one_or_many")]
    pub gmail: Vec<GmailConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self { enabled: true, clear_after: 60, extra_keywords: vec![], from_server: true, notify: true, http: None, gmail: vec![] }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub token: String,
}

fn default_host() -> String {
    "0.0.0.0".into()
}

fn default_port() -> u16 {
    8787
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GmailConfig {
    pub address: String,
    pub app_password: String,
}

fn one_or_many<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<GmailConfig>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(GmailConfig),
        Many(Vec<GmailConfig>),
    }
    Ok(match OneOrMany::deserialize(d)? {
        OneOrMany::One(one) => vec![one],
        OneOrMany::Many(many) => many,
    })
}

impl Config {
    /// Keyword regex fragments: the defaults plus escaped extra words.
    pub fn keywords(&self) -> Vec<String> {
        crate::detect::DEFAULT_KEYWORDS
            .iter()
            .map(|k| k.to_string())
            .chain(self.extra_keywords.iter().map(|k| regex::escape(k)))
            .collect()
    }
}
