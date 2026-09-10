// The iframe host for a `web` view (v2 §6.3). The frame is on the harness's
// own origin with a sandbox; this component is the shell's end of the bridge:
// it answers the surface's hello with the document, forwards the document
// again whenever Core changes it (to a surface holding a replica, Core's sync
// messages for that replica, under the name this frame gave it), carries the
// surface's writes and messages to Core, and passes the shell's commands in.

import { useEffect, useRef, useState } from "react";
import { ApiError, call, openSurface, pick } from "../api/client";
import { bus, themeTokens } from "../surfaces/bus";
import { useSession } from "../store";
import type { Panel } from "../store";

const PROTOCOL = 1;

/** A name for one replica in Core, unique across frames, tabs and reloads. `getRandomValues` needs no secure context. */
function replicaName(): string {
  return Array.from(crypto.getRandomValues(new Uint8Array(8)), (b) => b.toString(16).padStart(2, "0")).join("");
}

type FromSurface =
  | { ls: number; type: "hello"; protocol: number; wants?: "json" | "sync" }
  | { ls: number; type: "write"; doc: unknown; commit?: boolean; seq?: number }
  | { ls: number; type: "sync"; message: Uint8Array | number[] }
  | { ls: number; type: "send"; payload: number[] }
  | { ls: number; type: "action"; name: string }
  | { ls: number; type: "status"; zoom?: number }
  | { ls: number; type: "log"; level: string; text: string };

