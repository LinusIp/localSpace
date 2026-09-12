// The client's only door to Core (architecture v2 §5): `/api/v1` for request
// and response, `/ws/json` for the event stream. The types come from
// `generated/`, written by `cargo test -p localspace-proto`; nothing here is
// typed by hand.

import type { Artifact, DocumentInfo, Envelope, Event, Request, Response } from "./generated";

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

/**
 * A grant to show one `web` view in an iframe: the URL on the harness's own
 * origin, valid for this user (v2 §6.3).
 */
export const openSurface = (harness: string, view: string) =>
  fetch("/api/v1/surfaces", json({ harness, view })).then((r) => parse<{ url: string; origin: string }>(r));

/**
 * Any request, its response. Core checks every one; the client is never
 * trusted. A response of `{ error }` is raised as an `ApiError`, so callers
 * see one kind of failure.
 */
export async function call(request: Request): Promise<Response> {
  const response = await fetch("/api/v1/request", json(request)).then((r) => parse<Response>(r));
  if (typeof response !== "string" && "error" in response) {
    throw new ApiError(400, response.error.message);
  }
  return response;
}

type Variant = Exclude<Response, string>;
type VariantKey = Variant extends infer V ? (V extends object ? keyof V : never) : never;

/** The one variant a caller wanted, or null when Core answered otherwise. */
export function pick<K extends VariantKey>(
  response: Response,
  key: K,
): Extract<Variant, Record<K, unknown>>[K] | null {
  if (typeof response === "string" || !(key in response)) return null;
  return (response as Extract<Variant, Record<K, unknown>>)[key];
}

/**
 * A file a surface made from its harness's document, for Core to keep as a
 * document of its own and register as a typed artifact (6.0). The file is
 * the body of one upload; the rest rides in the query, so the bytes are
 * never copied into JSON.
 */
export async function produceArtifact(
  params: { harness: string; view: string; kind: string; name: string; mime: string; fields: Record<string, unknown>; summary: string },
  bytes: ArrayBuffer,
): Promise<Artifact> {
  const query = new URLSearchParams({
    harness: params.harness,
    view: params.view,
    kind: params.kind,
    name: params.name,
    fields: JSON.stringify(params.fields),
    summary: params.summary,
  });
  const response = await fetch(`/api/v1/artifacts?${query}`, {
    method: "POST",
    credentials: "same-origin",
    headers: { "content-type": params.mime },
    body: bytes,
  }).then((r) => parse<Response>(r));
  if (typeof response !== "string" && "error" in response) throw new ApiError(400, response.error.message);
  const artifact = pick(response, "artifact");
  if (!artifact) throw new ApiError(500, "Core answered without an artifact");
  return artifact;
}

/** Every document the session may see: each harness's, and the files of their own. */
export const documents = (): Promise<DocumentInfo[]> => call("list_documents").then((r) => pick(r, "documents")?.documents ?? []);

/**
 * Save a file document through the browser: fetched with the session, then
 * handed over as a download under its name. The server sends every file as
 * an attachment; this is how the shell's own origin offers it.
 */
export async function downloadDocument(id: string, name: string): Promise<void> {
  const r = await fetch(`/api/v1/documents/${encodeURIComponent(id)}/content`, { credentials: "same-origin" });
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
  const url = URL.createObjectURL(await r.blob());
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.rel = "noopener";
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
}

/** `118 KB`, `2.4 MB`: a size as a person reads it. */
export function bytesLabel(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

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
