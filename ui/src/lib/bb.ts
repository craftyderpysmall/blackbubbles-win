// The Rust bridge: proto types (serde's externally tagged JSON), commands, events, blobs.
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Timestamp = number; // ms since the Unix epoch
export type Service = "IMessage" | "Sms" | "Rcs";
export type BlobKind = "Attachment" | "PhotoThumbnail" | "Photo" | "VoiceMemo";

export interface Attachment {
  guid: string;
  mime_type: string | null;
  file_name: string | null;
  total_bytes: number;
}
export interface Message {
  guid: string;
  chat_guid: string;
  sender: string | null;
  is_from_me: boolean;
  text: string | null;
  date: Timestamp;
  attachments: Attachment[];
  /** Group-chat event shown as a centred line; its actor is `sender` (or you). */
  group_event: GroupEvent | null;
}
export type GroupEvent =
  | { Added: { who: string | null } }
  | { Removed: { who: string | null } }
  | "Left"
  | { Renamed: { name: string | null } }
  | "PhotoChanged"
  | "PhotoRemoved";
export interface Chat {
  guid: string;
  display_name: string | null;
  participants: string[];
  service: Service;
  last_message: Message | null;
  my_address: string | null;
  /** Incoming messages not yet read on any Apple device. */
  unread: number;
}
export interface UnreadCount {
  chat_guid: string;
  count: number;
}
export interface Calendar {
  id: string;
  title: string;
  color: number;
  writable: boolean;
}
export interface CalendarEvent {
  id: string;
  calendar_id: string;
  title: string;
  location: string | null;
  notes: string | null;
  start: Timestamp;
  end: Timestamp;
  all_day: boolean;
}
export type NewEvent = Omit<CalendarEvent, "id">;
export interface NoteFolder {
  id: string;
  name: string;
  account: string;
  count: number;
}
export interface NoteSummary {
  id: string;
  folder_id: string;
  title: string;
  snippet: string;
  modified: Timestamp;
  /** Password-protected in Notes: body unreadable here, editing refused. */
  locked: boolean;
}
export interface Note {
  summary: NoteSummary;
  body: string;
}
export interface ReminderList {
  id: string;
  title: string;
  color: number;
  writable: boolean;
}
export interface Reminder {
  id: string;
  list_id: string;
  title: string;
  notes: string | null;
  due: Timestamp | null;
  /** false: due on a whole day (no time). */
  due_has_time: boolean;
  completed: boolean;
  completed_at: Timestamp | null;
  /** 0 none, 1 high, 5 medium, 9 low. */
  priority: number;
}
export interface NewReminder {
  list_id: string | null;
  title: string;
  notes: string | null;
  due: Timestamp | null;
  due_has_time: boolean;
  priority: number;
}

export interface VoiceMemo {
  id: string;
  title: string;
  created: Timestamp;
  duration_secs: number;
  bytes: number;
}
export interface Album {
  id: string;
  title: string;
  count: number;
}
export interface Asset {
  id: string;
  created: Timestamp;
  width: number;
  height: number;
  is_video: boolean;
  favorite: boolean;
}
export interface Location {
  lat: number;
  lon: number;
  accuracy_m: number;
  at: Timestamp;
}
export interface FindMyDevice {
  id: string;
  name: string;
  location: Location | null;
  battery: number | null;
  model: string;
}

export type ServerEvent =
  | { NewMessage: Message }
  | { UnreadChanged: UnreadCount[] }
  | "CalendarChanged"
  | "NotesChanged"
  | "VoiceMemosChanged"
  | "PhotosChanged"
  | "RemindersChanged";

export type Status =
  | { state: "NotPaired" }
  | { state: "Connecting" }
  | { state: "Connected"; detail: { server_name: string } }
  | { state: "Disconnected"; detail: string };

export interface ApiError {
  code: "Unauthorized" | "NotFound" | "PermissionDenied" | "Unsupported" | "BadRequest" | "Internal";
  message: string;
}

export interface Notifications {
  enabled: boolean;
  messages: boolean;
  imessage: boolean;
  sms: boolean;
  show_sender: boolean;
  show_preview: boolean;
  skip_codes: boolean;
  only_when_unfocused: boolean;
  muted_chats: string[];
  calendar_reminders: boolean;
  reminder_minutes: number;
  reminders_due: boolean;
  photos: boolean;
  notes: boolean;
  voice_memos: boolean;
  low_battery: boolean;
  low_battery_percent: number;
  connection: boolean;
  quiet_hours: boolean;
  quiet_start: string;
  quiet_end: string;
}

