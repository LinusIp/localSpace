// The iframe host for a `web` view (v2 §6.3). The frame is on the harness's
// own origin with a sandbox; this component is the shell's end of the bridge:
// it answers the surface's hello with the document, forwards the document
// again whenever Core changes it, carries the surface's writes and messages
// to Core, and passes the shell's commands in.

import { useEffect, useRef, useState } from "react";
import { ApiError, call, openSurface, pick } from "../api/client";
import { bus, themeTokens } from "../surfaces/bus";
import { useSession } from "../store";
import type { Panel } from "../store";

const PROTOCOL = 1;

type FromSurface =
  | { ls: number; type: "hello"; protocol: number }
  | { ls: number; type: "write"; doc: unknown; commit?: boolean; seq?: number }
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
        case "hello":
          void fetchDoc()
            .then((doc) => {
              post({ type: "init", harness, view, doc, theme: themeTokens(), focused: active });
              ready.current = true;
            })
            .catch((err: unknown) => failed(`${panel.title} could not open its document`, err));
          break;
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
      bus.on("doc_patch", ({ doc }) => {
        if (!ready.current || doc !== docId.current) return;
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
      <div className="flex h-full flex-col items-center justify-center p-8 text-center">
        <p className="text-sm text-danger">{error}</p>
        <p className="mt-1 max-w-md text-xs text-faint">
          The view {view} of {harness} could not be opened on its own origin.
        </p>
      </div>
    );
  }
  if (!target) {
    return <div className="flex h-full items-center justify-center text-sm text-muted">Opening…</div>;
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
      className="h-full w-full border-0 bg-page"
    />
  );
}
