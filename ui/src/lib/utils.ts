import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/** Calendars carry their own colours; map them onto the desaturated semantic palette by
 *  nearest hue, so the UI stays monochrome-first. */
const SEMANTIC: [number, string][] = [
  [215, "var(--color-info)"],
  [134, "var(--color-success)"],
  [43, "var(--color-warning)"],
  [359, "var(--color-danger)"],
  [260, "var(--color-magenta)"],
  [186, "var(--color-cyan)"],
];

export function accent(rgb: number): string {
  const r = ((rgb >> 16) & 255) / 255;
  const g = ((rgb >> 8) & 255) / 255;
  const b = (rgb & 255) / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  if (max - min < 0.08) return "var(--color-muted)";
  const d = max - min;
  let h = max === r ? ((g - b) / d) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  h = (h * 60 + 360) % 360;
  let best = SEMANTIC[0];
  let bestDiff = 999;
  for (const entry of SEMANTIC) {
    const diff = Math.min(Math.abs(entry[0] - h), 360 - Math.abs(entry[0] - h));
    if (diff < bestDiff) [best, bestDiff] = [entry, diff];
  }
  return best[1];
}

export function initials(name: string): string {
  const out = name
    .split(/\s+/)
    .map((w) => [...w].find((c) => /[\p{L}\p{N}]/u.test(c)) ?? "")
    .filter(Boolean)
    .slice(0, 2)
    .join("")
    .toUpperCase();
  return out || "#";
}
