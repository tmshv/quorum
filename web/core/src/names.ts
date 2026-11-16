export type Rng = () => number;

export const NAMES: readonly string[] = [
  "Not Found", "Sandbox Violation", "Kernel Panic", "Side Effect", "Memory Leak",
  "Round Robin", "Access Denied", "Access Granted", "Read Only", "Attention Layer",
  "Null Pointer Exception", "Undefined", "Undefined Behavior", "Deadlock",
  "Race Condition", "Consensus Algorithm", "Lost Context", "Not a Number",
  "Bad Gateway", "Gateway Timeout", "Segmentation Fault", "Random Forest",
  "Decision Tree", "Gradient Descent", "Vanishing Gradient", "Overfitting",
  "Hallucination", "Prompt Injection", "Context Window", "Off By One",
  "Infinite Loop", "Stack Overflow", "Buffer Overflow", "Heap Corruption",
  "Dangling Pointer", "Zombie Process", "Orphan Process", "Fork Bomb", "Core Dump",
  "Cache Miss", "Cache Invalidation", "Merge Conflict", "Detached Head",
  "Force Push", "Magic Number", "Spaghetti Code", "Technical Debt", "Rounding Error",
  "Bit Flip", "Garbage Collector", "Cold Start", "Edge Case", "Rubber Duck",
  "Byzantine General", "Split Brain", "Eventual Consistency", "Dirty Read",
  "Phantom Read", "Lazy Evaluation", "Busy Wait", "Spin Lock", "Callback Hell",
  "Unhandled Rejection", "Promise Pending", "Syntax Error", "Type Mismatch",
  "Heisenbug", "Works On My Machine",
];

export const PALETTE: readonly string[] = [
  "#ffcf5a", "#ff7b6b", "#7ee0a1", "#c3a6ff", "#ff9fd1", "#7fd1ff", "#ffb36b", "#9be7e0",
];

function pickFree(all: readonly string[], taken: ReadonlySet<string>, rng: Rng): string {
  const free = all.filter((x) => !taken.has(x));
  const pool = free.length > 0 ? free : all;
  return pool[Math.min(Math.floor(rng() * pool.length), pool.length - 1)]!;
}

export function pickName(taken: ReadonlySet<string>, rng: Rng = Math.random): string {
  return pickFree(NAMES, taken, rng);
}

export function pickColor(taken: ReadonlySet<string>, rng: Rng = Math.random): string {
  return pickFree(PALETTE, taken, rng);
}

export function sanitizeName(raw: string): string {
  const clean = raw
    .replace(/[\p{Cc}\p{Cf}]/gu, "")
    .replace(/\s+/g, " ")
    .trim();
  const capped = Array.from(clean).slice(0, 32).join("").trim();
  return capped === "" ? "anonymous" : capped;
}
