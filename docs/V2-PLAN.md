# Architecture v2 against the code as it stands

Written 2026-09-09 at commit 6bf0b2d, from `localspace-architecture-v2.md`.
v2 keeps the plugin contract, the organisation deployment and the marketplace
specs, and replaces the client, the surface ABI, the inference backend and the
build order. This file says, section by section, what already exists, what v2
retires, what is new, and what has to be decided before the first step.

---

## 1. The decision v2 makes

| Area | Today | v2 |
|---|---|---|
| Client | egui on wgpu, Rust, one crate for native and browser | TypeScript, React 19, Vite, in a webview: Tauri 2 on the desktop, a browser in organisation mode |
| Harness surfaces | wasm modules with their own egui, painted by the host through a mesh ABI | sandboxed iframes on their own origin, a bridge SDK, an ES module bundle per harness |
| Inference | one `ModelWorker` over an OpenAI-compatible HTTP endpoint | `llama-server` sidecars supervised by Core, placement plan turned into flags |
| Documents | Automerge in Core, JSON projection to surfaces | Automerge in Core and in the client (`@automerge/automerge-repo`) |
| Transport | `Request`/`Response`/`Event` as postcard over one WebSocket | REST for request/response, JSON WebSocket for events, sync and tokens; OpenAPI and TypeScript generated from `localspace-proto` |
| Budgets | 50 MB for Client and Core each | Core ≤ 50 MB; the webview's 150–300 MB accepted; shell JS heap ≤ 80 MB; base bundle ≤ 2 MB |
| What ships | whiteboard installed at start, planner in the catalog | chat only; every harness downloaded on demand |

This reverses the decision recorded earlier the same day to keep egui. v2 §14
supersedes plugin spec §2–3, §5, §13 and §14 explicitly, so the document is
the newer instruction; §7 below lists what needs confirming.

---

## 2. What stays: Rust below the API

About 13,900 lines. None of it is touched by the stack change except at its
edges.

| Crate or module | State | Edge v2 touches |
|---|---|---|
| `localspace-proto` (47 types) | complete | derive `JsonSchema` and `TS`; the `Json` newtype existed for postcard and becomes plain `serde_json::Value` on a JSON wire |
| `core::dag`, `docs` (Automerge, reconcile, sync) | complete, tested | Core must speak `automerge-repo`'s sync protocol, or the client gets a small custom network adapter; the sync messages themselves are the ones `docs.rs` already produces |
| `core::registry`, `manifest`, `catalog`, `deps`, `lock` | complete | a `web` view kind with `module = "ui/index.js"`; `surface_mb` instead of `memory_mb.surface` |
| `core::exposure`, `grammar`, `prompt`, `agent`, `task` | complete | none |
| `core::runtime::wasm` (logic components, budgets, idle unload) | complete | AOT compile at install, which the cache already half does |
| `core::runtime::native` (Tier B) | no sandbox | v2 §6.4a wants sandbox, GPU pool, state streaming |
| `core::acl`, `audit`, `gateway`, `planner`, `footprint` | complete | none |
| `core::model` (`ModelWorker`, grammar field) | complete | already speaks `/chat/completions` with `grammar`, which is `llama-server`'s API |
| `localspace-server` (axum: `/ws`, `/healthz`, `/readyz`, `/metrics`, an OpenAPI stub) | partial | REST routes, JSON framing, serve the bundle at `/`, sessions, OIDC |
| `localspace-harness-sdk`, `whiteboard-logic` (20 tools), `planner-logic` | complete | none |

The Core budget v2 keeps is met: 5 MB private with a harness instantiated.

---

## 3. What v2 retires

About 6,900 lines, all of them the egui side.

| Crate | Lines | Replaced by |
|---|---|---|
| `localspace-client` | 4,172 | the TypeScript shell |
| `localspace-surface-sdk` | 527 | `@localspace/harness-sdk` and the iframe bridge |
| `harnesses/whiteboard-surface` | 1,829 | a web bundle on tldraw or Konva |
| `localspace-desktop` (eframe) | 414 | a Tauri 2 crate |

