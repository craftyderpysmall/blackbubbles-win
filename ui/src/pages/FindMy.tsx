import { useEffect, useMemo, useRef, useState } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { useQuery } from "@tanstack/react-query";
import * as maplibregl from "maplibre-gl";
import type { Map as MlMap, Marker as MlMarker } from "maplibre-gl";
import type { Feature, Polygon } from "geojson";
import workerUrl from "maplibre-gl/dist/maplibre-gl-worker.mjs?url";
import { convertFileSrc } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  ArrowLeft, BatteryFull, BatteryLow, BatteryMedium, ExternalLink, Headphones, Laptop, List, LocateFixed, MapPin, Minus, Plus, RotateCw, Smartphone, Tablet, Watch, X, type LucideIcon,
} from "lucide-react";
import { api, errorText, type FindMyDevice } from "@/lib/bb";
import { keys } from "@/lib/events";
import { NARROW_BELOW, useApp, useWidth } from "@/lib/app";
import { ago, longTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import { Button, Card, Center, EmptyState, IconButton, Spinner } from "@/components/ui";

const REFRESH_MS = 60_000;

function deviceIcon(d: FindMyDevice): LucideIcon {
  const m = `${d.model} ${d.name}`.toLowerCase();
  if (m.includes("mac")) return Laptop;
  if (m.includes("ipad")) return Tablet;
  if (m.includes("watch")) return Watch;
  if (m.includes("airpods") || m.includes("beats")) return Headphones;
  return Smartphone;
}

/** Translucent floating layer over the map (blur + soft shadow, per the theme). */
const glass = "border border-border bg-surface/85 shadow-soft backdrop-blur-xl";

export default function FindMyPage() {
  const { connected, config } = useApp();
  const family = config.services.find_my_family;
  const [ref, width] = useWidth<HTMLDivElement>();
  const [selected, setSelected] = useState<string | null>(null);
  const [listOpen, setListOpen] = useState(true);
  const devices = useQuery({ queryKey: [...keys.findMy, family], queryFn: () => api.findMy(family), enabled: connected, refetchInterval: REFRESH_MS });
  const list = useMemo(() => devices.data ?? [], [devices.data]);
  const narrow = width > 0 && width < NARROW_BELOW;
  const device = list.find((d) => d.id === selected) ?? null;

  const pick = (id: string | null) => {
    setSelected(id);
    if (narrow && id) setListOpen(false);
  };

  const panel = (
    <DeviceList
      devices={list}
      loading={devices.isLoading}
      fetching={devices.isFetching}
      error={devices.error}
      updatedAt={devices.dataUpdatedAt}
      selected={selected}
      onSelect={pick}
      onRefresh={() => void devices.refetch()}
    />
  );

  if (narrow && listOpen) return <div ref={ref} className="h-full"><Card className="flex h-full flex-col overflow-hidden">{panel}</Card></div>;

  return (
    <div ref={ref} className="h-full">
      <Card className="relative h-full overflow-hidden">
        <DeviceMap devices={list} selected={selected} onSelect={pick} leftInset={!narrow && listOpen ? 312 : 0} />
        {/* Device list floats over the map, like Apple's Find My. */}
        {!narrow && listOpen && (
          <div className={cn("absolute top-3 left-3 z-[500] flex max-h-[calc(100%-24px)] w-[300px] flex-col overflow-hidden rounded-card animate-enter", glass)}>{panel}</div>
        )}
        <div className="absolute top-3 z-[500] flex gap-2" style={{ left: !narrow && listOpen ? 316 : 12 }}>
          {(narrow || !listOpen) && (
            <Button className={glass} icon={narrow ? ArrowLeft : List} onClick={() => setListOpen(true)}>
              Devices
            </Button>
          )}
          {!narrow && listOpen && <IconButton icon={X} label="Hide device list" className={glass} onClick={() => setListOpen(false)} />}
        </div>
        {device && <DeviceCard device={device} onClose={() => setSelected(null)} narrow={narrow} listOpen={listOpen} />}
      </Card>
    </div>
  );
}

function DeviceList(props: {
  devices: FindMyDevice[];
  loading: boolean;
  fetching: boolean;
  error: unknown;
  updatedAt: number;
  selected: string | null;
  onSelect: (id: string) => void;
  onRefresh: () => void;
}) {
  const { connected, config } = useApp();
  const lowAt = config.notifications.low_battery_percent / 100;
  return (
    <>
      <div className="flex items-center gap-2 px-4 pt-3.5 pb-2">
        <div className="min-w-0 flex-1">
          <h1 className="text-[15px] font-semibold text-fg">Find My</h1>
          <div className="text-xs text-subtle">{props.updatedAt ? `Updated ${ago(props.updatedAt)}` : "Your devices and this PC"}</div>
        </div>
        {props.fetching ? <Spinner /> : <IconButton icon={RotateCw} label="Refresh" onClick={props.onRefresh} />}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
        {props.loading ? (
          <Center><Spinner /></Center>
        ) : props.error ? (
          <EmptyState icon={MapPin} title="Couldn't load devices" hint={errorText(props.error)} />
        ) : !props.devices.length ? (
          <EmptyState icon={MapPin} title="No devices" hint={connected ? "Run bbd findmy-login on the Mac." : "Connect to your Mac in Settings."} />
        ) : (
          props.devices.map((d) => {
            const Icon = deviceIcon(d);
            const active = d.id === props.selected;
            const low = d.battery !== null && d.battery < lowAt;
            return (
              <button
                key={d.id}
                onClick={() => props.onSelect(d.id)}
                className={cn("flex w-full items-center gap-3 rounded-control px-2.5 py-2.5 text-left transition-colors duration-150", active ? "bg-raised" : "hover:bg-raised/70")}
              >
                <span className={cn("flex size-9 shrink-0 items-center justify-center rounded-full", active ? "bg-primary text-primary-fg" : "border border-border-strong bg-raised text-fg")}>
                  <Icon className="size-4" />
                </span>
                <span className="min-w-0 flex-1">
                  <span className="flex items-center gap-2">
                    <span className="truncate text-[13px] font-medium text-fg">{d.name}</span>
                  </span>
                  <span className="block truncate text-xs text-subtle">{d.location ? ago(d.location.at) : "No location yet"}</span>
                </span>
                {d.battery !== null && (
                  <span className={cn("shrink-0 text-xs tabular-nums", low ? "text-danger" : "text-subtle")}>{Math.round(d.battery * 100)}%</span>
                )}
              </button>
            );
          })
        )}
      </div>
    </>
  );
}

function DeviceCard({ device, onClose, narrow, listOpen }: { device: FindMyDevice; onClose: () => void; narrow: boolean; listOpen: boolean }) {
  const { config } = useApp();
  const Icon = deviceIcon(device);
  const loc = device.location;
  const pct = device.battery !== null ? Math.round(device.battery * 100) : null;
  const low = device.battery !== null && device.battery < config.notifications.low_battery_percent / 100;
  const Battery = pct === null ? BatteryMedium : low ? BatteryLow : pct > 70 ? BatteryFull : BatteryMedium;
  return (
    <div
      key={device.id}
      className={cn("absolute bottom-3 z-[500] w-[min(360px,calc(100%-24px))] rounded-card p-4 animate-enter", glass)}
      style={narrow ? { left: 12 } : { left: listOpen ? 316 : 12 }}
    >
      <div className="flex items-start gap-3">
        <span className="flex size-10 shrink-0 items-center justify-center rounded-full bg-primary text-primary-fg">
          <Icon className="size-[18px]" />
        </span>
        <div className="min-w-0 flex-1">
          <div className="truncate text-[15px] font-semibold text-fg">{device.name}</div>
          <div className="truncate text-xs text-subtle">{device.model}</div>
        </div>
        <IconButton icon={X} label="Close" size="iconSm" onClick={onClose} />
      </div>
      <div className="selectable mt-3 grid grid-cols-2 gap-x-4 gap-y-2 text-xs">
        <div>
          <div className="text-subtle">Last seen</div>
          <div className="text-fg" title={loc ? longTime(loc.at) : undefined}>{loc ? ago(loc.at) : "Never"}</div>
        </div>
        <div>
          <div className="text-subtle">Accuracy</div>
          <div className="text-fg">{loc ? `±${Math.round(loc.accuracy_m)} m` : "–"}</div>
        </div>
        <div>
          <div className="text-subtle">Battery</div>
          <div className={cn("flex items-center gap-1", low ? "text-danger" : "text-fg")}>
            {pct === null ? "–" : <><Battery className="size-3.5" />{pct}%</>}
          </div>
        </div>
        <div>
          <div className="text-subtle">Coordinates</div>
          <div className="font-mono text-[11px] text-fg">{loc ? `${loc.lat.toFixed(4)}, ${loc.lon.toFixed(4)}` : "–"}</div>
        </div>
      </div>
      {loc && (
        <Button
          variant="outline"
          size="sm"
          icon={ExternalLink}
          className="mt-3"
          onClick={() => void openUrl(`https://www.openstreetmap.org/?mlat=${loc.lat}&mlon=${loc.lon}#map=17/${loc.lat}/${loc.lon}`)}
        >
          Open in OpenStreetMap
        </Button>
      )}
    </div>
  );
}

const iconCache = new Map<LucideIcon, Map<number, string>>();
function iconSvg(Icon: LucideIcon, size: number) {
  const bySize = iconCache.get(Icon) ?? new Map<number, string>();
  iconCache.set(Icon, bySize);
  if (!bySize.has(size)) bySize.set(size, renderToStaticMarkup(<Icon width={size} height={size} strokeWidth={2} />));
  return bySize.get(size)!;
}

/** A round device badge; the selected one is the inverted white pin with its name. */
function markerEl(d: FindMyDevice, selected: boolean, onClick: () => void) {
  const size = selected ? 40 : 32;
  const el = document.createElement("button");
  el.className = "bb-marker";
  el.title = d.name;
  // No `position` here: MapLibre positions markers absolutely (which also anchors the label).
  el.style.cssText = `width:${size}px;height:${size}px;padding:0;border:0;background:none`;
  const style = selected ? "background:#fafafa;color:#171717;border:2px solid #fafafa" : "background:#2a2a2a;color:#fafafa;border:1.5px solid #737373";
  const label = selected
    ? `<div style="position:absolute;top:${size + 6}px;left:50%;transform:translateX(-50%);white-space:nowrap;background:rgb(23 23 23/.9);border:1px solid #262626;color:#fafafa;border-radius:999px;padding:2px 9px;font:500 11px var(--font-sans)">${escapeHtml(d.name)}</div>`
    : "";
  el.innerHTML = `<div style="width:100%;height:100%;border-radius:999px;display:flex;align-items:center;justify-content:center;box-shadow:0 6px 20px rgb(0 0 0/.55);${style}">${iconSvg(deviceIcon(d), selected ? 18 : 15)}</div>${label}`;
  el.addEventListener("click", (e) => {
    e.stopPropagation();
    onClick();
  });
  return el;
}

function escapeHtml(s: string) {
  return s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);
}

