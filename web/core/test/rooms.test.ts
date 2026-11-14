import { describe, expect, it } from "vitest";
import { NAMES, ROOM_RE, normalizeRoom, randomRoom, slugify } from "../src/index";

describe("rooms", () => {
  it("normalizeRoom", () => {
    expect(normalizeRoom("Kernel-Panic-42")).toBe("kernel-panic-42");
    expect(normalizeRoom("kernel%20panic")).toBe("kernel-panic");
    expect(normalizeRoom("--a--")).toBe("a");
    expect(normalizeRoom("%E0%A4%A")).toBe("e0-a4-a");
    expect(normalizeRoom("a".repeat(60))).toBe("a".repeat(40));
  });

  it("randomRoom matches ROOM_RE", () => {
    for (let i = 0; i < 1000; i++) expect(randomRoom()).toMatch(ROOM_RE);
  });

  it("every slugified name matches ROOM_RE", () => {
    for (const n of NAMES) expect(slugify(n)).toMatch(ROOM_RE);
  });
});
