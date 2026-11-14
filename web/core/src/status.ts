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