These keep running until the web client reaches parity, then go. What was
learned in them carries over as rules for the bridge: a surface has a memory
budget and is restarted once when it breaks it; a surface is entered only when
something changed; only what is on screen is drawn.

---

## 4. What is new

- **TypeScript client**: React, Zustand, Tailwind with a token file, dockview,
  TanStack Query, Radix, Monaco where needed; login, environment switcher,
  panel layout, chat, store, settings, admin console.
- **Tauri 2 desktop crate**: Core and the server in-process on loopback,
  sidecar supervision, native dialogs, hardware probing, auto-update.
- **Bridge SDK and iframe hosting**: per-harness origins, CSP, the handshake,
  `doc()`, `send()`, `snapshot()`, commands; a cap of six open panels.
- **Generated API**: `schemars` and `ts-rs` on `proto`, OpenAPI at
  `/api/v1/openapi.json` replacing the stub, `@localspace/api` built in CI.
- **REST and JSON WebSocket** beside, then instead of, the postcard socket.
- **`llama-server` sidecars**: supervision, placement plan to flags, prompt
  cache slots, the model catalog with download and offline import.
- **Chat harness UI**, the ledger view, proposals, citations.
- **Whiteboard web surface** on tldraw or Konva, against the existing logic.
- **OIDC and sessions**, unchanged as a gap from before.

---

## 5. Build order §13 against today

| Step | Exists | Partial | New | Rough size |
|---|---|---|---|---|
| 1 proto + Core + axum + generated TS; Tauri and `serve` boot to a login | proto, Core, axum skeleton | OpenAPI stub, no REST | TS generation, REST, JSON WS, Vite shell, Tauri crate, local-token login | 1–2 weeks |
| 2 llama.cpp sidecar + planner + catalog download | **done**: the supervisor, the plan as flags, the catalog with verdicts, download and import, the Models page; verified live with llama.cpp b10869 and a 0.5B model (87 tokens/s on the laptop, evals 3 of 6) | the 15 tok/s gate needs a W32 machine and the 120B model | the utility model and speculative decoding of §4.3 | the gate |
| 3 chat harness | **done** except citations, which come with retrieval in step 6: the GUI, token streaming from the sidecar, persisted conversations | | | |
| 4 harness runtime with iframe surfaces | manifest, logic components, install and uninstall, local registry | `widgets` kind stays | `web` kind, bridge SDK, iframe host, CSP | 1–2 weeks |
| 5 whiteboard as the first package | logic, tools, context provider, evals, Automerge document; `@localspace/canvas`, `@localspace/ui`, the web surface with a replica in the frame, undo as a forward change, the gate walk as a browser test (2026-09-10) | | the gate number on a W32 machine; the CI check; snapping and SVG/PNG export | days, once a W32 machine is at hand |
| 6 retrieval, upload, gateway, web tools | gateway modes, `web.search` and `web.fetch` | BM25 for `find_capability` only | tantivy, usearch, ACL pre-filter, upload | 2 weeks |
| 7 multi-user `serve` | ACL, workspaces, proposals, audit, Core-side sync | one local user always | OIDC, sessions, browser-to-browser sync | 2 weeks |
| 8 Tier B runtime, physics harness, handoff bench | MCP subprocess, handoff, ledger | no sandbox | sandbox, GPU pool, state streaming, the engine | large |
| 9 CAD harness | | | everything | large |
| 10 registry service, signing, entitlements | kinds, dependencies, lock | | protocol, signing, org catalog | 2 weeks |

