// A small store for app state (architecture v2.1 §6.1: "a small own
// store"). One object, `set` to change part of it, `get` to read it
// outside React, and a hook that re-renders a component when the part it
// selects changes, on React's own `useSyncExternalStore`.

import { useSyncExternalStore } from "react";

export interface StoreApi<S> {
  get: () => S;
  set: (partial: Partial<S> | ((state: S) => Partial<S>)) => void;
  subscribe: (listener: () => void) => () => void;
}

export interface Store<S> extends StoreApi<S> {
  /** The hook: the whole state, or the part a selector picks. */
  <T = S>(selector?: (state: S) => T): T;
}

export function createStore<S extends object>(init: (set: StoreApi<S>["set"], get: StoreApi<S>["get"]) => S): Store<S> {
  let state: S;
  const listeners = new Set<() => void>();
  const get = () => state;
  const set: StoreApi<S>["set"] = (partial) => {
    const next = typeof partial === "function" ? partial(state) : partial;
    let changed = false;
    for (const key in next) {
      if (!Object.is(next[key], state[key])) {
        changed = true;
        break;
      }
    }
    if (!changed) return;
    state = { ...state, ...next };
    for (const fn of listeners) fn();
  };
  const subscribe = (fn: () => void) => {
    listeners.add(fn);
    return () => {
      listeners.delete(fn);
    };
  };
  state = init(set, get);
  const identity = (s: S) => s as unknown;
  const hook = (<T,>(selector?: (state: S) => T): T =>
    useSyncExternalStore(subscribe, () => (selector ?? (identity as (s: S) => T))(state), () => (selector ?? (identity as (s: S) => T))(state))) as Store<S>;
  hook.get = get;
  hook.set = set;
  hook.subscribe = subscribe;
  return hook;
}

/** Re-exported name for readability at call sites: `const value = useStore(store, s => s.x)`. */
export function useStore<S, T>(store: Store<S>, selector: (state: S) => T): T {
  return store(selector);
}
