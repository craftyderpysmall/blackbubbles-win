import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useInfiniteQuery, useQuery, useQueryClient } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowLeft, ArrowUp, Bell, BellOff, Download, Maximize2, File as FileIcon, Mail, MessageCircle, Paperclip, Phone, RotateCw, Upload, X } from "lucide-react";
import { api, blobUrl, errorText, native, saveAs, type Attachment, type Chat } from "@/lib/bb";
import { ContextMenu as CM } from "radix-ui";
import { addToThread, keys, type LocalMessage, type ThreadData } from "@/lib/events";
import { NARROW_BELOW, useApp, useWidth } from "@/lib/app";
import { bytes, dayLabel, groupEventText, sameDay, shortTime, time } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useNames, type NameOf } from "@/lib/contacts";
import { Avatar, Badge, Button, Card, Center, EmptyState, IconButton, SearchInput, Spinner, Textarea } from "@/components/ui";
import { Linkified } from "@/components/Linkified";
import { Lightbox } from "@/components/Lightbox";

const PAGE = 60;
const ROW = 64;

/** A real group name, or null. Older servers sent the number (or an internal `chat123…`
 *  id) as the name, which hid contact names. */
export function groupName(chat: Chat): string | null {
  const n = chat.display_name?.trim();
  if (!n || chat.participants.includes(n) || /^chat\d+$/.test(n)) return null;
  return n;
}

export const chatTitle = (chat: Chat, name: NameOf = (h) => h) => groupName(chat) ?? (chat.participants.map(name).join(", ") || "Unknown");

const isSms = (chat: Chat) => chat.service !== "IMessage";

export function MessagesPage() {
  const { connected, boot } = useApp();
  const [ref, width] = useWidth<HTMLDivElement>();
  const [selected, setSelected] = useState<string | null>(null);
  const chats = useQuery({ queryKey: keys.chats, queryFn: () => api.chats(300), enabled: connected, staleTime: Infinity });

  // Developer aid for screenshots.
  const opened = useRef(false);
  useEffect(() => {
    if (boot.dev_open_first_chat && !opened.current && chats.data?.length) {
      opened.current = true;
      setSelected((chats.data.find((c) => c.guid.startsWith("iMessage")) ?? chats.data[0]).guid);
    }
  }, [boot.dev_open_first_chat, chats.data]);

  const chat = chats.data?.find((c) => c.guid === selected) ?? null;
  const [seen, markSeen] = useSeen();
  // Opening a chat here can't mark it read on the Mac (Messages' AppleScript has no read
  // state), so remember it locally: the dot hides until something newer arrives.
  const lastDate = chat?.last_message?.date ?? 0;
  useEffect(() => {
    if (selected) markSeen(selected, lastDate);
  }, [selected, lastDate, markSeen]);
  const narrow = width > 0 && width < NARROW_BELOW;
  const listWidth = Math.min(360, Math.max(280, width * 0.3));

  const list = <ChatList chats={chats.data} seen={seen} loading={chats.isLoading} error={chats.error} selected={selected} onSelect={setSelected} />;
  return (
    <div ref={ref} className="flex h-full gap-4">
      {(!narrow || !chat) && (
        <Card className="flex min-w-0 flex-col overflow-hidden" style={narrow ? { flex: 1 } : { width: listWidth }}>
          {list}
        </Card>
      )}
      {(!narrow || chat) && (
        <Card className="flex min-w-0 flex-1 flex-col overflow-hidden">
          {chat ? (
            <Thread key={chat.guid} chat={chat} onBack={narrow ? () => setSelected(null) : undefined} />
          ) : (
            <EmptyState icon={MessageCircle} title={connected ? "Pick a conversation" : "Not connected"} hint={connected ? "Your iMessage and SMS chats from the Mac." : "Pair with your Mac in Settings."} />
          )}
        </Card>
      )}
    </div>
  );
}

const SEEN_KEY = "bb.seenChats";

/** When each chat was last opened here (guid → ms), kept in localStorage. */
function useSeen(): [Record<string, number>, (guid: string, at: number) => void] {
  const [seen, setSeen] = useState<Record<string, number>>(() => {
    try {
      return JSON.parse(localStorage.getItem(SEEN_KEY) ?? "{}") as Record<string, number>;
    } catch {
      return {};
    }
  });
  const mark = useCallback((guid: string, at: number) => {
    setSeen((prev) => {
      const when = Math.max(at, Date.now());
      if ((prev[guid] ?? 0) >= at) return prev;
      const next = { ...prev, [guid]: when };
      try {
        localStorage.setItem(SEEN_KEY, JSON.stringify(next));
      } catch {
        // Storage unavailable: still works for this session.
      }
      return next;
    });
  }, []);
  return [seen, mark];
}

