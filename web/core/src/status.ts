export const CLOSE = {
  BAD_ROOM: 4000,
  ROOM_FULL: 4001,
  BUSY: 4002,
  DOC_TOO_LARGE: 4003,
  RATE_LIMITED: 4004,
  TOO_SLOW: 4005,
  PROTOCOL: 4006,
} as const;

export type Status =
  | "connecting"
  | "connected"
  | "reconnecting"
  | "room-full"
  | "busy"
  | "too-large"
  | "rate-limited"
  | "bad-room"
  | "error";

export interface CloseStatus {
  status: Status;
  /** Reload required; retrying will not help. */
  fatal: boolean;
  /** Stopped; the user may click Retry. */
  retryable: boolean;
}

export function statusFromClose(code: number): CloseStatus {
  switch (code) {
    case CLOSE.BAD_ROOM:
      return { status: "bad-room", fatal: true, retryable: false };
    case CLOSE.ROOM_FULL:
      return { status: "room-full", fatal: false, retryable: true };
    case CLOSE.BUSY:
      return { status: "busy", fatal: false, retryable: true };
    case CLOSE.DOC_TOO_LARGE:
      return { status: "too-large", fatal: true, retryable: false };
    case CLOSE.RATE_LIMITED:
      return { status: "rate-limited", fatal: false, retryable: true };
    case CLOSE.PROTOCOL:
      return { status: "error", fatal: true, retryable: false };
    default:
      // TOO_SLOW and any non-application code: the provider auto-reconnects.
      return { status: "reconnecting", fatal: false, retryable: false };
  }
}

export interface StatusState {
  status: Status;
  /** Has been connected or closed at least once since the last (re)start. */
  settled: boolean;
  /** Closed with a code that stops the provider (fatal or retryable). */
  stopped: boolean;
}

export type StatusEvent =
  | { type: "connecting" }
  | { type: "disconnected" }
  | { type: "sync" }
  | { type: "close"; code: number }
  | { type: "retry" };

export const INITIAL_STATUS: StatusState = { status: "connecting", settled: false, stopped: false };

/**
 * "connected" is reported on first sync, not socket open: a rejected join opens
 * the socket just before the server closes it with 4001/4002.
 */
export function nextStatus(prev: StatusState, ev: StatusEvent): StatusState {
  switch (ev.type) {
    case "retry":
      return INITIAL_STATUS;
    case "sync":
      return { status: "connected", settled: true, stopped: false };
    case "close": {
      const s = statusFromClose(ev.code);
      return { status: s.status, settled: true, stopped: s.fatal || s.retryable };
    }
    case "connecting":
      if (prev.stopped) return prev;
      return prev.settled ? { ...prev, status: "reconnecting" } : prev;
    case "disconnected":
      if (prev.stopped) return prev;
      return { ...prev, status: "reconnecting", settled: true };
  }
}
