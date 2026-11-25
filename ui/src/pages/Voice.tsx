import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { ContextMenu as CM } from "radix-ui";
import { useQuery } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { AudioLines, Download, Pause, Play, RotateCw, Square } from "lucide-react";
import { api, errorText, native, saveAs, type AudioState, type VoiceMemo } from "@/lib/bb";
import { keys } from "@/lib/events";
import { useApp } from "@/lib/app";
import { bytes, duration, shortTime } from "@/lib/format";
import { Badge, Button, Card, Center, EmptyState, IconButton, SearchInput, Spinner, WorkspaceHeading, menuItem, menuSeparator, menuSurface } from "@/components/ui";
import { cn } from "@/lib/utils";

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
    <div className="flex h-full flex-col">
      <WorkspaceHeading title="Voice Memos" detail={memos.data ? `${memos.data.length} ${memos.data.length === 1 ? "recording" : "recordings"}` : connected ? undefined : "Pair with your Mac to play your recordings."}>
        <Button variant="secondary" icon={RotateCw} onClick={() => void memos.refetch()}>Refresh</Button>
      </WorkspaceHeading>
      <div className="flex min-h-0 flex-1 flex-col gap-4 lg:flex-row lg:gap-6">
        <Card className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
          <div className="border-b border-hairline p-3">
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
        {active ? (
          <PlayerCard state={active} memo={memos.data?.find((m) => m.id === active.id)} refresh={refresh} />
        ) : (
          <Card tone="window" className="hidden w-[40%] min-w-0 overflow-hidden lg:block">
            <EmptyState icon={AudioLines} title="Nothing playing" hint="Choose a recording to listen." />
          </Card>
        )}
      </div>
    </div>
  );
}

function MemoList({ memos, activeId, playing, loadingId, onPlay }: { memos: VoiceMemo[]; activeId: string | null; playing: boolean; loadingId: string | null; onPlay: (m: VoiceMemo) => void }) {
  const scroller = useRef<HTMLDivElement>(null);
  const virtual = useVirtualizer({ count: memos.length, getScrollElement: () => scroller.current, estimateSize: () => 64, overscan: 8 });
  return (
    <div ref={scroller} className="min-h-0 flex-1 overflow-y-auto">
      <div style={{ height: virtual.getTotalSize(), position: "relative" }}>
        {virtual.getVirtualItems().map((row) => {
          const m = memos[row.index];
          const active = m.id === activeId;
          return (
            <div key={m.id} className="absolute inset-x-0" style={{ height: row.size, transform: `translateY(${row.start}px)` }}>
              <MemoMenu memo={m} playing={active && playing} onPlay={() => onPlay(m)}>
              <button
                onClick={() => onPlay(m)}
                aria-current={active || undefined}
                className="list-row flex h-16 w-full items-center gap-3 border-b border-hairline px-4 text-left transition-colors duration-150 hover:bg-hover"
              >
                <span className={cn("flex size-8 shrink-0 items-center justify-center rounded-full border", active ? "border-primary bg-primary text-on-primary" : "border-hairline-strong text-ink")}>
                  {loadingId === m.id ? <Spinner /> : active && playing ? <Pause className="size-3.5" /> : <Play className="size-3.5" />}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm font-medium text-ink">{m.title || "New Recording"}</span>
                  <span className="block text-[13px] text-muted-soft">{shortTime(m.created)} · {bytes(m.bytes)}</span>
                </span>
                <span className="shrink-0 font-mono text-xs text-muted tabular-nums">{duration(m.duration_secs)}</span>
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
  const item = menuItem;
  return (
    <CM.Root>
      <CM.Trigger asChild>{children}</CM.Trigger>
      <CM.Portal>
        <CM.Content className={cn(menuSurface, "min-w-52")}>
          <CM.Item className={item} onSelect={onPlay}>
            {playing ? <Pause /> : <Play />} {playing ? "Pause" : "Play"}
          </CM.Item>
          <CM.Separator className={menuSeparator} />
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
    <Card tone="window" className="shrink-0 overflow-y-auto p-5 animate-enter lg:flex lg:w-[40%] lg:flex-col lg:justify-end lg:p-8">
      <Badge dot={playing ? "var(--color-success)" : "var(--color-muted-soft)"} className="mb-4 hidden lg:inline-flex lg:self-start">{state.ended ? "Finished" : playing ? "Playing" : "Paused"}</Badge>
      <h2 className="display-panel line-clamp-2 break-words text-[2rem] lg:text-[clamp(2.25rem,4vw,3.5rem)]" title={state.title || "Recording"}>{state.title || "Recording"}</h2>
      {memo && <p className="mt-2 text-sm text-muted-soft lg:mt-3">{shortTime(memo.created)} · {bytes(memo.bytes)}</p>}
      <div className="mt-4 flex min-w-0 items-center gap-3 lg:mt-8">
        <span className="w-11 shrink-0 font-mono text-xs text-muted tabular-nums">{duration(position)}</span>
        <div className="relative flex h-6 min-w-0 flex-1 items-center">
          <div aria-hidden className="pointer-events-none absolute inset-x-0 h-0.5 overflow-hidden rounded-full bg-hairline-strong">
            <div className="h-full bg-ink" style={{ width: `${Math.min(100, Math.max(0, pct))}%` }} />
          </div>
          <input
            type="range"
            aria-label="Playback position"
            min={0}
            max={total || 1}
            step={0.1}
            value={position}
            onChange={(e) => setScrub(Number(e.target.value))}
            onPointerUp={() => {
              if (scrub !== null) void native.audioSeek(scrub).then(() => setTimeout(() => setScrub(null), 200));
            }}
            onKeyUp={() => scrub !== null && void native.audioSeek(scrub).then(() => setScrub(null))}
            className="relative h-6 w-full cursor-pointer appearance-none bg-transparent [&::-webkit-slider-thumb]:size-3 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-ink [&::-webkit-slider-thumb]:shadow-[0_0_0_4px_var(--color-deep)]"
          />
        </div>
        <span className="w-11 shrink-0 text-right font-mono text-xs text-muted tabular-nums">{duration(total)}</span>
      </div>
      <div className="mt-3 flex items-center gap-2 lg:mt-6">
        <Button variant="primary" size="icon" className="size-11 rounded-full" icon={playing ? Pause : Play} aria-label={playing ? "Pause" : "Play"} onClick={() => void native.audioToggle().then(refresh)} />
        <IconButton icon={Square} label="Stop" variant="secondary" onClick={() => void native.audioStop().then(refresh)} />
        <Button variant="outline" icon={Download} busy={saving} onClick={save} className="ml-auto">Save As…</Button>
      </div>
    </Card>
  );
}
