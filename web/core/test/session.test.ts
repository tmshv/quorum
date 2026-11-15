import { afterEach, describe, expect, it } from "vitest";
import * as Y from "yjs";
import { WebsocketProvider } from "y-websocket";
import {
  Awareness, applyAwarenessUpdate, encodeAwarenessUpdate, removeAwarenessStates,
} from "y-protocols/awareness";
import {
  type Person, ensureDefaultLang, forgetRemotePeers, localAwarenessMessage, normalizeUser,
  dedupeNames, resolveClash, sendLocalAwarenessOnly, watchPeople,
} from "../src/session";
import { PALETTE } from "../src/names";
import type { Identity } from "../src/identity";

const sync = (a: Y.Doc, b: Y.Doc) => Y.applyUpdate(b, Y.encodeStateAsUpdate(a));

describe("ensureDefaultLang", () => {
  it("sets python on empty", () => {
    const d = new Y.Doc();
    ensureDefaultLang(d.getMap("meta"));
    expect(d.getMap("meta").get("lang")).toBe("python");
  });
  it("keeps an existing lang", () => {
    const a = new Y.Doc();
    const b = new Y.Doc();
    a.getMap("meta").set("lang", "rust");
    sync(a, b);
    ensureDefaultLang(b.getMap("meta"));
    expect(b.getMap("meta").get("lang")).toBe("rust");
  });
});

describe("resolveClash", () => {
  const me: Identity = { name: "Deadlock", color: PALETTE[0]!, edited: false };
  const at = { clientId: 5, since: 1000 };
  const rival = (name: string, color: string, since: number, clientId = 9) => ({ clientId, since, name, color });

  it("the later joiner re-picks a clashing name", () => {
    const r = resolveClash(me, at, [rival("Deadlock", PALETTE[1]!, 500)], () => 0)!;
    expect(r.name).not.toBe("Deadlock");
    expect(r.color).toBe(me.color);
  });
  it("the earlier joiner keeps its name", () => {
    expect(resolveClash(me, at, [rival("Deadlock", PALETTE[1]!, 2000)])).toBeNull();
  });
  it("simultaneous joiners: the higher client id gives way, the lower keeps", () => {
    const twin = [rival("Deadlock", PALETTE[1]!, 1000, 3)];
    expect(resolveClash(me, at, twin, () => 0)!.name).not.toBe("Deadlock");
    expect(resolveClash(me, { clientId: 1, since: 1000 }, twin)).toBeNull();
  });
  it("keeps an edited name", () => {
    expect(resolveClash({ ...me, edited: true }, at, [rival("Deadlock", PALETTE[1]!, 500)])).toBeNull();
  });
  it("re-picks a clashing colour even when edited, avoiding everyone's", () => {
    const others = [rival("x", me.color, 500), rival("y", PALETTE[1]!, 3000)];
    const r = resolveClash({ ...me, edited: true }, at, others, () => 0)!;
    expect(r.name).toBe("Deadlock");
    expect([me.color, PALETTE[1]]).not.toContain(r.color);
  });
  it("keeps a clashing colour when no colour is free", () => {
    const others = PALETTE.map((c, i) => rival(`n${i}`, c, 500, 10 + i));
    expect(resolveClash(me, at, others)).toBeNull();
  });
  it("re-picked names avoid everyone in the room", () => {
    const others = [rival("Deadlock", PALETTE[1]!, 500), rival("Not Found", PALETTE[2]!, 3000)];
    const r = resolveClash(me, at, others, () => 0)!;
    expect(["Deadlock", "Not Found"]).not.toContain(r.name);
  });
  it("returns null without any clash", () => {
    expect(resolveClash(me, at, [rival("a", PALETTE[2]!, 500)])).toBeNull();
  });
});

