import { type Rng, pickColor, pickName } from "./names";

export interface Identity {
  name: string;
  color: string;
  edited: boolean;
}

export function identityKey(room: string): string {
  return `quorum:identity:${room}`;
}

function parse(raw: string | null): Identity | null {
  if (raw === null) return null;
  try {
    const v: unknown = JSON.parse(raw);
    if (typeof v !== "object" || v === null) return null;
    const { name, color, edited } = v as Record<string, unknown>;
    if (typeof name !== "string" || typeof color !== "string" || typeof edited !== "boolean") {
      return null;
    }
    return { name, color, edited };
  } catch {
    return null;
  }
}

export function saveIdentity(room: string, identity: Identity, storage: Storage | null): void {
  try {
    storage?.setItem(identityKey(room), JSON.stringify(identity));
  } catch {
    // storage unavailable or full: identity just won't survive a reload
  }
}

export function loadIdentity(room: string, storage: Storage | null, rng: Rng = Math.random): Identity {
  let stored: Identity | null = null;
  try {
    stored = parse(storage?.getItem(identityKey(room)) ?? null);
  } catch {
    // ignore
  }
  if (stored) return stored;
  const identity: Identity = {
    name: pickName(new Set(), rng),
    color: pickColor(new Set(), rng),
    edited: false,
  };
  saveIdentity(room, identity, storage);
  return identity;
}
