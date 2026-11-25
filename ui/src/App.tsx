import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { AudioLines, Calendar, Check, ChevronDown, Image, ListChecks, MapPin, MessageCircle, NotebookPen, Settings, type LucideIcon } from "lucide-react";
import { DropdownMenu as Menu } from "radix-ui";
import { onServerEvent, type Status } from "./lib/bb";
import { applyServerEvent } from "./lib/events";
import { tabShown, useApp, useWidth, type Tab } from "./lib/app";
import { cn } from "./lib/utils";
import { Button, Center, Spinner } from "./components/ui";
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

function AppHeader() {
  const { status, setTab, tab, config } = useApp();
  const [headerRef, headerWidth] = useWidth<HTMLElement>();
  const fullNavigation = headerWidth >= 1100;
  const [label, dot] = statusLine(status);
  const services = NAV.filter(([id]) => tabShown(id, config.services));
  const [, CurrentIcon, currentLabel] = NAV.find(([id]) => id === tab)!;

  return (
    <header ref={headerRef} className="relative z-10 shrink-0 border-b border-border bg-nav">
      <div className="mx-auto flex h-16 max-w-[1280px] items-center justify-between gap-3 px-3 sm:px-6 xl:px-8">
        <div className="flex shrink-0 items-center gap-2.5 text-fg" aria-label="BlackBubbles">
          <span className="grid size-9 place-items-center rounded-control border border-border-strong bg-raised text-accent">
            <MessageCircle className="size-[19px]" aria-hidden />
          </span>
          <span className={cn("font-display text-2xl leading-none font-medium tracking-tight", headerWidth < 600 && "hidden")}>BlackBubbles</span>
        </div>
        <nav aria-label="Apple services" className={cn("flex min-w-0 flex-1", fullNavigation && "justify-center")}>
          {fullNavigation ? <div className="flex items-center gap-0.5">
            {services.map(([id, Icon, name]) => (
              <button
                key={id}
                title={id === "settings" ? name : undefined}
                aria-label={name}
                aria-current={tab === id ? "page" : undefined}
                onClick={() => setTab(id)}
                className={cn(
                  "relative flex h-10 items-center gap-1.5 whitespace-nowrap rounded-control px-2.5 text-sm font-medium transition-colors hover:bg-hover hover:text-fg",
                  tab === id ? "bg-selected text-fg after:absolute after:inset-x-2.5 after:bottom-0 after:h-px after:bg-accent" : "text-muted",
                )}
              >
                {id === "settings" ? <Icon className="size-4" aria-hidden /> : name}
              </button>
            ))}
          </div> : <div className="min-w-0">
            <Menu.Root>
              <Menu.Trigger asChild>
                <Button variant="outline" icon={CurrentIcon} aria-label={`Switch service. Current service: ${currentLabel}`} className="max-w-full gap-1.5 px-2.5">
                  <span className="truncate">{currentLabel}</span>
                  <ChevronDown aria-hidden className="size-3.5 text-muted" />
                </Button>
              </Menu.Trigger>
              <Menu.Portal>
                <Menu.Content align="start" sideOffset={8} className="z-50 min-w-48 rounded-card border border-border-strong bg-raised p-1.5 animate-pop">
                  {services.map(([id, Icon, name]) => (
                    <Menu.Item
                      key={id}
                      onSelect={() => setTab(id)}
                      className={cn("flex min-h-9 items-center gap-2.5 rounded-control px-2.5 text-sm text-fg outline-none data-highlighted:bg-hover", id === tab && "bg-selected")}
                    >
                      <Icon className="size-4 text-muted" aria-hidden />
                      <span className="flex-1">{name}</span>
                      {id === tab && <Check className="size-4 text-accent" aria-label="Current service" />}
                    </Menu.Item>
                  ))}
                </Menu.Content>
              </Menu.Portal>
            </Menu.Root>
          </div>}
        </nav>
        <button
          onClick={() => setTab("settings")}
          title={status.state === "Disconnected" ? status.detail : label}
          aria-label={`Mac connection: ${label}`}
          className="flex h-9 min-w-0 max-w-[min(34%,180px)] shrink-0 items-center gap-2 rounded-full border border-border-strong bg-raised px-2.5 text-[13px] font-medium text-body hover:bg-hover"
        >
          <span className={cn("size-2 shrink-0 rounded-full", status.state === "Connecting" && "animate-pulse")} style={{ background: dot }} />
          <span className="truncate" aria-live="polite">{label}</span>
        </button>
      </div>
    </header>
  );
}

function Toasts() {
  const { toasts } = useApp();
  return (
    <div className="pointer-events-none fixed right-4 bottom-4 z-50 flex w-[min(360px,calc(100vw-32px))] flex-col gap-2" role="status">
      {toasts.map((t) => (
        <div
          key={t.id}
          className={cn(
            "selectable pointer-events-auto rounded-card border border-border-strong bg-raised px-4 py-3 text-sm text-fg",
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
    <div className="flex h-full flex-col">
      <AppHeader />
      <main aria-label={NAV.find(([id]) => id === tab)?.[2]} className="relative mx-auto w-full max-w-[1280px] min-h-0 flex-1">
        {[...visited].filter((id) => tabShown(id, config.services)).map((id) => {
          const Page = PAGES[id];
          return (
            <div key={id} className={cn(
              "workspace-view absolute inset-x-3 top-4 bottom-3 sm:inset-x-6 sm:top-8 sm:bottom-6 xl:inset-x-8",
              id === tab ? "animate-enter" : "hidden",
            )}>
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
