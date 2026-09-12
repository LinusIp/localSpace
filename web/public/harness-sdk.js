// @localspace/harness-sdk — what a web surface runs against (architecture v2
// §6.3). This module runs inside the harness's iframe, on the harness's own
// origin, in a sandbox with scripts and nothing else. The bridge to the shell
// is the only capability it has: the document, one opaque message channel to
// the harness's logic in Core, the theme, focus, and commands from the shell.
// A surface that needs a file, a fetch, a model call or another harness's
// document asks its logic, which asks Core, which checks the manifest.
//
// Two ways to hold the document. As JSON: `doc()` and `write()`, the whole
// document each way, for small documents and simple surfaces. As a replica:
// `connect({ sync: true })` hands the surface Core's Automerge snapshot and a
// sync channel; the surface keeps its own copy, changes it, and exchanges
// sync messages with Core, which commits what arrives.
//
// Protocol, over postMessage, every message `{ ls: 1, type, ... }`:
//   surface → shell   hello {protocol, wants} · send {payload} · write {doc, commit, seq}
//                     sync {message} · action {name} · status {zoom} · log {level, text}
//                     artifact {kind, name, mime, fields, summary, bytes}
//   shell → surface   init {harness, view, doc, snapshot, theme, focused} · doc {doc, written}
//                     sync {message} · message {payload} · focus {focused} · command {name, args}
//                     artifact {ok, id, name, bytes, error}
//
// `seq` numbers the surface's writes; `written` on a document says which of
// them Core had taken in before that document was read, so a surface can
// tell a document that predates its latest write from one that reflects it.

const PROTOCOL = 1;

/**
 * Open the bridge. Resolves once the shell has answered with the document.
 * `{ sync: true }` asks for the Automerge snapshot and sync messages instead
 * of JSON documents.
 * @returns {Promise<Harness>}
 */
export function connect(options) {
  const wants = options && options.sync ? "sync" : "json";
  return new Promise((resolve, reject) => {
    if (window.parent === window) {
      reject(new Error("this surface is not inside the localSpace shell"));
      return;
    }
    const listeners = new Map();
    let doc = null;
    let snapshot = null;
    let focused = false;
    let theme = {};
    let harness = "";
    let view = "";
    let seq = 0;

    const post = (msg, transfer) => window.parent.postMessage({ ls: PROTOCOL, ...msg }, "*", transfer);
    const emit = (event, value, meta) => {
      for (const fn of listeners.get(event) ?? []) {
        try {
          fn(value, meta);
        } catch (err) {
          post({ type: "log", level: "error", text: String(err) });
        }
      }
    };

    const handle = {
      get harness() {
        return harness;
      },
      get view() {
        return view;
      },
      get theme() {
        return theme;
      },
      get focused() {
        return focused;
      },
      /** The number of the last write sent. */
      get lastWrite() {
        return seq;
      },
      /** The harness document as Core holds it, as JSON. */
      doc() {
        return doc;
      },
      /** Core's Automerge document, saved, when connected with `sync: true`. */
      snapshot() {
        return snapshot;
      },
      /**
       * Replace the document. Core reconciles it field by field and commits
       * the difference as the user's edit. `{ commit: false }` moves the
       * document without a commit: for state that is the user's but not an
       * edit, such as the selection, so undo steps over it. Returns the
       * write's number; documents that arrive with `written` below it were
       * read before this write landed.
       */
      write(next, options) {
        doc = next;
        seq += 1;
        post({ type: "write", doc: next, commit: !(options && options.commit === false), seq });
        return seq;
      },
      /** An Automerge sync message for Core, from the surface's replica. */
      sync(message) {
        const bytes = message instanceof Uint8Array ? message : new Uint8Array(message);
        post({ type: "sync", message: bytes });
      },
      /** A message to this harness's logic in Core, at most 64 KB. */
      send(payload) {
        const bytes =
          payload instanceof Uint8Array
            ? Array.from(payload)
            : Array.from(new TextEncoder().encode(typeof payload === "string" ? payload : JSON.stringify(payload)));
        if (bytes.length > 64 * 1024) throw new Error("a surface message may not exceed 64 KB");
        post({ type: "send", payload: bytes });
      },
      /** Undo and redo are the environment's, through the history, not the surface's. */
      undo() {
        post({ type: "action", name: "undo" });
      },
      redo() {
        post({ type: "action", name: "redo" });
      },
      /** Tell the shell what the surface shows, so its controls stay true: `{ zoom }`. */
      report(status) {
        post({ type: "status", ...status });
      },
      /**
       * A file rendered from this harness's document — a PNG or an SVG of a
       * board — for Core to keep as a document of its own and register as a
       * typed artifact of `kind` (plugin spec §18.3). `name` is the stem;
       * Core adds the commit it shows and the kind's extension. `bytes` is
       * transferred, not copied. The shell answers with an "artifact" event:
       * `{ ok, id, name, bytes }`, or `{ ok: false, error }`.
       */
      export(artifact) {
        const b = artifact.bytes;
        const buffer = b instanceof Uint8Array ? b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength) : b;
        post(
          {
            type: "artifact",
            kind: artifact.kind,
            name: artifact.name,
            mime: artifact.mime,
            fields: artifact.fields ?? {},
            summary: artifact.summary ?? "",
            bytes: buffer,
          },
          [buffer],
        );
      },
      /** "doc" (with `{ written }`), "sync", "message", "focus", "command", "artifact" */
      on(event, fn) {
        if (!listeners.has(event)) listeners.set(event, new Set());
        listeners.get(event).add(fn);
        return () => listeners.get(event)?.delete(fn);
      },
      log(text) {
        post({ type: "log", level: "info", text: String(text) });
      },
    };

    window.addEventListener("message", (e) => {
      const msg = e.data;
      if (!msg || msg.ls !== PROTOCOL || e.source !== window.parent) return;
      switch (msg.type) {
        case "init":
          harness = msg.harness;
          view = msg.view;
          doc = msg.doc;
          snapshot = msg.snapshot ? new Uint8Array(msg.snapshot) : null;
          theme = msg.theme ?? {};
          focused = !!msg.focused;
          applyTheme(theme);
          resolve(handle);
          break;
        case "doc": {
          const written = typeof msg.written === "number" ? msg.written : Number.POSITIVE_INFINITY;
          if (written >= seq) doc = msg.doc;
          emit("doc", msg.doc, { written });
          break;
        }
        case "sync":
          emit("sync", new Uint8Array(msg.message ?? []));
          break;
        case "message": {
          const bytes = new Uint8Array(msg.payload ?? []);
          emit("message", bytes);
          break;
        }
        case "focus":
          focused = !!msg.focused;
          emit("focus", focused);
          break;
        case "artifact":
          emit("artifact", { ok: msg.ok === true, id: msg.id, name: msg.name, bytes: msg.bytes, error: msg.error });
          break;
        case "command":
          emit("command", { name: msg.name, args: msg.args });
          break;
        default:
          break;
      }
    });
    post({ type: "hello", protocol: PROTOCOL, wants });
  });
}

/** The shell's tokens become CSS variables on the surface's root. */
function applyTheme(theme) {
  const root = document.documentElement;
  for (const [key, value] of Object.entries(theme)) {
    if (typeof value === "string") root.style.setProperty(`--ls-${key}`, value);
  }
}

/** Decode a message from the logic as text, when it is text. */
export function text(bytes) {
  return new TextDecoder().decode(bytes);
}
