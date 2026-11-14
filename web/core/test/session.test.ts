import { describe, expect, it } from "vitest";
import * as Y from "yjs";
import { ensureDefaultLang, normalizeUser, resolveClash } from "../src/session";
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
  it("re-picks the name on clash when not edited", () => {
    const others = [{ name: "Deadlock", color: PALETTE[1]! }];
    const r = resolveClash(me, others, () => 0)!;
    expect(r.name).not.toBe("Deadlock");
    expect(r.color).toBe(me.color);
  });
  it("keeps an edited name", () => {
    const r = resolveClash({ ...me, edited: true }, [{ name: "Deadlock", color: PALETTE[1]! }]);
    expect(r).toBeNull();
  });
  it("re-picks a clashing colour even when edited", () => {
    const others = [{ name: "x", color: me.color }];
    const r = resolveClash({ ...me, edited: true }, others, () => 0)!;
    expect(r.name).toBe("Deadlock");
    expect(r.color).not.toBe(me.color);
    expect(others.map((o) => o.color)).not.toContain(r.color);
  });
  it("returns null without any clash", () => {
    expect(resolveClash(me, [{ name: "a", color: PALETTE[2]! }])).toBeNull();
  });
  it("re-picked values are not used by others", () => {
    const others = [{ name: "Deadlock", color: me.color }];
    const r = resolveClash(me, others, () => 0)!;
    expect(r.name).not.toBe("Deadlock");
    expect(r.color).not.toBe(me.color);
  });
});

describe("normalizeUser", () => {
  it("keeps a valid user and derives colorLight", () => {
    expect(normalizeUser({ name: "Bob", color: "#AABBCC" })).toEqual({
      name: "Bob", color: "#AABBCC", colorLight: "#AABBCC33",
    });
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