/** Unread on the Mac, and nothing newer than when it was last opened here. */
const isUnread = (chat: Chat, seen: Record<string, number>) => chat.unread > 0 && (chat.last_message?.date ?? 0) > (seen[chat.guid] ?? 0);

function ChatList({ chats, seen, loading, error, selected, onSelect }: { chats?: Chat[]; seen: Record<string, number>; loading: boolean; error: unknown; selected: string | null; onSelect: (g: string) => void }) {
  const [filter, setFilter] = useState("");
  const name = useNames();
  const scroller = useRef<HTMLDivElement>(null);
  const shown = useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!chats || !q) return chats ?? [];
    return chats.filter((c) => chatTitle(c, name).toLowerCase().includes(q) || c.participants.some((p) => p.toLowerCase().includes(q)) || c.last_message?.text?.toLowerCase().includes(q));
  }, [chats, filter, name]);
  const virtual = useVirtualizer({ count: shown.length, getScrollElement: () => scroller.current, estimateSize: () => ROW, overscan: 8 });

  return (
    <>
      <div className="flex flex-col gap-3 p-4 pb-2">
        <div className="flex items-baseline justify-between">
          <h1 className="text-[15px] font-semibold text-fg">Messages</h1>
          {chats && <span className="text-xs text-subtle">{unreadLabel(chats, seen)}</span>}
        </div>
        <SearchInput value={filter} onChange={setFilter} placeholder="Search" />
      </div>
      <div ref={scroller} className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
        {loading ? (
          <Center><Spinner /></Center>
        ) : error ? (
          <EmptyState icon={MessageCircle} title="Couldn't load chats" hint={errorText(error)} />
        ) : !shown.length ? (
          <EmptyState icon={MessageCircle} title={filter ? "No matches" : "No conversations"} />
        ) : (
          <div style={{ height: virtual.getTotalSize(), position: "relative" }}>
            {virtual.getVirtualItems().map((row) => {
              const chat = shown[row.index];
              return (
                <div key={chat.guid} style={{ position: "absolute", top: 0, left: 0, right: 0, height: row.size, transform: `translateY(${row.start}px)` }}>
                  <ChatRow chat={chat} name={name} unread={chat.guid !== selected && isUnread(chat, seen)} active={chat.guid === selected} onSelect={onSelect} />
                </div>
              );
            })}
          </div>
        )}
      </div>
    </>
  );
}

function unreadLabel(chats: Chat[], seen: Record<string, number>) {
  const n = chats.filter((c) => isUnread(c, seen)).length;
  return n ? `${n} unread` : `${chats.length} chats`;
}

const ChatRow = memo(function ChatRow({ chat, name, unread, active, onSelect }: { chat: Chat; name: NameOf; unread: boolean; active: boolean; onSelect: (g: string) => void }) {
  const last = chat.last_message;
  const event = last && groupEventText(last, name);
  const preview = last ? (event ?? (last.text?.trim() || (last.attachments.length ? "Attachment" : ""))) : "";
  return (
    <button
      onClick={() => onSelect(chat.guid)}
      className={cn("flex h-[60px] w-full items-center gap-3 rounded-control px-2.5 text-left transition-colors duration-150", active ? "bg-border" : "hover:bg-raised")}
    >
      <Avatar name={chatTitle(chat, name)} size={38} />
      <span className="min-w-0 flex-1">
        <span className="flex items-baseline gap-2">
          <span className={cn("min-w-0 flex-1 truncate text-[13px] text-fg", unread ? "font-semibold" : "font-medium")}>{chatTitle(chat, name)}</span>
          {last && <span className={cn("shrink-0 text-[11px]", unread ? "font-medium text-fg" : "text-subtle")}>{shortTime(last.date)}</span>}
        </span>
        <span className="flex items-center gap-1.5">
          {isSms(chat) && <span className="shrink-0 rounded-full border border-border px-1.5 text-[10px] leading-4 text-subtle">SMS</span>}
          <span className={cn("min-w-0 flex-1 truncate text-xs", unread ? "text-fg" : "text-muted")}>{last?.is_from_me && !event ? `You: ${preview}` : preview}</span>
          {unread && <span aria-label="Unread" className="size-2 shrink-0 rounded-full bg-fg" />}
        </span>
      </span>
    </button>
  );
});

