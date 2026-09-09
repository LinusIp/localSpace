// The client's only door to Core (architecture v2 §5): `/api/v1` for request
// and response, `/ws/json` for the event stream. The types come from
// `generated/`, written by `cargo test -p localspace-proto`; nothing here is
// typed by hand.

import type { Envelope, Event, Request, Response } from "./generated";

export type Me = {
  user: string;
  topology: "personal" | "organisation";
  version: string;
  harness_api: string;
};

export class ApiError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

async function parse<T>(r: globalThis.Response): Promise<T> {
  if (!r.ok) {
    let detail = r.statusText || `HTTP ${r.status}`;
    try {
      const body = (await r.json()) as { error?: unknown };
      if (typeof body.error === "string") detail = body.error;
    } catch {
      // not JSON; the status text will do
    }
    throw new ApiError(r.status, detail);
  }
  return (await r.json()) as T;
}

const json = (body: unknown): RequestInit => ({
  method: "POST",
  credentials: "same-origin",
  headers: { "content-type": "application/json" },
  body: JSON.stringify(body),
});

export const login = (token: string) =>
  fetch("/api/v1/login", json({ token })).then((r) => parse<{ user: string }>(r));

export const logout = () =>
  fetch("/api/v1/logout", { method: "POST", credentials: "same-origin" }).then(() => undefined);

export const me = () => fetch("/api/v1/me", { credentials: "same-origin" }).then((r) => parse<Me>(r));

/** Any request, its response. Core checks every one; the client is never trusted. */
export const call = (request: Request) =>
  fetch("/api/v1/request", json(request)).then((r) => parse<Response>(r));

/**
 * The event stream. Reconnects with backoff until `stop` is called; `onLive`
 * says whether the socket is up so the shell can show it.
 */
export function events(onEvent: (event: Event) => void, onLive: (live: boolean) => void): () => void {
  const scheme = location.protocol === "https:" ? "wss" : "ws";
  const url = `${scheme}://${location.host}/ws/json`;
  let socket: WebSocket | null = null;
  let stopped = false;
  let delay = 1000;

  const open = () => {
    socket = new WebSocket(url);
    socket.onopen = () => {
      delay = 1000;
      onLive(true);
    };
    socket.onmessage = (message: MessageEvent<string>) => {
      const envelope = JSON.parse(message.data) as Envelope;
      if ("Event" in envelope.body) onEvent(envelope.body.Event);
    };
    socket.onclose = () => {
      onLive(false);
      if (!stopped) {
        setTimeout(open, delay);
        delay = Math.min(delay * 2, 15000);
      }
    };
  };
  open();
  return () => {
    stopped = true;
    socket?.close();
  };
}