maplibregl.setWorkerUrl(workerUrl);

/** OpenFreeMap's dark style, with every request routed through Rust's cached `bb://map/` proxy. */
const STYLE = "https://tiles.openfreemap.org/styles/dark";
const OFM = "https://tiles.openfreemap.org/";
const viaProxy = (url: string) => (url.startsWith(OFM) ? { url: convertFileSrc(`map/${decodeURIComponent(url.slice(OFM.length).split("?")[0])}`, "bb") } : { url });

/** A circle polygon for the accuracy ring (metres → degrees, fine at these sizes). */
function circle(lon: number, lat: number, metres: number): Feature<Polygon> {
  const points: [number, number][] = [];
  const dLat = metres / 111_320;
  const dLon = metres / (111_320 * Math.cos((lat * Math.PI) / 180));
  for (let i = 0; i <= 64; i++) {
    const a = (i / 64) * 2 * Math.PI;
    points.push([lon + dLon * Math.cos(a), lat + dLat * Math.sin(a)]);
  }
  return { type: "Feature", properties: {}, geometry: { type: "Polygon", coordinates: [points] } };
}

function bounds(devices: FindMyDevice[]) {
  const b = new maplibregl.LngLatBounds();
  for (const d of devices) if (d.location) b.extend([d.location.lon, d.location.lat]);
  return b;
}