type Row =
  | { kind: "day"; key: string; label: string }
  | { kind: "event"; key: string; text: string; date: number }
  | { kind: "msg"; key: string; m: LocalMessage; sender: string | null; tail: boolean };

let localId = 0;

function Thread({ chat, onBack }: { chat: Chat; onBack?: () => void }) {
  const { toast, config, setConfig } = useApp();
  const name = useNames();
  const title = chatTitle(chat, name);
  const people = chat.participants.map(name).join(", ");
  const qc = useQueryClient();
  const scroller = useRef<HTMLDivElement>(null);
  const [draft, setDraft] = useState("");
  const [dragging, setDragging] = useState(false);
  const [viewing, setViewing] = useState<Attachment | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const atBottom = useRef(true);
  const group = chat.participants.length > 1;

  const thread = useInfiniteQuery({
    queryKey: keys.thread(chat.guid),
    queryFn: ({ pageParam }) => api.messages(chat.guid, PAGE, pageParam) as Promise<LocalMessage[]>,
    initialPageParam: null as number | null,
    getNextPageParam: (last) => (last.length < PAGE ? undefined : last[0]?.date),
    staleTime: Infinity,
  });

  const rows = useMemo<Row[]>(() => {
    const messages = thread.data ? thread.data.pages.slice().reverse().flat() : [];
    const out: Row[] = [];
    messages.forEach((m, i) => {
      // Events split runs of bubbles, like a change of sender does.
      const prev = messages[i - 1]?.group_event ? undefined : messages[i - 1];
      const next = messages[i + 1]?.group_event ? undefined : messages[i + 1];
      if (!messages[i - 1] || !sameDay(new Date(messages[i - 1].date), new Date(m.date))) out.push({ kind: "day", key: `d${m.date}-${m.guid}`, label: dayLabel(m.date) });
      const event = groupEventText(m, name);
      if (event) {
        out.push({ kind: "event", key: m.guid, text: event, date: m.date });
        return;
      }
      const sender = group && !m.is_from_me && (!prev || prev.sender !== m.sender || prev.is_from_me) ? m.sender : null;
      const tail = !next || next.is_from_me !== m.is_from_me || next.sender !== m.sender || next.date - m.date > 5 * 60_000;
      out.push({ kind: "msg", key: m.guid, m, sender, tail });
    });
    return out;
  }, [thread.data, group, name]);

  const virtual = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: (i) => (rows[i].kind === "msg" ? 44 : 32),
    getItemKey: (i) => rows[i].key,
    overscan: 10,
  });

  // Open at the newest message; follow new ones while the reader is at the bottom.
  const count = rows.length;
  const firstKey = rows[0]?.key;
  const prevFirst = useRef<string | undefined>(undefined);
  const prevCount = useRef(0);
  useLayoutEffect(() => {
    if (!count) return;
    const prepended = prevFirst.current !== undefined && firstKey !== prevFirst.current && count > prevCount.current;
    if (prepended) {
      const added = rows.findIndex((r) => r.key === prevFirst.current);
      if (added > 0) virtual.scrollToIndex(added, { align: "start" });
    } else if (prevCount.current === 0 || atBottom.current) {
      virtual.scrollToIndex(count - 1, { align: "end" });
      requestAnimationFrame(() => virtual.scrollToIndex(count - 1, { align: "end" }));
    }
    prevFirst.current = firstKey;
    prevCount.current = count;
  }, [count, firstKey]); // eslint-disable-line react-hooks/exhaustive-deps

  const onScroll = () => {
    const el = scroller.current;
    if (el) atBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 120;
  };

  // Pasted images wait above the composer and go out with Send, so a stray paste can't.
  const [staged, setStaged] = useState<File[]>([]);
  const stage = (files: File[]) => setStaged((s) => [...s, ...files]);
  const onPaste = (e: React.ClipboardEvent<HTMLTextAreaElement>) => {
    const files = Array.from(e.clipboardData.files);
    if (files.length) {
      e.preventDefault();
      stage(files);
      return;
    }
    // WebView2 normally puts pasted images and Explorer files in `files` (above). If it
    // didn't, but the clipboard advertises an image or file, read the clipboard natively.
    const types = Array.from(e.clipboardData.types);
    const uri = e.clipboardData.getData("text/uri-list");
    const image = types.some((t) => t.startsWith("image/") || t === "Files");
    if (types.includes("text/plain") && !image && !uri.startsWith("file://")) return;
    e.preventDefault();
    native.clipboardFiles().then(
      (found) => (found.length ? stage(found) : toast("Nothing on the clipboard that can be sent")),
      (err) => toast(`Couldn't paste: ${errorText(err)}`),
    );
  };

  const send = async () => {
    if (staged.length) {
      atBottom.current = true;
      sendFiles(staged);
      setStaged([]);
    }
    const text = draft.trim();
    if (!text) return;
    setDraft("");
    atBottom.current = true;
    const local: LocalMessage = { guid: `local-${++localId}`, chat_guid: chat.guid, sender: null, is_from_me: true, text, date: Date.now(), attachments: [], group_event: null };
    addToThread(qc, local);
    try {
      await api.sendText(chat.guid, text);
    } catch (err) {
      toast(`Not sent: ${errorText(err)}`);
      qc.setQueryData<ThreadData>(keys.thread(chat.guid), (d) => d && { ...d, pages: d.pages.map((p) => p.map((m) => (m.guid === local.guid ? { ...m, failed: true } : m))) });
    }
  };

  const sendFiles = (files: FileList | File[]) => {
    for (const file of Array.from(files)) {
      toast(`Sending ${file.name}…`);
      api.sendAttachment(chat.guid, file).catch((err) => toast(`${file.name} not sent: ${errorText(err)}`));
    }
  };

  const muted = config.notifications.muted_chats.includes(chat.guid);
  const toggleMute = () => {
    const n = config.notifications;
    const muted_chats = muted ? n.muted_chats.filter((g) => g !== chat.guid) : [...n.muted_chats, chat.guid];
    native.saveNotifications({ ...n, muted_chats }).then(setConfig, (e) => toast(errorText(e)));
  };

  const email = chat.my_address?.includes("@");
  return (
    <div
      className="relative flex h-full flex-col"
      onDragOver={(e) => {
        if (e.dataTransfer.types.includes("Files")) {
          e.preventDefault();
          setDragging(true);
        }
      }}
      onDragLeave={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node)) setDragging(false);
      }}
      onDrop={(e) => {
        e.preventDefault();
        setDragging(false);
        if (e.dataTransfer.files.length) sendFiles(e.dataTransfer.files);
      }}
    >
      <div className="flex min-h-14 items-center gap-3 border-b border-border px-3 py-2">
        {onBack && <IconButton icon={ArrowLeft} label="Back" onClick={onBack} />}
        <Avatar name={title} size={34} />
        <div className="min-w-0 flex-1">
          <div className="truncate text-[14px] font-semibold text-fg">{title}</div>
          {/* The numbers behind the names, or the people in a named group. */}
          {(people !== title || people !== chat.participants.join(", ")) && (
            <div className="selectable truncate text-xs text-subtle">{people !== title ? people : chat.participants.join(", ")}</div>
          )}
        </div>
        <div className="hidden items-center gap-1.5 sm:flex">
          <Badge>{chat.service === "IMessage" ? "iMessage" : chat.service === "Sms" ? "SMS" : "RCS"}</Badge>
          {chat.my_address && (
            <Badge
              icon={email ? Mail : Phone}
              dot={email ? "var(--color-warning)" : undefined}
              title={email ? "This chat sends from your email. See Settings → Sending address to use your phone number." : "Replies go out from this address"}
            >
              {chat.my_address}
            </Badge>
          )}
        </div>
        <IconButton icon={muted ? BellOff : Bell} label={muted ? "Unmute notifications" : "Mute notifications"} onClick={toggleMute} className={muted ? "text-fg" : undefined} />
      </div>

      <div ref={scroller} onScroll={onScroll} className="min-h-0 flex-1 overflow-y-auto px-4">
        {thread.isLoading ? (
          <Center><Spinner /></Center>
        ) : thread.error ? (
          <EmptyState icon={MessageCircle} title="Couldn't load messages" hint={errorText(thread.error)} />
        ) : !rows.length ? (
          <EmptyState icon={MessageCircle} title="No messages yet" />
        ) : (
          <>
            <div className="flex h-12 items-center justify-center">
              {thread.hasNextPage ? (
                <Button variant="outline" size="sm" icon={RotateCw} busy={thread.isFetchingNextPage} onClick={() => void thread.fetchNextPage()}>
                  Load earlier
                </Button>
              ) : (
                <span className="text-[11px] text-subtle">Beginning of conversation</span>
              )}
            </div>
            <div style={{ height: virtual.getTotalSize(), position: "relative" }}>
              {virtual.getVirtualItems().map((item) => {
                const row = rows[item.index];
                return (
                  <div key={item.key} data-index={item.index} ref={virtual.measureElement} style={{ position: "absolute", top: 0, left: 0, right: 0, transform: `translateY(${item.start}px)` }}>
                    {row.kind === "day" ? (
                      <div className="py-3 text-center text-[11px] font-medium text-subtle">{row.label}</div>
                    ) : row.kind === "event" ? (
                      <div title={time(row.date)} className="selectable px-6 py-2 text-center text-[11.5px] text-subtle">{row.text}</div>
                    ) : (
                      <Bubble m={row.m} sms={isSms(chat)} sender={row.sender && name(row.sender)} tail={row.tail} onView={setViewing} />
                    )}
                  </div>
                );
              })}
            </div>
            <div className="h-3" />
          </>
        )}
      </div>

      {staged.length > 0 && (
        <div className="flex gap-2 overflow-x-auto border-t border-border px-3 pt-3">
          {staged.map((file, i) => (
            <StagedFile key={`${i}-${file.name}`} file={file} onRemove={() => setStaged((s) => s.filter((_, j) => j !== i))} />
          ))}
        </div>
      )}
      <div className={cn("flex items-end gap-2 p-3", !staged.length && "border-t border-border")}>
        <input ref={fileInput} type="file" multiple hidden onChange={(e) => e.target.files && (sendFiles(e.target.files), (e.target.value = ""))} />
        <IconButton icon={Paperclip} label="Attach files" onClick={() => fileInput.current?.click()} />
        <AutoTextarea value={draft} onChange={setDraft} onSubmit={send} onPaste={onPaste} placeholder={chat.service === "IMessage" ? "iMessage" : "Text message"} />
        <IconButton icon={ArrowUp} label="Send" variant="primary" disabled={!draft.trim() && !staged.length} onClick={send} />
      </div>

      {dragging && (
        <div className="pointer-events-none absolute inset-2 z-10 flex items-center justify-center rounded-card border border-dashed border-border-strong bg-bg/80 animate-fade">
          <div className="flex flex-col items-center gap-2 text-fg">
            <Upload className="size-6" />
            <span className="text-sm font-medium">Drop to send to {title}</span>
          </div>
        </div>
      )}
      {viewing && <Lightbox kind="Attachment" id={viewing.guid} name={viewing.file_name ?? "image.jpg"} onClose={() => setViewing(null)} />}
    </div>
  );
}

