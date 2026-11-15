import * as Y from "yjs";
import { WebsocketProvider } from "y-websocket";
import { type Awareness, encodeAwarenessUpdate, removeAwarenessStates } from "y-protocols/awareness";
import { type Identity, saveIdentity } from "./identity";
import { DEFAULT_LANGUAGE, type LanguageId } from "./languages";
import { PALETTE, type Rng, pickColor, pickName, sanitizeName } from "./names";
import { INITIAL_STATUS, type Status, type StatusEvent, nextStatus, statusFromClose } from "./status";

export interface Person {
  clientId: number;
  name: string;
  color: string;
  self: boolean;
  /** Tab in the background or no input for a while. */
  away: boolean;
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
  /** Publish whether this tab is away (hidden or idle). */
  setAway(away: boolean): void;
  rename(name: string): void;
  retry(): void;
  destroy(): void;
}

export interface SessionOptions {
  room: string;
  serverUrl: string;
  identity: Identity;
  storage: Storage | null;
  /** Language for a brand-new room; ignored when the room already has one. */
  initialLang?: LanguageId;
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

/** y-websocket's message type for awareness updates. */
const MESSAGE_AWARENESS = 1;

function writeVarUint(out: number[], n: number): void {
  while (n > 0x7f) {
    out.push(0x80 | (n & 0x7f));
    n = Math.floor(n / 128);
  }
  out.push(n);
}

export interface AwarenessChanges {
  added: number[];
  updated: number[];
  removed: number[];
}

/**
 * The y-websocket awareness message for the local client, or null when the
 * changes concern remote clients only (those are never re-broadcast).
 */
export function localAwarenessMessage(
  awareness: Awareness,
  changes: AwarenessChanges,
): Uint8Array<ArrayBuffer> | null {
  const id = awareness.clientID;
  const { added, updated, removed } = changes;
  if (!added.includes(id) && !updated.includes(id) && !removed.includes(id)) return null;
  const update = encodeAwarenessUpdate(awareness, [id]);
  const head: number[] = [MESSAGE_AWARENESS];
  writeVarUint(head, update.length);
  const msg = new Uint8Array(head.length + update.length);
  msg.set(head);
  msg.set(update, head.length);
  return msg;
}

/**
 * Stock y-websocket re-sends every awareness change it applies, so each
 * client echoes everyone else's cursor moves to the server. Replace its
 * handler with one that sends only our own state. Returns the undo.
 */
export function sendLocalAwarenessOnly(provider: WebsocketProvider): () => void {
  const { awareness } = provider;
  awareness.off("update", provider._awarenessUpdateHandler);
  const onUpdate = (changes: AwarenessChanges) => {
    const msg = localAwarenessMessage(awareness, changes);
    const ws = provider.ws;
    if (msg && provider.wsconnected && ws && ws.readyState === ws.OPEN) ws.send(msg);
  };
  awareness.on("update", onUpdate);
  return () => awareness.off("update", onUpdate);
}

/**
 * Drops every remote client, clocks included. y-websocket removes remote
 * states on disconnect but keeps their clocks, so after a reconnect the
 * server's greeting (same clocks) would be ignored until each peer renews.
 */
export function forgetRemotePeers(awareness: Awareness, origin: unknown = "reconnect"): void {
  const remote = [...awareness.meta.keys()].filter((id) => id !== awareness.clientID);
  removeAwarenessStates(awareness, remote, origin);
  for (const id of remote) awareness.meta.delete(id);
}

function peopleOf(awareness: Awareness): Person[] {
  const out: Person[] = [];
  awareness.getStates().forEach((state, clientId) => {
    const user = (state as { user?: unknown }).user;
    if (user === undefined) return;
    const u = normalizeUser(user, clientId);
    const away = (state as { presence?: unknown }).presence === "away";
    out.push({ clientId, name: u.name, color: u.color, self: clientId === awareness.clientID, away });
  });
  return out;
}

/**
 * Normalises remote `user` values in place and calls `cb` when the list of
 * people (id, name, colour, presence) changes; cursor moves alone do not notify.
 * Register before the editor's awareness listener so carets render
 * normalised values. Nothing is re-broadcast. Returns the unsubscribe.
 */
export function watchPeople(awareness: Awareness, cb: (p: Person[]) => void): () => void {
  let key = "";
  const onChange = () => {
    awareness.getStates().forEach((state, clientId) => {
      if (clientId === awareness.clientID) return;
      const s = state as { user?: unknown };
      if (s.user !== undefined) s.user = normalizeUser(s.user, clientId);
    });
    const list = peopleOf(awareness);
    const next = JSON.stringify(list.map((p) => [p.clientId, p.name, p.color, p.away]));
    if (next === key) return;
    key = next;
    cb(list);
  };
  awareness.on("change", onChange);
  return () => awareness.off("change", onChange);
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
  const stopAwarenessRelay = sendLocalAwarenessOnly(provider);

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

  const people = (): Person[] => peopleOf(awareness);
  const stopPeople = watchPeople(awareness, (list) => peopleCbs.forEach((cb) => cb(list)));

  // Only the joiner avoids names already in the room: check once, at first sync,
  // when the server's awareness snapshot has been applied.
  let synced = false;
  const onSync = (isSynced: boolean) => {
    if (!isSynced) return;
    dispatch({ type: "sync" });
    if (synced) return;
    synced = true;
    ensureDefaultLang(meta, opts.initialLang);
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
    // Status first, so the people list drops its baseline before the removals
    // below (no "left" toasts for a disconnect). A null event is a local close.
    if (event != null) dispatch({ type: "close", code: event.code });
    else if (provider.wsconnected) dispatch({ type: "disconnected" });
    forgetRemotePeers(awareness, provider);
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
    setAway(away) {
      const next = away ? "away" : "active";
      if (awareness.getLocalState()?.presence !== next) awareness.setLocalStateField("presence", next);
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
      stopPeople();
      peopleCbs.clear();
      statusCbs.clear();
      langCbs.clear();
      provider.destroy();
      stopAwarenessRelay();
      doc.destroy();
    },
  };
  return session;
}
