/** No keyboard or pointer input for this long marks the tab away. */
export const IDLE_MS = 120_000;
const CHECK_MS = 5_000;
const INPUT_EVENTS = ["keydown", "pointerdown", "pointermove", "wheel", "focus"] as const;

export interface PresenceEnv {
  win: Pick<EventTarget, "addEventListener" | "removeEventListener">;
  doc: EventTarget & { readonly visibilityState: string };
  now: () => number;
}

/**
 * Calls `setAway(true)` while the tab is hidden or idle for `idleMs`, and
 * `setAway(false)` as soon as it is visible and used again. Only changes are
 * reported. Returns the stop function.
 */
export function trackPresence(
  setAway: (away: boolean) => void,
  env: PresenceEnv = { win: window, doc: document, now: Date.now },
  idleMs = IDLE_MS,
): () => void {
  const { win, doc, now } = env;
  let last = now();
  let away: boolean | null = null;

  const refresh = () => {
    const next = doc.visibilityState === "hidden" || now() - last >= idleMs;
    if (next === away) return;
    away = next;
    setAway(next);
  };
  const onInput = () => {
    last = now();
    if (away) refresh();
  };

  for (const ev of INPUT_EVENTS) win.addEventListener(ev, onInput, { passive: true });
  doc.addEventListener("visibilitychange", refresh);
  const timer = setInterval(refresh, Math.min(CHECK_MS, idleMs));
  refresh();

  return () => {
    clearInterval(timer);
    for (const ev of INPUT_EVENTS) win.removeEventListener(ev, onInput);
    doc.removeEventListener("visibilitychange", refresh);
  };
}
