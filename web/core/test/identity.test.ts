import { describe, expect, it } from "vitest";
import { identityKey, loadIdentity, saveIdentity } from "../src/index";

class MemoryStorage implements Storage {
  private m = new Map<string, string>();
  get length() { return this.m.size; }
  clear() { this.m.clear(); }
  getItem(k: string) { return this.m.get(k) ?? null; }
  key(i: number) { return [...this.m.keys()][i] ?? null; }
  removeItem(k: string) { this.m.delete(k); }
  setItem(k: string, v: string) { this.m.set(k, v); }
}

describe("identity", () => {
  it("is stable for the same room", () => {
    const s = new MemoryStorage();
    expect(loadIdentity("r1", s)).toEqual(loadIdentity("r1", s));
  });

  it("uses a different key per room", () => {
    const s = new MemoryStorage();
    loadIdentity("r1", s);
    loadIdentity("r2", s);
    expect(s.length).toBe(2);
    expect(identityKey("r1")).toBe("quorum:identity:r1");
    expect(s.getItem(identityKey("r2"))).not.toBeNull();
  });

  it("works with null storage", () => {
    const id = loadIdentity("r1", null);
    expect(id.name).toBeTruthy();
    expect(id.edited).toBe(false);
    saveIdentity("r1", id, null);
  });

  it("replaces corrupted JSON", () => {
    const s = new MemoryStorage();
    s.setItem(identityKey("r1"), "{nope");
    const id = loadIdentity("r1", s);
    expect(id.name).toBeTruthy();
    expect(JSON.parse(s.getItem(identityKey("r1"))!)).toEqual(id);
  });

  it("round-trips a saved identity", () => {
    const s = new MemoryStorage();
    const id = { name: "Mine", color: "#ffcf5a", edited: true };
    saveIdentity("r1", id, s);
    expect(loadIdentity("r1", s)).toEqual(id);
  });

  it("swallows storage exceptions", () => {
    const bad = { getItem() { throw new Error("x"); }, setItem() { throw new Error("x"); } } as unknown as Storage;
    expect(loadIdentity("r1", bad).name).toBeTruthy();
    expect(() => saveIdentity("r1", { name: "a", color: "#fff", edited: false }, bad)).not.toThrow();
  });

  it("never uses localStorage", () => {
    expect((globalThis as { localStorage?: unknown }).localStorage).toBeUndefined();
  });
});