describe("duplicate names after a simultaneous join", () => {
  it("two sessions that both missed each other end up with different names", () => {
    const a = awarenessOf(1);
    const b = awarenessOf(2);
    const user = { name: "Round Robin", color: PALETTE[0]!, colorLight: PALETTE[0]! + "33", since: 1000 };
    a.setLocalStateField("user", user);
    b.setLocalStateField("user", user);
    const ida: Identity = { name: "Round Robin", color: PALETTE[0]!, edited: false };
    const idb: Identity = { ...ida };
    cleanups.push(dedupeNames(a, () => ida, (n) => { Object.assign(ida, n); a.setLocalStateField("user", { ...user, ...n }); }));
    cleanups.push(dedupeNames(b, () => idb, (n) => { Object.assign(idb, n); b.setLocalStateField("user", { ...user, ...n }); }));
    receive(a, b, [2]);
    receive(b, a, [1]);
    receive(a, b, [2]);
    expect(ida.name).toBe("Round Robin");
    expect(idb.name).not.toBe("Round Robin");
    expect(ida.color).toBe(PALETTE[0]);
    expect(idb.color).not.toBe(PALETTE[0]);
  });
});

describe("normalizeUser", () => {
  it("keeps a valid user and derives colorLight", () => {
    expect(normalizeUser({ name: "Bob", color: "#AABBCC", since: 42 })).toEqual({
      name: "Bob", color: "#AABBCC", colorLight: "#AABBCC33", since: 42,
    });
    expect(normalizeUser({ name: "Bob", since: "soon" }).since).toBe(Number.MAX_SAFE_INTEGER);
    expect(normalizeUser({ name: "Bob", since: Infinity }).since).toBe(Number.MAX_SAFE_INTEGER);
  });
  it("rejects CSS injection in colour", () => {
    const u = normalizeUser({ name: "x", color: "red;background:url(http://evil)", colorLight: "url(x)" }, 2);
    expect(u.color).toBe(PALETTE[2]);
    expect(u.colorLight).toBe(PALETTE[2] + "33");
  });
  it("sanitises names and tolerates junk", () => {
    expect(normalizeUser({ name: "a\u0000b\n  c", color: "#000000" }).name).toBe("ab c");
    expect(normalizeUser({ name: 5 }).name).toBe("anonymous");
    expect(normalizeUser(null).name).toBe("anonymous");
    expect(normalizeUser("str").color).toMatch(/^#[0-9a-f]{6}$/i);
  });
});

describe("undo wiring", () => {
  it("Y.UndoManager tracking local origin does not revert remote edits", () => {
    const a = new Y.Doc();
    const b = new Y.Doc();
    a.on("update", (u: Uint8Array, o: unknown) => o !== "remote" && Y.applyUpdate(b, u, "remote"));
    b.on("update", (u: Uint8Array, o: unknown) => o !== "remote" && Y.applyUpdate(a, u, "remote"));
    const ta = a.getText("code");
    const um = new Y.UndoManager(ta);
    ta.insert(0, "mine");
    b.getText("code").insert(0, "peer-");
    um.undo();
    expect(ta.toString()).toBe("peer-");
  });
});

const cleanups: (() => void)[] = [];
afterEach(() => {
  while (cleanups.length) cleanups.pop()!();
});

function awarenessOf(clientID: number): Awareness {
  const doc = new Y.Doc();
  doc.clientID = clientID;
  const a = new Awareness(doc);
  cleanups.push(() => doc.destroy());
  return a;
}

/** Copies `from`'s state for `ids` into `to`, as if received from the server. */
function receive(to: Awareness, from: Awareness, ids: number[], origin: unknown = "remote") {
  applyAwarenessUpdate(to, encodeAwarenessUpdate(from, ids), origin);
}

/** Client IDs with a state in `a`, other than its own. */
const remoteIds = (a: Awareness) => [...a.getStates().keys()].filter((id) => id !== a.clientID);

/** Strips the y-websocket envelope (type 1, varuint length) from a message. */
function unwrap(msg: Uint8Array): Uint8Array {
  expect(msg[0]).toBe(1);
  let len = 0;
  let shift = 0;
  let i = 1;
  for (;;) {
    const b = msg[i++]!;
    len += (b & 0x7f) * 2 ** shift;
    shift += 7;
    if (b < 0x80) break;
  }
  expect(msg.length - i).toBe(len);
  return msg.subarray(i);
}

describe("localAwarenessMessage", () => {
  it("ignores changes to remote clients", () => {
    const me = awarenessOf(1);
    const peer = awarenessOf(2);
    peer.setLocalStateField("user", { name: "p" });
    expect(localAwarenessMessage(me, { added: [2], updated: [], removed: [] })).toBeNull();
  });

  it("encodes only the local client", () => {
    const me = awarenessOf(1);
    const peer = awarenessOf(2);
    peer.setLocalStateField("user", { name: "p" });
    receive(me, peer, [2]);
    me.setLocalStateField("user", { name: "x".repeat(200) });
    const msg = localAwarenessMessage(me, { added: [], updated: [1, 2], removed: [] })!;
    const other = awarenessOf(3);
    applyAwarenessUpdate(other, unwrap(msg), "test");
    expect(remoteIds(other)).toEqual([1]);
    expect(other.getStates().get(1)).toEqual({ user: { name: "x".repeat(200) } });
  });

  it("encodes the local removal", () => {
    const me = awarenessOf(1);
    const other = awarenessOf(3);
    receive(other, me, [1]);
    removeAwarenessStates(me, [1], "app closed");
    const msg = localAwarenessMessage(me, { added: [], updated: [], removed: [1] })!;
    applyAwarenessUpdate(other, unwrap(msg), "test");
    expect(other.getStates().has(1)).toBe(false);
  });
});

describe("sendLocalAwarenessOnly", () => {
  it("does not echo remote awareness changes", () => {
    const doc = new Y.Doc();
    const provider = new WebsocketProvider("ws://localhost:1", "r", doc, { connect: false, disableBc: true });
    cleanups.push(() => {
      provider.destroy();
      doc.destroy();
    });
    const sent: Uint8Array[] = [];
    const ws = { readyState: 1, OPEN: 1, send: (d: Uint8Array) => sent.push(d), close: () => {} };
    provider.ws = ws as unknown as WebSocket;
    provider.wsconnected = true;
    const stop = sendLocalAwarenessOnly(provider);

    const peer = awarenessOf(2);
    peer.setLocalStateField("cursor", { i: 1 });
    receive(provider.awareness, peer, [2], provider);
    expect(sent).toHaveLength(0);

    provider.awareness.setLocalStateField("cursor", { i: 2 });
    expect(sent).toHaveLength(1);
    const other = awarenessOf(3);
    applyAwarenessUpdate(other, unwrap(sent[0]!), "test");
    expect(remoteIds(other)).toEqual([doc.clientID]);

    stop();
    provider.awareness.setLocalStateField("cursor", { i: 3 });
    expect(sent).toHaveLength(1);
  });
});

describe("watchPeople", () => {
  it("notifies on joins and renames, not on cursor moves", () => {
    const me = awarenessOf(1);
    me.setLocalStateField("user", { name: "me", color: "#111111" });
    const peer = awarenessOf(2);
    peer.setLocalStateField("user", { name: "p", color: "#222222" });
    const calls: Person[][] = [];
    cleanups.push(watchPeople(me, (p) => calls.push(p)));

    receive(me, peer, [2]);
    expect(calls).toHaveLength(1);
    expect(calls[0]!.map((p) => p.name)).toEqual(["me", "p"]);

    peer.setLocalStateField("cursor", { anchor: 1, head: 2 });
    receive(me, peer, [2]);
    peer.setLocalStateField("cursor", { anchor: 3, head: 3 });
    receive(me, peer, [2]);
    me.setLocalStateField("cursor", { anchor: 0, head: 0 });
    expect(calls).toHaveLength(1);

    peer.setLocalStateField("user", { name: "q", color: "#222222" });
    receive(me, peer, [2]);
    expect(calls).toHaveLength(2);
    expect(calls[1]!.map((p) => p.name)).toEqual(["me", "q"]);
  });

  it("normalises remote users before anyone reads them", () => {
    const me = awarenessOf(1);
    const peer = awarenessOf(2);
    peer.setLocalStateField("user", { name: "p", color: "red;x" });
    cleanups.push(watchPeople(me, () => {}));
    receive(me, peer, [2]);
    const user = (me.getStates().get(2) as { user: { color: string } }).user;
    expect(user.color).toMatch(/^#[0-9a-f]{6}$/i);
  });
});

describe("forgetRemotePeers", () => {
  it("lets the server's greeting re-add peers after a reconnect", () => {
    const me = awarenessOf(1);
    me.setLocalStateField("user", { name: "me" });
    const peer = awarenessOf(2);
    peer.setLocalStateField("user", { name: "p" });
    receive(me, peer, [2]);

    forgetRemotePeers(me);
    expect([...me.getStates().keys()]).toEqual([1]);
    expect(me.meta.has(2)).toBe(false);
    expect(me.meta.has(1)).toBe(true);
    // The greeting carries the same clock as before the disconnect.
    receive(me, peer, [2]);
    expect(me.getStates().has(2)).toBe(true);
  });

  it("is needed: y-websocket's own removal keeps the old clock", () => {
    const me = awarenessOf(1);
    const peer = awarenessOf(2);
    peer.setLocalStateField("user", { name: "p" });
    receive(me, peer, [2]);
    removeAwarenessStates(me, [2], "provider");
    receive(me, peer, [2]);
    expect(me.getStates().has(2)).toBe(false);
  });
});

describe("ensureDefaultLang with a chosen language", () => {
  it("uses the creator's pick on an empty room", () => {
    const d = new Y.Doc();
    ensureDefaultLang(d.getMap("meta"), "go");
    expect(d.getMap("meta").get("lang")).toBe("go");
  });
  it("never overrides a room that already has one", () => {
    const d = new Y.Doc();
    d.getMap("meta").set("lang", "rust");
    ensureDefaultLang(d.getMap("meta"), "go");
    expect(d.getMap("meta").get("lang")).toBe("rust");
  });
});

describe("presence", () => {
  it("reports away peers and notifies when presence flips", () => {
    const me = awarenessOf(1);
    me.setLocalStateField("user", { name: "me", color: "#111111" });
    const peer = awarenessOf(2);
    peer.setLocalStateField("user", { name: "p", color: "#222222" });
    const calls: Person[][] = [];
    cleanups.push(watchPeople(me, (p) => calls.push(p)));

    receive(me, peer, [2]);
    expect(calls.at(-1)!.find((p) => p.clientId === 2)!.away).toBe(false);

    peer.setLocalStateField("presence", "away");
    receive(me, peer, [2]);
    expect(calls).toHaveLength(2);
    expect(calls.at(-1)!.find((p) => p.clientId === 2)!.away).toBe(true);

    peer.setLocalStateField("presence", "active");
    receive(me, peer, [2]);
    expect(calls).toHaveLength(3);
    expect(calls.at(-1)!.find((p) => p.clientId === 2)!.away).toBe(false);
  });
  it("treats junk presence as active", () => {
    const me = awarenessOf(1);
    const peer = awarenessOf(2);
    peer.setLocalState({ user: { name: "p", color: "#222222" }, presence: { evil: true } });
    const calls: Person[][] = [];
    cleanups.push(watchPeople(me, (p) => calls.push(p)));
    receive(me, peer, [2]);
    expect(calls.at(-1)![0]!.away).toBe(false);
  });
});
