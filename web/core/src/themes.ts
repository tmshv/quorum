import type { Extension } from "@codemirror/state";
import { coolGlow, rosePineDawn } from "thememirror";

export type ThemeId = "cool-glow" | "rose-pine-dawn";

export interface Chrome {
  bg: string;
  fg: string;
  muted: string;
  accent: string;
  border: string;
  panel: string;
}

export interface ThemeDef {
  id: ThemeId;
  label: string;
  dark: boolean;
  extension: Extension;
  chrome: Chrome;
}

export const DEFAULT_THEME: ThemeId = "cool-glow";
const THEME_KEY = "quorum:theme";

export const THEMES: readonly ThemeDef[] = [
  {
    id: "cool-glow",
    label: "Cool Glow",
    dark: true,
    extension: coolGlow,
    // Copied from node_modules/thememirror/dist/themes/cool-glow.js `settings`.
    chrome: {
      bg: "#060521",       // background
      fg: "#E0E0E0",       // foreground
      muted: "#E0E0E090",  // gutterForeground
      accent: "#FFFFFFA6", // caret
      border: "#122BBB",   // selection
      panel: "#FFFFFF0F",  // lineHighlight
    },
  },
  {
    id: "rose-pine-dawn",
    label: "Rosé Pine Dawn",
    dark: false,
    extension: rosePineDawn,
    // Copied from node_modules/thememirror/dist/themes/rose-pine-dawn.js `settings`.
    chrome: {
      bg: "#faf4ed",       // background
      fg: "#575279",       // foreground
      muted: "#57527970",  // gutterForeground
      accent: "#575279",   // caret
      border: "#6e6a8614", // selection
      panel: "#6e6a860d",  // lineHighlight
    },
  },
];

export function themeById(id: unknown): ThemeDef {
  return THEMES.find((t) => t.id === id) ?? THEMES[0]!;
}

export function loadThemePref(storage: Storage | null): ThemeId {
  try {
    return themeById(storage?.getItem(THEME_KEY)).id;
  } catch {
    return DEFAULT_THEME;
  }
}

export function saveThemePref(id: ThemeId, storage: Storage | null): void {
  try {
    storage?.setItem(THEME_KEY, id);
  } catch {
    // storage unavailable or full; preference is best-effort
  }
}
