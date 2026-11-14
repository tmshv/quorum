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
