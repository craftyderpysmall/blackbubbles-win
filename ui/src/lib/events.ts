// Server events → React Query cache, so every screen updates without polling.
import type { InfiniteData, QueryClient } from "@tanstack/react-query";
import type { Chat, Message, ServerEvent } from "./bb";

export type LocalMessage = Message & { failed?: boolean };
export type ThreadData = InfiniteData<LocalMessage[], number | null>;

export const keys = {
  chats: ["chats"] as const,
  thread: (guid: string) => ["thread", guid] as const,
  calendars: ["calendars"] as const,
  events: ["events"] as const,
  noteFolders: ["noteFolders"] as const,
  notes: ["notes"] as const,
  note: (id: string) => ["note", id] as const,
  memos: ["memos"] as const,
  albums: ["albums"] as const,
  assets: ["assets"] as const,
  findMy: ["findMy"] as const,
  reminderLists: ["reminderLists"] as const,
  reminders: ["reminders"] as const,
};

/** Add a message to its thread (newest page is `pages[0]`, each page oldest→newest),
 *  replacing the optimistic copy of something we just sent. */
export function addToThread(qc: QueryClient, message: LocalMessage) {
  qc.setQueryData<ThreadData>(keys.thread(message.chat_guid), (data) => {
    if (!data) return data;
    const pages = data.pages.map((p) => p.slice());
    if (pages.some((p) => p.some((m) => m.guid === message.guid))) return data;
    if (message.is_from_me && !message.guid.startsWith("local-")) {
      const local = pages[0].findIndex((m) => m.guid.startsWith("local-") && m.text === message.text);
      if (local >= 0) pages[0].splice(local, 1);
    }
    pages[0].push(message);
    return { ...data, pages };
  });
}

export function applyServerEvent(qc: QueryClient, event: ServerEvent) {
  if (typeof event === "string") {
    const map = {
      CalendarChanged: [keys.events, keys.calendars],
      NotesChanged: [keys.notes, keys.noteFolders, ["note"]],
      VoiceMemosChanged: [keys.memos],
      PhotosChanged: [keys.albums, keys.assets],
      RemindersChanged: [keys.reminders, keys.reminderLists],
    } as const;
    for (const key of map[event] ?? []) void qc.invalidateQueries({ queryKey: key });
    return;
  }
  if ("NewMessage" in event) {
    const message = event.NewMessage;
    addToThread(qc, message);
    let known = false;
    qc.setQueryData<Chat[]>(keys.chats, (chats) => {
      if (!chats) return chats;
      const i = chats.findIndex((c) => c.guid === message.chat_guid);
      if (i < 0) return chats;
      known = true;
      // Count it at once; the Mac's own count follows in an UnreadChanged.
      const unread = chats[i].unread + (message.is_from_me || message.group_event ? 0 : 1);
      const chat = { ...chats[i], last_message: message, unread: message.is_from_me ? 0 : unread };
      return [chat, ...chats.slice(0, i), ...chats.slice(i + 1)];
    });
    if (!known) void qc.invalidateQueries({ queryKey: keys.chats });
  }
  if ("UnreadChanged" in event) {
    const counts = new Map(event.UnreadChanged.map((u) => [u.chat_guid, u.count]));
    qc.setQueryData<Chat[]>(keys.chats, (chats) => chats?.map((c) => {
      const unread = counts.get(c.guid) ?? 0;
      return unread === c.unread ? c : { ...c, unread };
    }));
  }
}
