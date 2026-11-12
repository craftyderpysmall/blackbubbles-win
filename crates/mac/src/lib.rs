//! macOS data access for BlackBubbles.
//!
//! Messages, Notes and Voice Memos read Apple's SQLite stores directly (fast, and testable
//! on any OS with fixtures). Calendar, Reminders and Photos use EventKit and PhotoKit. Writes go
//! through AppleScript or the frameworks, never by modifying Apple's databases.

pub mod contacts;
mod diagnose;
pub mod findmy;
pub mod messages;
pub mod notes;
#[doc(hidden)]
pub mod notecrypt;
mod typedstream;
mod util;
pub mod voicememos;

#[cfg(target_os = "macos")]
pub mod calendar;
#[cfg(target_os = "macos")]
pub mod photos;
#[cfg(target_os = "macos")]
pub mod reminders;

use std::path::Path;
use std::sync::Arc;

use bb_proto::{ApiError, Backend, BlobKind, EventSink, Request, Response};

/// Serve the main thread's run loop, forever. Cocoa frameworks do part of their work on the
/// main queue/run loop: EventKit refreshes its cache from change notifications there and
/// releases or resets a store through it. Without one (bbd used to run tokio on the main
/// thread), `reset()` and releasing a store hung every EventKit call, and a long-lived
/// store never saw changes made on the iPhone (2026-11-07).
#[cfg(target_os = "macos")]
pub fn run_main_loop() -> ! {
    use objc2_core_foundation::{CFRunLoop, CFRunLoopRunResult, kCFRunLoopDefaultMode};
    loop {
        // With no sources yet the loop returns at once; don't spin.
        if CFRunLoop::run_in_mode(unsafe { kCFRunLoopDefaultMode }, 60.0, false) == CFRunLoopRunResult::Finished {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}

/// Lock an EventKit store's mutex, giving up after `timeout`: if an EventKit call ever
/// hangs, later requests fail with a message instead of spinning forever.
pub(crate) fn lock_within<T>(m: &std::sync::Mutex<T>, timeout: std::time::Duration) -> Option<std::sync::MutexGuard<'_, T>> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match m.try_lock() {
            Ok(g) => return Some(g),
            Err(std::sync::TryLockError::Poisoned(p)) => return Some(p.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(std::sync::TryLockError::WouldBlock) => return None,
        }
    }
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64
}

pub struct MacBackend {
    messages: Arc<messages::Messages>,
    contacts: contacts::Contacts,
    notes: Arc<notes::Notes>,
    voice_memos: Arc<voicememos::VoiceMemos>,
    findmy: findmy::FindMy,
    #[cfg(target_os = "macos")]
    calendar: calendar::Calendar,
    #[cfg(target_os = "macos")]
    photos: photos::Photos,
    #[cfg(target_os = "macos")]
    reminders: reminders::Reminders,
}

impl MacBackend {
    pub fn new(data_dir: &Path) -> anyhow::Result<Self> {
        let cache = data_dir.join("cache");
        Ok(Self {
            messages: Arc::new(messages::Messages::new(messages::Messages::default_path(), cache.clone())),
            contacts: contacts::Contacts::new(contacts::Contacts::default_root()),
            notes: Arc::new(notes::Notes::new(notes::Notes::default_path())),
            voice_memos: Arc::new(voicememos::VoiceMemos::new(voicememos::VoiceMemos::default_dir(), cache.clone())),
            findmy: findmy::FindMy::new(data_dir),
            #[cfg(target_os = "macos")]
            calendar: calendar::Calendar::new(),
            #[cfg(target_os = "macos")]
            photos: photos::Photos::new(),
            #[cfg(target_os = "macos")]
            reminders: reminders::Reminders::new(),
        })
    }

    #[cfg(target_os = "macos")]
    fn calendar_or_photos(&self, req: Request) -> Result<Response, ApiError> {
        match req {
            Request::ListCalendars => self.calendar.calendars(),
            Request::ListEvents { from, to } => self.calendar.events(from, to),
            Request::CreateEvent(new) => self.calendar.create(&new),
            Request::DeleteEvent { id } => self.calendar.delete(&id),
            Request::ListReminderLists => self.reminders.lists(),
            Request::ListReminders { list_id, include_completed } => self.reminders.reminders(list_id.as_deref(), include_completed),
            Request::CreateReminder(new) => self.reminders.create(&new),
            Request::SetReminderCompleted { id, completed } => self.reminders.set_completed(&id, completed),
            Request::DeleteReminder { id } => self.reminders.delete(&id),
            Request::ListAlbums => self.photos.albums(),
            Request::ListAssets { album_id, offset, limit } => self.photos.assets(album_id.as_deref(), offset, limit),
            Request::GetBlob { kind: BlobKind::PhotoThumbnail, id, offset, len } => self.photos.image(&id, 320.0, offset, len),
            Request::GetBlob { kind: BlobKind::Photo, id, offset, len } => self.photos.image(&id, 2048.0, offset, len),
            other => Err(ApiError::unsupported(format!("{other:?}"))),
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn calendar_or_photos(&self, _req: Request) -> Result<Response, ApiError> {
        Err(ApiError::unsupported("Calendar, Reminders and Photos outside macOS"))
    }
}

impl Backend for MacBackend {
    fn handle(&self, req: Request) -> Result<Response, ApiError> {
        match req {
            Request::ListChats { limit, before } => self.messages.list_chats(limit, before),
            Request::ListMessages { chat_guid, limit, before } => self.messages.list_messages(&chat_guid, limit, before),
            Request::SendText { chat_guid, text } => messages::Messages::send_text(&chat_guid, &text),
            Request::SendAttachment { chat_guid, file_name, data } => {
                messages::Messages::send_attachment(&chat_guid, &file_name, &data)
            }
            Request::ListNoteFolders => self.notes.folders(),
            Request::ListNotes { folder_id } => self.notes.list(folder_id.as_deref()),
            Request::GetNote { id } => self.notes.get(&id),
            Request::CreateNote { folder_id, body } => notes::Notes::create(folder_id.as_deref(), &body),
            Request::UpdateNote { id, body } => {
                self.notes.ensure_unlocked(&id)?;
                notes::Notes::update(&id, &body)
            }
            Request::ListVoiceMemos => self.voice_memos.list(),
            Request::UnlockNote { id, password } => self.notes.unlock(&id, &password),
            Request::DeleteNote { id } => self.notes.delete(&id),
            Request::Diagnose => Ok(Response::Diagnostics(diagnose::report(
                self.notes.db_path(),
                &self.contacts,
                &self.messages.handles().unwrap_or_default(),
                self.voice_memos.dir(),
            ))),
            Request::NoteCryptoDump { id } => self.notes.crypto_dump(&id),
            Request::ListContacts => self.contacts.names(&self.messages.handles()?),
            Request::ListFindMyDevices { include_family } => self.findmy.list(include_family),
            Request::GetBlob { kind: BlobKind::Attachment, id, offset, len } => self.messages.attachment_blob(&id, offset, len),
            Request::GetBlob { kind: BlobKind::VoiceMemo, id, offset, len } => self.voice_memos.blob(&id, offset, len),
            other => self.calendar_or_photos(other),
        }
    }

    fn start_events(&self, sink: EventSink) {
        self.messages.clone().watch(sink.clone());
        self.notes.clone().watch(sink.clone());
        self.voice_memos.clone().watch(sink.clone());
        #[cfg(target_os = "macos")]
        {
            self.calendar.watch(sink.clone());
            self.reminders.watch(sink.clone());
            self.photos.watch(sink);
        }
    }
}
