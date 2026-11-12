// Reminders pieces shared by the Reminders tab and the Calendar.
import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, Trash2 } from "lucide-react";
import { api, errorText, type Reminder, type ReminderList } from "@/lib/bb";

/** A reminder just ticked here: it stays visible (ticked) until the list refreshes. */
export type LocalReminder = Reminder & { justDone?: boolean };
/** Open, or completed so recently that it should still show its tick. */
export const stillShown = (r: LocalReminder) => !r.completed || !!r.justDone;
import { keys } from "@/lib/events";
import { useApp } from "@/lib/app";
import { sameDay, time } from "@/lib/format";
import { accent, cn } from "@/lib/utils";
import { Button, IconButton } from "./ui";

export function useReminderLists() {
  const { connected, config } = useApp();
  return useQuery({ queryKey: keys.reminderLists, queryFn: api.reminderLists, enabled: connected && config.services.reminders });
}

/** Incomplete reminders across all lists (plus completed ones when asked). */
export function useReminders(includeCompleted = false, enabled = true) {
  const { connected, config } = useApp();
  return useQuery({
    queryKey: [...keys.reminders, includeCompleted ? "all" : "open"],
    queryFn: () => api.reminders(null, includeCompleted),
    enabled: enabled && connected && config.services.reminders,
  });
}

export const listColor = (lists: ReminderList[] | undefined, id: string) => {
  const list = lists?.find((l) => l.id === id);
  return list ? accent(list.color) : "var(--color-muted)";
};

const DAY = 86_400_000;
const dayFmt = new Intl.DateTimeFormat(undefined, { weekday: "short", month: "short", day: "numeric" });

/** When a reminder is due counts as "overdue": past its time, or past its day. */
export function isOverdue(r: Reminder, now = Date.now()) {
  if (r.completed || r.due === null) return false;
  return r.due_has_time ? r.due < now : r.due + DAY <= now;
}

export function dueLabel(r: Reminder): string | null {
  if (r.due === null) return null;
  const d = new Date(r.due);
  const today = new Date();
  const tomorrow = new Date(today.getFullYear(), today.getMonth(), today.getDate() + 1);
  const yesterday = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 1);
  const day = sameDay(d, today) ? "Today" : sameDay(d, tomorrow) ? "Tomorrow" : sameDay(d, yesterday) ? "Yesterday" : dayFmt.format(d);
  return r.due_has_time ? `${day} ${time(r.due)}` : day;
}

/** EventKit priority: 1–4 high, 5 medium, 6–9 low, 0 none. */
export const priorityMark = (p: number) => (p === 0 ? "" : p < 5 ? "!!!" : p === 5 ? "!!" : "!");

export function useReminderActions() {
  const { toast } = useApp();
  const qc = useQueryClient();
  const refresh = () => qc.invalidateQueries({ queryKey: keys.reminders });
  return {
    toggle: async (r: Reminder) => {
      // Show the tick at once; the list refreshes after the Mac confirms.
      qc.setQueriesData<LocalReminder[]>({ queryKey: keys.reminders }, (list) =>
        list?.map((x) => (x.id === r.id ? { ...x, completed: !r.completed, justDone: !r.completed } : x)),
      );
      try {
        await api.setReminderCompleted(r.id, !r.completed);
      } catch (err) {
        toast(`Couldn't update the reminder: ${errorText(err)}`);
      }
      setTimeout(() => void refresh(), r.completed ? 0 : 900);
    },
    remove: async (r: Reminder) => {
      try {
        await api.deleteReminder(r.id);
        toast("Reminder deleted");
      } catch (err) {
        toast(`Couldn't delete: ${errorText(err)}`);
      }
      void refresh();
    },
  };
}

export function ReminderCheck({ done, onClick }: { done: boolean; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      aria-label={done ? "Mark as not done" : "Mark as done"}
      className={cn(
        "mt-px flex size-[18px] shrink-0 items-center justify-center rounded-full border transition-colors duration-200",
        done ? "border-fg bg-fg text-primary-fg" : "border-border-strong hover:border-muted",
      )}
    >
      {done && <Check className="size-3 animate-pop" strokeWidth={3} />}
    </button>
  );
}

export function ReminderRow({ r, lists, showList = true }: { r: Reminder; lists?: ReminderList[]; showList?: boolean }) {
  const { toggle, remove } = useReminderActions();
  const [confirm, setConfirm] = useState(false);
  const list = lists?.find((l) => l.id === r.list_id);
  const due = dueLabel(r);
  const overdue = isOverdue(r);
  return (
    <div className="group flex items-start gap-3 rounded-control px-2.5 py-2 hover:bg-raised" onMouseLeave={() => setConfirm(false)}>
      <ReminderCheck done={r.completed} onClick={() => void toggle(r)} />
      <div className={cn("selectable min-w-0 flex-1 transition-opacity duration-300", r.completed && "opacity-50")}>
        <div className={cn("text-[13px] text-fg", r.completed && "line-through decoration-subtle")}>
          {priorityMark(r.priority) && <span className="mr-1 font-semibold text-muted">{priorityMark(r.priority)}</span>}
          {r.title || "New Reminder"}
        </div>
        {r.notes && <div className="line-clamp-2 text-xs text-subtle">{r.notes}</div>}
        {(due || (showList && list)) && (
          <div className="mt-0.5 flex items-center gap-2 text-xs">
            {due && <span className={overdue ? "text-danger" : "text-muted"}>{due}</span>}
            {showList && list && (
              <span className="flex items-center gap-1.5 text-subtle">
                <span className="size-1.5 rounded-full" style={{ background: listColor(lists, r.list_id) }} />
                {list.title}
              </span>
            )}
          </div>
        )}
      </div>
      {list?.writable !== false &&
        (confirm ? (
          <Button variant="danger" size="sm" onClick={() => void remove(r)}>Delete</Button>
        ) : (
          <IconButton icon={Trash2} label="Delete reminder" size="iconSm" className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100" onClick={() => setConfirm(true)} />
        ))}
    </div>
  );
}
