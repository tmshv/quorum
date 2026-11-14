import * as Y from "yjs";
import { WebsocketProvider } from "y-websocket";
import type { Awareness } from "y-protocols/awareness";
import { type Identity, saveIdentity } from "./identity";
import { DEFAULT_LANGUAGE, type LanguageId } from "./languages";
import { PALETTE, type Rng, pickColor, pickName, sanitizeName } from "./names";
import { type Status, statusFromClose } from "./status";

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

interface UserState {
  name?: unknown;
  color?: unknown;
}

function userOf(state: unknown): { name: string; color: string } | null {
  const user = (state as { user?: UserState } | undefined)?.user;
  if (!user || typeof user.color !== "string") return null;
  return { name: sanitizeName(typeof user.name === "string" ? user.name : ""), color: user.color };
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

  let status: Status = "connecting";
  let stopped = false; // closed with a code that stops the provider
  const setStatus = (s: Status) => {
    if (s === status) return;
    status = s;
    statusCbs.forEach((cb) => cb(s));
  };

  const people = (): Person[] => {
    const out: Person[] = [];
    awareness.getStates().forEach((state, clientId) => {
      const u = userOf(state);
      if (u) out.push({ clientId, ...u, self: clientId === awareness.clientID });
    });
    return out;
  };

  let clashChecked = false;
  const onAwareness = () => {
    const list = people();
    peopleCbs.forEach((cb) => cb(list));
    const others = list.filter((p) => !p.self);
    if (!clashChecked && others.length > 0) {
      clashChecked = true;
      const next = resolveClash(identity, others);
      if (next) {
        identity = next;
        session.identity = next;
        saveIdentity(room, next, storage);
        publish();
      }
    }
  };
  awareness.on("change", onAwareness);

  const onSync = (synced: boolean) => {
    if (synced) {
      ensureDefaultLang(meta);
      provider.off("sync", onSync);
    }
  };
  provider.on("sync", onSync);

  const onClose = (event: CloseEvent | null) => {
    if (event == null) return; // closed locally
    const s = statusFromClose(event.code);
    stopped = s.fatal || s.retryable;
    setStatus(s.status);
  };
  provider.on("connection-close", onClose);

  provider.on("status", ({ status: st }: { status: "connected" | "disconnected" | "connecting" }) => {
    if (st === "connected") {
      stopped = false;
      setStatus("connected");
    } else if (!stopped) {
      setStatus(st === "connecting" ? "connecting" : "reconnecting");
    }
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
      stopped = false;
      setStatus("connecting");
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
