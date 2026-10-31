// The look behind the `themes` switch (docs/design/0.2.1-THEMES-AND-
// TRANSLATION.md §2): one of three palettes, light or dark or the same as
// Windows. ui.css holds the colours; this sets <html data-palette> and
// <html data-theme> for them. With the switch off neither is set, and the
// app keeps the look it had. Core holds the switch and the person's choice
// (preferences `theme` and `mode`); this browser keeps the last look it
// showed, so a page drawn before Core answers (the sign-in page, a reload)
// starts in it rather than flashing the old one.

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
export function oneOf<T extends string>(value: string | null | undefined, list: readonly T[]): T {
  return (list as readonly string[]).includes(value ?? "") ? (value as T) : list[0];
}

/** The look to draw: off, or a palette and a mode. */
export type Look = { on: false } | { on: true; palette: Palette; mode: Mode };

const KEY = "ls.look";

/** The look this browser showed last, if it kept one. */
export function rememberedLook(): Look {
  try {
    const kept = JSON.parse(globalThis.localStorage?.getItem(KEY) ?? "null") as { palette?: string; mode?: string } | null;
    return kept ? { on: true, palette: oneOf(kept.palette, PALETTES), mode: oneOf(kept.mode, MODES) } : { on: false };
  } catch {
    return { on: false };
  }
}

function remember(look: Look): void {
  try {
    if (look.on) globalThis.localStorage?.setItem(KEY, JSON.stringify({ palette: look.palette, mode: look.mode }));
    else globalThis.localStorage?.removeItem(KEY);
  } catch {
    // a browser that keeps nothing starts from the old look until Core answers
  }
}

const darkQuery = () => window.matchMedia("(prefers-color-scheme: dark)");
let following: (() => void) | null = null;

/** Puts `look` on the page, keeps "same as Windows" in step, and remembers it. */
export function applyLook(look: Look, root: HTMLElement = document.documentElement): void {
  if (following) darkQuery().removeEventListener("change", following);
  following = null;
  remember(look);
  if (!look.on) {
    delete root.dataset.palette;
    delete root.dataset.theme;
    return;
  }
  const dark = darkQuery();
  const draw = () => {
    const attributes = lookAttributes(look.palette, look.mode, dark.matches);
    root.dataset.palette = attributes.palette;
    root.dataset.theme = attributes.theme;
  };
  draw();
  if (look.mode === "system") {
    following = draw;
    dark.addEventListener("change", draw);
  }
}