/** A pasted file waiting to be sent: a thumbnail for images, else its name. */
function StagedFile({ file, onRemove }: { file: File; onRemove: () => void }) {
  const url = useMemo(() => (file.type.startsWith("image/") ? URL.createObjectURL(file) : null), [file]);
  useEffect(() => () => {
    if (url) URL.revokeObjectURL(url);
  }, [url]);
  return (
    <div className="group relative shrink-0 animate-enter">
      {url ? (
        <img src={url} alt={file.name} className="size-16 rounded-control border border-border object-cover" />
      ) : (
        <div className="flex h-16 max-w-48 items-center gap-2 rounded-control border border-border bg-raised px-3 text-xs text-muted">
          <FileIcon className="size-4 shrink-0" />
          <span className="truncate">{file.name}</span>
        </div>
      )}
      <button
        aria-label={`Remove ${file.name}`}
        onClick={onRemove}
        className="absolute -top-1.5 -right-1.5 flex size-5 items-center justify-center rounded-full border border-border-strong bg-surface text-fg shadow-lg transition-colors hover:bg-raised"
      >
        <X className="size-3" />
      </button>
    </div>
  );
}

function AutoTextarea({ value, onChange, onSubmit, onPaste, placeholder }: { value: string; onChange: (v: string) => void; onSubmit: () => void; onPaste: (e: React.ClipboardEvent<HTMLTextAreaElement>) => void; placeholder: string }) {
  const ref = useRef<HTMLTextAreaElement>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "0px";
    el.style.height = `${Math.min(160, el.scrollHeight)}px`;
  }, [value]);
  return (
    <Textarea
      ref={ref}
      rows={1}
      autoFocus
      value={value}
      placeholder={placeholder}
      onChange={(e) => onChange(e.target.value)}
      onPaste={onPaste}
      onKeyDown={(e) => {
        if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
          e.preventDefault();
          onSubmit();
        }
      }}
      className="max-h-40 min-h-9 flex-1 rounded-[18px] py-[7px] leading-5"
    />
  );
}

