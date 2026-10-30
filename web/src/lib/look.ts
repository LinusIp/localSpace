// The look behind the `themes` switch (docs/design/0.2.1-THEMES-AND-
// TRANSLATION.md §2): one of three palettes, light or dark or the same as
// Windows. ui.css holds the colours; this sets <html data-palette> and
// <html data-theme> for them. With the switch off neither is set, and the
// app keeps the look it had.

import { featureOn } from "./features.ts";

export type Palette = "indigo" | "violet" | "teal";
export type Mode = "system" | "light" | "dark";

export const PALETTES: readonly Palette[] = ["indigo", "violet", "teal"];
export const MODES: readonly Mode[] = ["system", "light", "dark"];

/** What <html> carries for a palette and mode; "system" follows Windows. */
export function lookAttributes(palette: Palette, mode: Mode, systemDark: boolean): { palette: Palette; theme: "light" | "dark" } {
  const theme = mode === "system" ? (systemDark ? "dark" : "light") : mode;
  return { palette, theme };
}

/** A stored value read as one of a list, or the first of the list. */
function oneOf<T extends string>(value: string | null, list: readonly T[]): T {
  return (list as readonly string[]).includes(value ?? "") ? (value as T) : list[0];
}

function stored(key: string): string | null {
  try {
    return globalThis.localStorage?.getItem(key) ?? null;
  } catch {
    return null;
  }
}

/**
 * Puts the look on the page when `themes` is on, and keeps "same as Windows"
 * in step when Windows changes mode. Until Settings → Appearance exists, the
 * palette and mode are read from localStorage ("ls.palette", "ls.mode").
 */
export function startLook(root: HTMLElement = document.documentElement): void {
  if (!featureOn("themes")) return;
  const palette = oneOf(stored("ls.palette"), PALETTES);
  const mode = oneOf(stored("ls.mode"), MODES);
  const dark = window.matchMedia("(prefers-color-scheme: dark)");
  const apply = () => {
    const look = lookAttributes(palette, mode, dark.matches);
    root.dataset.palette = look.palette;
    root.dataset.theme = look.theme;
  };
  apply();
  dark.addEventListener("change", apply);
}
