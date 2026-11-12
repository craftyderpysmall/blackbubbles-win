import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowLeft, Check, Lock, LockOpen, NotebookPen, Plus, Trash2 } from "lucide-react";
import { ContextMenu as CM } from "radix-ui";
import { api, errorText, type NoteFolder, type NoteSummary } from "@/lib/bb";
import { keys } from "@/lib/events";
import { NARROW_BELOW, useApp, useWidth } from "@/lib/app";
import { shortTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import { Badge, Button, Card, Center, Chip, Dialog, EmptyState, IconButton, Input, SearchInput, Spinner } from "@/components/ui";

export default function NotesPage() {
  const { connected, toast } = useApp();
  const qc = useQueryClient();
  const [ref, width] = useWidth<HTMLDivElement>();
  const [folder, setFolder] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  const [body, setBody] = useState("");
  const [saved, setSaved] = useState("");
  const [saving, setSaving] = useState(false);
  const [creating, setCreating] = useState(false);
  const [deleting, setDeleting] = useState<NoteSummary | null>(null);
  // Notes being deleted: hidden until the Mac has caught up.
  const [pending, setPending] = useState<ReadonlySet<string>>(new Set());

  const folders = useQuery({ queryKey: keys.noteFolders, queryFn: api.noteFolders, enabled: connected });
  const notes = useQuery({ queryKey: [...keys.notes, folder], queryFn: () => api.notes(folder), enabled: connected });
  const note = useQuery({ queryKey: keys.note(open ?? ""), queryFn: () => api.note(open!), enabled: connected && !!open });

  const locked = note.data?.summary.locked ?? false;
  const dirty = open !== null && note.data !== undefined && !locked && body !== saved;
  // Load the note, and reload it after a change on the Mac unless it has unsaved edits.
  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;
  useEffect(() => {
    if (note.data && !dirtyRef.current) {
      setBody(note.data.body);
      setSaved(note.data.body);
    }
  }, [note.data]);

  const save = useCallback(async () => {
    if (!open || body === saved) return;
    const [id, text] = [open, body];
    setSaving(true);
    try {
      await api.updateNote(id, text);
      setSaved(text);
      void qc.invalidateQueries({ queryKey: keys.notes });
    } catch (err) {
      toast(`Not saved: ${errorText(err)}`);
    } finally {
      setSaving(false);
    }
  }, [open, body, saved, qc, toast]);

  const select = (id: string | null) => {
    if (dirty) void save();
    setOpen(id);
    setBody("");
    setSaved("");
  };

  const create = async () => {
    setCreating(true);
    try {
      const { id } = await api.createNote(folder, "New Note\n");
      await qc.invalidateQueries({ queryKey: keys.notes });
      select(id);
    } catch (err) {
      toast(`Couldn't create note: ${errorText(err)}`);
    } finally {
      setCreating(false);
    }
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        void save();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [save]);

  const shown = useMemo(() => {
    const q = filter.trim().toLowerCase();
    const list = (notes.data ?? []).filter((n) => !pending.has(n.id));
    return q ? list.filter((n) => n.title.toLowerCase().includes(q) || n.snippet.toLowerCase().includes(q)) : list;
  }, [notes.data, filter, pending]);

  // Deleting goes through AppleScript and Notes.app, which can take a while; the note leaves
  // every list at once and stays hidden until the Mac has caught up.
  const remove = async () => {
    if (!deleting) return;
    const target = deleting;
    setDeleting(null);
    setPending((p) => new Set(p).add(target.id));
    if (open === target.id) {
      setOpen(null);
      setBody("");
      setSaved("");
    }
    qc.setQueriesData<NoteSummary[]>({ queryKey: keys.notes }, (list) => list?.filter((n) => n.id !== target.id));
    qc.setQueryData<NoteFolder[]>(keys.noteFolders, (list) =>
      list?.map((f) => (f.id === target.folder_id ? { ...f, count: Math.max(0, f.count - 1) } : f)),
    );
    try {
      await api.deleteNote(target.id);
      toast("Moved to Recently Deleted");
    } catch (err) {
      toast(`Couldn't delete: ${errorText(err)}`);
    } finally {
      // Either way, the Mac's database is the truth now.
      await Promise.all([qc.invalidateQueries({ queryKey: keys.notes }), qc.invalidateQueries({ queryKey: keys.noteFolders })]);
      setPending((p) => {
        const next = new Set(p);
        next.delete(target.id);
        return next;
      });
    }
  };

  const narrow = width > 0 && width < NARROW_BELOW;
  const listWidth = Math.min(380, Math.max(280, width * 0.34));
  const title = body.split("\n")[0]?.trim() || note.data?.summary.title || "New Note";

  return (
    <div ref={ref} className="flex h-full gap-4">
      {(!narrow || !open) && (
        <Card className="flex min-w-0 flex-col overflow-hidden" style={narrow ? { flex: 1 } : { width: listWidth }}>
          <div className="flex flex-col gap-3 p-4 pb-2">
            <div className="flex items-center justify-between">
              <h1 className="text-[15px] font-semibold text-fg">Notes</h1>
              <Button variant="outline" size="sm" icon={Plus} busy={creating} disabled={!connected} onClick={create}>
                New note
              </Button>
            </div>
            <div className="flex gap-1.5 overflow-x-auto [scrollbar-width:none]">
              <Chip active={folder === null} onClick={() => setFolder(null)}>All</Chip>
              {folders.data?.map((f) => (
                <Chip key={f.id} active={folder === f.id} onClick={() => setFolder(f.id)}>
                  <span title={f.account}>{f.name}</span>
                  <span className="opacity-60">{f.count}</span>
                </Chip>
              ))}
            </div>
            <SearchInput value={filter} onChange={setFilter} placeholder="Search notes" />
          </div>
          {notes.isLoading ? (
            <Center><Spinner /></Center>
          ) : notes.error ? (
            <EmptyState icon={NotebookPen} title="Couldn't load notes" hint={errorText(notes.error)} />
          ) : !shown.length ? (
            <EmptyState icon={NotebookPen} title={filter ? "No matches" : "No notes"} hint={connected ? undefined : "Connect to your Mac in Settings."} />
          ) : (
            <NoteList notes={shown} open={open} onOpen={select} onDelete={setDeleting} />
          )}
        </Card>
      )}
      {(!narrow || open) && (
        <Card className="flex min-w-0 flex-1 flex-col overflow-hidden">
          {!open ? (
            <EmptyState icon={NotebookPen} title="Select a note" hint="The first line becomes the title." />
          ) : note.isLoading ? (
            <Center><Spinner /></Center>
          ) : note.error ? (
            <EmptyState icon={NotebookPen} title="Couldn't open this note" hint={errorText(note.error)} />
          ) : locked ? (
            <>
              <div className="flex min-h-14 items-center gap-2 border-b border-border px-3 py-2">
                {narrow && <IconButton icon={ArrowLeft} label="Back" onClick={() => select(null)} />}
                <div className="min-w-0 flex-1 truncate pl-1 text-[14px] font-semibold text-fg">{note.data?.summary.title}</div>
                <Badge icon={Lock}>Locked</Badge>
              </div>
              <LockedNote key={open} id={open} />
            </>
          ) : (
            <>
              <div className="flex min-h-14 items-center gap-2 border-b border-border px-3 py-2">
                {narrow && <IconButton icon={ArrowLeft} label="Back" onClick={() => select(null)} />}
                <div className="min-w-0 flex-1 truncate pl-1 text-[14px] font-semibold text-fg">{title}</div>
                <Badge dot={saving || dirty ? "var(--color-warning)" : "var(--color-success)"}>{saving ? "Saving…" : dirty ? "Edited" : "Saved"}</Badge>
                <Button variant="primary" icon={Check} disabled={!dirty} busy={saving} onClick={() => void save()} title="Ctrl/⌘ + S">
                  Save
                </Button>
              </div>
              <textarea
                key={open}
                autoFocus
                value={body}
                onChange={(e) => setBody(e.target.value)}
                spellCheck
                className="selectable min-h-0 flex-1 resize-none bg-transparent px-6 py-5 text-[14px] leading-[1.65] text-fg outline-none placeholder:text-subtle"
                placeholder="Start typing…"
              />
            </>
          )}
        </Card>
      )}
      <Dialog
        open={!!deleting}
        onOpenChange={(o) => !o && setDeleting(null)}
        title={`Delete “${deleting?.title || "New Note"}”?`}
        description="It moves to Recently Deleted on all your devices, where you can recover it for 30 days."
      >
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={() => setDeleting(null)}>Cancel</Button>
          <Button variant="primary" icon={Trash2} onClick={() => void remove()}>Delete</Button>
        </div>
      </Dialog>
    </div>
  );
}

/** A password-locked note: ask for the Notes password, then show it read-only. Nothing is
 *  stored; leaving the note locks it again. */
function LockedNote({ id }: { id: string }) {
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [body, setBody] = useState<string | null>(null);
  const unlock = async () => {
    setBusy(true);
    setError(null);
    try {
      setBody((await api.unlockNote(id, password)).body);
      setPassword("");
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };
  if (body !== null) {
    return (
      <div className="flex min-h-0 flex-1 flex-col">
        <div className="flex items-center gap-2 px-6 pt-4">
          <Badge icon={LockOpen}>Unlocked · read-only</Badge>
          <Button variant="ghost" size="sm" icon={Lock} onClick={() => setBody(null)}>Lock</Button>
        </div>
        <div className="selectable min-h-0 flex-1 overflow-y-auto px-6 py-4 text-[14px] leading-[1.65] whitespace-pre-wrap text-fg">{body}</div>
      </div>
    );
  }
  return (
    <Center className="p-6">
      <form
        className="flex w-full max-w-xs flex-col items-center gap-3 text-center animate-fade"
        onSubmit={(e) => {
          e.preventDefault();
          void unlock();
        }}
      >
        <div className="flex size-11 items-center justify-center rounded-full border border-border bg-raised">
          <Lock className="size-5 text-muted" />
        </div>
        <div className="text-sm font-semibold text-fg">This note is locked</div>
        <div className="text-xs text-subtle">Enter your Notes password. It goes to your Mac to decrypt the note and isn't saved.</div>
        <Input type="password" autoFocus placeholder="Notes password" value={password} onChange={(e) => setPassword(e.target.value)} />
        {error && <div className="text-xs text-danger">{error}</div>}
        <Button type="submit" variant="primary" icon={LockOpen} busy={busy} disabled={!password}>Unlock</Button>
        <div className="text-[11px] text-subtle">Notes locked with your iPhone or Mac passcode can only be opened on those devices.</div>
      </form>
    </Center>
  );
}

const menuItem = "flex h-8 cursor-default items-center gap-2 rounded-[8px] px-2.5 text-[13px] text-fg outline-none data-[highlighted]:bg-raised [&_svg]:size-4 [&_svg]:text-muted";

function NoteList({ notes, open, onOpen, onDelete }: { notes: NoteSummary[]; open: string | null; onOpen: (id: string) => void; onDelete: (n: NoteSummary) => void }) {
  const scroller = useRef<HTMLDivElement>(null);
  const virtual = useVirtualizer({ count: notes.length, getScrollElement: () => scroller.current, estimateSize: () => 66, overscan: 8 });
  return (
    <div ref={scroller} className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
      <div style={{ height: virtual.getTotalSize(), position: "relative" }}>
        {virtual.getVirtualItems().map((row) => {
          const n = notes[row.index];
          return (
            <div key={n.id} className="absolute inset-x-0" style={{ height: row.size, transform: `translateY(${row.start}px)` }}>
              <CM.Root>
              <CM.Trigger asChild>
              <button
                onClick={() => onOpen(n.id)}
                className={cn("flex h-[62px] w-full flex-col justify-center rounded-control px-3 text-left transition-colors duration-150 data-[state=open]:bg-raised", n.id === open ? "bg-border" : "hover:bg-raised")}
              >
                <span className="flex items-baseline gap-2">
                  <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-fg">{n.title || "New Note"}</span>
                  {n.locked && <Lock className="size-3 shrink-0 self-center text-subtle" aria-label="Locked" />}
                  <span className="shrink-0 text-[11px] text-subtle">{shortTime(n.modified)}</span>
                </span>
                <span className="truncate text-xs text-muted">{n.locked ? "Locked" : n.snippet || "No additional text"}</span>
              </button>
              </CM.Trigger>
              <CM.Portal>
                <CM.Content className="z-50 min-w-40 rounded-control border border-border bg-surface p-1 shadow-soft animate-pop">
                  <CM.Item className={menuItem} onSelect={() => onOpen(n.id)}>
                    <NotebookPen /> Open
                  </CM.Item>
                  <CM.Separator className="my-1 h-px bg-border" />
                  <CM.Item className={cn(menuItem, "text-danger [&_svg]:text-danger")} onSelect={() => onDelete(n)}>
                    <Trash2 /> Delete
                  </CM.Item>
                </CM.Content>
              </CM.Portal>
              </CM.Root>
            </div>
          );
        })}
      </div>
    </div>
  );
}
