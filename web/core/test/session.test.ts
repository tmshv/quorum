import { describe, expect, it } from "vitest";
import * as Y from "yjs";
import { ensureDefaultLang, resolveClash } from "../src/session";
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
