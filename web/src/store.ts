// Application state (v2 §6.1: Zustand for the app, Automerge for content).
// The environment is whatever Core last said it was; events keep it current.

import { create } from "zustand";
import type { EnvironmentState, Event, NoticeLevel } from "./api/generated";
import type { Me } from "./api/client";

export type Notice = { level: NoticeLevel; text: string; at: number };

export type Session = {
  me: Me | null;
  environment: EnvironmentState | null;
  notices: Notice[];
  live: boolean;
  signIn: (me: Me) => void;
  signOut: () => void;
  setEnvironment: (environment: EnvironmentState) => void;
  setLive: (live: boolean) => void;
  onEvent: (event: Event) => void;
};

const KEEP = 50;

export const useSession = create<Session>((set) => ({
  me: null,
  environment: null,
  notices: [],
  live: false,
  signIn: (me) => set({ me }),
  signOut: () => set({ me: null, environment: null, notices: [], live: false }),
  setEnvironment: (environment) => set({ environment }),
  setLive: (live) => set({ live }),
  onEvent: (event) =>
    set((state) => {
      if (typeof event === "string") return {};
      if ("environment_changed" in event) return { environment: event.environment_changed };
      if ("notice" in event) {
        const notice = { ...event.notice, at: Date.now() };
        return { notices: [...state.notices.slice(1 - KEEP), notice] };
      }
      return {};
    }),
}));
