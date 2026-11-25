import { useEffect, useMemo, useRef, useState } from "react";
import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Heart, Image as ImageIcon, Play, RotateCw } from "lucide-react";
import { api, blobUrl, errorText, type Asset } from "@/lib/bb";
import { keys } from "@/lib/events";
import { useApp, useWidth } from "@/lib/app";
import { longTime } from "@/lib/format";
import { Button, Center, Chip, EmptyState, Spinner, WorkspaceHeading } from "@/components/ui";
import { Lightbox } from "@/components/Lightbox";

/** Tiles stretch from this size so the grid always fills the width exactly. */
const MIN_TILE = 152;
const GAP = 12;
const PAGE = 240;

export default function PhotosPage() {
  const { connected } = useApp();
  const [album, setAlbum] = useState("all");
  const [viewing, setViewing] = useState<number | null>(null);
  const albums = useQuery({ queryKey: keys.albums, queryFn: api.albums, enabled: connected });
  const assets = useInfiniteQuery({
    queryKey: [...keys.assets, album],
    queryFn: ({ pageParam }) => api.assets(album, pageParam, PAGE),
    initialPageParam: 0,
    getNextPageParam: (last, all) => (last.length < PAGE ? undefined : all.reduce((n, p) => n + p.length, 0)),
    enabled: connected,
  });
  const list = useMemo(() => assets.data?.pages.flat() ?? [], [assets.data]);
  const current = viewing !== null ? list[viewing] : undefined;
  const shownAlbum = albums.data?.find((a) => a.id === album);
  const albumDetail = shownAlbum ? `${shownAlbum.title}${shownAlbum.count ? ` · ${shownAlbum.count.toLocaleString()} items` : ""}` : connected ? undefined : "Pair with your Mac to browse your library.";

  return (
    <section className="flex h-full flex-col">
      <WorkspaceHeading title="Photos" detail={albumDetail}>
        <Button variant="secondary" icon={RotateCw} onClick={() => void (albums.refetch(), assets.refetch())}>Refresh</Button>
      </WorkspaceHeading>
      <div className="mb-4 border-b border-hairline pb-3">
        <div aria-label="Photo albums" className="flex min-w-0 gap-1.5 overflow-x-auto [scrollbar-width:none]">
          {(albums.data ?? [{ id: "all", title: "Recents", count: 0 }]).map((a) => (
            <Chip key={a.id} active={a.id === album} onClick={() => setAlbum(a.id)}>
              {a.title}
              {a.count > 0 && <span className="text-muted-soft">{a.count}</span>}
            </Chip>
          ))}
        </div>
      </div>
      <div className="min-h-0 flex-1">
        {assets.isLoading ? (
          <Center><Spinner /></Center>
        ) : assets.error ? (
          <EmptyState icon={ImageIcon} title="Couldn't load photos" hint={errorText(assets.error)} />
        ) : !list.length ? (
          <EmptyState icon={ImageIcon} title="No photos" hint={connected ? "This album is empty." : "Connect to your Mac in Settings."} />
        ) : (
          <Grid assets={list} onOpen={setViewing} onNearEnd={() => assets.hasNextPage && !assets.isFetchingNextPage && void assets.fetchNextPage()} />
        )}
      </div>
      {current && viewing !== null && (
        <Lightbox
          kind="Photo"
          id={current.id}
          name={`${current.id.replace(/[^\w.-]+/g, "_")}.jpg`}
          placeholder={blobUrl("PhotoThumbnail", current.id)}
          caption={`${longTime(current.created)} · ${current.width}×${current.height}${current.is_video ? " · video (still shown)" : ""}`}
          onClose={() => setViewing(null)}
          onPrev={viewing > 0 ? () => setViewing(viewing - 1) : undefined}
          onNext={viewing < list.length - 1 ? () => setViewing(viewing + 1) : undefined}
        />
      )}
    </section>
  );
}

function Grid({ assets, onOpen, onNearEnd }: { assets: Asset[]; onOpen: (i: number) => void; onNearEnd: () => void }) {
  const scroller = useRef<HTMLDivElement>(null);
  const [measure, width] = useWidth<HTMLDivElement>();
  const inner = Math.max(0, width - 8);
  const columns = Math.max(1, Math.floor((inner + GAP) / (MIN_TILE + GAP)));
  const tile = (inner - GAP * (columns - 1)) / columns;
  const rows = Math.ceil(assets.length / columns);
  const virtual = useVirtualizer({ count: rows, getScrollElement: () => scroller.current, estimateSize: () => tile + GAP, overscan: 4 });
  useEffect(() => virtual.measure(), [tile, virtual]);

  const items = virtual.getVirtualItems();
  const lastRow = items.at(-1)?.index ?? 0;
  useEffect(() => {
    if (rows && lastRow + 6 >= rows) onNearEnd();
  }, [lastRow, rows, onNearEnd]);

  return (
    <div ref={(el) => ((scroller.current = el), measure(el))} className="h-full overflow-y-auto pb-3">
      {width > 0 && (
        <div style={{ height: virtual.getTotalSize(), position: "relative" }}>
          {items.map((row) => (
            <div key={row.key} className="absolute left-0 flex" style={{ top: 0, transform: `translateY(${row.start}px)`, gap: GAP, height: tile }}>
              {assets.slice(row.index * columns, row.index * columns + columns).map((asset, col) => (
                <Tile key={asset.id} asset={asset} size={tile} onClick={() => onOpen(row.index * columns + col)} />
              ))}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function Tile({ asset, size, onClick }: { asset: Asset; size: number; onClick: () => void }) {
  const [loaded, setLoaded] = useState(false);
  return (
    <button aria-label={`Open ${asset.is_video ? "video" : "photo"} from ${longTime(asset.created)}`} onClick={onClick} className="group relative shrink-0 overflow-hidden rounded-control bg-card ring-1 ring-inset ring-hairline" style={{ width: size, height: size }}>
      <img
        src={blobUrl("PhotoThumbnail", asset.id)}
        alt=""
        decoding="async"
        draggable={false}
        onLoad={() => setLoaded(true)}
        className="size-full object-cover transition-[opacity,transform] duration-300 ease-decel group-hover:scale-[1.03]"
        style={{ opacity: loaded ? 1 : 0 }}
      />
      {asset.is_video && (
        <span className="absolute right-2 bottom-2 flex size-6 items-center justify-center rounded-full border border-hairline-strong bg-deep/80 text-ink">
          <Play className="size-3 fill-current" />
        </span>
      )}
      {asset.favorite && (
        <span className="absolute left-2 bottom-2 flex size-6 items-center justify-center rounded-full border border-hairline-strong bg-deep/80 text-ink">
          <Heart className="size-3 fill-current" />
        </span>
      )}
    </button>
  );
}
