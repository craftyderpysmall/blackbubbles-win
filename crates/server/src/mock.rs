//! Fake data for developing clients without a Mac (`bbd run --mock`).

use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bb_proto::*;

const HOUR: i64 = 3_600_000;
const DAY: i64 = 24 * HOUR;

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64
}

struct State {
    chats: Vec<Chat>,
    messages: Vec<Message>,
    events: Vec<CalendarEvent>,
    notes: Vec<Note>,
    reminders: Vec<Reminder>,
    next_id: u64,
}

pub struct MockBackend {
    state: Mutex<State>,
    code_interval: Option<Duration>,
    /// Set by `start_events`; writes echo events like the real Mac does.
    sink: Mutex<Option<EventSink>>,
}

impl MockBackend {
    /// Emit an event shortly after a write, like the Mac's chat.db / store watchers.
    fn emit_later(&self, event: Event) {
        if let Some(sink) = self.sink.lock().unwrap().clone() {
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(500));
                sink(event);
            });
        }
    }
}

impl MockBackend {
    pub fn new(code_interval: Option<Duration>) -> Self {
        let now = now_ms();
        let people = [
            ("iMessage;-;+15551230001", "Alex", Service::IMessage),
            ("iMessage;-;sam@example.com", "Sam", Service::IMessage),
            // Shown by number in chat.db; the name comes from Contacts.
            ("SMS;-;+15551230003", "+15551230003", Service::Sms),
            ("SMS;-;55512", "55512", Service::Sms),
        ];
        let mut messages = Vec::new();
        let mut chats = Vec::new();
        for (i, (guid, name, service)) in people.iter().enumerate() {
            let lines: &[(&str, bool)] = match i {
                0 => &[("are we still on for tonight?", false), ("yep! 7pm", true), ("see you there 🎉", false)],
                1 => &[("sent you the photos", false), ("thanks!!", true)],
                2 => &[("running 10 min late", false)],
                _ => &[("Your Acme verification code is 482913. Don't share it.", false)],
            };
            for (j, (text, from_me)) in lines.iter().enumerate() {
                messages.push(Message {
                    guid: format!("m-{i}-{j}"),
                    chat_guid: guid.to_string(),
                    sender: (!from_me).then(|| name.to_string()),
                    is_from_me: *from_me,
                    text: Some(text.to_string()),
                    date: now - (i as i64 + 1) * HOUR + j as i64 * 60_000,
                    attachments: if i == 1 && j == 0 {
                        vec![Attachment {
                            guid: "att-1".into(),
                            mime_type: Some("image/bmp".into()),
                            file_name: Some("beach.bmp".into()),
                            total_bytes: 0,
                        }]
                    } else {
                        vec![]
                    },
                    group_event: None,
                });
            }
            chats.push(Chat {
                guid: guid.to_string(),
                display_name: (!name.starts_with('+')).then(|| name.to_string()),
                participants: vec![name.to_string()],
                service: *service,
                last_message: None,
                my_address: Some(if i == 1 { "me@icloud.com".into() } else { "+15550000000".into() }),
                unread: if i == 0 { 2 } else { 0 },
            });
        }
        // A group chat with the events Messages records (rename, add, leave).
        let group = "iMessage;+;chat-weekend";
        let at = |m: i64| now - 30 * 60_000 + m * 60_000;
        let mut line = |id: &str, sender: Option<&str>, text: Option<&str>, date: i64, event: Option<GroupEvent>| {
            messages.push(Message {
                guid: id.into(),
                chat_guid: group.into(),
                sender: sender.map(str::to_owned),
                is_from_me: sender.is_none(),
                text: text.map(str::to_owned),
                date,
                attachments: vec![],
                group_event: event,
            })
        };
        line("g-0", None, None, at(0), Some(GroupEvent::Renamed { name: Some("Weekend trip 🏕️".into()) }));
        line("g-1", None, None, at(1), Some(GroupEvent::Added { who: Some("Anthony".into()) }));
        line("g-2", Some("Anthony"), Some("thanks for adding me!"), at(2), None);
        line("g-3", Some("Alex"), Some("I'll bring the tent"), at(3), None);
        line("g-4", Some("Sam"), None, at(4), Some(GroupEvent::Left));
        line("g-5", Some("Alex"), None, at(5), Some(GroupEvent::PhotoChanged));
        chats.push(Chat {
            guid: group.into(),
            display_name: Some("Weekend trip 🏕️".into()),
            participants: vec!["Alex".into(), "Anthony".into()],
            service: Service::IMessage,
            last_message: None,
            my_address: Some("+15550000000".into()),
            unread: 1,
        });
        let today = now - now.rem_euclid(DAY);
        let events = vec![
            event("e1", "home", "Dentist", today + 9 * HOUR, today + 10 * HOUR),
            event("e2", "work", "Standup", today + DAY + 15 * HOUR, today + DAY + 15 * HOUR + HOUR / 4),
            event("e3", "home", "Dinner with Alex", today + 2 * DAY + 19 * HOUR, today + 2 * DAY + 21 * HOUR),
        ];
        let notes = vec![
            note("n1", "f1", "Groceries", "Groceries\n- oat milk\n- coffee\n- limes", now - DAY),
            note("n2", "f1", "Ideas", "Ideas\nBlackBubbles but faster", now - 3 * DAY),
            note("n3", "f2", "Wi-Fi", "Wi-Fi\nnetwork: home5g", now - 30 * DAY),
            locked_note("n4", "f1", "Diary", now - 5 * DAY),
        ];
        let reminders = vec![
            reminder("r1", "inbox", "Call the bank", Some((today + 10 * HOUR, true)), false, 1),
            reminder("r2", "inbox", "Pay rent", Some((today - DAY, false)), false, 0),
            reminder("r3", "inbox", "Pick up package 📦", Some((today + DAY + 17 * HOUR, true)), false, 0),
            reminder("r4", "groceries", "Oat milk", None, false, 0),
            reminder("r5", "groceries", "Coffee beans ☕", None, false, 5),
            reminder("r6", "inbox", "Renew passport", Some((today - 3 * DAY, false)), true, 0),
            // Text-default emoji + VS16: a rendering regression check.
            reminder("r7", "inbox", "Flight to Lisbon \u{2708}\u{fe0f}", Some((today + 2 * DAY, false)), false, 0),
        ];
        Self {
            state: Mutex::new(State { chats, messages, events, notes, reminders, next_id: 100 }),
            code_interval,
            sink: Mutex::default(),
        }
    }
}

