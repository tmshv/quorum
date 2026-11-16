import { describe, expect, it } from "vitest";
import type { Person } from "@quorum/core";
import { diffPeople } from "../src/people";

const p = (clientId: number, name: string, self = false): Person => ({ clientId, name, color: "#000000", self, away: false });

describe("diffPeople", () => {
  it("reports joiners and leavers by clientId", () => {
    const prev = [p(1, "Me", true), p(2, "A")];
    const next = [p(1, "Me", true), p(3, "B")];
    const d = diffPeople(prev, next);
    expect(d.joined.map((x) => x.name)).toEqual(["B"]);
    expect(d.left.map((x) => x.name)).toEqual(["A"]);
  });
  it("ignores self and renames", () => {
    const d = diffPeople([p(1, "Me", true), p(2, "A")], [p(1, "New", true), p(2, "A2")]);
    expect(d).toEqual({ joined: [], left: [] });
  });
  it("reports a first joiner into a lone room", () => {
    expect(diffPeople([p(1, "Me", true)], [p(1, "Me", true), p(2, "A")]).joined).toHaveLength(1);
  });
});
