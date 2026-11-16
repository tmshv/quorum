import { describe, expect, it } from "vitest";
import { yUndoManagerKeymap } from "y-codemirror.next";

describe("undo keymap", () => {
  it("yUndoManagerKeymap binds Mod-z, Mod-y and Mod-Shift-z", () => {
    const keys = yUndoManagerKeymap.map((k) => k.key);
    expect(keys).toEqual(expect.arrayContaining(["Mod-z", "Mod-y", "Mod-Shift-z"]));
  });
});
