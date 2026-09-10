// @localspace/harness-sdk — what a web surface runs against (architecture v2
// §6.3). This module runs inside the harness's iframe, on the harness's own
// origin, in a sandbox with scripts and nothing else. The bridge to the shell
// is the only capability it has: the document, one opaque message channel to
// the harness's logic in Core, the theme, focus, and commands from the shell.
// A surface that needs a file, a fetch, a model call or another harness's
// document asks its logic, which asks Core, which checks the manifest.
//
// Protocol, over postMessage, every message `{ ls: 1, type, ... }`:
//   surface → shell   hello · send {payload} · write {doc} · log {level, text}
//   shell → surface   init {harness, view, doc, theme, focused} · doc {doc}
//                     message {payload} · focus {focused} · command {name, args}

const PROTOCOL = 1;

/**
 * Open the bridge. Resolves once the shell has answered with the document.
 * @returns {Promise<Harness>}
 */
export function connect() {
  return new Promise((resolve, reject) => {
    if (window.parent === window) {
      reject(new Error("this surface is not inside the localSpace shell"));
      return;
    }
    const listeners = new Map();
    let doc = null;
    let focused = false;
    let theme = {};
    let harness = "";
    let view = "";

    const emit = (event, value) => {
      for (const fn of listeners.get(event) ?? []) {
        try {
          fn(value);
        } catch (err) {
          post({ type: "log", level: "error", text: String(err) });
        }
      }
    };
    const post = (msg) => window.parent.postMessage({ ls: PROTOCOL, ...msg }, "*");

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
      /** The harness document as Core holds it, as JSON. */
      doc() {
        return doc;
      },
      /** Replace the document. Core reconciles it field by field and commits. */
      write(next) {
        doc = next;
        post({ type: "write", doc: next });
      },
      /** A message to this harness's logic in Core, at most 64 KB. */
      send(payload) {
        const bytes = payload instanceof Uint8Array ? Array.from(payload) : Array.from(new TextEncoder().encode(typeof payload === "string" ? payload : JSON.stringify(payload)));
        if (bytes.length > 64 * 1024) throw new Error("a surface message may not exceed 64 KB");
        post({ type: "send", payload: bytes });
      },
      /** "doc", "message", "focus", "command" */
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
          theme = msg.theme ?? {};
          focused = !!msg.focused;
          applyTheme(theme);
          resolve(handle);
          break;
        case "doc":
          doc = msg.doc;
          emit("doc", doc);
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
        case "command":
          emit("command", { name: msg.name, args: msg.args });
          break;
        default:
          break;
      }
    });
    post({ type: "hello", protocol: PROTOCOL });
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