export function HarnessFrame({ panel, active }: { panel: Panel; active: boolean }) {
  const frame = useRef<HTMLIFrameElement>(null);
  const [target, setTarget] = useState<{ url: string; origin: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const docId = useRef<string | null>(null);
  const ready = useRef(false);
  /** The highest write number Core has answered, stamped on every document sent in. */
  const written = useRef(0);
  /** Whether the surface holds a replica (sync messages) or takes JSON documents. */
  const wants = useRef<"json" | "sync">("json");
  /** The replica's name in Core while the frame holds one: its sync state there is its own. */
  const peer = useRef<string | null>(null);
  const notify = useSession((s) => s.notify);
  const trace = useSession((s) => s.traceLine);
  const undo = useSession((s) => s.undo);
  const redo = useSession((s) => s.redo);
  const reportZoom = useSession((s) => s.reportZoom);
  const { harness, view } = panel;

  // A grant for this view, from the server: the URL on the harness origin.
  useEffect(() => {
    let cancelled = false;
    setTarget(null);
    setError(null);
    ready.current = false;
    openSurface(harness, view)
      .then((t) => {
        if (!cancelled) setTarget(t);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(err instanceof ApiError ? err.message : "the server could not be reached");
      });
    return () => {
      cancelled = true;
    };
  }, [harness, view]);

  // The replica's sync state in Core lasts as long as the frame: it ends when
  // the panel closes or the frame is given a new grant.
  useEffect(() => {
    if (!target) return;
    return () => {
      const doc = docId.current;
      const name = peer.current;
      peer.current = null;
      if (doc && name) void call({ doc_sync_end: { doc, peer: name } }).catch(() => undefined);
    };
  }, [target]);

  // The bridge itself.
  useEffect(() => {
    if (!target) return;
    const { origin } = target;
    const post = (message: Record<string, unknown>) => {
      frame.current?.contentWindow?.postMessage({ ls: PROTOCOL, ...message }, origin);
    };
    const fetchDoc = async (): Promise<unknown> => {
      const opened = pick(await call({ get_doc_json: { harness } }), "doc_json");
      if (!opened) return null;
      docId.current = opened.doc;
      return opened.json;
    };
    const failed = (what: string, err: unknown) =>
      notify("error", `${what}: ${err instanceof ApiError ? err.message : "the server could not be reached"}`);

    const onMessage = (e: MessageEvent<FromSurface>) => {
      if (e.source !== frame.current?.contentWindow || e.origin !== origin) return;
      const message = e.data;
      if (!message || message.ls !== PROTOCOL) return;
      switch (message.type) {
        case "hello": {
          wants.current = message.wants === "sync" ? "sync" : "json";
          // A hello after the first is a reloaded frame: its old replica is gone.
          const old = peer.current;
          if (old && docId.current) void call({ doc_sync_end: { doc: docId.current, peer: old } }).catch(() => undefined);
          peer.current = wants.current === "sync" ? replicaName() : null;
          // A replica gets Core's Automerge snapshot as well as the JSON; the
          // sync messages that follow keep it current.
          const opening =
            wants.current === "sync"
              ? call({ open_doc: { harness } }).then((r) => {
                  const opened = pick(r, "doc_opened");
                  if (opened) docId.current = opened.doc;
                  return opened?.snapshot ?? null;
                })
              : Promise.resolve(null);
          void Promise.all([fetchDoc(), opening])
            .then(([doc, snapshot]) => {
              post({ type: "init", harness, view, doc, snapshot, theme: themeTokens(), focused: active });
              ready.current = true;
            })
            .catch((err: unknown) => failed(`${panel.title} could not open its document`, err));
          break;
        }
        case "sync": {
          const id = docId.current;
          const name = peer.current;
          if (!id || !name) break;
          const bytes = message.message instanceof Uint8Array ? Array.from(message.message) : message.message;
          void call({ doc_sync: { doc: id, peer: name, message: bytes } }).catch((err: unknown) => failed(`${panel.title} could not sync its document`, err));
          break;
        }
        case "write": {
          // After Core has taken the write, the surface gets the document
          // back stamped with this write's number, so it can tell a document
          // read before the write from one that reflects it.
          const seq = typeof message.seq === "number" ? message.seq : 0;
          void call({
            write_doc: { harness, view, doc: message.doc as never, commit: message.commit !== false },
          })
            .then(async () => {
              written.current = Math.max(written.current, seq);
              const doc = await fetchDoc();
              post({ type: "doc", doc, written: written.current });
            })
            .catch((err: unknown) => failed(`${panel.title} could not write its document`, err));
          break;
        }
        case "send": {
          const payload = Array.isArray(message.payload) ? message.payload : [];
          void call({ harness_event: { harness, view, payload } }).catch((err: unknown) =>
            failed(`${panel.title} could not reach its logic`, err),
          );
          break;
        }
        case "action":
          // Undo and redo are the environment's: the history, not the surface.
          if (message.name === "undo") void undo();
          else if (message.name === "redo") void redo();
          break;
        case "status":
          if (typeof message.zoom === "number") reportZoom(panel.key, message.zoom);
          break;
        case "log":
          trace(`[${harness}/${view}] ${message.text}`);
          break;
        default:
          break;
      }
    };
    window.addEventListener("message", onMessage);
    const off = [
      bus.on("doc_patch", ({ doc, peer: to, message }) => {
        if (ready.current && doc === docId.current && to === peer.current) post({ type: "sync", message });
      }),
      bus.on("doc_changed", ({ doc }) => {
        if (!ready.current || doc !== docId.current || wants.current === "sync") return;
        const stamp = written.current;
        void fetchDoc()
          .then((next) => post({ type: "doc", doc: next, written: stamp }))
          .catch(() => undefined);
      }),
      bus.on("harness_message", (m) => {
        if (m.harness === harness && m.view === view) post({ type: "message", payload: m.payload });
      }),
      bus.on("command", (c) => {
        if (c.harness === harness && c.view === view) post({ type: "command", name: c.name, args: c.args });
      }),
    ];
    return () => {
      window.removeEventListener("message", onMessage);
      for (const f of off) f();
    };
  }, [target, harness, view, panel.key, panel.title, active, notify, trace, undo, redo, reportZoom]);

  // Focus follows the active tab.
  useEffect(() => {
    if (!target || !ready.current) return;
    frame.current?.contentWindow?.postMessage({ ls: PROTOCOL, type: "focus", focused: active }, target.origin);
  }, [active, target]);

  if (error) {
    return (
      <div className="ls-empty">
        <p className="ls-danger">{error}</p>
        <p className="ls-mt-1 ls-small ls-faint" style={{ maxWidth: "28rem" }}>
          The view {view} of {harness} could not be opened on its own origin.
        </p>
      </div>
    );
  }
  if (!target) {
    return <div className="ls-empty ls-muted">Opening…</div>;
  }
  return (
    <iframe
      ref={frame}
      src={target.url}
      title={panel.title}
      // Its own origin, so `allow-same-origin` keeps the harness's cookies
      // and storage on that origin and away from the shell's. Nothing else.
      sandbox="allow-scripts allow-same-origin"
      referrerPolicy="no-referrer"
      style={{ width: "100%", height: "100%", border: 0, background: "var(--ls-page)", display: "block" }}
    />
  );
}
