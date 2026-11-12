//! Desktop notifications, filtered by the user's settings.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use bb_proto::{Event, Message, Request, Response, Service};

use crate::config::{Notifications, Services};
use crate::conn::{Client, Status};

/// What the notifier reads: notification choices plus which services are shown.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub notifications: Notifications,
    pub services: Services,
}

impl From<&crate::config::Config> for Settings {
    fn from(c: &crate::config::Config) -> Self {
        Self { notifications: c.notifications.clone(), services: c.services }
    }
}

pub type Shared = Arc<RwLock<Settings>>;

/// When a reminder alerts: at its time, or 09:00 local on its day (as iOS does) if it has
/// only a date. EventKit gives date-only due dates as local midnight.
fn reminder_alert_at(r: &bb_proto::Reminder) -> Option<i64> {
    let due = r.due?;
    (!r.completed).then_some(if r.due_has_time { due } else { due + 9 * 3_600_000 })
}

/// Fire within five minutes after the alert time (polls are 30 s apart); never early, and
/// never for something long past (e.g. after the PC was asleep for a day).
fn alert_due(at: i64, now: i64) -> bool {
    at <= now && now - at < 5 * 60_000
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reminder(due: Option<i64>, has_time: bool, completed: bool) -> bb_proto::Reminder {
        bb_proto::Reminder {
            id: "r".into(),
            list_id: "l".into(),
            title: "t".into(),
            notes: None,
            due,
            due_has_time: has_time,
            completed,
            completed_at: None,
            priority: 0,
        }
    }

    #[test]
    fn reminder_alert_times() {
        let midnight = 1_800_000_000_000;
        assert_eq!(reminder_alert_at(&reminder(Some(midnight + 60_000), true, false)), Some(midnight + 60_000));
        assert_eq!(reminder_alert_at(&reminder(Some(midnight), false, false)), Some(midnight + 9 * 3_600_000));
        assert_eq!(reminder_alert_at(&reminder(None, false, false)), None);
        assert_eq!(reminder_alert_at(&reminder(Some(midnight), true, true)), None);
        assert!(alert_due(1000, 1000) && alert_due(1000, 1000 + 4 * 60_000));
        assert!(!alert_due(1000, 999) && !alert_due(1000, 1000 + 6 * 60_000));
    }
}

pub struct Notifier {
    client: Client,
    settings: Shared,
    /// Whether the window has focus (always false in the daemon).
    focused: Arc<AtomicBool>,
    /// Autofill is running in this process and notifies about codes itself.
    autofill_notifies: bool,
    chat_names: Mutex<HashMap<String, String>>,
}

fn send(summary: &str, body: &str) {
    bb_autofill::platform::simple_notify(summary, body);
}

impl Notifier {
    pub fn start(client: &Client, settings: Shared, focused: Arc<AtomicBool>, autofill_notifies: bool) -> Arc<Self> {
        let this = Arc::new(Self {
            client: client.clone(),
            settings,
            focused,
            autofill_notifies,
            chat_names: Mutex::default(),
        });
        let hook = this.clone();
        client.on_event(Arc::new(move |event| hook.on_event(event)));
        let poller = this.clone();
        let closed = client.clone();
        client.spawn(async move {
            tokio::select! {
                _ = closed.closed() => {}
                _ = poller.poll_loop() => {}
            }
        });
        this
    }

    /// Current settings if notifications may be shown right now.
    fn active(&self) -> Option<(Notifications, Services)> {
        let Settings { notifications, services } = self.settings.read().unwrap().clone();
        (notifications.enabled && !notifications.in_quiet_hours(chrono::Local::now().time())).then_some((notifications, services))
    }

    fn on_event(self: &Arc<Self>, event: &Event) {
        let Some((s, on)) = self.active() else { return };
        match event {
            Event::NewMessage(message) if on.messages => self.on_message(&s, message),
            Event::PhotosChanged if s.photos && on.photos => send("Photos", "New items in your library"),
            Event::NotesChanged if s.notes && on.notes => send("Notes", "A note was changed on your Mac"),
            Event::VoiceMemosChanged if s.voice_memos && on.voice_memos => send("Voice Memos", "Your recordings changed"),
            _ => {}
        }
    }

    fn on_message(self: &Arc<Self>, s: &Notifications, message: &Message) {
        if message.is_from_me || message.group_event.is_some() || !s.messages || s.muted_chats.contains(&message.chat_guid) {
            return;
        }
        if s.only_when_unfocused && self.focused.load(Ordering::Relaxed) {
            return;
        }
        let service = if message.chat_guid.starts_with("SMS") || message.chat_guid.starts_with("RCS") { Service::Sms } else { Service::IMessage };
        if (service == Service::Sms && !s.sms) || (service == Service::IMessage && !s.imessage) {
            return;
        }
        let text = message.text.clone().unwrap_or_else(|| if message.attachments.is_empty() { String::new() } else { "Attachment".into() });
        if s.skip_codes && self.autofill_notifies && bb_autofill::find_code(&text).is_some() {
            return;
        }
        let body = if s.show_preview { text } else { "New message".into() };
        let (show_sender, guid, sender) = (s.show_sender, message.chat_guid.clone(), message.sender.clone().unwrap_or_default());
        let known = self.chat_names.lock().unwrap().get(&guid).cloned();
        let this = self.clone();
        self.client.spawn(async move {
            let name = match known {
                Some(name) => name,
                None => this.lookup_chat(&guid).await.unwrap_or(sender),
            };
            let title = if show_sender { name } else { "BlackBubbles".into() };
            send(&title, &body);
        });
    }

