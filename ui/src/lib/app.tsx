import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { onStatus, type Bootstrap, type ConfigView, type Status } from "./bb";

import type { Services } from "./bb";

export type Tab = "messages" | "photos" | "notes" | "calendar" | "reminders" | "voice" | "findmy" | "settings";

/** The service setting behind each tab (Settings is always shown). */
export const TAB_SERVICE: Record<Exclude<Tab, "settings">, keyof Services> = {
  messages: "messages",
  photos: "photos",
  notes: "notes",
  calendar: "calendar",
  reminders: "reminders",
  voice: "voice_memos",
  findmy: "find_my",
};

export const tabShown = (tab: Tab, services: Services) => tab === "settings" || services[TAB_SERVICE[tab]];

export interface Toast {
  id: number;
  text: string;
  leaving?: boolean;
}

interface AppCx {
  boot: Bootstrap;
  status: Status;
  connected: boolean;
  config: ConfigView;
  setConfig: (c: ConfigView) => void;
  toast: (text: string) => void;
  toasts: Toast[];
  tab: Tab;
  setTab: (t: Tab) => void;
}

const Cx = createContext<AppCx | null>(null);

export function useApp(): AppCx {
  const cx = useContext(Cx);
  if (!cx) throw new Error("useApp outside AppProvider");
  return cx;
}

const TOAST_MS = 4500;

function devTab(name: string | null): Tab | null {
  if (!name) return null;
  const n = name.toLowerCase().replace(/[\s_-]/g, "");
  const map: Record<string, Tab> = { messages: "messages", photos: "photos", notes: "notes", calendar: "calendar", reminders: "reminders", voice: "voice", voicememos: "voice", findmy: "findmy", settings: "settings" };
  return map[n] ?? null;
}

export function AppProvider({ boot, children }: { boot: Bootstrap; children: ReactNode }) {
  const [status, setStatus] = useState<Status>(boot.status);
  const [config, setConfig] = useState<ConfigView>(boot.config);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [tab, setTab] = useState<Tab>(devTab(boot.dev_tab) ?? (boot.config.server ? "messages" : "settings"));
  const next = useRef(1);

  useEffect(() => {
    const off = onStatus(setStatus);
    return () => void off.then((f) => f());
  }, []);

  const toast = useCallback((text: string) => {
    const id = next.current++;
    setToasts((t) => [...t.slice(-3), { id, text }]);
    // Exit animation at about half the enter time, then remove.
    setTimeout(() => setToasts((t) => t.map((x) => (x.id === id ? { ...x, leaving: true } : x))), TOAST_MS);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), TOAST_MS + 160);
  }, []);

  useEffect(() => {
    if (boot.startup_error) toast(boot.startup_error);
  }, [boot.startup_error, toast]);

  const value = useMemo<AppCx>(
    () => ({ boot, status, connected: status.state === "Connected", config, setConfig, toast, toasts, tab, setTab }),
    [boot, status, config, toast, toasts, tab],
  );
  return <Cx.Provider value={value}>{children}</Cx.Provider>;
}

/** An element's content width, kept current with a ResizeObserver. Pages use it for
 *  layout decisions CSS can't make alone (single-pane vs split). */
export function useWidth<T extends HTMLElement>(): [React.RefCallback<T>, number] {
  const [w, setW] = useState(0);
  const observer = useRef<ResizeObserver | null>(null);
  const ref = useCallback((el: T | null) => {
    observer.current?.disconnect();
    if (!el) return;
    setW(el.clientWidth);
    observer.current = new ResizeObserver(([entry]) => setW(entry.contentRect.width));
    observer.current.observe(el);
  }, []);
  return [ref, w];
}

/** Breakpoints carried over from the egui client. */
export const RAIL_COMPACT_BELOW = 1040;
export const NARROW_BELOW = 760;
export const CALENDAR_STACK_BELOW = 980;
