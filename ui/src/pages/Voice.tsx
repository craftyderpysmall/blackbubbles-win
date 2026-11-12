import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { ContextMenu as CM } from "radix-ui";
import { useQuery } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { AudioLines, Download, Pause, Play, RotateCw, Square } from "lucide-react";
import { api, errorText, native, saveAs, type AudioState, type VoiceMemo } from "@/lib/bb";
import { keys } from "@/lib/events";
import { useApp } from "@/lib/app";
import { bytes, duration, shortTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import { Button, Card, Center, EmptyState, IconButton, SearchInput, Spinner } from "@/components/ui";

/** "Weekly sync.m4a": the memo's title, minus characters Windows doesn't allow in names. */
function fileName(title: string): string {
  const clean = title.replace(/[\\/:*?"<>|]/g, "-").trim();
  return `${clean || "Recording"}.m4a`;
}

/** The Rust player's state, polled while something is loaded. */
function usePlayer(): [AudioState | null, () => void] {
  const [state, setState] = useState<AudioState | null>(null);
  const [tick, setTick] = useState(0);
  const loaded = !!state?.id;
  useEffect(() => {
    let alive = true;
    const poll = () => native.audioState().then((s) => alive && setState(s));
    void poll();
    const timer = loaded ? setInterval(poll, 250) : undefined;
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [tick, loaded]);
  return [state, () => setTick((t) => t + 1)];
}

export default function VoicePage() {
  const { connected, toast } = useApp();
  const [filter, setFilter] = useState("");
  const [loadingId, setLoadingId] = useState<string | null>(null);
  const [player, refresh] = usePlayer();
  const memos = useQuery({ queryKey: keys.memos, queryFn: api.voiceMemos, enabled: connected });
  const shown = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return (memos.data ?? []).filter((m) => !q || m.title.toLowerCase().includes(q));
  }, [memos.data, filter]);

  useEffect(() => {
    if (player?.error) toast(player.error);
  }, [player?.error, toast]);

  const play = async (m: VoiceMemo) => {
    if (player?.id === m.id) return void native.audioToggle().then(refresh);
    setLoadingId(m.id);
    try {
      await native.audioPlay(m.id, m.title, m.duration_secs);
      refresh();
    } catch (err) {
      toast(`Can't play this recording: ${errorText(err)}`);
    } finally {
      setLoadingId(null);
    }
  };

  const active = player?.id ? player : null;
  return (
    <div className="flex h-full flex-col gap-4">
      <Card className="flex min-h-0 flex-1 flex-col overflow-hidden">
        <div className="flex flex-col gap-3 p-4 pb-2">
          <div className="flex items-center gap-2">
            <h1 className="flex-1 text-[15px] font-semibold text-fg">Voice Memos</h1>
            {memos.data && <span className="text-xs text-subtle">{memos.data.length} recordings</span>}
            <IconButton icon={RotateCw} label="Refresh" onClick={() => void memos.refetch()} />
          </div>
          <SearchInput value={filter} onChange={setFilter} placeholder="Search recordings" />
        </div>
        {memos.isLoading ? (
          <Center><Spinner /></Center>
        ) : memos.error ? (
          <EmptyState icon={AudioLines} title="Couldn't load recordings" hint={errorText(memos.error)} />
        ) : !shown.length ? (
          <EmptyState icon={AudioLines} title={filter ? "No matches" : "No recordings"} hint={connected ? undefined : "Connect to your Mac in Settings."} />
        ) : (
          <MemoList memos={shown} activeId={active?.id ?? null} playing={!!active && !active.paused} loadingId={loadingId} onPlay={play} />
        )}
      </Card>
      {active && <PlayerCard state={active} memo={memos.data?.find((m) => m.id === active.id)} refresh={refresh} />}
    </div>
  );
}

function MemoList({ memos, activeId, playing, loadingId, onPlay }: { memos: VoiceMemo[]; activeId: string | null; playing: boolean; loadingId: string | null; onPlay: (m: VoiceMemo) => void }) {
  const scroller = useRef<HTMLDivElement>(null);
  const virtual = useVirtualizer({ count: memos.length, getScrollElement: () => scroller.current, estimateSize: () => 60, overscan: 8 });
  return (
    <div ref={scroller} className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
      <div style={{ height: virtual.getTotalSize(), position: "relative" }}>
        {virtual.getVirtualItems().map((row) => {
          const m = memos[row.index];
          const active = m.id === activeId;
          return (
            <div key={m.id} className="absolute inset-x-0" style={{ height: row.size, transform: `translateY(${row.start}px)` }}>
              <MemoMenu memo={m} playing={active && playing} onPlay={() => onPlay(m)}>
              <button
                onClick={() => onPlay(m)}
                className={cn("flex h-14 w-full items-center gap-3 rounded-control px-3 text-left transition-colors duration-150", active ? "bg-border" : "hover:bg-raised")}
              >
                <span className="flex size-8 shrink-0 items-center justify-center rounded-full border border-border-strong text-fg">
                  {loadingId === m.id ? <Spinner /> : active && playing ? <Pause className="size-3.5" /> : <Play className="size-3.5" />}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-[13px] font-medium text-fg">{m.title || "New Recording"}</span>
                  <span className="block text-xs text-subtle">{shortTime(m.created)} · {bytes(m.bytes)}</span>
                </span>
                <span className="shrink-0 font-mono text-xs text-muted">{duration(m.duration_secs)}</span>
              </button>
              </MemoMenu>
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** Right-click menu for a recording: play/pause and Save Recording As…. */
function MemoMenu({ memo, playing, onPlay, children }: { memo: VoiceMemo; playing: boolean; onPlay: () => void; children: ReactNode }) {
  const { toast } = useApp();
  const item = "flex h-8 cursor-default items-center gap-2 rounded-[8px] px-2.5 text-[13px] text-fg outline-none data-[highlighted]:bg-raised [&_svg]:size-4 [&_svg]:text-muted";
  return (
    <CM.Root>
      <CM.Trigger asChild>{children}</CM.Trigger>
      <CM.Portal>
        <CM.Content className="z-50 min-w-52 rounded-control border border-border bg-surface p-1 shadow-soft animate-pop">
          <CM.Item className={item} onSelect={onPlay}>
            {playing ? <Pause /> : <Play />} {playing ? "Pause" : "Play"}
          </CM.Item>
          <CM.Separator className="my-1 h-px bg-border" />
          <CM.Item className={item} onSelect={() => void saveAs("VoiceMemo", memo.id, fileName(memo.title), toast)}>
            <Download /> Save Recording As…
          </CM.Item>
        </CM.Content>
      </CM.Portal>
    </CM.Root>
  );
}

function PlayerCard({ state, memo, refresh }: { state: AudioState; memo?: VoiceMemo; refresh: () => void }) {
  const { toast } = useApp();
  const [scrub, setScrub] = useState<number | null>(null);
  const [saving, setSaving] = useState(false);
  const total = state.duration || memo?.duration_secs || 0;
  const position = scrub ?? Math.min(state.position, total || state.position);
  const pct = total > 0 ? (position / total) * 100 : 0;
  const playing = !state.paused && !state.ended;

  const save = () => {
    setSaving(true);
    void saveAs("VoiceMemo", state.id!, fileName(state.title), toast).finally(() => setSaving(false));
  };

  return (
    <Card className="shrink-0 p-4 shadow-lift animate-enter">
      <div className="flex items-center gap-3">
        <Button variant="primary" size="icon" className="size-10" icon={playing ? Pause : Play} aria-label={playing ? "Pause" : "Play"} onClick={() => void native.audioToggle().then(refresh)} />
        <div className="min-w-0 flex-1">
          <div className="truncate text-[13.5px] font-medium text-fg">{state.title || "Recording"}</div>
          <div className="mt-2 flex items-center gap-3">
            <span className="w-11 shrink-0 font-mono text-[11px] text-subtle">{duration(position)}</span>
            <input
              type="range"
              min={0}
              max={total || 1}
              step={0.1}
              value={position}
              onChange={(e) => setScrub(Number(e.target.value))}
              onPointerUp={() => {
                if (scrub !== null) void native.audioSeek(scrub).then(() => setTimeout(() => setScrub(null), 200));
              }}
              onKeyUp={() => scrub !== null && void native.audioSeek(scrub).then(() => setScrub(null))}
              className="h-1 min-w-0 flex-1 cursor-pointer appearance-none rounded-full bg-border-strong accent-fg [&::-webkit-slider-thumb]:size-3 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-fg"
              style={{ background: `linear-gradient(to right, var(--color-fg) ${pct}%, var(--color-border-strong) ${pct}%)` }}
            />
            <span className="w-11 shrink-0 text-right font-mono text-[11px] text-subtle">{duration(total)}</span>
          </div>
        </div>
        <IconButton icon={Download} label="Save As…" busy={saving} onClick={save} />
        <IconButton icon={Square} label="Stop" onClick={() => void native.audioStop().then(refresh)} />
      </div>
    </Card>
  );
}
