import { type Rng, pickName } from "./names";

export const ROOM_RE = /^[a-z0-9][a-z0-9-]{0,39}$/;

export function slugify(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

export function normalizeRoom(raw: string): string {
  let s = raw;
  try {
    s = decodeURIComponent(raw);
  } catch {
    // keep the raw text
  }
  return slugify(s).slice(0, 40).replace(/-+$/, "");
}

export function randomRoom(rng: Rng = Math.random): string {
  const n = Math.min(Math.floor(rng() * 100), 99);
  return `${slugify(pickName(new Set(), rng))}-${String(n).padStart(2, "0")}`;
}