/** Which Apple services BlackBubbles shows (tabs + notifications). */
export interface Services {
  messages: boolean;
  photos: boolean;
  notes: boolean;
  calendar: boolean;
  reminders: boolean;
  voice_memos: boolean;
  find_my: boolean;
  find_my_family: boolean;
}

export interface ConfigView {
  server: { host: string; port: number; fingerprint: string } | null;
  autofill: {
    enabled: boolean;
    from_server: boolean;
    notify: boolean;
    clear_after: number;
    http: { port: number; token: string } | null;
    gmail: string[];
  };
  notifications: Notifications;
  services: Services;
}

export interface Bootstrap {
  status: Status;
  config: ConfigView;
  primary: boolean;
  startup_error: string | null;
  platform: "windows";
  device_name: string;
  user: string;
  version: string;
  config_path: string;
  dev_tab: string | null;
  dev_open_first_chat: boolean;
  dev_force_width: number | null;
}

export interface AudioState {
  id: string | null;
  title: string;
  duration: number;
  position: number;
  paused: boolean;
  ended: boolean;
  error: string | null;
}

/** Error text for toasts, from an ApiError, a string or anything else. */
export function errorText(err: unknown): string {
  if (typeof err === "string") return err;
  if (err && typeof err === "object" && "message" in err) return String((err as { message: unknown }).message);
  return String(err);
}

type Variant<T, K extends string> = { [P in K]: T };

async function call<T, K extends string>(req: object | string, key: K): Promise<T> {
  const res = await invoke<Variant<T, K> | string>("request", { req });
  if (typeof res === "object" && res && key in res) return (res as Variant<T, K>)[key];
  throw { code: "Internal", message: `unexpected reply ${JSON.stringify(res).slice(0, 80)}` } satisfies ApiError;
}

async function ok(req: object): Promise<void> {
  await invoke("request", { req });
}

export const api = {
  chats: (limit = 300) => call<Chat[], "Chats">({ ListChats: { limit, before: null } }, "Chats"),
  messages: (chat_guid: string, limit: number, before: Timestamp | null) =>
    call<Message[], "Messages">({ ListMessages: { chat_guid, limit, before } }, "Messages"),
  sendText: (chat_guid: string, text: string) => ok({ SendText: { chat_guid, text } }),
  sendAttachment: (chat_guid: string, file: File) =>
    file.arrayBuffer().then((buf) =>
      invoke<void>("send_attachment", new Uint8Array(buf), {
        headers: { "x-chat": encodeURIComponent(chat_guid), "x-name": encodeURIComponent(file.name || "attachment") },
      }),
    ),
  calendars: () => call<Calendar[], "Calendars">("ListCalendars", "Calendars"),
  events: (from: Timestamp, to: Timestamp) => call<CalendarEvent[], "Events">({ ListEvents: { from, to } }, "Events"),
  createEvent: (event: NewEvent) => call<CalendarEvent, "Event">({ CreateEvent: event }, "Event"),
  deleteEvent: (id: string) => ok({ DeleteEvent: { id } }),
  noteFolders: () => call<NoteFolder[], "NoteFolders">("ListNoteFolders", "NoteFolders"),
  notes: (folder_id: string | null) => call<NoteSummary[], "Notes">({ ListNotes: { folder_id } }, "Notes"),
  note: (id: string) => call<Note, "Note">({ GetNote: { id } }, "Note"),
  createNote: (folder_id: string | null, body: string) =>
    call<{ id: string }, "Created">({ CreateNote: { folder_id, body } }, "Created"),
  updateNote: (id: string, body: string) => ok({ UpdateNote: { id, body } }),
  /** Decrypt a password-locked note (read-only). The password goes to your Mac only. */
  unlockNote: (id: string, password: string) => call<Note, "Note">({ UnlockNote: { id, password } }, "Note"),
  /** Move a note to Recently Deleted. */
  deleteNote: (id: string) => ok({ DeleteNote: { id } }),
  reminderLists: () => call<ReminderList[], "ReminderLists">("ListReminderLists", "ReminderLists"),
  reminders: (list_id: string | null, include_completed: boolean) =>
    call<Reminder[], "Reminders">({ ListReminders: { list_id, include_completed } }, "Reminders"),
  createReminder: (reminder: NewReminder) => call<Reminder, "Reminder">({ CreateReminder: reminder }, "Reminder"),
  setReminderCompleted: (id: string, completed: boolean) => ok({ SetReminderCompleted: { id, completed } }),
  deleteReminder: (id: string) => ok({ DeleteReminder: { id } }),
  /** handle → contact name */
  contacts: () =>
    call<{ handle: string; name: string }[], "Contacts">("ListContacts", "Contacts").then((list) => new Map(list.map((c) => [c.handle, c.name]))),
  voiceMemos: () => call<VoiceMemo[], "VoiceMemos">("ListVoiceMemos", "VoiceMemos"),
  albums: () => call<Album[], "Albums">("ListAlbums", "Albums"),
  assets: (album_id: string | null, offset: number, limit: number) =>
    call<Asset[], "Assets">({ ListAssets: { album_id, offset, limit } }, "Assets"),
  findMy: (include_family: boolean) => call<FindMyDevice[], "FindMyDevices">({ ListFindMyDevices: { include_family } }, "FindMyDevices"),
  /** Structure/count facts about the Mac's databases (debugging). */
  diagnose: () => call<string, "Diagnostics">("Diagnose", "Diagnostics"),
};

