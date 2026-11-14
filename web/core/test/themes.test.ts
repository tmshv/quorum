import { describe, expect, it } from "vitest";
import {
  DEFAULT_THEME, THEMES, loadThemePref, saveThemePref, themeById,
} from "../src/index";

class MemoryStorage implements Storage {
  private m = new Map<string, string>();
  get length() { return this.m.size; }
  clear() { this.m.clear(); }
  getItem(k: string) { return this.m.get(k) ?? null; }
  key(i: number) { return [...this.m.keys()][i] ?? null; }
  removeItem(k: string) { this.m.delete(k); }
  setItem(k: string, v: string) { this.m.set(k, v); }
}

describe("themes", () => {
  it("has two themes, one dark one light", () => {
    expect(THEMES.map((t) => t.id)).toEqual(["cool-glow", "rose-pine-dawn"]);
    expect(THEMES.map((t) => t.dark)).toEqual([true, false]);
  });

  it("themeById falls back to cool-glow", () => {
    expect(DEFAULT_THEME).toBe("cool-glow");
    expect(themeById(undefined).id).toBe("cool-glow");
    expect(themeById("rose-pine-dawn").id).toBe("rose-pine-dawn");
  });

  it("round-trips the preference via storage", () => {
    const s = new MemoryStorage();
    saveThemePref("rose-pine-dawn", s);
    expect(s.getItem("quorum:theme")).toBe("rose-pine-dawn");
    expect(loadThemePref(s)).toBe("rose-pine-dawn");
  });

  it("falls back on garbage and null storage", () => {
    const s = new MemoryStorage();
    expect(loadThemePref(s)).toBe("cool-glow");
    s.setItem("quorum:theme", "neon");
    expect(loadThemePref(s)).toBe("cool-glow");
    expect(loadThemePref(null)).toBe("cool-glow");
    expect(() => saveThemePref("cool-glow", null)).not.toThrow();
  });
});
