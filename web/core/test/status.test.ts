import { describe, expect, it } from "vitest";
import { CLOSE, statusFromClose } from "../src/status";

describe("statusFromClose", () => {
  const cases: [number, string, boolean, boolean][] = [
    [CLOSE.BAD_ROOM, "bad-room", true, false],
    [CLOSE.ROOM_FULL, "room-full", false, true],
    [CLOSE.BUSY, "busy", false, true],
    [CLOSE.DOC_TOO_LARGE, "too-large", true, false],
    [CLOSE.RATE_LIMITED, "rate-limited", false, true],
    [CLOSE.TOO_SLOW, "reconnecting", false, false],
    [CLOSE.PROTOCOL, "error", true, false],
    [1006, "reconnecting", false, false],
    [1000, "reconnecting", false, false],
  ];
  for (const [code, status, fatal, retryable] of cases) {
    it(`maps ${code} to ${status}`, () => {
      expect(statusFromClose(code)).toEqual({ status, fatal, retryable });
    });
  }
});

import { INITIAL_STATUS, nextStatus } from "../src/status";

describe("nextStatus", () => {
  it("stays connecting until first sync, not on socket events", () => {
    let s = nextStatus(INITIAL_STATUS, { type: "connecting" });
    expect(s.status).toBe("connecting");
    s = nextStatus(s, { type: "sync" });
    expect(s.status).toBe("connected");
  });
  it("maps provider connecting to reconnecting once settled", () => {
    let s = nextStatus(INITIAL_STATUS, { type: "sync" });
    s = nextStatus(s, { type: "disconnected" });
    expect(s.status).toBe("reconnecting");
    s = nextStatus(s, { type: "connecting" });
    expect(s.status).toBe("reconnecting");
    s = nextStatus(s, { type: "sync" });
    expect(s.status).toBe("connected");
  });
  it("reconnecting after a TOO_SLOW close", () => {
    let s = nextStatus(INITIAL_STATUS, { type: "close", code: CLOSE.TOO_SLOW });
    s = nextStatus(s, { type: "disconnected" });
    s = nextStatus(s, { type: "connecting" });
    expect(s.status).toBe("reconnecting");
  });
  it("keeps a stopped status through later provider events", () => {
    let s = nextStatus(INITIAL_STATUS, { type: "close", code: CLOSE.ROOM_FULL });
    s = nextStatus(s, { type: "disconnected" });
    s = nextStatus(s, { type: "connecting" });
    expect(s.status).toBe("room-full");
  });
  it("retry returns to connecting", () => {
    let s = nextStatus(INITIAL_STATUS, { type: "close", code: CLOSE.BUSY });
    s = nextStatus(s, { type: "retry" });
    expect(s.status).toBe("connecting");
    s = nextStatus(s, { type: "connecting" });
    expect(s.status).toBe("connecting");
  });
});
