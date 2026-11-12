import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { AudioLines, Calendar, Image, ListChecks, MapPin, MessageCircle, NotebookPen, Settings, type LucideIcon } from "lucide-react";
import { onServerEvent, type Status } from "./lib/bb";
import { applyServerEvent } from "./lib/events";
import { RAIL_COMPACT_BELOW, tabShown, useApp, useWidth, type Tab } from "./lib/app";
import { cn } from "./lib/utils";
import { Center, Spinner } from "./components/ui";
import { MessagesPage } from "./pages/Messages";

// Messages is the landing tab; the rest load on first visit.
const PhotosPage = lazy(() => import("./pages/Photos"));
const NotesPage = lazy(() => import("./pages/Notes"));
const CalendarPage = lazy(() => import("./pages/Calendar"));
const RemindersPage = lazy(() => import("./pages/Reminders"));
const VoicePage = lazy(() => import("./pages/Voice"));
const FindMyPage = lazy(() => import("./pages/FindMy"));
const SettingsPage = lazy(() => import("./pages/Settings"));

const NAV: [Tab, LucideIcon, string][] = [
  ["messages", MessageCircle, "Messages"],
  ["photos", Image, "Photos"],
  ["notes", NotebookPen, "Notes"],
  ["calendar", Calendar, "Calendar"],
  ["reminders", ListChecks, "Reminders"],
  ["voice", AudioLines, "Voice Memos"],
  ["findmy", MapPin, "Find My"],
  ["settings", Settings, "Settings"],
];

const PAGES: Record<Tab, React.ComponentType> = {
  messages: MessagesPage,
  photos: PhotosPage,
  notes: NotesPage,
  calendar: CalendarPage,
  reminders: RemindersPage,
  voice: VoicePage,
  findmy: FindMyPage,
  settings: SettingsPage,
};

function statusLine(status: Status): [string, string] {
  switch (status.state) {
    case "NotPaired":
      return ["Not paired", "var(--color-subtle)"];
    case "Connecting":
      return ["Connecting…", "var(--color-warning)"];
    case "Connected":
      return [status.detail.server_name, "var(--color-success)"];
    case "Disconnected":
      return ["Offline", "var(--color-danger)"];
  }
}

function Rail({ compact }: { compact: boolean }) {
  const { tab, setTab, status, config } = useApp();
  const [label, dot] = statusLine(status);
  return (
    <nav className={cn("flex shrink-0 flex-col gap-1 py-1 transition-[width] duration-300 ease-decel", compact ? "w-11" : "w-48")}>
      <div className={cn("mb-3 flex h-9 items-center gap-2.5", compact ? "justify-center" : "px-2")}>
        <Logo />
        {!compact && <span className="text-[14px] font-semibold tracking-tight text-fg">BlackBubbles</span>}
      </div>
      {NAV.filter(([id]) => tabShown(id, config.services)).map(([id, Icon, name]) => {
        const active = tab === id;
        return (
          <button
            key={id}
            title={compact ? name : undefined}
            onClick={() => setTab(id)}
            className={cn(
              "flex h-9 items-center gap-3 rounded-full text-[13px] transition-colors duration-200",
              compact ? "w-11 justify-center" : "px-3.5",
              active ? "bg-primary font-medium text-primary-fg" : "text-muted hover:bg-raised hover:text-fg",
            )}
          >
            <Icon className="size-4 shrink-0" />
            {!compact && <span className="truncate">{name}</span>}
          </button>
        );
      })}
      <div className="flex-1" />
      <button
        onClick={() => setTab("settings")}
        title={status.state === "Disconnected" ? status.detail : label}
        className={cn("flex h-9 items-center gap-2.5 rounded-full text-xs text-subtle hover:bg-raised hover:text-muted", compact ? "w-11 justify-center" : "px-3.5")}
      >
        <span className={cn("size-2 shrink-0 rounded-full", status.state === "Connecting" && "animate-pulse")} style={{ background: dot }} />
        {!compact && <span className="truncate">{label}</span>}
      </button>
    </nav>
  );
}

function Logo() {
  return (
    <svg viewBox="0 0 32 32" className="size-7 shrink-0" aria-hidden>
      <rect x="0.5" y="0.5" width="31" height="31" rx="9" fill="#171717" stroke="#404040" />
      <path d="M9 10.5h10a3.5 3.5 0 0 1 3.5 3.5v2a3.5 3.5 0 0 1-3.5 3.5h-5.5L10 22.5v-3.1A3.5 3.5 0 0 1 5.5 16v-2A3.5 3.5 0 0 1 9 10.5Z" fill="#fafafa" />
      <circle cx="23.5" cy="21.5" r="3" fill="#737373" />
    </svg>
  );
}

function Toasts() {
  const { toasts } = useApp();
  return (
    <div className="pointer-events-none fixed right-4 bottom-4 z-50 flex w-[min(360px,calc(100vw-32px))] flex-col gap-2">
      {toasts.map((t) => (
        <div
          key={t.id}
          className={cn(
            "selectable pointer-events-auto rounded-card border border-border bg-surface px-4 py-3 text-[13px] text-fg shadow-soft",
            t.leaving ? "opacity-0 translate-y-2 scale-[0.96] transition-all duration-150 ease-accel" : "animate-toast",
          )}
        >
          {t.text}
        </div>
      ))}
    </div>
  );
}

export function App() {
  const { tab, setTab, status, config } = useApp();
  const qc = useQueryClient();
  const [rootRef, width] = useWidth<HTMLDivElement>();
  const [visited, setVisited] = useState<Set<Tab>>(() => new Set([tab]));
  const wasConnected = useRef(status.state === "Connected");

  useEffect(() => setVisited((v) => (v.has(tab) ? v : new Set(v).add(tab))), [tab]);

  // A service switched off in Settings takes its tab with it.
  useEffect(() => {
    if (!tabShown(tab, config.services)) setTab(NAV.find(([id]) => tabShown(id, config.services))?.[0] ?? "settings");
  }, [tab, config.services, setTab]);

  useEffect(() => {
    const off = onServerEvent((event) => applyServerEvent(qc, event));
    return () => void off.then((f) => f());
  }, [qc]);

  // Refetch everything after a reconnect: we may have missed events while offline.
  useEffect(() => {
    const connected = status.state === "Connected";
    if (connected && !wasConnected.current) void qc.invalidateQueries();
    wasConnected.current = connected;
  }, [status, qc]);

  return (
    <div ref={rootRef} className="flex h-full gap-4 p-4">
      <Rail compact={width > 0 && width < RAIL_COMPACT_BELOW} />
      <main className="relative min-w-0 flex-1">
        {[...visited].filter((id) => tabShown(id, config.services)).map((id) => {
          const Page = PAGES[id];
          return (
            <div key={id} className={cn("absolute inset-0", id === tab ? "animate-enter" : "hidden")}>
              <Suspense fallback={<Center><Spinner /></Center>}>
                <Page />
              </Suspense>
            </div>
          );
        })}
      </main>
      <Toasts />
    </div>
  );
}