fn reminder(id: &str, list: &str, title: &str, due: Option<(i64, bool)>, completed: bool, priority: u8) -> Reminder {
    Reminder {
        id: id.into(),
        list_id: list.into(),
        title: title.into(),
        notes: None,
        due: due.map(|d| d.0),
        due_has_time: due.is_some_and(|d| d.1),
        completed,
        completed_at: completed.then(now_ms),
        priority,
    }
}

fn event(id: &str, cal: &str, title: &str, start: i64, end: i64) -> CalendarEvent {
    CalendarEvent {
        id: id.into(),
        calendar_id: cal.into(),
        title: title.into(),
        location: None,
        notes: None,
        start,
        end,
        all_day: false,
    }
}

fn note(id: &str, folder: &str, title: &str, body: &str, modified: i64) -> Note {
    Note {
        summary: NoteSummary {
            id: id.into(),
            folder_id: folder.into(),
            title: title.into(),
            snippet: body.lines().nth(1).unwrap_or_default().into(),
            modified,
            locked: false,
        },
        body: body.into(),
    }
}

fn locked_note(id: &str, folder: &str, title: &str, modified: i64) -> Note {
    let mut n = note(id, folder, title, "", modified);
    n.summary.snippet = "Locked".into();
    n.summary.locked = true;
    n.body = "🔒 This note is locked. Unlock it in Notes on your Mac.".into();
    n
}

/// A 24-bit BMP gradient, so the mock needs no image encoder.
fn bmp(width: u32, height: u32, seed: u32) -> Vec<u8> {
    let row = (width * 3).div_ceil(4) * 4;
    let size = 54 + row * height;
    let mut out = Vec::with_capacity(size as usize);
    out.extend_from_slice(b"BM");
    for v in [size, 0, 54, 40] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&(width as i32).to_le_bytes());
    out.extend_from_slice(&(height as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    for v in [0u32, row * height, 2835, 2835, 0, 0] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    let hue = seed.wrapping_mul(2_654_435_761);
    for y in 0..height {
        for x in 0..width {
            let (fx, fy) = (x * 255 / width.max(1), y * 255 / height.max(1));
            out.push(((fx + (hue & 0xff)) / 2) as u8);
            out.push(((fy + ((hue >> 8) & 0xff)) / 2) as u8);
            out.push((((255 - fx) + ((hue >> 16) & 0xff)) / 2) as u8);
        }
        out.resize(out.len() + (row - width * 3) as usize, 0);
    }
    out
}

/// A mono 16-bit WAV with a few tones.
fn wav(seconds: u32) -> Vec<u8> {
    let rate = 16_000u32;
    let samples = rate * seconds;
    let mut out = Vec::with_capacity(44 + samples as usize * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + samples * 2).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(samples * 2).to_le_bytes());
    for i in 0..samples {
        let t = i as f32 / rate as f32;
        let freq = [440.0, 554.4, 659.3][(t as usize) % 3];
        let sample = (t * freq * std::f32::consts::TAU).sin() * 0.25 * i16::MAX as f32;
        out.extend_from_slice(&(sample as i16).to_le_bytes());
    }
    out
}