Steps 1–5 are the MVP: five to eight weeks of focused work before the
whiteboard gate ("install from catalog, agent puts a plan on the board, user
edits it live, undo through the DAG") can be run again on the new stack. That
gate passes today on the egui stack; v2 re-earns it.

---

## 6. This machine

- Node 24.19 and npm 11.17 are installed. pnpm, yarn and bun are not.
- `cargo-tauri` is not installed. The WebView2 runtime did not answer at the
  standard registry key; Windows 11 normally ships it with Edge, so verify
  before step 1 rather than assume.
- `schemars`, `ts-rs` and `tauri` are not in the local crate cache: the first
  `cargo fetch` needs the network.
- `llama-server` is not installed; step 2 starts with a build or a download.
- The display driver fault found on 2026-09-08 (AMD integrated GPU, problem
  code 31, panel on the Basic Display Driver) is below every stack. WebView2
  presents through the same DXGI path, so the v2 client will show the same six
  frames a second on this laptop until the driver is repaired.

---

## 7. Decisions taken (2026-09-09)

1. **v2 stands**, in full and as written.
2. **tldraw** for the whiteboard surface, as v2 §6.4 lists first; the licence
   is to be budgeted before step 5. *Superseded on 2026-09-10 by architecture
   v2.1 (§11 below): the canvas is built in-house.*
3. **The egui client stays in the tree until the web client reaches parity.**
   The server keeps answering its postcard socket at `/ws`.

## 8. Step 1, done the same day

Additively, nothing deleted: `JsonSchema` and `TS` derives on every proto
type, with the `Json` newtype carrying the value itself on a JSON wire and its
text on postcard; TypeScript bindings generated by `cargo test -p
localspace-proto` into `web/src/api/generated/`; `localspace-server` as a
library with a token login, a session cookie or bearer, `POST /api/v1/request`
for any request, `GET` routes for the common reads, `/ws/json` streaming
events and answering requests under their id, OpenAPI generated from the
schemas, and the web bundle served at `/`; a React shell in `web/` that signs
in and shows the environment; `localspace-app` in `crates/localspace-shell`,
Tauri 2, with Core and the server in one process on a loopback port and the
window signed in through the token it generated. Both boot to a login and an
empty shell.

## 9. The main GUI, the same day

The shell is the chat-centred layout of the design handed over on 2026-09-09:
a rail with Chat, Agents, Tools, Models, Data, History and Library, Settings
and Help below; a top bar with the workspace, the model, canvas zoom
(disabled until a canvas panel exists), readiness, settings and the user; the
chat with the agent's tool calls inline, approval cards and a composer with
stop; a right column with the active model, tools as switches, the context
and recent changes. Every page shows Core's own state through the JSON API
and every control is one request; nothing on screen is decoration. The
readiness pill says "No model" until one is connected in Models, because
that is the truth of a fresh install. Next is step 2, the llama.cpp sidecar,
which is what turns that pill green without a hand-connected endpoint.

## 10. Step 4, the harness runtime with iframe surfaces (2026-09-10)

The `web` view kind exists end to end. In the manifest it is `kind = "web"`
with `module` naming an `index.js`; the directory of that file is all the
harness's origin serves, and Core refuses any path that leaves it. The
server gives every harness its own origin, `h-<slug>.localhost:<port>`,
chosen by the `Host` header in front of every other route; the shell asks
`POST /api/v1/surfaces` for a view and receives a URL carrying a grant as
the first path segment of everything the frame loads (a cookie would not
do: a browser withholds cookies from a third-party frame). The page on that
origin is generated: an import map for `@localspace/harness-sdk`
under a per-response nonce, the entry module, and a Content-Security-Policy
that admits scripts and connections from the origin itself and frames from
the one shell that opened the view. The SDK is `connect()`, `doc()`,
`write()`, `send()`, `on("doc" | "message" | "focus" | "command")`, `theme`,
over `postMessage` with a protocol number. The shell hosts the frames as
panels beside the chat, six at most, keeps them mounted behind their tabs,
answers hello with the document, forwards the document again on every
`doc_patch`, carries writes to Core's new `WriteDoc` (reconciled into the
Automerge document and committed as `surface:<view>` by the user) and
messages to `harness_event`, and sends zoom from the top bar as a command.
The `widgets` kind is rendered by the shell from the logic's tree. `egui`
and `stream` panels say plainly that the web shell does not run them. The
whiteboard package carries a small `web` view to prove the loop; step 5
replaces it with tldraw. Not in this step: Automerge in the client (the
frame refetches JSON on each patch), a memory budget per frame (browsers do
not expose one), a policy header on the shell page itself, and the
`snapshot()` call of §6.3.

