import type { Message, Timestamp } from "./bb";

const sameDay = (a: Date, b: Date) =>
  a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();

const dayDiff = (a: Date, b: Date) => {
  const start = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  return Math.round((start(b) - start(a)) / 86_400_000);
};

// American 12-hour clock ("2:05 PM"), as the owner asked.
const hm = new Intl.DateTimeFormat("en-US", { hour: "numeric", minute: "2-digit", hour12: true });
const weekday = new Intl.DateTimeFormat(undefined, { weekday: "short" });
const monthDay = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" });
const monthDayYear = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", year: "numeric" });
const longDay = new Intl.DateTimeFormat(undefined, { weekday: "long", month: "long", day: "numeric" });
const longStamp = new Intl.DateTimeFormat(undefined, {
  weekday: "short", month: "short", day: "numeric", year: "numeric", hour: "numeric", minute: "2-digit", hour12: true,
});

export const time = (ms: Timestamp) => hm.format(ms);

/** "2:05 PM" today, "Tue" this week, else "Mar 3" (or "Mar 3, 2025"). */
export function shortTime(ms: Timestamp): string {
  const t = new Date(ms);
  const now = new Date();
  const days = dayDiff(t, now);
  if (days === 0) return hm.format(t);
  if (days > 0 && days < 7) return weekday.format(t);
  return t.getFullYear() === now.getFullYear() ? monthDay.format(t) : monthDayYear.format(t);
}

/** Day separators in a thread: "Today", "Yesterday", "Monday, March 3". */
export function dayLabel(ms: Timestamp): string {
  const t = new Date(ms);
  const now = new Date();
  if (sameDay(t, now)) return "Today";
  if (dayDiff(t, now) === 1) return "Yesterday";
  const label = longDay.format(t);
  return t.getFullYear() === now.getFullYear() ? label : `${label}, ${t.getFullYear()}`;
}

export const longTime = (ms: Timestamp) => longStamp.format(ms);

export function ago(ms: Timestamp): string {
  const secs = Math.floor((Date.now() - ms) / 1000);
  if (secs < 60) return "just now";
  if (secs < 3600) return `${Math.floor(secs / 60)} min ago`;
  if (secs < 86400) return `${Math.floor(secs / 3600)} h ago`;
  return `${Math.floor(secs / 86400)} d ago`;
}

export function duration(secs: number): string {
  const s = Math.max(0, Math.round(secs));
  const mm = Math.floor(s / 60) % 60;
  const ss = String(s % 60).padStart(2, "0");
  return s >= 3600 ? `${Math.floor(s / 3600)}:${String(mm).padStart(2, "0")}:${ss}` : `${Math.floor(s / 60)}:${ss}`;
}

export function bytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1_048_576) return `${Math.round(n / 1024)} KB`;
  return `${(n / 1_048_576).toFixed(1)} MB`;
}

export { sameDay };

/** "You left the conversation", "Alex added Anthony", "Sam named the conversation “Trip”". */
export function groupEventText(m: Pick<Message, "group_event" | "is_from_me" | "sender">, name: (handle: string) => string = (h) => h): string | null {
  const e = m.group_event;
  if (!e) return null;
  const actor = m.is_from_me ? "You" : m.sender ? name(m.sender) : "Someone";
  if (e === "Left") return `${actor} left the conversation`;
  if (e === "PhotoChanged") return `${actor} changed the group photo`;
  if (e === "PhotoRemoved") return `${actor} removed the group photo`;
  if ("Added" in e) return e.Added.who ? `${actor} added ${name(e.Added.who)}` : `${actor} added someone`;
  if ("Removed" in e) return e.Removed.who ? `${actor} removed ${name(e.Removed.who)}` : `${actor} removed someone`;
  return e.Renamed.name ? `${actor} named the conversation “${e.Renamed.name}”` : `${actor} removed the conversation name`;
}
