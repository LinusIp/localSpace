// What passes between the shell and its open surfaces (v2 §6.3). Core's
// events arrive through the store; the frames and widget views that care
// subscribe here, and the shell's own commands to a surface go the same way.

import type { Widget } from "../api/generated";

export type BusEvents = {
  /** A document changed in Core; the frames showing it fetch it again. */
  doc_patch: { doc: string };
  /** A harness's logic sent its surface a message. */
  harness_message: { harness: string; view: string; payload: number[] };
  /** A `widgets` view was redrawn by its logic. */
  widget_view_changed: { harness: string; view: string; root: Widget };
  /** The shell telling a surface to do something: zoom, fit. */
  command: { harness: string; view: string; name: string; args: unknown };
};

type Listener<K extends keyof BusEvents> = (value: BusEvents[K]) => void;

const listeners = new Map<keyof BusEvents, Set<Listener<keyof BusEvents>>>();

export const bus = {
  on<K extends keyof BusEvents>(event: K, fn: Listener<K>): () => void {
    let set = listeners.get(event);
    if (!set) {
      set = new Set();
      listeners.set(event, set);
    }
    set.add(fn as Listener<keyof BusEvents>);
    return () => {
      set?.delete(fn as Listener<keyof BusEvents>);
    };
  },
  emit<K extends keyof BusEvents>(event: K, value: BusEvents[K]): void {
    for (const fn of listeners.get(event) ?? []) {
      try {
        (fn as Listener<K>)(value);
      } catch (err) {
        console.error(`surface bus: a listener for ${event} failed`, err);
      }
    }
  },
};

/** The shell's tokens, read from the stylesheet, as a surface receives them. */
export function themeTokens(): Record<string, string> {
  const style = getComputedStyle(document.documentElement);
  const names = [
    "page",
    "ink",
    "muted",
    "faint",
    "line",
    "accent",
    "accent-strong",
    "accent-soft",
    "warn",
    "warn-soft",
    "danger",
    "danger-soft",
  ];
  const tokens: Record<string, string> = {};
  for (const name of names) {
    const value = style.getPropertyValue(`--color-${name}`).trim();
    if (value) tokens[name] = value;
  }
  const sans = style.getPropertyValue("--font-sans").trim();
  const mono = style.getPropertyValue("--font-mono").trim();
  if (sans) tokens["font-sans"] = sans;
  if (mono) tokens["font-mono"] = mono;
  return tokens;
}