## 11. Architecture v2.1 and step 5 (2026-09-10)

Mid step 5 the architecture was revised to v2.1 (`docs/localspace-architecture-v2.md`),
and the coding-agent prompt became `CLAUDE.md`. Principle 4 now reads "own the
product, stand on foundations": the canvas engine, the 3D viewport, physics,
the sketcher, the UI component library, docking, the app store and the fetch
client are in-house; React, Vite, Automerge, tokio, axum, wgpu, wasmtime and
llama.cpp are the foundations. tldraw, Excalidraw, Konva, Three.js, Rapier,
Tailwind, Radix, Zustand, TanStack, dockview and Monaco are out. Step 5 is
now **`@localspace/canvas` and the whiteboard as the first downloadable
package**, with a gate of install from the catalog, the agent's plan on the
board, live editing at 60 fps with 5,000 shapes, undo through the DAG.

What had been built on tldraw that morning is parked on the branch
`spike/tldraw` as a reference for the bridge semantics; nothing on `master`
imports from it. It had found two things worth keeping: tldraw 5 allows
unlicensed use in development environments only and hides its editor on a
production build, and it calls `cdn.tldraw.com`, which the deployment spec's
no-outbound rule forbids anyway. The decisions taken on the questions this
raised are in `docs/DECISIONS.md`.

Step 5 under v2.1, as approved: `@localspace/canvas` (camera, retained scene
graph, R-tree culling, hit-testing, selection and handles, text, the seven
whiteboard shape kinds, undo through Core; snapping and export after the
gate; WebGPU later) with its benchmark and tests from the first commit;
`@localspace/ui` for what the shell needs today (tokens, buttons, inputs,
menus, dialogs, icons, docking) and the shell moved onto it, off Tailwind,
Radix, Zustand, lucide-react and react-markdown's components; the
whiteboard's surface on the canvas with `@automerge/automerge` in the frame
over an own network adapter to Core and an own IndexedDB storage adapter;
the gate measured on the W32 machine, a frame-time regression check in CI,
the catalog-to-undo walk driven in a real browser through playwright-core
(approved, dev-only).

Landed the same day before the canvas work, each as one commit: catalog
installs persist under the data directory; a document write without a
commit and numbered writes on the bridge; only chat in the box by default in
the desktop shell; `data:` in the harness origin's connect-src; the `native`
view kind reserved in the contract; the workspace's lints, `deny(unsafe_code)`
and SAFETY comments; one output module for the CLI; the TanStack dependency
gone; a binary that writes the TypeScript bindings where a fresh test
executable is refused.

Landed later the same day: `@localspace/canvas` with its tests and
benchmark; `@localspace/ui` with the shell on it; React, the canvas, the UI
library and Automerge served on every harness origin through the import
map; the replica channel in the SDK and Core; the whiteboard's web surface
on the canvas with the document as a replica in the frame; undo, redo and
run drop as forward changes, so a connected replica follows them; the gate
walk as a browser test, which passes on the review laptop with a 0.5B model
(the agent's note, a live edit, two undos and two redos through Core's
history, zoom both ways). The numbers are in `docs/STATUS.md`. Open: the
W32 measurement, the CI check, snapping and export, the storage adapter
(`docs/DECISIONS.md` and the step report's questions).
