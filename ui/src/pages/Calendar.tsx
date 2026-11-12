import { useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Calendar as CalendarIcon, ChevronLeft, ChevronRight, MapPin, Plus, Trash2 } from "lucide-react";
import { api, errorText, type Calendar, type CalendarEvent, type Reminder, type ReminderList } from "@/lib/bb";
import { listColor, ReminderRow, useReminderLists, useReminders } from "@/components/Reminders";
import { keys } from "@/lib/events";
import { CALENDAR_STACK_BELOW, useApp, useWidth } from "@/lib/app";
import { sameDay, time } from "@/lib/format";
import { accent, cn } from "@/lib/utils";
import { Button, Card, Center, Dialog, EmptyState, IconButton, Input, Select, Spinner, Switch } from "@/components/ui";

const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const monthTitle = new Intl.DateTimeFormat(undefined, { month: "long", year: "numeric" });
const agendaTitle = new Intl.DateTimeFormat(undefined, { weekday: "long", month: "long", day: "numeric" });

const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate());
const addDays = (d: Date, n: number) => new Date(d.getFullYear(), d.getMonth(), d.getDate() + n);
/** Monday on or before the 1st of the month. */
const gridStart = (month: Date) => addDays(month, -((month.getDay() + 6) % 7));

function overlaps(e: CalendarEvent, day: Date) {
  const start = day.getTime();
  const end = addDays(day, 1).getTime();
  return e.start < end && (e.end > start || (e.end === e.start && e.start >= start));
}