fn slice_blob(data: Vec<u8>, offset: u64, len: u32, mime: &str) -> Result<Response, ApiError> {
    let total = data.len() as u64;
    let start = offset.min(total) as usize;
    let end = (offset + u64::from(len.min(MAX_BLOB_CHUNK))).min(total) as usize;
    Ok(Response::Blob { data: data[start..end].to_vec(), total, mime_type: Some(mime.into()) })
}

impl Backend for MockBackend {
    fn handle(&self, req: Request) -> Result<Response, ApiError> {
        let mut state = self.state.lock().unwrap();
        let state = &mut *state;
        Ok(match req {
            Request::ListChats { .. } => {
                let mut chats = state.chats.clone();
                for chat in &mut chats {
                    chat.last_message = state
                        .messages
                        .iter()
                        .filter(|m| m.chat_guid == chat.guid)
                        .max_by_key(|m| m.date)
                        .cloned();
                }
                chats.sort_by_key(|c| std::cmp::Reverse(c.last_message.as_ref().map(|m| m.date)));
                Response::Chats(chats)
            }
            Request::ListMessages { chat_guid, limit, before } => {
                let mut list: Vec<_> = state
                    .messages
                    .iter()
                    .filter(|m| m.chat_guid == chat_guid && before.is_none_or(|b| m.date < b))
                    .cloned()
                    .collect();
                list.sort_by_key(|m| m.date);
                let skip = list.len().saturating_sub(limit as usize);
                Response::Messages(list.split_off(skip))
            }
            Request::SendText { chat_guid, text } => {
                state.next_id += 1;
                let message = Message {
                    guid: format!("sent-{}", state.next_id),
                    chat_guid,
                    sender: None,
                    is_from_me: true,
                    text: Some(text),
                    date: now_ms(),
                    attachments: vec![],
                    group_event: None,
                };
                state.messages.push(message.clone());
                self.emit_later(Event::NewMessage(message));
                Response::Ok
            }
            Request::SendAttachment { chat_guid, file_name, data } => {
                state.next_id += 1;
                let mime = if file_name.to_lowercase().ends_with(".bmp") { Some("image/bmp".to_string()) } else { None };
                let message = Message {
                    guid: format!("sent-{}", state.next_id),
                    chat_guid,
                    sender: None,
                    is_from_me: true,
                    text: None,
                    date: now_ms(),
                    attachments: vec![Attachment { guid: format!("sent-att-{}", state.next_id), mime_type: mime, file_name: Some(file_name), total_bytes: data.len() as u64 }],
                    group_event: None,
                };
                state.messages.push(message.clone());
                self.emit_later(Event::NewMessage(message));
                Response::Ok
            }
            Request::ListCalendars => Response::Calendars(vec![
                Calendar { id: "home".into(), title: "Home".into(), color: 0x34c759, writable: true },
                Calendar { id: "work".into(), title: "Work".into(), color: 0x007aff, writable: true },
            ]),
            Request::ListEvents { from, to } => Response::Events(
                state.events.iter().filter(|e| e.end >= from && e.start <= to).cloned().collect(),
            ),
            Request::CreateEvent(new) => {
                state.next_id += 1;
                let id = format!("e{}", state.next_id);
                let created = CalendarEvent {
                    id: id.clone(),
                    calendar_id: new.calendar_id,
                    title: new.title,
                    location: new.location,
                    notes: new.notes,
                    start: new.start,
                    end: new.end,
                    all_day: new.all_day,
                };
                state.events.push(created.clone());
                self.emit_later(Event::CalendarChanged);
                Response::Event(created)
            }
            Request::DeleteEvent { id } => {
                state.events.retain(|e| e.id != id);
                self.emit_later(Event::CalendarChanged);
                Response::Ok
            }
            Request::ListNoteFolders => Response::NoteFolders(
                [("f1", "Notes"), ("f2", "Home")]
                    .iter()
                    .map(|(id, name)| NoteFolder {
                        id: id.to_string(),
                        name: name.to_string(),
                        account: "iCloud".into(),
                        count: state.notes.iter().filter(|n| n.summary.folder_id == *id).count() as u32,
                    })
                    .collect(),
            ),
            Request::ListNotes { folder_id } => Response::Notes(
                state
                    .notes
                    .iter()
                    .filter(|n| folder_id.as_ref().is_none_or(|f| *f == n.summary.folder_id))
                    .map(|n| n.summary.clone())
                    .collect(),
            ),
            Request::GetNote { id } => Response::Note(
                state.notes.iter().find(|n| n.summary.id == id).cloned().ok_or_else(|| ApiError::not_found("note"))?,
            ),
            Request::CreateNote { folder_id, body } => {
                state.next_id += 1;
                let id = format!("n{}", state.next_id);
                let title = body.lines().next().unwrap_or("New Note").to_string();
                state.notes.push(note(&id, folder_id.as_deref().unwrap_or("f1"), &title, &body, now_ms()));
                self.emit_later(Event::NotesChanged);
                Response::Created { id }
            }
            Request::UpdateNote { id, body } => {
                let n = state.notes.iter_mut().find(|n| n.summary.id == id).ok_or_else(|| ApiError::not_found("note"))?;
                if n.summary.locked {
                    return Err(ApiError::new(ErrorCode::PermissionDenied, "This note is locked. Unlock it in Notes on your Mac to edit it."));
                }
                let updated = note(&id, &n.summary.folder_id.clone(), body.lines().next().unwrap_or(""), &body, now_ms());
                *n = updated;
                self.emit_later(Event::NotesChanged);
                Response::Ok
            }
            Request::UnlockNote { id, password } => {
                let n = state.notes.iter().find(|n| n.summary.id == id).ok_or_else(|| ApiError::not_found("note"))?;
                if !n.summary.locked {
                    return Ok(Response::Note(n.clone()));
                }
                if password != "1234" {
                    return Err(ApiError::new(ErrorCode::PermissionDenied, "Wrong password. (The mock's password is 1234.)"));
                }
                let mut open = n.clone();
                open.body = format!("{}\nDear diary, the mock password is 1234 🔓", n.summary.title);
                Response::Note(open)
            }
            Request::DeleteNote { id } => {
                state.notes.retain(|n| n.summary.id != id);
                self.emit_later(Event::NotesChanged);
                Response::Ok
            }
            Request::Diagnose => Response::Diagnostics("mock server: nothing to diagnose".into()),
            Request::NoteCryptoDump { .. } => Response::Diagnostics("{}".into()),
            Request::ListContacts => Response::Contacts(vec![ContactName { handle: "+15551230003".into(), name: "Taylor Kim".into() }]),
            Request::ListVoiceMemos => Response::VoiceMemos(
                (1..=3)
                    .map(|i| VoiceMemo {
                        id: format!("vm{i}"),
                        title: ["Song idea", "Lecture", "Reminder"][i - 1].into(),
                        created: now_ms() - i as i64 * DAY,
                        duration_secs: (i * 4) as f64,
                        bytes: 44 + 32_000 * (i as u64 * 4),
                    })
                    .collect(),
            ),
            Request::ListReminderLists => Response::ReminderLists(vec![
                ReminderList { id: "inbox".into(), title: "Reminders".into(), color: 0xff9500, writable: true },
                ReminderList { id: "groceries".into(), title: "Groceries".into(), color: 0x34c759, writable: true },
            ]),
            Request::ListReminders { list_id, include_completed } => Response::Reminders(
                state
                    .reminders
                    .iter()
                    .filter(|r| list_id.as_ref().is_none_or(|l| *l == r.list_id) && (include_completed || !r.completed))
                    .cloned()
                    .collect(),
            ),
            Request::CreateReminder(new) => {
                state.next_id += 1;
                let created = Reminder {
                    id: format!("r{}", state.next_id),
                    list_id: new.list_id.unwrap_or_else(|| "inbox".into()),
                    title: new.title,
                    notes: new.notes,
                    due: new.due,
                    due_has_time: new.due.is_some() && new.due_has_time,
                    completed: false,
                    completed_at: None,
                    priority: new.priority,
                };
                state.reminders.push(created.clone());
                self.emit_later(Event::RemindersChanged);
                Response::Reminder(created)
            }
            Request::SetReminderCompleted { id, completed } => {
                let r = state.reminders.iter_mut().find(|r| r.id == id).ok_or_else(|| ApiError::not_found("reminder"))?;
                r.completed = completed;
                r.completed_at = completed.then(now_ms);
                self.emit_later(Event::RemindersChanged);
                Response::Ok
            }
            Request::DeleteReminder { id } => {
                state.reminders.retain(|r| r.id != id);
                self.emit_later(Event::RemindersChanged);
                Response::Ok
            }
            Request::ListAlbums => Response::Albums(vec![
                Album { id: "all".into(), title: "Recents".into(), count: 120 },
                Album { id: "fav".into(), title: "Favorites".into(), count: 12 },
            ]),
            Request::ListAssets { album_id, offset, limit } => {
                let total = if album_id.as_deref() == Some("fav") { 12 } else { 120 };
                Response::Assets(
                    (offset..(offset + limit).min(total))
                        .map(|i| Asset {
                            id: format!("p{i}"),
                            created: now_ms() - i as i64 * DAY / 3,
                            width: 1024,
                            height: 768,
                            is_video: i % 17 == 0,
                            favorite: i < 12,
                        })
                        .collect(),
                )
            }
            Request::ListFindMyDevices { include_family } => {
                let mut devices = vec![
                    FindMyDevice {
                        id: "iphone".into(),
                        name: "iPhone".into(),
                        model: "iPhone 16".into(),
                        battery: Some(0.82),
                        location: Some(Location { lat: 37.3349, lon: -122.0090, accuracy_m: 15.0, at: now_ms() - 120_000 }),
                    },
                    FindMyDevice {
                        id: "mac".into(),
                        name: "Mac mini".into(),
                        model: "Mac mini".into(),
                        battery: None,
                        location: Some(Location { lat: 37.3318, lon: -122.0312, accuracy_m: 30.0, at: now_ms() - 600_000 }),
                    },
                ];
                if include_family {
                    devices.push(FindMyDevice {
                        id: "family-iphone".into(),
                        name: "Mom's iPhone".into(),
                        model: "iPhone 15".into(),
                        battery: Some(0.12),
                        location: Some(Location { lat: 37.3305, lon: -122.0280, accuracy_m: 20.0, at: now_ms() - 900_000 }),
                    });
                }
                Response::FindMyDevices(devices)
            }
            Request::GetBlob { kind, id, offset, len } => match kind {
                BlobKind::PhotoThumbnail => slice_blob(bmp(256, 256, hash(&id)), offset, len, "image/bmp")?,
                BlobKind::Photo | BlobKind::Attachment => {
                    slice_blob(bmp(1024, 768, hash(&id)), offset, len, "image/bmp")?
                }
                BlobKind::VoiceMemo => {
                    let n: u32 = id.trim_start_matches("vm").parse().unwrap_or(1);
                    slice_blob(wav(n * 4), offset, len, "audio/wav")?
                }
            },
        })
    }

    fn start_events(&self, sink: EventSink) {
        *self.sink.lock().unwrap() = Some(sink.clone());
        let Some(interval) = self.code_interval else { return };
        std::thread::spawn(move || {
            let mut n = 0u32;
            loop {
                std::thread::sleep(interval);
                n += 1;
                let code = 100_000 + (n.wrapping_mul(7919) % 900_000);
                let text = format!("Your Acme verification code is {code}. Don't share it.");
                let message = Message {
                    guid: format!("code-{n}"),
                    chat_guid: "SMS;-;55512".into(),
                    sender: Some("55512".into()),
                    is_from_me: false,
                    text: Some(text.clone()),
                    date: now_ms(),
                    attachments: vec![],
                    group_event: None,
                };
                sink(Event::NewMessage(message));
                sink(Event::IncomingText { sender: "55512".into(), text, service: Service::Sms });
                // As if the group chat had been read on the iPhone meanwhile.
                sink(Event::UnreadChanged(vec![UnreadCount { chat_guid: "SMS;-;55512".into(), count: n }]));
            }
        });
    }
}

fn hash(s: &str) -> u32 {
    s.bytes().fold(2166136261u32, |h, b| (h ^ u32::from(b)).wrapping_mul(16777619))
}
