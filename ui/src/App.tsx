import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { AudioLines, Calendar, Check, ChevronDown, Image, ListChecks, MapPin, MessageCircle, NotebookPen, Settings, type LucideIcon } from "lucide-react";
import { DropdownMenu as Menu } from "radix-ui";
import { onServerEvent, type Status } from "./lib/bb";
import { applyServerEvent } from "./lib/events";
import { tabShown, useApp, useWidth, type Tab } from "./lib/app";
import { cn } from "./lib/utils";
import { Button, Center, IconButton, menuItem, menuSurface, Spinner, StatusDot } from "./components/ui";
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
      return ["Not paired", "var(--color-muted-soft)"];
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
  const fullNavigation = headerWidth >= 1040;
  const [label, dot] = statusLine(status);
  const services = NAV.filter(([id]) => tabShown(id, config.services));
  const [, CurrentIcon, currentLabel] = NAV.find(([id]) => id === tab)!;

  return (
    <header ref={headerRef} className="relative z-10 shrink-0 border-b border-hairline bg-canvas">
      <div className="mx-auto flex h-16 max-w-[1200px] items-center justify-between gap-3 px-4 sm:px-6 xl:px-8">
        <div className="flex shrink-0 items-center gap-2 text-ink" aria-label="BlackBubbles">
          <MessageCircle className="size-5 text-primary" aria-hidden strokeWidth={2.25} />
          <span className={cn("font-display text-[26px] font-medium leading-none tracking-[-0.02em]", headerWidth < 600 && "sr-only")}>BlackBubbles</span>
        </div>
        <nav aria-label="Apple services" className={cn("flex min-w-0 flex-1", fullNavigation && "justify-center")}>
          {fullNavigation ? <div className="flex items-center gap-1">
            {services.filter(([id]) => id !== "settings").map(([id, , name]) => (
              <button
                key={id}
                aria-current={tab === id ? "page" : undefined}
                onClick={() => setTab(id)}
                className={cn(
                  "nav-label h-9 whitespace-nowrap rounded-control px-3.5 text-sm transition-colors duration-150",
                  tab === id ? "bg-elevated text-ink" : "text-muted hover:text-ink",
                )}
              >
                {name}
              </button>
            ))}
          </div> : <div className="min-w-0">
            <Menu.Root>
              <Menu.Trigger asChild>
                <Button variant="outline" icon={CurrentIcon} aria-label={`Switch service. Current service: ${currentLabel}`} className="max-w-full gap-1.5 px-3">
                  <span className="nav-label truncate">{currentLabel}</span>
                  <ChevronDown aria-hidden className="size-3.5 text-muted" />
                </Button>
              </Menu.Trigger>
              <Menu.Portal>
                <Menu.Content align="start" sideOffset={8} className={cn(menuSurface, "min-w-52")}>
                  {services.map(([id, Icon, name]) => (
                    <Menu.Item
                      key={id}
                      onSelect={() => setTab(id)}
                      className={cn(menuItem, "h-9", id === tab && "bg-hover")}
                    >
                      <Icon aria-hidden />
                      <span className="flex-1">{name}</span>
                      {id === tab && <Check className="!text-ink" aria-label="Current service" />}
                    </Menu.Item>
                  ))}
                </Menu.Content>
              </Menu.Portal>
            </Menu.Root>
          </div>}
        </nav>
        <div className="flex min-w-0 shrink-0 items-center gap-2">
          <button
            onClick={() => setTab("settings")}
            title={status.state === "Disconnected" ? status.detail : label}
            aria-label={`Mac connection: ${label}`}
            className="flex h-8 min-w-0 max-w-[min(34vw,200px)] items-center gap-2 rounded-full border border-hairline bg-elevated px-3 text-xs text-body transition-colors hover:border-hairline-strong hover:text-ink"
          >
            <StatusDot color={dot} pulse={status.state === "Connecting"} />
            <span className="truncate" aria-live="polite">{label}</span>
          </button>
          {fullNavigation && tabShown("settings", config.services) && (
            <IconButton
              icon={Settings}
              label="Settings"
              aria-current={tab === "settings" ? "page" : undefined}
              onClick={() => setTab("settings")}
              className={cn(tab === "settings" && "bg-elevated text-ink")}
            />
          )}
        </div>
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
            "selectable pointer-events-auto rounded-card border border-hairline-strong bg-elevated px-4 py-3 text-sm text-ink",
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

  // Ctrl+Tab / Ctrl+Shift+Tab cycle through the shown services in nav order, wrapping.
  // Capture phase, so a focused field or list can't swallow it first.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Tab" || !e.ctrlKey || e.altKey || e.metaKey) return;
      e.preventDefault();
      e.stopPropagation();
      const shown = NAV.map(([id]) => id).filter((id) => tabShown(id, config.services));
      const i = Math.max(0, shown.indexOf(tab));
      setTab(shown[(i + (e.shiftKey ? -1 : 1) + shown.length) % shown.length]);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [tab, config.services, setTab]);

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
      <main aria-label={NAV.find(([id]) => id === tab)?.[2]} className="relative mx-auto w-full max-w-[1200px] min-h-0 flex-1">
        {[...visited].filter((id) => tabShown(id, config.services)).map((id) => {
          const Page = PAGES[id];
          return (
            <div key={id} className={cn(
              "workspace-view absolute inset-x-4 top-5 bottom-4 sm:inset-x-6 sm:top-8 sm:bottom-6 xl:inset-x-8",
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