export default function CalendarPage() {
  const { connected, config } = useApp();
  const [ref, width] = useWidth<HTMLDivElement>();
  const today = startOfDay(new Date());
  const [month, setMonth] = useState(() => new Date(today.getFullYear(), today.getMonth(), 1));
  const [selected, setSelected] = useState(today);
  const [creating, setCreating] = useState(false);

  const from = gridStart(month);
  const to = addDays(from, 42);
  const calendars = useQuery({ queryKey: keys.calendars, queryFn: api.calendars, enabled: connected });
  const events = useQuery({ queryKey: [...keys.events, from.getTime()], queryFn: () => api.events(from.getTime(), to.getTime()), enabled: connected, placeholderData: (prev) => prev });

  const colorOf = useMemo(() => {
    const byId = new Map((calendars.data ?? []).map((c) => [c.id, c]));
    return (e: CalendarEvent) => {
      const c = byId.get(e.calendar_id);
      return c ? accent(c.color) : "var(--color-muted)";
    };
  }, [calendars.data]);

  // Reminders with a due date show on their day, like in the iOS Calendar.
  const remindersOn = config.services.reminders;
  const reminders = useReminders(false, remindersOn);
  const lists = useReminderLists();
  const dated = useMemo(() => (remindersOn ? (reminders.data ?? []).filter((r) => r.due !== null).sort((a, b) => a.due! - b.due!) : []), [reminders.data, remindersOn]);
  const remindersFor = (day: Date) => dated.filter((r) => sameDay(new Date(r.due!), day));

  const sorted = useMemo(() => [...(events.data ?? [])].sort((a, b) => Number(b.all_day) - Number(a.all_day) || a.start - b.start), [events.data]);
  const stacked = width > 0 && width < CALENDAR_STACK_BELOW;
  const go = (delta: number) => setMonth(new Date(month.getFullYear(), month.getMonth() + delta, 1));
  const goToday = () => {
    setSelected(today);
    setMonth(new Date(today.getFullYear(), today.getMonth(), 1));
  };

  const grid = (
    <Card className={cn("flex min-h-0 min-w-0 flex-col overflow-hidden", stacked ? "h-[min(520px,62%)] shrink-0" : "flex-1")}>
      <div className="flex items-center gap-2 px-4 pt-3 pb-2">
        <h1 className="min-w-0 flex-1 truncate text-[15px] font-semibold text-fg">{monthTitle.format(month)}</h1>
        {events.isFetching && <Spinner />}
        <Button variant="outline" size="sm" onClick={goToday}>Today</Button>
        <IconButton icon={ChevronLeft} label="Previous month" onClick={() => go(-1)} />
        <IconButton icon={ChevronRight} label="Next month" onClick={() => go(1)} />
      </div>
      <div className="grid grid-cols-7 px-3 text-[11px] font-medium text-subtle">
        {WEEKDAYS.map((d) => (
          <div key={d} className="px-2 pb-1.5">{width < 560 ? d[0] : d}</div>
        ))}
      </div>
      <div className="grid min-h-0 flex-1 grid-cols-7 grid-rows-6 gap-1 px-3 pb-3">
        {Array.from({ length: 42 }, (_, i) => {
          const day = addDays(from, i);
          return (
            <DayCell
              key={day.getTime()}
              day={day}
              inMonth={day.getMonth() === month.getMonth()}
              today={sameDay(day, today)}
              selected={sameDay(day, selected)}
              events={sorted.filter((e) => overlaps(e, day))}
              reminders={remindersFor(day)}
              lists={lists.data}
              colorOf={colorOf}
              onClick={() => setSelected(day)}
            />
          );
        })}
      </div>
    </Card>
  );

  const dayEvents = sorted.filter((e) => overlaps(e, selected));
  const dayReminders = remindersFor(selected);
  const writable = (calendars.data ?? []).filter((c) => c.writable);
  const writableLists = remindersOn ? (lists.data ?? []).filter((l) => l.writable) : [];
  const total = dayEvents.length + dayReminders.length;
  const agenda = (
    <Card className={cn("flex min-h-0 flex-col overflow-hidden", stacked ? "flex-1" : "w-[320px] shrink-0")}>
      <div className="flex items-center gap-2 px-4 pt-3 pb-2">
        <div className="min-w-0 flex-1">
          <div className="truncate text-[14px] font-semibold text-fg">{agendaTitle.format(selected)}</div>
          <div className="text-xs text-subtle">{summary(dayEvents.length, dayReminders.length)}</div>
        </div>
        <Button variant="primary" size="sm" icon={Plus} disabled={!connected || !(writable.length || writableLists.length)} onClick={() => setCreating(true)}>
          New
        </Button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
        {events.isLoading ? (
          <Center><Spinner /></Center>
        ) : events.error ? (
          <EmptyState icon={CalendarIcon} title="Couldn't load events" hint={errorText(events.error)} />
        ) : !total ? (
          <EmptyState icon={CalendarIcon} title="A free day" hint={connected ? "Add an event or a reminder with New." : "Connect to your Mac in Settings."} />
        ) : (
          <>
            {dayEvents.map((e) => <AgendaRow key={e.id + e.start} event={e} color={colorOf(e)} calendar={calendars.data?.find((c) => c.id === e.calendar_id)} />)}
            {dayReminders.length > 0 && (
              <>
                <div className="px-2.5 pt-3 pb-1 text-[11px] font-medium tracking-wide text-subtle uppercase">Reminders</div>
                {dayReminders.map((r) => <ReminderRow key={r.id} r={r} lists={lists.data} />)}
              </>
            )}
          </>
        )}
      </div>
    </Card>
  );

  return (
    <div ref={ref} className={cn("flex h-full gap-4", stacked && "flex-col")}>
      {grid}
      {agenda}
      {creating && <NewItemDialog day={selected} calendars={writable} lists={writableLists} onClose={() => setCreating(false)} />}
    </div>
  );
}

function summary(events: number, reminders: number) {
  const parts = [events && `${events} event${events > 1 ? "s" : ""}`, reminders && `${reminders} reminder${reminders > 1 ? "s" : ""}`].filter(Boolean);
  return parts.length ? parts.join(" · ") : "Nothing scheduled";
}

/** Events are solid dots; reminders are hollow rings in their list's colour. */
const Ring = ({ color }: { color: string }) => <span className="size-1.5 shrink-0 rounded-full border-[1.5px]" style={{ borderColor: color }} />;

