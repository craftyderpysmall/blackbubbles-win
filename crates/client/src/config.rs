//! `%LOCALAPPDATA%\craftyderpysmall\BlackBubbles\config\config.toml`. Not the install
//! folder (`%LOCALAPPDATA%\BlackBubbles`), so an uninstall never takes the pairing with it.
//!
//! Holds the device token and autofill secrets, so it lives in the local (not roaming)
//! profile and its access list allows only you and SYSTEM (Windows' "mode 600").

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub server: Option<ServerConfig>,
    pub autofill: bb_autofill::Config,
    pub notifications: Notifications,
    pub services: Services,
}

/// Which Apple services BlackBubbles shows. A hidden service gets no tab and no
/// notifications. Everything is on by default.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Services {
    pub messages: bool,
    pub photos: bool,
    pub notes: bool,
    pub calendar: bool,
    pub reminders: bool,
    pub voice_memos: bool,
    pub find_my: bool,
    /// Include Family Sharing members' devices in Find My.
    pub find_my_family: bool,
}

impl Default for Services {
    fn default() -> Self {
        Self { messages: true, photos: true, notes: true, calendar: true, reminders: true, voice_memos: true, find_my: true, find_my_family: true }
    }
}

/// What gets a desktop notification. Shown in the process that owns autofill (the
/// daemon if it runs, otherwise the window).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Notifications {
    pub enabled: bool,
    // Messages
    pub messages: bool,
    pub imessage: bool,
    pub sms: bool,
    pub show_sender: bool,
    pub show_preview: bool,
    /// Skip texts that only carry a verification code (autofill shows those).
    pub skip_codes: bool,
    /// Don't notify while the BlackBubbles window is focused.
    pub only_when_unfocused: bool,
    pub muted_chats: Vec<String>,
    // Calendar
    pub calendar_reminders: bool,
    pub reminder_minutes: u32,
    /// Reminders (the app) at their due time; date-only ones at 09:00.
    pub reminders_due: bool,
    // Library changes
    pub photos: bool,
    pub notes: bool,
    pub voice_memos: bool,
    // Find My
    pub low_battery: bool,
    pub low_battery_percent: u8,
    // Connection
    pub connection: bool,
    // Quiet hours (local time, HH:MM; may wrap past midnight)
    pub quiet_hours: bool,
    pub quiet_start: String,
    pub quiet_end: String,
}

impl Default for Notifications {
    fn default() -> Self {
        Self {
            enabled: true,
            messages: true,
            imessage: true,
            sms: true,
            show_sender: true,
            show_preview: true,
            skip_codes: true,
            only_when_unfocused: true,
            muted_chats: vec![],
            calendar_reminders: true,
            reminder_minutes: 10,
            reminders_due: true,
            photos: false,
            notes: false,
            voice_memos: false,
            low_battery: true,
            low_battery_percent: 20,
            connection: true,
            quiet_hours: false,
            quiet_start: "22:00".into(),
            quiet_end: "07:00".into(),
        }
    }
}

impl Notifications {
    pub fn in_quiet_hours(&self, now: chrono::NaiveTime) -> bool {
        if !self.quiet_hours {
            return false;
        }
        let parse = |s: &str| chrono::NaiveTime::parse_from_str(s.trim(), "%H:%M").ok();
        match (parse(&self.quiet_start), parse(&self.quiet_end)) {
            (Some(start), Some(end)) if start <= end => now >= start && now < end,
            (Some(start), Some(end)) => now >= start || now < end,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;

    #[test]
    fn services_default_on_and_old_configs_load() {
        let old: Config = toml::from_str("[notifications]\nenabled = false\n").unwrap();
        assert_eq!(old.services, Services::default());
        assert!(old.services.reminders && old.notifications.reminders_due);
        let partial: Config = toml::from_str("[services]\nphotos = false\n").unwrap();
        assert!(!partial.services.photos && partial.services.messages);
    }

    #[test]
    fn quiet_hours_wrap_midnight() {
        let t = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        let mut n = Notifications { quiet_hours: true, ..Default::default() };
        assert!(n.in_quiet_hours(t(23, 0)));
        assert!(n.in_quiet_hours(t(6, 59)));
        assert!(!n.in_quiet_hours(t(7, 0)));
        assert!(!n.in_quiet_hours(t(12, 0)));
        n.quiet_start = "13:00".into();
        n.quiet_end = "14:00".into();
        assert!(n.in_quiet_hours(t(13, 30)));
        assert!(!n.in_quiet_hours(t(23, 0)));
        n.quiet_hours = false;
        assert!(!n.in_quiet_hours(t(13, 30)));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    /// Pinned SHA-256 of the server certificate (hex).
    pub fingerprint: String,
    pub token: String,
}

pub fn dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("BLACKBUBBLES_CONFIG_DIR") {
        return Ok(PathBuf::from(dir));
    }
    Ok(directories::ProjectDirs::from("", "craftyderpysmall", "BlackBubbles").context("no config directory")?.config_local_dir().to_path_buf())
}

pub fn path() -> Result<PathBuf> {
    Ok(dir()?.join("config.toml"))
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = path()?;
        let Ok(text) = std::fs::read_to_string(&path) else { return Ok(Self::default()) };
        // A config copied in by hand (e.g. from the Linux PC) inherits the folder's access
        // list; lock it down instead of refusing to start.
        #[cfg(windows)]
        bb_net::restrict_to_owner(&path).with_context(|| format!("{} holds secrets; couldn't make it private", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn save(&self) -> Result<()> {
        let path = path()?;
        std::fs::create_dir_all(path.parent().unwrap())?;
        bb_net::write_private(&path, toml::to_string_pretty(self)?.as_bytes())
    }
}

