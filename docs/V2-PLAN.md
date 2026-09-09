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
| 2 llama.cpp sidecar + planner + catalog download | planner, worker protocol | plan is not executed | supervision, flags, catalog, download, the 15 tok/s gate | 1 week, then the gate |
| 3 chat harness | agent loop, ledger, grammar, streaming in Core | | the React UI | 1 week |
| 4 harness runtime with iframe surfaces | manifest, logic components, install and uninstall, local registry | `widgets` kind stays | `web` kind, bridge SDK, iframe host, CSP | 1–2 weeks |
| 5 whiteboard as the first package | logic, tools, context provider, evals, Automerge document | | the surface bundle; the tldraw decision | 1–2 weeks |
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

## 7. Decisions before step 1

1. **The reversal.** The last instruction in the session was to keep egui;
   v2 replaces it. v2 is the newer document and says so in §14, but the word
   should be given, not inferred.
2. **tldraw or Konva.** v2 §15 says decide before step 5, and it decides more
   than a licence: on Konva the existing document schema and the 20 tools
   carry over unchanged and the surface is a port; on tldraw the document
   becomes tldraw's store and the tools are rewritten against it.
3. **The egui client's end.** Keep it running until parity, which costs
   nothing but leaving the crates in the tree, or freeze it now.

The recommended first slice, if v2 stands: step 1 done additively. Derives on
`proto` and TypeScript generation from `cargo test`; REST and a JSON socket
beside the postcard one; a Vite shell that logs in with the local token and
lists the environment, served by `localspace serve` at `/` and loaded by a
Tauri shell. Nothing is deleted, the egui client keeps working, and the
generated types are the first artifact the two sides share.