function DayCell({ day, inMonth, today, selected, events, reminders, lists, colorOf, onClick }: { day: Date; inMonth: boolean; today: boolean; selected: boolean; events: CalendarEvent[]; reminders: Reminder[]; lists?: ReminderList[]; colorOf: (e: CalendarEvent) => string; onClick: () => void }) {
  const [ref, w] = useWidth<HTMLButtonElement>();
  const compact = w > 0 && w < 96;
  return (
    <button
      ref={ref}
      onClick={onClick}
      className={cn(
        "relative flex min-h-0 flex-col overflow-hidden rounded-control border p-1.5 text-left transition-colors duration-150",
        selected ? "border-border-strong bg-raised" : "border-transparent hover:bg-raised",
      )}
    >
      <span
        className={cn(
          "flex h-5 min-w-5 items-center justify-center self-start rounded-full px-1 text-[11.5px] font-medium",
          today ? "bg-primary text-primary-fg" : inMonth ? "text-fg" : "text-subtle",
        )}
      >
        {day.getDate()}
      </span>
      {compact ? (
        <span className="absolute bottom-1.5 left-2 flex gap-1">
          {events.slice(0, 4).map((e) => (
            <span key={e.id + e.start} className="size-1.5 rounded-full" style={{ background: colorOf(e) }} />
          ))}
          {reminders.slice(0, Math.max(0, 4 - events.length)).map((r) => (
            <Ring key={r.id} color={listColor(lists, r.list_id)} />
          ))}
        </span>
      ) : (
        <span className="mt-0.5 flex min-h-0 flex-1 flex-col gap-px overflow-hidden">
          {events.slice(0, 3).map((e) => (
            <span key={e.id + e.start} className={cn("flex items-center gap-1.5 truncate text-[11px]", inMonth ? "text-muted" : "text-subtle")}>
              <span className="size-1.5 shrink-0 rounded-full" style={{ background: colorOf(e) }} />
              <span className="truncate">{e.title}</span>
            </span>
          ))}
          {reminders.slice(0, Math.max(0, 3 - events.length)).map((r) => (
            <span key={r.id} className={cn("flex items-center gap-1.5 truncate text-[11px]", inMonth ? "text-muted" : "text-subtle", r.completed && "line-through")}>
              <Ring color={listColor(lists, r.list_id)} />
              <span className="truncate">{r.title}</span>
            </span>
          ))}
          {events.length + reminders.length > 3 && <span className="pl-3 text-[10.5px] text-subtle">+{events.length + reminders.length - 3} more</span>}
        </span>
      )}
    </button>
  );
}

function AgendaRow({ event, color, calendar }: { event: CalendarEvent; color: string; calendar?: Calendar }) {
  const { toast } = useApp();
  const qc = useQueryClient();
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const when = event.all_day ? "All day" : `${time(event.start)} – ${time(event.end)}`;
  const remove = async () => {
    setBusy(true);
    try {
      await api.deleteEvent(event.id);
      toast("Event deleted");
      await qc.invalidateQueries({ queryKey: keys.events });
    } catch (err) {
      toast(`Couldn't delete: ${errorText(err)}`);
      setBusy(false);
    }
  };
  return (
    <div className="group flex items-start gap-3 rounded-control px-2.5 py-2.5 hover:bg-raised" onMouseLeave={() => setConfirm(false)}>
      <span className="mt-1.5 size-2 shrink-0 rounded-full" style={{ background: color }} />
      <div className="selectable min-w-0 flex-1">
        <div className="text-[13px] font-medium text-fg">{event.title || "Untitled"}</div>
        <div className="text-xs text-muted">{when}{calendar ? ` · ${calendar.title}` : ""}</div>
        {event.location && (
          <div className="mt-0.5 flex items-center gap-1 text-xs text-subtle">
            <MapPin className="size-3" />
            <span className="truncate">{event.location}</span>
          </div>
        )}
        {event.notes && <div className="mt-1 line-clamp-3 text-xs text-subtle">{event.notes}</div>}
      </div>
      {calendar?.writable !== false &&
        (confirm ? (
          <Button variant="danger" size="sm" busy={busy} onClick={remove}>Delete</Button>
        ) : (
          <IconButton icon={Trash2} label="Delete event" size="iconSm" className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100" onClick={() => setConfirm(true)} />
        ))}
    </div>
  );
}

const pad = (n: number) => String(n).padStart(2, "0");
const isoDate = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