const Bubble = memo(function Bubble({ m, sms, sender, tail, onView }: { m: LocalMessage; sms: boolean; sender: string | null; tail: boolean; onView: (a: Attachment) => void }) {
  const mine = m.is_from_me;
  const style = mine ? (sms ? "border border-border-strong text-fg" : "bb-inverted bg-primary text-primary-fg") : "bg-raised text-fg";
  const text = m.text?.replace(/￼/g, "").trim();
  return (
    <div className={cn("flex flex-col", mine ? "items-end" : "items-start", tail ? "pb-2" : "pb-0.5")}>
      {sender && <span className="mb-0.5 px-3 text-[11px] text-subtle">{sender}</span>}
      {m.attachments.map((a) => (
        <AttachmentView key={a.guid} a={a} mine={mine} onView={onView} />
      ))}
      {text && (
        <div title={time(m.date)} className={cn("selectable max-w-[min(78%,560px)] rounded-[18px] px-3.5 py-[7px] text-[13.5px] leading-[1.4] whitespace-pre-wrap break-words", style, m.failed && "opacity-60")}>
          <Linkified text={text} />
        </div>
      )}
      {tail && (
        <span className={cn("mt-0.5 px-2 text-[10.5px]", m.failed ? "text-danger" : "text-subtle")}>
          {m.failed ? "Not delivered" : m.guid.startsWith("local-") ? "Sending…" : time(m.date)}
        </span>
      )}
    </div>
  );
});

