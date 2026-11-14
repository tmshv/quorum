import type { ThemeDef } from "@quorum/core";

/** Publish the theme's chrome colours as CSS variables; all page styles read only these. */
export function applyChrome(theme: ThemeDef, root: HTMLElement): void {
  const { chrome } = theme;
  root.style.setProperty("--bg", chrome.bg);
  root.style.setProperty("--fg", chrome.fg);
  root.style.setProperty("--muted", chrome.muted);
  root.style.setProperty("--accent", chrome.accent);
  root.style.setProperty("--border", chrome.border);
  root.style.setProperty("--panel", chrome.panel);
  root.style.colorScheme = theme.dark ? "dark" : "light";
}
