import { useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, CalendarClock, CalendarDays, CheckCircle2, Inbox, ListChecks, Plus, type LucideIcon } from "lucide-react";
import { api, errorText, type Reminder, type ReminderList } from "@/lib/bb";
import { keys } from "@/lib/events";
import { NARROW_BELOW, useApp, useWidth } from "@/lib/app";
import { sameDay } from "@/lib/format";
import { cn } from "@/lib/utils";
import { Button, Card, Center, EmptyState, IconButton, Input, Spinner, Switch } from "@/components/ui";
import { isOverdue, listColor, ReminderRow, stillShown, useReminderLists, useReminders, type LocalReminder } from "@/components/Reminders";

type Smart = "today" | "scheduled" | "all" | "completed";
type View = { smart: Smart } | { list: string };

const SMART: [Smart, LucideIcon, string][] = [
  ["today", CalendarDays, "Today"],
  ["scheduled", CalendarClock, "Scheduled"],
  ["all", Inbox, "All"],
  ["completed", CheckCircle2, "Completed"],
];

const groupDay = new Intl.DateTimeFormat(undefined, { weekday: "long", month: "long", day: "numeric" });

/** Due today, or overdue (iOS's Today list includes overdue items). */
const dueToday = (r: LocalReminder) => r.due !== null && (sameDay(new Date(r.due), new Date()) || isOverdue({ ...r, completed: false }));

export default function RemindersPage() {
  const { connected } = useApp();
  const [ref, width] = useWidth<HTMLDivElement>();
  const [view, setView] = useState<View>({ smart: "today" });
  const [picked, setPicked] = useState(false);
  const [showCompleted, setShowCompleted] = useState(false);
  const lists = useReminderLists();
  const open = useReminders(false);
  const wantsCompleted = ("smart" in view && view.smart === "completed") || showCompleted;
  // Completed history can be years long: only fetch it when something shows it.
  const everything = useReminders(true, wantsCompleted);
  const source = wantsCompleted ? everything : open;
  const all = source.data ?? [];

  const counts = useMemo(() => {
    const o = open.data ?? [];
    return {
      today: o.filter((r) => !r.completed && dueToday(r)).length,
      scheduled: o.filter((r) => !r.completed && r.due !== null).length,
      all: o.filter((r) => !r.completed).length,
      completed: everything.data?.filter((r) => r.completed).length,
      byList: (id: string) => o.filter((r) => !r.completed && r.list_id === id).length,
    };
  }, [open.data, everything.data]);

  const shown = useMemo(() => {
    let items: LocalReminder[] = all;
    if ("list" in view) items = items.filter((r) => r.list_id === view.list && (showCompleted || stillShown(r)));
    else if (view.smart === "today") items = items.filter((r) => stillShown(r) && dueToday(r));
    else if (view.smart === "scheduled") items = items.filter((r) => r.due !== null && (showCompleted || stillShown(r)));
    else if (view.smart === "all") items = items.filter((r) => showCompleted || stillShown(r));
    else items = items.filter((r) => r.completed);
    const due = (r: Reminder) => r.due ?? Number.MAX_SAFE_INTEGER;
    return [...items].sort((a, b) =>
      "smart" in view && view.smart === "completed" ? (b.completed_at ?? 0) - (a.completed_at ?? 0) : Number(a.completed) - Number(b.completed) || due(a) - due(b),
    );
  }, [all, view, showCompleted]);

  const narrow = width > 0 && width < NARROW_BELOW;
  const title = "list" in view ? (lists.data?.find((l) => l.id === view.list)?.title ?? "List") : SMART.find(([id]) => id === view.smart)![2];
  const choose = (v: View) => {
    setView(v);
    setPicked(true);
  };

  const sidebar = (
    <Card className="flex min-w-0 flex-col overflow-hidden" style={narrow ? { flex: 1 } : { width: Math.min(320, Math.max(260, width * 0.3)) }}>
      <div className="p-4 pb-2">
        <h1 className="text-[15px] font-semibold text-fg">Reminders</h1>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-3 pb-3">
        <div className="grid grid-cols-2 gap-2">
          {SMART.map(([id, Icon, name]) => {
            const active = "smart" in view && view.smart === id;
            const count = counts[id];
            return (
              <button
                key={id}
                onClick={() => choose({ smart: id })}
                className={cn(
                  "flex flex-col gap-2 rounded-control border p-3 text-left transition-colors duration-150",
                  active ? "border-border-strong bg-raised" : "border-border hover:bg-raised",
                )}
              >
                <span className="flex items-center justify-between">
                  <Icon className={cn("size-[18px]", active ? "text-fg" : "text-muted")} />
                  <span className="text-[17px] font-semibold text-fg tabular-nums">{count ?? "–"}</span>
                </span>
                <span className="text-xs font-medium text-muted">{name}</span>
              </button>
            );
          })}
        </div>
        <div className="mt-4 mb-1 px-1 text-[11px] font-medium tracking-wide text-subtle uppercase">My lists</div>
        {lists.isLoading ? (
          <div className="py-4"><Center><Spinner /></Center></div>
        ) : (
          lists.data?.map((l) => {
            const active = "list" in view && view.list === l.id;
            return (
              <button
                key={l.id}
                onClick={() => choose({ list: l.id })}
                className={cn("flex h-10 w-full items-center gap-3 rounded-control px-2.5 text-left transition-colors duration-150", active ? "bg-border" : "hover:bg-raised")}
              >
                <span className="flex size-6 items-center justify-center rounded-full" style={{ background: listColor(lists.data, l.id) }}>
                  <ListChecks className="size-3.5 text-bg" strokeWidth={2.5} />
                </span>
                <span className="min-w-0 flex-1 truncate text-[13px] text-fg">{l.title}</span>
                <span className="text-xs text-subtle tabular-nums">{counts.byList(l.id)}</span>
              </button>
            );
          })
        )}
      </div>
    </Card>
  );

  const groups = useMemo(() => groupFor(view, shown, lists.data), [view, shown, lists.data]);
  const main = (
    <Card className="flex min-w-0 flex-1 flex-col overflow-hidden">
      <div className="flex min-h-14 items-center gap-2 border-b border-border px-3 py-2">
        {narrow && <IconButton icon={ArrowLeft} label="Back" onClick={() => setPicked(false)} />}
        <div className="min-w-0 flex-1 pl-1">
          <div className="truncate text-[14px] font-semibold text-fg">{title}</div>
          <div className="text-xs text-subtle">{shown.filter((r) => !r.completed).length} open</div>
        </div>
        {!("smart" in view && (view.smart === "completed" || view.smart === "today")) && (
          <div className="w-44">
            <Switch checked={showCompleted} onChange={setShowCompleted} label="Show completed" />
          </div>
        )}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-2 py-2">
        {source.isLoading ? (
          <Center><Spinner /></Center>
        ) : source.error ? (
          <EmptyState icon={ListChecks} title="Couldn't load reminders" hint={errorText(source.error)} />
        ) : !shown.length ? (
          <EmptyState icon={CheckCircle2} title={"smart" in view && view.smart === "completed" ? "Nothing completed yet" : "All done"} hint={connected ? undefined : "Connect to your Mac in Settings."} />
        ) : (
          groups.map(([label, items]) => (
            <div key={label || "_"} className="mb-2">
              {label && <div className="px-2.5 pt-2 pb-1 text-xs font-semibold text-muted">{label}</div>}
              {items.map((r) => (
                <ReminderRow key={r.id} r={r} lists={lists.data} showList={!("list" in view)} />
              ))}
            </div>
          ))
        )}
      </div>
      {!("smart" in view && view.smart === "completed") && (
        <QuickAdd listId={"list" in view ? view.list : null} defaultToday={"smart" in view && view.smart === "today"} lists={lists.data} />
      )}
    </Card>
  );

  return (
    <div ref={ref} className="flex h-full gap-4">
      {(!narrow || !picked) && sidebar}
      {(!narrow || picked) && main}
    </div>
  );
}

