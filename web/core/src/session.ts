import * as Y from "yjs";
import { WebsocketProvider } from "y-websocket";
import type { Awareness } from "y-protocols/awareness";
import { type Identity, saveIdentity } from "./identity";
import { DEFAULT_LANGUAGE, type LanguageId } from "./languages";
import { PALETTE, type Rng, pickColor, pickName, sanitizeName } from "./names";
import { INITIAL_STATUS, type Status, type StatusEvent, nextStatus, statusFromClose } from "./status";

export interface Person {
  clientId: number;
  name: string;
  color: string;
  self: boolean;
}

export function ensureDefaultLang(meta: Y.Map<unknown>, lang: LanguageId = DEFAULT_LANGUAGE): void {
  if (!meta.has("lang")) meta.set("lang", lang);
}

export function resolveClash(
  me: Identity,
  others: readonly { name: string; color: string }[],
  rng: Rng = Math.random,
): Identity | null {
  const names = new Set(others.map((o) => o.name));
  const colors = new Set(others.map((o) => o.color));
  let { name, color } = me;
  if (!me.edited && names.has(name)) name = pickName(names, rng);
  if (colors.has(color)) color = pickColor(colors, rng);
  if (name === me.name && color === me.color) return null;
  return { ...me, name, color };
}

export interface Session {
  doc: Y.Doc;
  text: Y.Text;
  meta: Y.Map<unknown>;
  awareness: Awareness;
  provider: WebsocketProvider;
  identity: Identity;
  people(): Person[];
  onPeople(cb: (p: Person[]) => void): () => void;
  onStatus(cb: (s: Status) => void): () => void;
  onLang(cb: (id: LanguageId) => void): () => void;
  setLang(id: LanguageId): void;
  rename(name: string): void;
  retry(): void;
  destroy(): void;
}

export interface SessionOptions {
  room: string;
  serverUrl: string;
  identity: Identity;
  storage: Storage | null;
  WebSocketImpl?: typeof WebSocket;
}

const COLOR_RE = /^#[0-9a-f]{6}$/i;

export interface NormalizedUser {
  name: string;
  color: string;
  colorLight: string;
}

/** Validate an untrusted remote awareness `user` value. */
export function normalizeUser(user: unknown, clientId = 0): NormalizedUser {
  const u = (typeof user === "object" && user !== null ? user : {}) as Record<string, unknown>;
  const color =
    typeof u.color === "string" && COLOR_RE.test(u.color)
      ? u.color
      : PALETTE[Math.abs(clientId) % PALETTE.length]!;
  return {
    name: sanitizeName(typeof u.name === "string" ? u.name : ""),
    color,
    colorLight: color + "33",
  };
}

export function createSession(opts: SessionOptions): Session {
  const { room, serverUrl, storage } = opts;
  const doc = new Y.Doc();
  const text = doc.getText("code");
  const meta = doc.getMap<unknown>("meta");

  let identity: Identity = {
    ...opts.identity,
    name: sanitizeName(opts.identity.name),
    color: PALETTE.includes(opts.identity.color) ? opts.identity.color : pickColor(new Set()),
  };

  const provider = new WebsocketProvider(serverUrl, room, doc, {
    disableBc: true,
    WebSocketPolyfill: opts.WebSocketImpl,
    shouldReconnect: (e) => {
      const s = statusFromClose(e.code);
      return !s.fatal && !s.retryable;
    },
  });
  const awareness = provider.awareness;

  const publish = () =>
    awareness.setLocalStateField("user", {
      name: identity.name,
      color: identity.color,
      colorLight: identity.color + "33",
    });
  publish();

  const peopleCbs = new Set<(p: Person[]) => void>();
  const statusCbs = new Set<(s: Status) => void>();
  const langCbs = new Set<(id: LanguageId) => void>();

  let st = INITIAL_STATUS;
  const dispatch = (ev: StatusEvent) => {
    const prev = st.status;
    st = nextStatus(st, ev);
    if (st.status !== prev) statusCbs.forEach((cb) => cb(st.status));
  };

  const people = (): Person[] => {
    const out: Person[] = [];
    awareness.getStates().forEach((state, clientId) => {
      const user = (state as { user?: unknown }).user;
      if (user === undefined) return;
      const u = normalizeUser(user, clientId);
      out.push({ clientId, name: u.name, color: u.color, self: clientId === awareness.clientID });
    });
    return out;
  };

  // Registered before the editor's own awareness listener, so remote carets
  // render normalised values. Mutates the local copy only; nothing is re-broadcast.
  const onAwareness = () => {
    awareness.getStates().forEach((state, clientId) => {
      if (clientId === awareness.clientID) return;
      const s = state as { user?: unknown };
      if (s.user !== undefined) s.user = normalizeUser(s.user, clientId);
    });
    const list = people();
    peopleCbs.forEach((cb) => cb(list));
  };
  awareness.on("change", onAwareness);

  // Only the joiner avoids names already in the room: check once, at first sync,
  // when the server's awareness snapshot has been applied.
  let synced = false;
  const onSync = (isSynced: boolean) => {
    if (!isSynced) return;
    dispatch({ type: "sync" });
    if (synced) return;
    synced = true;
    ensureDefaultLang(meta);
    const others = people().filter((p) => !p.self);
    const next = resolveClash(identity, others);
    if (next) {
      identity = next;
      session.identity = next;
      saveIdentity(room, next, storage);
      publish();
    }
  };
  provider.on("sync", onSync);

  provider.on("connection-close", (event: CloseEvent | null) => {
    if (event == null) return; // closed locally
    dispatch({ type: "close", code: event.code });
  });
  provider.on("status", ({ status: s }: { status: "connected" | "disconnected" | "connecting" }) => {
    if (s === "connecting") dispatch({ type: "connecting" });
    else if (s === "disconnected") dispatch({ type: "disconnected" });
  });

  const onMeta = (e: Y.YMapEvent<unknown>) => {
    if (e.keysChanged.has("lang")) {
      const id = meta.get("lang") as LanguageId;
      langCbs.forEach((cb) => cb(id));
    }
  };
  meta.observe(onMeta);

  const session: Session = {
    doc,
    text,
    meta,
    awareness,
    provider,
    identity,
    people,
    onPeople(cb) {
      peopleCbs.add(cb);
      return () => peopleCbs.delete(cb);
    },
    onStatus(cb) {
      statusCbs.add(cb);
      return () => statusCbs.delete(cb);
    },
    onLang(cb) {
      langCbs.add(cb);
      return () => langCbs.delete(cb);
    },
    setLang(id) {
      meta.set("lang", id);
    },
    rename(name) {
      identity = { ...identity, name: sanitizeName(name), edited: true };
      session.identity = identity;
      saveIdentity(room, identity, storage);
      publish();
    },
    retry() {
      dispatch({ type: "retry" });
      provider.connect();
    },
    destroy() {
      meta.unobserve(onMeta);
      awareness.off("change", onAwareness);
      peopleCbs.clear();
      statusCbs.clear();
      langCbs.clear();
      provider.destroy();
      doc.destroy();
    },
  };
  return session;
}