/** Right-click menu for an attachment. Replaces the web view's own menu, whose "Save Image
 *  As…" can't save images served by the app. */
function AttachmentMenu({ a, onOpen, children }: { a: Attachment; onOpen?: () => void; children: React.ReactNode }) {
  const { toast } = useApp();
  const item = "flex h-8 cursor-default items-center gap-2 rounded-[8px] px-2.5 text-[13px] text-fg outline-none data-[highlighted]:bg-raised [&_svg]:size-4 [&_svg]:text-muted";
  return (
    <CM.Root>
      <CM.Trigger asChild>{children}</CM.Trigger>
      <CM.Portal>
        <CM.Content className="z-50 min-w-44 rounded-control border border-border bg-surface p-1 shadow-soft animate-pop">
          {onOpen && (
            <CM.Item className={item} onSelect={onOpen}>
              <Maximize2 /> Open
            </CM.Item>
          )}
          <CM.Item className={item} onSelect={() => void saveAs("Attachment", a.guid, a.file_name ?? "attachment", toast)}>
            <Download /> Save As…
          </CM.Item>
        </CM.Content>
      </CM.Portal>
    </CM.Root>
  );
}

function AttachmentView({ a, mine, onView }: { a: Attachment; mine: boolean; onView: (a: Attachment) => void }) {
  const { toast } = useApp();
  const [failed, setFailed] = useState(false);
  const image = a.mime_type?.startsWith("image/") && !failed;
  const save = useCallback(() => void saveAs("Attachment", a.guid, a.file_name ?? "attachment", toast), [a, toast]);
  if (image) {
    return (
      <AttachmentMenu a={a} onOpen={() => onView(a)}>
        <button onClick={() => onView(a)} className="mb-0.5 overflow-hidden rounded-[18px] border border-border bg-raised">
          <img src={blobUrl("Attachment", a.guid)} alt={a.file_name ?? ""} loading="lazy" decoding="async" draggable={false} onError={() => setFailed(true)} className="block max-h-72 max-w-[min(280px,60vw)] min-h-16 min-w-16 object-cover" />
        </button>
      </AttachmentMenu>
    );
  }
  return (
    <AttachmentMenu a={a}>
    <button onClick={save} title="Save As…" className={cn("mb-0.5 flex max-w-72 items-center gap-3 rounded-[18px] border px-3 py-2 text-left", mine ? "border-border-strong" : "border-border bg-raised")}>
      <FileIcon className="size-5 shrink-0 text-muted" />
      <span className="min-w-0">
        <span className="block truncate text-[13px] text-fg">{a.file_name ?? "Attachment"}</span>
        <span className="block text-[11px] text-subtle">{bytes(a.total_bytes)}</span>
      </span>
      <Download className="size-4 shrink-0 text-subtle" />
    </button>
    </AttachmentMenu>
  );
}
