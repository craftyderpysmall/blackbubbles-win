//! Put a code on the clipboard, show a notification, and restore the clipboard later.
//!
//! Same rules as linuxautofill's `Deliverer`:
//! - the same value within 120 s is ignored (codes often arrive by SMS *and* email);
//! - the previous clipboard is saved first, unless a password manager put it there;
//! - after `clear_after` seconds it is restored, but only if the code is still there;
//! - nothing is typed or opened without a click.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tracing::info;

use crate::platform::{Clip, Platform};

const DEDUPE: Duration = Duration::from_secs(120);

#[derive(Default)]
struct State {
    recent: HashMap<String, Instant>,
    /// Clipboard before the first of a run of codes. `Some(None)` = it was empty.
    saved: Option<Option<Clip>>,
    active: Option<String>,
    generation: u64,
}

#[derive(Clone)]
pub struct Deliverer {
    platform: Arc<dyn Platform>,
    /// Milliseconds. Shared and atomic so Settings changes apply to the next code without a
    /// restart.
    clear_after: Arc<AtomicU64>,
    state: Arc<Mutex<State>>,
    notify: Arc<AtomicBool>,
}

fn host_of(url: &str) -> String {
    url.split("://").nth(1).unwrap_or(url).split(['/', '?', '#']).next().unwrap_or_default().rsplit('@').next().unwrap_or_default().to_string()
}