const IMAGE_TYPES: Record<string, string> = { png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", gif: "image/gif", webp: "image/webp", heic: "image/heic", tiff: "image/tiff", bmp: "image/bmp" };

export const native = {
  bootstrap: () => invoke<Bootstrap>("bootstrap"),
  /** Images or copied files on the system clipboard (fallback when WebView2 doesn't hand them to the paste event). */
  clipboardFiles: async (): Promise<File[]> => {
    const list = await invoke<{ name: string; size: number }[]>("clipboard_files");
    return Promise.all(
      list.map(async (f, index) => {
        const data = await invoke<ArrayBuffer>("clipboard_file", { index });
        const ext = f.name.split(".").pop()?.toLowerCase() ?? "";
        return new File([data], f.name, { type: IMAGE_TYPES[ext] ?? "" });
      }),
    );
  },
  pair: (uri: string, name: string) => invoke<ConfigView>("pair", { uri, name }),
  saveNotifications: (notifications: Notifications) => invoke<ConfigView>("save_notifications", { notifications }),
  saveServices: (services: Services) => invoke<ConfigView>("save_services", { services }),
  saveAutofill: (options: { enabled: boolean; from_server: boolean; notify: boolean; clear_after: number }) =>
    invoke<ConfigView>("save_autofill", { options }),
  autofillHttp: (enable: boolean) => invoke<ConfigView>("autofill_http", { enable }),
  gmailAdd: (address: string, appPassword: string) => invoke<ConfigView>("gmail_add", { address, appPassword }),
  gmailRemove: (address: string) => invoke<ConfigView>("gmail_remove", { address }),
  /** Start BlackBubbles in the tray when you sign in to Windows. */
  getAutostart: () => invoke<boolean>("get_autostart"),
  setAutostart: (enabled: boolean) => invoke<boolean>("set_autostart", { enabled }),
  saveBlob: (kind: BlobKind, id: string, name: string) => invoke<string>("save_blob", { kind, id, name }),
  /** Native "Save As" dialog; resolves to the saved path, or null if cancelled. */
  saveBlobAs: (kind: BlobKind, id: string, name: string) => invoke<string | null>("save_blob_as", { kind, id, name }),
  audioPlay: (id: string, title: string, duration: number) => invoke<void>("audio_play", { id, title, duration }),
  audioToggle: () => invoke<void>("audio_toggle"),
  audioSeek: (secs: number) => invoke<void>("audio_seek", { secs }),
  audioStop: () => invoke<void>("audio_stop"),
  audioState: () => invoke<AudioState>("audio_state"),
};

/** A URL the webview can load a server blob from (served by Rust's `bb://` handler). */
export function blobUrl(kind: BlobKind, id: string): string {
  return convertFileSrc(`blob/${kind}/${id}`, "bb");
}

export function onServerEvent(fn: (event: ServerEvent) => void): Promise<UnlistenFn> {
  return listen<ServerEvent>("server-event", (e) => fn(e.payload));
}

export function onStatus(fn: (status: Status) => void): Promise<UnlistenFn> {
  return listen<Status>("status", (e) => fn(e.payload));
}

/** Save As… with a toast for the outcome. */
export async function saveAs(kind: BlobKind, id: string, name: string, toast: (t: string) => void): Promise<void> {
  try {
    const path = await native.saveBlobAs(kind, id, name);
    if (path) toast(`Saved to ${path}`);
  } catch (err) {
    toast(`Couldn't save: ${errorText(err)}`);
  }
}
