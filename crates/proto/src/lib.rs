//! BlackBubbles wire protocol.
//!
//! Frames are postcard-encoded [`ClientFrame`] / [`ServerFrame`] values sent as binary
//! WebSocket messages. Bump [`PROTOCOL_VERSION`] on any incompatible change.

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 7;

/// Milliseconds since the Unix epoch.
pub type Timestamp = i64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClientFrame {
    /// First frame from an already-paired device.
    Hello { protocol: u32, device_token: String },
    /// First frame from a new device, using a one-time code from `bbd pair`.
    Pair { protocol: u32, code: String, device_name: String },
    Request { id: u64, req: Request },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServerFrame {
    Welcome { protocol: u32, server_name: String },
    Paired { device_token: String },
    /// Fatal error; the server closes the connection after sending it.
    Rejected(ApiError),
    Response { id: u64, res: Result<Response, ApiError> },
    Event(Event),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Request {
    ListChats { limit: u32, before: Option<Timestamp> },
    ListMessages { chat_guid: String, limit: u32, before: Option<Timestamp> },
    SendText { chat_guid: String, text: String },
    ListCalendars,
    ListEvents { from: Timestamp, to: Timestamp },
    ListNoteFolders,
    ListNotes { folder_id: Option<String> },
    GetNote { id: String },
    ListVoiceMemos,
    ListAlbums,
    ListAssets { album_id: Option<String>, offset: u32, limit: u32 },
    SendAttachment { chat_guid: String, file_name: String, data: Vec<u8> },
    CreateEvent(NewEvent),
    DeleteEvent { id: String },
    CreateNote { folder_id: Option<String>, body: String },
    UpdateNote { id: String, body: String },
    /// `include_family`: also your Family Sharing members' devices.
    ListFindMyDevices { include_family: bool },
    /// Read part of a binary object (attachment, photo, recording).
    GetBlob { kind: BlobKind, id: String, offset: u64, len: u32 },
    ListReminderLists,
    /// `list_id: None` = every list.
    ListReminders { list_id: Option<String>, include_completed: bool },
    CreateReminder(NewReminder),
    SetReminderCompleted { id: String, completed: bool },
    DeleteReminder { id: String },
    /// Contact names for every phone number / email in Messages.
    ListContacts,
    /// Decrypt a password-protected note (read-only). The password isn't stored.
    UnlockNote { id: String, password: String },
    /// Move a note to Recently Deleted.
    DeleteNote { id: String },
    /// Structure/count facts about the Mac's Apple databases, for debugging.
    Diagnose,
    /// Every stored field of one locked note (and its account) as JSON, blobs in hex, for
    /// working out an encryption format offline (`note-probe`). Answered with `Diagnostics`.
    /// Still encrypted: no password or key leaves the Mac.
    NoteCryptoDump { id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BlobKind {
    Attachment,
    PhotoThumbnail,
    /// Full-size photo, converted to JPEG when needed.
    Photo,
    VoiceMemo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewEvent {
    pub calendar_id: String,
    pub title: String,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub start: Timestamp,
    pub end: Timestamp,
    pub all_day: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewReminder {
    /// `None` = the default list for new reminders.
    pub list_id: Option<String>,
    pub title: String,
    pub notes: Option<String>,
    pub due: Option<Timestamp>,
    /// `false` = the due date is a whole day (like an all-day event).
    pub due_has_time: bool,
    /// 0 none, 1 high, 5 medium, 9 low (EventKit's scale).
    pub priority: u8,
}

/// Largest blob chunk a client should ask for.
pub const MAX_BLOB_CHUNK: u32 = 4 * 1024 * 1024;

/// A source of data for the server: the real macOS backend or a mock.
///
/// `handle` may block (SQLite, EventKit); the server calls it off the async runtime.
pub trait Backend: Send + Sync + 'static {
    fn handle(&self, req: Request) -> Result<Response, ApiError>;
    /// Start pushing events. Called once at startup.
    fn start_events(&self, sink: EventSink);
}

pub type EventSink = std::sync::Arc<dyn Fn(Event) + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Response {
    Ok,
    Chats(Vec<Chat>),
    Messages(Vec<Message>),
    Calendars(Vec<Calendar>),
    Events(Vec<CalendarEvent>),
    NoteFolders(Vec<NoteFolder>),
    Notes(Vec<NoteSummary>),
    Note(Note),
    VoiceMemos(Vec<VoiceMemo>),
    Albums(Vec<Album>),
    Assets(Vec<Asset>),
    FindMyDevices(Vec<FindMyDevice>),
    Event(CalendarEvent),
    Created { id: String },
    Blob { data: Vec<u8>, total: u64, mime_type: Option<String> },
    ReminderLists(Vec<ReminderList>),
    Reminders(Vec<Reminder>),
    Reminder(Reminder),
    Contacts(Vec<ContactName>),
    Diagnostics(String),
}

/// A chat handle (exactly as in `Chat.participants` / `Message.sender`) and its contact name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContactName {
    pub handle: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
    pub fn internal(err: impl std::fmt::Display) -> Self {
        Self::new(ErrorCode::Internal, err.to_string())
    }
    pub fn not_found(what: impl std::fmt::Display) -> Self {
        Self::new(ErrorCode::NotFound, format!("{what} not found"))
    }
    pub fn unsupported(what: impl std::fmt::Display) -> Self {
        Self::new(ErrorCode::Unsupported, format!("{what} is not supported by this server"))
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for ApiError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    Unauthorized,
    NotFound,
    PermissionDenied,
    Unsupported,
    BadRequest,
    Internal,
}

/// Pushed by the server without a request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Event {
    NewMessage(Message),
    /// Plain text of every incoming iMessage/SMS, for autofill.
    IncomingText { sender: String, text: String, service: Service },
    CalendarChanged,
    NotesChanged,
    VoiceMemosChanged,
    PhotosChanged,
    RemindersChanged,
    /// Unread counts changed (read on another device, or a new message). Every chat with
    /// unread messages; chats not listed have none.
    UnreadChanged(Vec<UnreadCount>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnreadCount {
    pub chat_guid: String,
    pub count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Service {
    IMessage,
    Sms,
    Rcs,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chat {
    pub guid: String,
    pub display_name: Option<String>,
    pub participants: Vec<String>,
    pub service: Service,
    pub last_message: Option<Message>,
    /// Which of *your* addresses this conversation uses (phone number or email).
    /// Replies always go out from this address.
    pub my_address: Option<String>,
    /// Incoming messages not yet read on any Apple device (chat.db's `is_read`).
    pub unread: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub guid: String,
    pub chat_guid: String,
    pub sender: Option<String>,
    pub is_from_me: bool,
    pub text: Option<String>,
    pub date: Timestamp,
    pub attachments: Vec<Attachment>,
    /// A group-chat event shown as a line in the thread ("Anthony left the conversation").
    /// Its actor is `sender` (or you, when `is_from_me`).
    pub group_event: Option<GroupEvent>,
}

/// chat.db rows with a non-zero `item_type`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GroupEvent {
    /// The actor added `who` (a handle).
    Added { who: Option<String> },
    /// The actor removed `who`.
    Removed { who: Option<String> },
    /// The actor left the conversation.
    Left,
    /// The actor named the conversation (`None`: removed the name).
    Renamed { name: Option<String> },
    PhotoChanged,
    PhotoRemoved,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    pub guid: String,
    pub mime_type: Option<String>,
    pub file_name: Option<String>,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Calendar {
    pub id: String,
    pub title: String,
    /// 0xRRGGBB
    pub color: u32,
    pub writable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: String,
    pub calendar_id: String,
    pub title: String,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub start: Timestamp,
    pub end: Timestamp,
    pub all_day: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteFolder {
    pub id: String,
    pub name: String,
    pub account: String,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteSummary {
    pub id: String,
    pub folder_id: String,
    pub title: String,
    pub snippet: String,
    pub modified: Timestamp,
    /// Password-protected in Notes: the body isn't readable or editable here.
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub summary: NoteSummary,
    pub body: String,
}

/// A Reminders list (an EventKit reminder calendar).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReminderList {
    pub id: String,
    pub title: String,
    /// 0xRRGGBB
    pub color: u32,
    pub writable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reminder {
    pub id: String,
    pub list_id: String,
    pub title: String,
    pub notes: Option<String>,
    pub due: Option<Timestamp>,
    pub due_has_time: bool,
    pub completed: bool,
    pub completed_at: Option<Timestamp>,
    pub priority: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceMemo {
    pub id: String,
    pub title: String,
    pub created: Timestamp,
    pub duration_secs: f64,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Album {
    pub id: String,
    pub title: String,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub id: String,
    pub created: Timestamp,
    pub width: u32,
    pub height: u32,
    pub is_video: bool,
    pub favorite: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FindMyDevice {
    pub id: String,
    pub name: String,
    pub location: Option<Location>,
    pub battery: Option<f32>,
    /// e.g. "iPhone 16 Pro", "MacBook Air".
    pub model: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Location {
    pub lat: f64,
    pub lon: f64,
    pub accuracy_m: f32,
    pub at: Timestamp,
}

pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, postcard::Error> {
    postcard::to_stdvec(value)
}

pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, postcard::Error> {
    postcard::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip() {
        let frame = ServerFrame::Event(Event::IncomingText {
            sender: "+15551234567".into(),
            text: "Your code is 482913".into(),
            service: Service::Sms,
        });
        let bytes = encode(&frame).unwrap();
        assert_eq!(decode::<ServerFrame>(&bytes).unwrap(), frame);
    }
}