impl Deliverer {
    pub fn new(platform: Arc<dyn Platform>, clear_after: Duration) -> Self {
        Self {
            platform,
            clear_after: Arc::new(AtomicU64::new(clear_after.as_millis() as u64)),
            state: Arc::default(),
            notify: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Without notifications codes are only copied (links can't be offered at all).
    pub fn with_notifications(self, notify: bool) -> Self {
        self.set_notifications(notify);
        self
    }

    /// Change the restore delay; applies from the next code on.
    pub fn set_clear_after(&self, clear_after: Duration) {
        self.clear_after.store(clear_after.as_millis() as u64, Ordering::Relaxed);
    }

    pub fn set_notifications(&self, notify: bool) {
        self.notify.store(notify, Ordering::Relaxed);
    }

    fn clear_after(&self) -> Duration {
        Duration::from_millis(self.clear_after.load(Ordering::Relaxed))
    }

    fn notifies(&self) -> bool {
        self.notify.load(Ordering::Relaxed)
    }

    fn is_duplicate(&self, value: &str) -> bool {
        let mut state = self.state.lock().unwrap();
        state.recent.retain(|_, at| at.elapsed() < DEDUPE);
        if state.recent.contains_key(value) {
            return true;
        }
        state.recent.insert(value.to_string(), Instant::now());
        false
    }

    fn copy_with_restore(&self, value: &str) {
        let generation = {
            let mut state = self.state.lock().unwrap();
            if state.saved.is_none() {
                state.saved = Some(self.platform.snapshot());
            }
            self.platform.copy(value);
            state.active = Some(value.to_string());
            state.generation += 1;
            state.generation
        };
        let (this, value, delay) = (self.clone(), value.to_string(), self.clear_after());
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            let mut state = this.state.lock().unwrap();
            if state.generation != generation {
                return; // a newer code took over; it will restore
            }
            if this.platform.current_text().as_deref() == Some(value.as_str()) {
                let saved = state.saved.take().flatten();
                this.platform.restore(saved.as_ref());
            }
            state.saved = None;
            state.active = None;
        });
    }

    /// Restore the clipboard immediately if a code is still on it (call on shutdown, so a
    /// code is never left behind when the process exits before the timer fires).
    pub fn restore_now(&self) {
        let mut state = self.state.lock().unwrap();
        state.generation += 1; // cancel pending timers
        if let Some(active) = state.active.take()
            && self.platform.current_text().as_deref() == Some(active.as_str())
        {
            let saved = state.saved.take().flatten();
            self.platform.restore(saved.as_ref());
        }
        state.saved = None;
    }

    pub fn deliver_code(&self, code: &str, sender: &str) {
        if self.is_duplicate(code) {
            return;
        }
        self.copy_with_restore(code);
        if !self.notifies() {
            info!("autofill: copied a code");
            return;
        }
        let can_type = self.platform.can_type();
        let body = if can_type { format!("From {sender}. Click to paste.") } else { format!("From {sender}. Copied to the clipboard.") };
        let (platform, code, secs) = (self.platform.clone(), code.to_string(), self.clear_after().as_secs());
        std::thread::spawn(move || {
            let clicked = platform.notify(&code, &body, can_type.then_some("Paste"), secs);
            if clicked && can_type {
                platform.type_text(&code);
            }
        });
        info!("autofill: delivered a code");
    }

    pub fn deliver_link(&self, url: &str, _sender: &str) {
        if !self.notifies() || self.is_duplicate(url) {
            return;
        }
        // Show the host so lookalike domains stand out.
        let body = format!("Opens {}. Click to open.", host_of(url));
        let (platform, url, secs) = (self.platform.clone(), url.to_string(), self.clear_after().as_secs());
        std::thread::spawn(move || {
            if platform.notify("Verification link", &body, Some("Open"), secs) {
                platform.open_url(&url);
            }
        });
        info!("autofill: offered a link");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Fake {
        clipboard: Mutex<Option<String>>,
        notes: Mutex<Vec<String>>,
        typed: Mutex<Vec<String>>,
    }

    impl Platform for Fake {
        fn snapshot(&self) -> Option<Clip> {
            self.clipboard.lock().unwrap().clone().map(|t| Clip { mime: "text/plain".into(), data: t.into_bytes() })
        }
        fn copy(&self, text: &str) {
            *self.clipboard.lock().unwrap() = Some(text.into());
        }
        fn current_text(&self) -> Option<String> {
            self.clipboard.lock().unwrap().clone()
        }
        fn restore(&self, clip: Option<&Clip>) {
            *self.clipboard.lock().unwrap() = clip.map(|c| String::from_utf8_lossy(&c.data).into_owned());
        }
        fn notify(&self, summary: &str, _body: &str, _action: Option<&str>, _secs: u64) -> bool {
            self.notes.lock().unwrap().push(summary.into());
            true
        }
        fn can_type(&self) -> bool {
            true
        }
        fn type_text(&self, text: &str) {
            self.typed.lock().unwrap().push(text.into());
        }
        fn open_url(&self, _url: &str) {}
    }

    #[test]
    fn copies_types_dedupes_and_restores() {
        let fake = Arc::new(Fake::default());
        *fake.clipboard.lock().unwrap() = Some("my notes".into());
        let d = Deliverer::new(fake.clone(), Duration::from_millis(200));
        d.deliver_code("482913", "Acme");
        d.deliver_code("482913", "Acme"); // duplicate via email
        assert_eq!(fake.current_text().as_deref(), Some("482913"));
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(fake.notes.lock().unwrap().len(), 1);
        assert_eq!(fake.typed.lock().unwrap().as_slice(), ["482913"]);
        std::thread::sleep(Duration::from_millis(250));
        assert_eq!(fake.current_text().as_deref(), Some("my notes"));
    }

    #[test]
    fn leaves_clipboard_alone_if_user_copied_something() {
        let fake = Arc::new(Fake::default());
        let d = Deliverer::new(fake.clone(), Duration::from_millis(100));
        d.deliver_code("111111", "x");
        fake.copy("something else");
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(fake.current_text().as_deref(), Some("something else"));
    }

    #[test]
    fn restore_now_puts_clipboard_back() {
        let fake = Arc::new(Fake::default());
        *fake.clipboard.lock().unwrap() = Some("before".into());
        let d = Deliverer::new(fake.clone(), Duration::from_secs(60));
        d.deliver_code("424242", "x");
        assert_eq!(fake.current_text().as_deref(), Some("424242"));
        d.restore_now();
        assert_eq!(fake.current_text().as_deref(), Some("before"));
    }

    #[test]
    fn restore_delay_changes_apply_to_the_next_code() {
        let fake = Arc::new(Fake::default());
        *fake.clipboard.lock().unwrap() = Some("before".into());
        let d = Deliverer::new(fake.clone(), Duration::from_secs(60)).with_notifications(false);
        d.set_clear_after(Duration::from_millis(100));
        d.deliver_code("135790", "x");
        std::thread::sleep(Duration::from_millis(250));
        assert_eq!(fake.current_text().as_deref(), Some("before"));
    }

    #[test]
    fn host_extraction() {
        assert_eq!(host_of("https://user@login.example.com:443/path?q"), "login.example.com:443");
    }
}