function NewItemDialog({ day, calendars, lists, onClose }: { day: Date; calendars: Calendar[]; lists: ReminderList[]; onClose: () => void }) {
  const { toast } = useApp();
  const qc = useQueryClient();
  const [kind, setKind] = useState<"event" | "reminder">(calendars.length ? "event" : "reminder");
  const [title, setTitle] = useState("");
  const [location, setLocation] = useState("");
  const [notes, setNotes] = useState("");
  const [date, setDate] = useState(isoDate(day));
  const [allDay, setAllDay] = useState(false);
  const [start, setStart] = useState("09:00");
  const [end, setEnd] = useState("10:00");
  const [calendar, setCalendar] = useState(calendars[0]?.id ?? "");
  const [list, setList] = useState(lists[0]?.id ?? "");
  const [priority, setPriority] = useState("0");
  const [busy, setBusy] = useState(false);

  const [y, m, d] = date.split("-").map(Number);
  const at = (hm: string) => {
    const [h, min] = hm.split(":").map(Number);
    return new Date(y, m - 1, d, h, min).getTime();
  };

  const submit = async () => {
    setBusy(true);
    try {
      if (kind === "event") {
        const range = allDay ? [new Date(y, m - 1, d).getTime(), new Date(y, m - 1, d + 1).getTime()] : [at(start), at(end)];
        if (!allDay && !(range[1] > range[0])) {
          setBusy(false);
          return toast("The end has to be after the start");
        }
        await api.createEvent({ calendar_id: calendar, title: title.trim(), location: location.trim() || null, notes: notes.trim() || null, start: range[0], end: range[1], all_day: allDay });
        await qc.invalidateQueries({ queryKey: keys.events });
        toast("Event created");
      } else {
        await api.createReminder({
          list_id: list || null,
          title: title.trim(),
          notes: notes.trim() || null,
          due: allDay ? new Date(y, m - 1, d).getTime() : at(start),
          due_has_time: !allDay,
          priority: Number(priority),
        });
        await qc.invalidateQueries({ queryKey: keys.reminders });
        toast("Reminder created");
      }
      onClose();
    } catch (err) {
      toast(`Couldn't create it: ${errorText(err)}`);
      setBusy(false);
    }
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()} title={kind === "event" ? "New event" : "New reminder"}>
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        {calendars.length > 0 && lists.length > 0 && (
          <div className="grid grid-cols-2 gap-1 rounded-full border border-border bg-bg p-1">
            {(["event", "reminder"] as const).map((k) => (
              <button
                key={k}
                type="button"
                onClick={() => setKind(k)}
                className={cn("h-7 rounded-full text-xs font-medium transition-colors duration-200", kind === k ? "bg-raised text-fg" : "text-muted hover:text-fg")}
              >
                {k === "event" ? "Event" : "Reminder"}
              </button>
            ))}
          </div>
        )}
        <Input autoFocus placeholder="Title" value={title} onChange={(e) => setTitle(e.target.value)} />
        {kind === "event" && <Input placeholder="Location" value={location} onChange={(e) => setLocation(e.target.value)} />}
        <Input placeholder="Notes" value={notes} onChange={(e) => setNotes(e.target.value)} />
        <div className="flex gap-2">
          <Input type="date" value={date} onChange={(e) => setDate(e.target.value)} className="flex-1" />
          {!allDay && (
            <>
              <Input type="time" value={start} onChange={(e) => setStart(e.target.value)} className="w-28" aria-label={kind === "event" ? "Starts" : "Remind me at"} />
              {kind === "event" && <Input type="time" value={end} onChange={(e) => setEnd(e.target.value)} className="w-28" aria-label="Ends" />}
            </>
          )}
        </div>
        <Switch checked={allDay} onChange={setAllDay} label={kind === "event" ? "All day" : "Date only"} description={kind === "reminder" ? "Off: alerts at the time, on your iPhone too" : undefined} />
        {kind === "event" ? (
          <Select value={calendar} onChange={setCalendar}>
            {calendars.map((c) => (
              <option key={c.id} value={c.id}>{c.title}</option>
            ))}
          </Select>
        ) : (
          <div className="flex gap-2">
            <Select value={list} onChange={setList} className="flex-1">
              {lists.map((l) => (
                <option key={l.id} value={l.id}>{l.title}</option>
              ))}
            </Select>
            <Select value={priority} onChange={setPriority} className="w-36">
              <option value="0">No priority</option>
              <option value="9">Low</option>
              <option value="5">Medium</option>
              <option value="1">High</option>
            </Select>
          </div>
        )}
        <div className="mt-1 flex justify-end gap-2">
          <Button type="button" variant="ghost" onClick={onClose}>Cancel</Button>
          <Button type="submit" variant="primary" busy={busy} disabled={!title.trim() || (kind === "event" ? !calendar : !list)}>Create</Button>
        </div>
      </form>
    </Dialog>
  );
}