function DeviceMap({ devices, selected, onSelect, leftInset }: { devices: FindMyDevice[]; selected: string | null; onSelect: (id: string) => void; leftInset: number }) {
  // Keep devices clear of the floating list when fitting the view.
  const padding = { top: 90, bottom: 90, right: 90, left: 90 + leftInset };
  const host = useRef<HTMLDivElement>(null);
  const map = useRef<MlMap | null>(null);
  const markers = useRef<MlMarker[]>([]);
  const fitted = useRef(false);
  const [ready, setReady] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);

  useEffect(() => {
    if (!host.current) return;
    let m: MlMap;
    try {
      m = new maplibregl.Map({
        container: host.current,
        style: STYLE,
        center: [0, 30],
        zoom: 1.5,
        attributionControl: { compact: true },
        transformRequest: viaProxy,
        dragRotate: false,
        pitchWithRotate: false,
        fadeDuration: 150,
      });
    } catch (err) {
      setFailed(errorText(err));
      return;
    }
    m.touchZoomRotate.disableRotation();
    m.on("load", () => {
      m.addSource("accuracy", { type: "geojson", data: { type: "FeatureCollection", features: [] } });
      m.addLayer({ id: "accuracy-fill", type: "fill", source: "accuracy", paint: { "fill-color": "#fafafa", "fill-opacity": 0.07 } });
      m.addLayer({ id: "accuracy-line", type: "line", source: "accuracy", paint: { "line-color": "#fafafa", "line-opacity": 0.3, "line-width": 1 } });
      setReady(true);
    });
    m.on("error", (e) => console.warn("map:", e.error?.message));
    map.current = m;
    return () => {
      m.remove();
      map.current = null;
    };
  }, []);

  useEffect(() => {
    const m = map.current;
    if (!m || !ready) return;
    for (const mk of markers.current) mk.remove();
    markers.current = [];
    const located = devices.filter((d) => d.location);
    // Unselected first so the selected pin draws on top.
    for (const d of [...located].sort((a, b) => Number(a.id === selected) - Number(b.id === selected))) {
      const el = markerEl(d, d.id === selected, () => onSelect(d.id));
      markers.current.push(new maplibregl.Marker({ element: el, anchor: "center" }).setLngLat([d.location!.lon, d.location!.lat]).addTo(m));
    }
    const sel = located.find((d) => d.id === selected);
    const source = m.getSource("accuracy") as maplibregl.GeoJSONSource | undefined;
    source?.setData({ type: "FeatureCollection", features: sel ? [circle(sel.location!.lon, sel.location!.lat, sel.location!.accuracy_m)] : [] });
    if (!fitted.current && located.length) {
      fitted.current = true;
      m.fitBounds(bounds(located), { padding, maxZoom: 15, animate: false });
    }
  }, [devices, selected, onSelect, ready]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    const d = devices.find((x) => x.id === selected);
    if (d?.location && map.current) map.current.flyTo({ center: [d.location.lon, d.location.lat], zoom: Math.max(map.current.getZoom(), 16), duration: 700, padding: { top: 0, bottom: 0, right: 0, left: leftInset } });
  }, [selected]); // eslint-disable-line react-hooks/exhaustive-deps

  const fitAll = () => {
    const located = devices.filter((d) => d.location);
    if (map.current && located.length) map.current.fitBounds(bounds(located), { padding, maxZoom: 15, duration: 700 });
  };

  return (
    <>
      {/* MapLibre makes its container position:relative, so it gets a wrapper. */}
      <div className="absolute inset-0">
        <div ref={host} className="h-full w-full" />
      </div>
      {failed && (
        <div className="absolute inset-0">
          <EmptyState icon={MapPin} title="The map couldn't start" hint={`It needs WebGL. ${failed}`} />
        </div>
      )}
      <div className={cn("absolute right-3 bottom-10 z-[500] flex flex-col overflow-hidden rounded-control", glass)}>
        <IconButton icon={Plus} label="Zoom in" className="rounded-none" onClick={() => map.current?.zoomIn()} />
        <div className="mx-2 h-px bg-border" />
        <IconButton icon={Minus} label="Zoom out" className="rounded-none" onClick={() => map.current?.zoomOut()} />
        <div className="mx-2 h-px bg-border" />
        <IconButton icon={LocateFixed} label="Show all devices" className="rounded-none" onClick={fitAll} />
      </div>
    </>
  );
}