/** Scheduled groups by day, All groups by list; everything else is one flat group. */
function groupFor(view: View, items: Reminder[], lists?: ReminderList[]): [string, Reminder[]][] {
  if ("smart" in view && view.smart === "scheduled") {
    const map = new Map<string, Reminder[]>();
    for (const r of items) {
      const label = isOverdue(r) ? "Overdue" : groupDay.format(r.due!);
      map.set(label, [...(map.get(label) ?? []), r]);
    }
    return [...map];
  }
  if ("smart" in view && view.smart === "all" && lists) {
    return lists.map((l) => [l.title, items.filter((r) => r.list_id === l.id)] as [string, Reminder[]]).filter(([, rs]) => rs.length);
  }
  return [["", items]];
}

const pad = (n: number) => String(n).padStart(2, "0");
const isoDate = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

function QuickAdd({ listId, defaultToday, lists }: { listId: string | null; defaultToday: boolean; lists?: ReminderList[] }) {
  const { toast, connected } = useApp();
  const qc = useQueryClient();
  const [title, setTitle] = useState("");
  const [date, setDate] = useState("");
  const [time, setTime] = useState("");
  const [busy, setBusy] = useState(false);
  const writable = lists?.filter((l) => l.writable) ?? [];

  const add = async () => {
    if (!title.trim()) return;
    const day = date || (defaultToday ? isoDate(new Date()) : "");
    let due: number | null = null;
    if (day) {
      const [y, m, d] = day.split("-").map(Number);
      const [h, min] = time ? time.split(":").map(Number) : [0, 0];
      due = new Date(y, m - 1, d, h, min).getTime();
    }
    setBusy(true);
    try {
      await api.createReminder({ list_id: listId, title: title.trim(), notes: null, due, due_has_time: !!day && !!time, priority: 0 });
      setTitle("");
      setDate("");
      setTime("");
      await qc.invalidateQueries({ queryKey: keys.reminders });
    } catch (err) {
      toast(`Couldn't add the reminder: ${errorText(err)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <form
      className="flex flex-wrap items-center gap-2 border-t border-border p-3"
      onSubmit={(e) => {
        e.preventDefault();
        void add();
      }}
    >
      <Input placeholder="New reminder" value={title} onChange={(e) => setTitle(e.target.value)} className="min-w-40 flex-1" disabled={!connected || !writable.length} />
      {/* WebKit draws empty date/time fields as today's date: dim them until set. */}
      <Input type="date" value={date} onChange={(e) => setDate(e.target.value)} className={cn("w-36", !date && "text-subtle")} aria-label="Due date" title={defaultToday && !date ? "Defaults to today" : "Optional due date"} />
      <Input type="time" value={time} onChange={(e) => setTime(e.target.value)} className={cn("w-28", !time && "text-subtle")} aria-label="Due time" disabled={!date && !defaultToday} title="Optional time (alerts on your iPhone too)" />
      <Button type="submit" variant="primary" icon={Plus} busy={busy} disabled={!title.trim()}>Add</Button>
    </form>
  );
}
