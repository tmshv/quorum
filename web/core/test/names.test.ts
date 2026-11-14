import { describe, expect, it } from "vitest";
import { NAMES, PALETTE, pickColor, pickName, sanitizeName } from "../src/index";

describe("names", () => {
  it("NAMES has 68 unique ascii entries", () => {
    expect(NAMES).toHaveLength(68);
    expect(new Set(NAMES).size).toBe(68);
    for (const n of NAMES) expect(n).toMatch(/^[\x20-\x7e]+$/);
  });

  it("pickName avoids taken", () => {
    const taken = new Set(NAMES.filter((n) => n !== "Deadlock"));
    expect(pickName(taken)).toBe("Deadlock");
  });

  it("pickName falls back when all taken", () => {
    expect(NAMES).toContain(pickName(new Set(NAMES)));
  });

  it("pickColor avoids taken", () => {
    const taken = new Set(PALETTE.slice(1));
    expect(pickColor(taken)).toBe(PALETTE[0]);
    expect(PALETTE).toContain(pickColor(new Set(PALETTE)));
  });

  it("sanitizeName", () => {
    expect(sanitizeName("  Kernel\u0000  Panic ")).toBe("Kernel Panic");
    expect(sanitizeName("‮evil")).toBe("evil");
    expect(sanitizeName("a".repeat(40))).toBe("a".repeat(32));
    expect(sanitizeName("   ")).toBe("anonymous");
  });
});
