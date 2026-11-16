import { describe, expect, it } from "vitest";
import { parseRoute, serverUrl } from "../src/route";

describe("parseRoute", () => {
  it("maps / to home", () => {
    expect(parseRoute("/")).toEqual({ kind: "home" });
  });
  it("normalises a room path", () => {
    expect(parseRoute("/Kernel-Panic-42")).toEqual({
      kind: "room",
      room: "kernel-panic-42",
      canonical: "/kernel-panic-42",
    });
  });
  it("ignores a trailing slash", () => {
    expect(parseRoute("/kernel-panic-42/")).toEqual({
      kind: "room",
      room: "kernel-panic-42",
      canonical: "/kernel-panic-42",
    });
  });
  it("maps an empty normalisation to home", () => {
    expect(parseRoute("/%%%")).toEqual({ kind: "home" });
  });
});

describe("serverUrl", () => {
  it("uses wss for https", () => {
    expect(serverUrl({ protocol: "https:", host: "h" } as Location)).toBe("wss://h/ws");
  });
  it("uses ws for http", () => {
    expect(serverUrl({ protocol: "http:", host: "h" } as Location)).toBe("ws://h/ws");
  });
});
