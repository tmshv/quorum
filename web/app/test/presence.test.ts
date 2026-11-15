import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { trackPresence } from "../src/presence";

function env() {
  const win = new EventTarget();
  const doc = Object.assign(new EventTarget(), { visibilityState: "visible" });
  return { win, doc, now: () => Date.now() };
}

describe("trackPresence", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("starts active and goes away after the idle period", () => {
    const calls: boolean[] = [];
    const stop = trackPresence((a) => calls.push(a), env(), 60_000);
    expect(calls).toEqual([false]);
    vi.advanceTimersByTime(59_000);
    expect(calls).toEqual([false]);
    vi.advanceTimersByTime(5_000);
    expect(calls).toEqual([false, true]);
    stop();
  });

  it("comes back on input and reports only changes", () => {
    const e = env();
    const calls: boolean[] = [];
    const stop = trackPresence((a) => calls.push(a), e, 60_000);
    vi.advanceTimersByTime(65_000);
    e.win.dispatchEvent(new Event("keydown"));
    e.win.dispatchEvent(new Event("pointermove"));
    expect(calls).toEqual([false, true, false]);
    stop();
  });

  it("is away while the tab is hidden, whatever the input", () => {
    const e = env();
    const calls: boolean[] = [];
    const stop = trackPresence((a) => calls.push(a), e, 60_000);
    e.doc.visibilityState = "hidden";
    e.doc.dispatchEvent(new Event("visibilitychange"));
    e.win.dispatchEvent(new Event("keydown"));
    expect(calls).toEqual([false, true]);
    e.doc.visibilityState = "visible";
    e.doc.dispatchEvent(new Event("visibilitychange"));
    expect(calls).toEqual([false, true, false]);
    stop();
  });

  it("stops listening after stop()", () => {
    const e = env();
    const calls: boolean[] = [];
    trackPresence((a) => calls.push(a), e, 60_000)();
    vi.advanceTimersByTime(120_000);
    expect(calls).toEqual([false]);
  });
});
