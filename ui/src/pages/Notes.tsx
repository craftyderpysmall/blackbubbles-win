import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowLeft, Check, Lock, LockOpen, NotebookPen, Plus, Trash2 } from "lucide-react";
import { ContextMenu as CM } from "radix-ui";
import { api, errorText, type NoteSummary } from "@/lib/bb";
import { keys } from "@/lib/events";
import { NARROW_BELOW, useApp, useReturnFocus, useWidth } from "@/lib/app";
import { shortTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import { Badge, Button, Card, Center, Chip, Dialog, EmptyState, IconButton, Input, PaneHeader, SearchInput, Spinner, WorkspaceHeading, menuItem, menuSeparator, menuSurface } from "@/components/ui";

/** Pause after the last keystroke before saving to the Mac. */
const AUTOSAVE_MS = 1200;

/** Equal apart from whitespace (Notes' HTML round trip can change line breaks and spaces). */
const sameText = (a: string, b: string) => a.replace(/\s+/g, " ").trim() === b.replace(/\s+/g, " ").trim();

export default function NotesPage() {
  const { connected, config, tab, toast } = useApp();
  const qc = useQueryClient();
  const [ref, width] = useWidth<HTMLDivElement>();
  const paneFocus = useReturnFocus();
  const [folder, setFolder] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  const currentNote = useRef(open);
  currentNote.current = open;
  const [unlocked, setUnlocked] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [body, setBody] = useState("");
  const [saved, setSaved] = useState("");
  const inFlightSave = useRef<Promise<boolean> | null>(null);
  const [saving, setSaving] = useState(false);
  const [creating, setCreating] = useState(false);
  const [deleting, setDeleting] = useState<NoteSummary | null>(null);
  // Keep deleted IDs hidden until the Mac confirms they have left its list.
  const [pending, setPending] = useState<ReadonlySet<string>>(new Set());

  const folders = useQuery({ queryKey: keys.noteFolders, queryFn: api.noteFolders, enabled: connected });
  const notes = useQuery({ queryKey: [...keys.notes, folder], queryFn: () => api.notes(folder), enabled: connected });
  const note = useQuery({ queryKey: keys.note(open ?? ""), queryFn: () => api.note(open!), enabled: connected && !!open });

  const locked = note.data?.summary.locked ?? false;
  const needsUnlock = locked && !unlocked;
  const dirty = open !== null && body !== saved;
  useEffect(() => {
    if (locked && (!connected || tab !== "notes")) {
      setUnlocked(false);
      if (!dirty) {
        setBody("");
        setSaved("");
      }
    }
  }, [connected, dirty, locked, tab]);
  useEffect(() => setPending(new Set()), [config.server?.fingerprint]);
  // Load the note, and reload it after a change on the Mac unless it has unsaved edits.
  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;
  const savedRef = useRef(saved);
  savedRef.current = saved;
  useEffect(() => {
    if (note.data && !note.data.summary.locked && !dirtyRef.current) {
      // Our own save comes back from Notes' HTML with whitespace that can differ from
      // what was typed. Replacing the text then would move the caret mid-sentence.
      if (sameText(note.data.body, savedRef.current)) return;
      setBody(note.data.body);
      setSaved(note.data.body);
    }
  }, [note.data]);

  const save = useCallback((): Promise<boolean> => {
    if (inFlightSave.current) return inFlightSave.current;
    if (!open || !dirty) return Promise.resolve(true);
    if (pending.has(open)) return Promise.resolve(false);
    if (needsUnlock || !connected) {
      toast(needsUnlock ? "Unlock the note again to save your changes." : "Reconnect to your Mac to save your changes.");
      return Promise.resolve(false);
    }
    setSaveError(null);
    setSaving(true);
    const task = api.updateNote(open, body).then(
      () => {
        setSaved(body);
        void qc.invalidateQueries({ queryKey: keys.notes });
        return true;
      },
      (err) => {
        const message = errorText(err);
        if (locked && message.includes("Unlock this note in BlackBubbles")) setUnlocked(false);
        setSaveError(message);
        return false;
      },
    ).finally(() => {
      inFlightSave.current = null;
      setSaving(false);
    });
    inFlightSave.current = task;
    return task;
  }, [open, body, dirty, locked, needsUnlock, connected, pending, qc, toast]);

  // Notes on the Mac and iPhone save as you type, so save here once typing pauses, and
  // when leaving the note for another service or hiding the window. A failed save waits
  // for the next edit (which clears the error) instead of retrying on its own.
  const canAutoSave = dirty && !saving && !saveError && !needsUnlock && connected && !!open && !pending.has(open);
  useEffect(() => {
    if (!canAutoSave) return;
    const timer = setTimeout(() => void save(), AUTOSAVE_MS);
    return () => clearTimeout(timer);
  }, [canAutoSave, save]);
  const leave = useRef(() => {});
  leave.current = () => {
    if (dirty && !saveError && !needsUnlock && connected) void save();
  };
  useEffect(() => {
    if (tab !== "notes") leave.current();
  }, [tab]);
  useEffect(() => {
    const onHide = () => document.visibilityState === "hidden" && leave.current();
    document.addEventListener("visibilitychange", onHide);
    return () => document.removeEventListener("visibilitychange", onHide);
  }, []);

  const resetEditor = () => {
    setBody("");
    setSaved("");
    setUnlocked(false);
    setSaveError(null);
  };

  const select = async (id: string | null) => {
    if (creating || id === open) return;
    if (id) paneFocus.remember();
    if (!(await save())) return;
    currentNote.current = id;
    resetEditor();
    setOpen(id);
    if (!id) paneFocus.restore();
  };

  const create = async () => {
    setCreating(true);
    try {
      if (!(await save())) return;
      const { id } = await api.createNote(folder, "New Note\n");
      currentNote.current = id;
      resetEditor();
      setOpen(id);
      void Promise.all([
        qc.invalidateQueries({ queryKey: keys.notes }),
        qc.invalidateQueries({ queryKey: keys.noteFolders }),
      ]);
    } catch (err) {
      toast(`Could not create note: ${errorText(err)}`);
    } finally {
      setCreating(false);
    }
  };

  useEffect(() => {
    if (tab !== "notes" || !open) return;
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        void save();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, save, tab]);

  const shown = useMemo(() => {
    const q = filter.trim().toLowerCase();
    const list = (notes.data ?? []).filter((n) => !pending.has(n.id));
    return q ? list.filter((n) => n.title.toLowerCase().includes(q) || n.snippet.toLowerCase().includes(q)) : list;
  }, [notes.data, filter, pending]);

  const showAgain = (id: string) => setPending((items) => {
    const next = new Set(items);
    next.delete(id);
    return next;
  });

  const remove = async () => {
    if (!deleting || inFlightSave.current) return;
    const target = deleting;
    setDeleting(null);
    setPending((items) => new Set(items).add(target.id));
    try {
      await api.deleteNote(target.id);
      toast("Moved to Recently Deleted");
    } catch (err) {
      const code = err && typeof err === "object" && "code" in err ? err.code : undefined;
      // Older servers wrapped the missing-object AppleScript error as Internal.
      if (code !== "NotFound" && !/AppleScript failed:.*\(-1728\)/is.test(errorText(err))) {
        showAgain(target.id);
        toast(`Could not delete: ${errorText(err)}`);
        return;
      }
      toast("Note is already gone from Apple Notes");
    }
    if (currentNote.current === target.id) {
      currentNote.current = null;
      resetEditor();
      setOpen(null);
    }
    await qc.cancelQueries({ queryKey: keys.notes });
    qc.setQueriesData<NoteSummary[]>({ queryKey: keys.notes }, (list) => list?.filter((n) => n.id !== target.id));
    qc.removeQueries({ queryKey: keys.note(target.id), exact: true });
    await Promise.all([
      qc.invalidateQueries({ queryKey: keys.notes }),
      qc.invalidateQueries({ queryKey: keys.noteFolders }),
    ]);
    // Some Macs still return a cached row immediately after a successful delete.
    try {
      if (!(await api.notes(null)).some((n) => n.id === target.id)) showAgain(target.id);
    } catch {
      // Keep the ID hidden for this pairing when the confirmation request fails.
    }
  };

  const narrow = width > 0 && width < NARROW_BELOW;
  const listWidth = Math.min(352, Math.max(280, width * 0.3));
  const title = (needsUnlock ? "" : body).split("\n")[0]?.trim() || note.data?.summary.title || "New Note";

  return (
    <div ref={ref} className="flex h-full flex-col">
      <WorkspaceHeading title="Notes" detail={notes.data ? `${shown.length} ${shown.length === 1 ? "note" : "notes"}${folder ? ` in ${folders.data?.find((f) => f.id === folder)?.name ?? "this folder"}` : ""}` : connected ? undefined : "Pair with your Mac to read and edit Apple Notes."}>
        {(!narrow || !open) && (
          <Button variant={dirty ? "outline" : "primary"} icon={Plus} busy={creating} disabled={!connected} onClick={create}>
            New note
          </Button>
        )}
      </WorkspaceHeading>
      <div className="flex min-h-0 flex-1 gap-4 sm:gap-6">
        <Card data-pane-index tabIndex={-1} className={cn("flex min-w-0 flex-col overflow-hidden", narrow && open && "hidden")} style={narrow ? { flex: 1 } : { width: listWidth }}>
          <div className="flex flex-col gap-3 border-b border-hairline p-3">
            <SearchInput value={filter} onChange={setFilter} placeholder="Search notes" />
            <div role="group" aria-label="Note folders" className="flex gap-1.5 overflow-x-auto [scrollbar-width:none]">
              <Chip active={folder === null} onClick={() => setFolder(null)}>All</Chip>
              {folders.data?.map((f) => (
                <Chip key={f.id} active={folder === f.id} onClick={() => setFolder(f.id)}>
                  <span title={f.account}>{f.name}</span>
                  <span className="text-muted-soft">{f.count}</span>
                </Chip>
              ))}
            </div>
          </div>
          {notes.isLoading ? (
            <Center><Spinner /></Center>
          ) : notes.error ? (
            <EmptyState icon={NotebookPen} title="Couldn't load notes" hint={errorText(notes.error)} />
          ) : !shown.length ? (
            <EmptyState icon={NotebookPen} title={filter ? "No matches" : "No notes"} hint={connected ? undefined : "Connect to your Mac in Settings."} />
          ) : (
            <NoteList notes={shown} open={open} onOpen={(id) => void select(id)} onDelete={setDeleting} />
          )}
        </Card>
        {(!narrow || open) && (
          <Card tone="window" className="flex min-w-0 flex-1 flex-col overflow-hidden">
            {!open ? (
              <EmptyState icon={NotebookPen} title="Select a note" hint="The first line becomes the title." />
            ) : note.isLoading ? (
              <Center><Spinner /></Center>
            ) : !note.data ? (
              <EmptyState icon={NotebookPen} title="Couldn't open this note" hint={note.error ? errorText(note.error) : "Connect to your Mac in Settings."}>
                {narrow && <Button variant="outline" icon={ArrowLeft} onClick={() => void select(null)}>Back to notes</Button>}
              </EmptyState>
            ) : needsUnlock ? (
              <>
                <PaneHeader title={note.data.summary.title} leading={narrow && <IconButton icon={ArrowLeft} label="Back" onClick={() => void select(null)} />}>
                  <Badge icon={Lock}>Locked</Badge>
                </PaneHeader>
                <LockedNote key={open} id={open} onUnlock={(text) => {
                  if (currentNote.current !== open) return;
                  if (!dirty) {
                    setBody(text);
                    setSaved(text);
                  }
                  setUnlocked(true);
                }} />
              </>
            ) : (
              <>
                <PaneHeader title={title} leading={narrow && <IconButton icon={ArrowLeft} label="Back" onClick={() => void select(null)} />}>
                  {locked && <IconButton icon={LockOpen} label="Lock note" disabled={saving} onClick={async () => {
                    if (await save()) resetEditor();
                  }} />}
                  <span role="status"><Badge dot={saving || dirty ? "var(--color-warning)" : "var(--color-success)"}>{saving ? "Saving…" : dirty ? "Edited" : "Saved"}</Badge></span>
                  <Button variant="primary" icon={Check} disabled={!dirty || !connected || pending.has(open)} busy={saving} onClick={() => void save()} title="Ctrl/⌘ + S">
                    Save
                  </Button>
                </PaneHeader>
                {/* Full-width so the scrollbar sits at the pane edge; padding keeps a ~68ch
                    reading column centred. The caret marks focus. */}
                <textarea
                  key={open}
                  autoFocus
                  value={body}
                  onChange={(e) => {
                    setBody(e.target.value);
                    if (saveError) setSaveError(null);
                  }}
                  spellCheck={!locked}
                  autoComplete="off"
                  aria-label="Note body"
                  readOnly={saving || pending.has(open)}
                  className="selectable min-h-0 w-full flex-1 resize-none bg-transparent px-[max(1.5rem,calc((100%-680px)/2))] py-8 text-[17px] leading-[1.7] text-body caret-primary placeholder:text-muted-soft focus-visible:outline-none sm:py-10"
                  placeholder="Start typing…"
                />
              </>
            )}
            {open && dirty && (saveError || needsUnlock) && (
              <div className="flex flex-wrap items-center gap-2 border-t border-hairline bg-card px-4 py-2.5 sm:px-6">
                <p className="min-w-0 flex-1 text-[13px] text-danger" role="alert">{saveError || "Unlock the note again to save your draft."}</p>
                <Button size="sm" onClick={() => {
                  setBody(saved);
                  setSaveError(null);
                }}>Discard edits</Button>
              </div>
            )}
          </Card>
        )}
      </div>
      <Dialog
        open={!!deleting}
        onOpenChange={(o) => !o && setDeleting(null)}
        title={`Delete “${deleting?.title || "New Note"}”?`}
        description="You can recover it from Recently Deleted for 30 days."
      >
        <div className="flex justify-end gap-2">
          <Button variant="outline" onClick={() => setDeleting(null)}>Cancel</Button>
          <Button variant="danger" icon={Trash2} disabled={saving} onClick={() => void remove()}>Move to Recently Deleted</Button>
        </div>
      </Dialog>
    </div>
  );
}

function LockedNote({ id, onUnlock }: { id: string; onUnlock: (text: string) => void }) {
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const unlock = async () => {
    setBusy(true);
    setError(null);
    try {
      const { body } = await api.unlockNote(id, password);
      setPassword("");
      onUnlock(body);
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="flex min-h-0 flex-1 overflow-y-auto p-4 sm:p-6">
      <form
        className="m-auto flex w-full max-w-sm flex-col items-center gap-3 text-center animate-fade"
        onSubmit={(e) => {
          e.preventDefault();
          void unlock();
        }}
      >
        <Lock className="mb-1 size-5 text-muted-soft" aria-hidden />
        <h3 className="display-panel">This note is locked</h3>
        <div className="text-sm text-muted">Enter your custom Notes password to open and edit this note.</div>
        <label htmlFor="notes-password" className="w-full text-left text-sm text-body">Notes password</label>
        <Input id="notes-password" type="password" autoComplete="off" autoFocus placeholder="Notes password" value={password} onChange={(e) => setPassword(e.target.value)} />
        {error && <div role="alert" className="text-xs text-danger">{error}</div>}
        <Button type="submit" variant="primary" icon={LockOpen} busy={busy} disabled={!password}>Unlock</Button>
        <div className="text-xs text-muted-soft">Notes locked with your iPhone or Mac passcode can only be opened on those devices.</div>
      </form>
    </div>
  );
}

function NoteList({ notes, open, onOpen, onDelete }: { notes: NoteSummary[]; open: string | null; onOpen: (id: string) => void; onDelete: (n: NoteSummary) => void }) {
  const scroller = useRef<HTMLDivElement>(null);
  const virtual = useVirtualizer({ count: notes.length, getScrollElement: () => scroller.current, estimateSize: () => 68, overscan: 8 });
  return (
    <div ref={scroller} className="min-h-0 flex-1 overflow-y-auto">
      <div style={{ height: virtual.getTotalSize(), position: "relative" }}>
        {virtual.getVirtualItems().map((row) => {
          const n = notes[row.index];
          return (
            <div key={n.id} className="absolute inset-x-0" style={{ height: row.size, transform: `translateY(${row.start}px)` }}>
              <CM.Root>
              <CM.Trigger asChild>
              <button
                onClick={() => onOpen(n.id)}
                data-pane-item={n.id}
                aria-current={n.id === open || undefined}
                className="list-row flex h-[68px] w-full items-center gap-3 border-b border-hairline px-4 text-left transition-colors duration-150 hover:bg-hover data-[state=open]:bg-hover"
              >
                <span className="min-w-0 flex-1">
                  <span className="flex items-baseline gap-2">
                    <span className="min-w-0 flex-1 truncate text-sm font-medium text-ink">{n.title || "New Note"}</span>
                    <span className="shrink-0 text-xs text-muted-soft tabular-nums">{shortTime(n.modified)}</span>
                  </span>
                  <span className="mt-0.5 flex items-center gap-1.5 truncate text-[13px] text-muted-soft">
                    {n.locked && <Lock aria-label="Locked" className="size-3 shrink-0 text-muted" />}
                    <span className="truncate">{n.locked ? "Locked" : n.snippet || "No additional text"}</span>
                  </span>
                </span>
              </button>
              </CM.Trigger>
              <CM.Portal>
                <CM.Content className={cn(menuSurface, "min-w-40")}>
                  <CM.Item className={menuItem} onSelect={() => onOpen(n.id)}>
                    <NotebookPen /> Open
                  </CM.Item>
                  <CM.Separator className={menuSeparator} />
                  <CM.Item className={cn(menuItem, "text-danger [&_svg]:!text-danger")} onSelect={() => onDelete(n)}>
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