    async fn lookup_chat(&self, guid: &str) -> Option<String> {
        let Ok(Response::Chats(chats)) = self.client.request(Request::ListChats { limit: 500, before: None }).await else { return None };
        let contacts: HashMap<String, String> = match self.client.request(Request::ListContacts).await {
            Ok(Response::Contacts(list)) => list.into_iter().map(|c| (c.handle, c.name)).collect(),
            _ => HashMap::new(),
        };
        let mut names = self.chat_names.lock().unwrap();
        for chat in chats {
            let people: Vec<&str> = chat.participants.iter().map(|h| contacts.get(h).map_or(h.as_str(), String::as_str)).collect();
            let name = chat.display_name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| people.join(", "));
            names.insert(chat.guid, name);
        }
        names.get(guid).cloned()
    }

    /// Calendar reminders, Find My battery checks and connection health.
    async fn poll_loop(self: Arc<Self>) {
        let mut reminded: HashSet<(String, i64)> = HashSet::new();
        let mut due_told: HashSet<(String, i64)> = HashSet::new();
        let mut low: HashSet<String> = HashSet::new();
        let mut last_find_my = Instant::now() - Duration::from_secs(3600);
        let mut down_since: Option<Instant> = None;
        let mut told_down = false;
        let mut tick = tokio::time::interval(Duration::from_secs(30));
        loop {
            tick.tick().await;
            let settings = self.active();
            let connected = matches!(self.client.status(), Status::Connected { .. });

            // Connection health (reported even if other categories are off).
            match (connected, down_since) {
                (false, None) => down_since = Some(Instant::now()),
                (false, Some(since)) if !told_down && since.elapsed() > Duration::from_secs(90) => {
                    if settings.as_ref().is_some_and(|(s, _)| s.connection) && !matches!(self.client.status(), Status::NotPaired) {
                        send("Lost connection to your Mac", "Messages and autofill from the Mac are paused until it's back.");
                    }
                    told_down = true;
                }
                (true, Some(_)) => {
                    if told_down && settings.as_ref().is_some_and(|(s, _)| s.connection) {
                        send("Reconnected to your Mac", "Everything is syncing again.");
                    }
                    down_since = None;
                    told_down = false;
                }
                _ => {}
            }
            let Some((s, on)) = settings else { continue };
            if !connected {
                continue;
            }

            if s.reminders_due && on.reminders {
                let now = chrono::Utc::now().timestamp_millis();
                if let Ok(Response::Reminders(list)) = self.client.request(Request::ListReminders { list_id: None, include_completed: false }).await {
                    for r in list {
                        let Some(at) = reminder_alert_at(&r) else { continue };
                        if alert_due(at, now) && due_told.insert((r.id.clone(), at)) {
                            send(&r.title, if r.due_has_time { "Reminder" } else { "Reminder · due today" });
                        }
                    }
                }
                due_told.retain(|(_, at)| *at > now - 86_400_000);
            }

            if s.calendar_reminders && on.calendar {
                let now = chrono::Utc::now().timestamp_millis();
                let lead = i64::from(s.reminder_minutes) * 60_000;
                if let Ok(Response::Events(events)) = self.client.request(Request::ListEvents { from: now, to: now + lead + 60_000 }).await {
                    for event in events.into_iter().filter(|e| !e.all_day && e.start > now - 60_000 && e.start - lead <= now) {
                        if reminded.insert((event.id.clone(), event.start)) {
                            let minutes = ((event.start - now) / 60_000).max(0);
                            let when = if minutes == 0 { "Starting now".to_string() } else { format!("In {minutes} min") };
                            let body = match &event.location {
                                Some(location) => format!("{when} · {location}"),
                                None => when,
                            };
                            send(&event.title, &body);
                        }
                    }
                }
                reminded.retain(|(_, start)| *start > now - 86_400_000);
            }

            if on.find_my && s.low_battery && last_find_my.elapsed() > Duration::from_secs(600) {
                last_find_my = Instant::now();
                if let Ok(Response::FindMyDevices(devices)) = self.client.request(Request::ListFindMyDevices { include_family: on.find_my_family }).await {
                    for device in devices {
                        if let Some(battery) = device.battery {
                            let percent = (battery * 100.0).round() as u8;
                            if percent < s.low_battery_percent {
                                if low.insert(device.id.clone()) && s.low_battery {
                                    send(&format!("{} battery low", device.name), &format!("{percent}% remaining"));
                                }
                            } else {
                                low.remove(&device.id);
                            }
                        }
                    }
                }
            }
        }
    }
}
