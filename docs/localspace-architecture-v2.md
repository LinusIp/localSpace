# localSpace — Technical Architecture v2

Supersedes the client, surface, inference-backend and build-order sections of the Harness Plugin System spec. The plugin contract (tools, context providers, task ledger, inter-harness communication, package management), the Organisation Deployment spec and the Marketplace spec remain in force; §14 lists the adjustments to them.

What is decided here: Rust for the Core, TypeScript for the client, Tauri for the desktop shell, llama.cpp as the inference engine, Automerge on both sides, harness surfaces as sandboxed web bundles downloaded from the catalog. The default install is a chat window; everything else is a package.

---

## 1. Principles

1. **Rust below the API, TypeScript above it.** Core owns state, permissions, models, documents and plugin logic. The client owns pixels and input. The boundary is one HTTP + WebSocket API whose types are generated from the Rust definitions.
2. **The web client is the product; the desktop app is the web client in a shell.** One bundle, served by Core to browsers in organisation mode and loaded by Tauri on a workstation. No feature exists in one and not the other.
3. **Only chat ships in the box.** Whiteboard, physics, CAD, planning and every other harness are downloaded from the catalog on demand and loaded lazily.
4. **Own the product, stand on foundations.** Everything a user sees or an agent drives is built in-house: the whiteboard and every canvas, the 3D viewport renderer, the physics engine, the sketcher and CSG, the planning board, the UI component library, the docking layout, the agent runtime, the harness runtime, the registry, the placement planner. No third-party product SDKs (no tldraw, no Excalidraw, no Rapier, no Three.js). What is *not* rebuilt is the foundation layer that no product team should rewrite: the language runtimes and their standard ecosystems (tokio, serde, axum, React), the GPU abstraction (wgpu — the alternative is three native graphics backends), the WebAssembly runtime (wasmtime), the CRDT (Automerge), the inference kernels (llama.cpp, with the in-house `native` executor planned to replace it once it beats it on the W32 bench). The rule for adding any dependency: if it is a *product* (a canvas, an editor, an engine, a UI kit), build it; if it is *infrastructure* (a runtime, a codec, a driver abstraction, TLS, a parser), use the standard one and wrap it behind a trait so it can be replaced. §6.4 lists what this means per harness.
5. **Nothing calls out.** Offline by default; the gateway (Organisation Deployment §8) is the only socket.

---

## 2. Topology

```
Personal (workstation)                       Organisation (server)
┌───────────────────────────────┐            ┌──────────────────────────────────┐
│ Tauri shell (Rust)            │            │ localspace serve (Rust)          │
│  ├─ system webview            │            │  ├─ axum: /api, /ws, /  (bundle) │
│  │   └─ web client + harness  │            │  ├─ Core                          │
│  │       iframes              │   same     │  ├─ harness logic (wasmtime)      │
│  ├─ Core (in-process)         │  bundle    │  ├─ llama-server sidecars (GPUs)  │
│  ├─ harness logic (wasmtime)  │            │  └─ gateway                       │
│  └─ llama-server sidecar      │            └──────────────────────────────────┘
└───────────────────────────────┘                     ▲ browsers, many users, IdP login
```

In personal mode Core runs inside the Tauri process and the client reaches it over a local WebSocket (loopback, token-authenticated) — the same transport as the server, so there is one code path. Team mode (one workstation, ≤10 users) is `serve` on a W32 machine.

---

## 3. Core (Rust)

Crates:

```
localspace-proto      API types (serde) → OpenAPI + TypeScript via schemars/ts-rs. Single source of truth.
localspace-core       documents, DAG, permissions, scheduler, planner, harness runtime, retrieval, gateway
localspace-server     axum: REST + WebSocket, auth (OIDC), static bundle, admin API, metrics
localspace-desktop    Tauri 2 app: embeds core + server on loopback, sidecar supervision, native dialogs
localspace-harness-sdk  Rust: WIT bindings and helpers for harness logic components
```

Storage: redb for metadata, DAG and sessions; blake3 content-addressed blobs; Automerge documents with incremental saves; tantivy (BM25) + usearch (HNSW) for retrieval; all memory-mapped. Encryption at rest per Organisation Deployment §9.

Runtime: tokio; one task per environment for tool routing and DAG commits (synchronous, in-memory); audit on a separate bounded channel; `tracing` everywhere; Prometheus `/metrics`.

Core budget: ≤ 50 MB private RSS at idle with the reference environment, models excluded, measured in CI. (The client is a webview and is budgeted separately, §6.6.)

---

## 4. Inference

### 4.1 Engine: llama.cpp

`llama-server` runs as a supervised sidecar per model, pinned to its GPUs, talked to over a loopback HTTP/Unix socket. Core never links a Python runtime. Why llama.cpp and nothing else for v1: it already does everything the placement planner needs — per-tensor device placement and CPU MoE expert offload (`--n-cpu-moe`, `-ot`), GBNF grammars for constrained tool calls, speculative decoding (`--model-draft`), prefix/prompt caching with slots, continuous batching, embeddings, CUDA/ROCm/Metal/Vulkan. Behind Core's worker trait so mistral.rs (Rust, multi-GPU TP) or vLLM/SGLang can be added for large servers later without touching callers.

### 4.2 Placement planner

Unchanged from Plugin spec §11.1: given the model's tensor map and the machine, it emits a plan (which tensors on GPU, which experts in RAM, KV precision, context, draft model, VRAM reservations for harnesses) and a verdict — `resident` / `hybrid` / `streaming` / `does not fit` — with an estimated tok/s shown in the catalog before download. The plan becomes `llama-server` flags. Usage statistics per model warm the expert cache on the next load.

Hardware profiles and reference models: Plugin spec §1.1 (W32 32 GB VRAM / 64 GB RAM with 100B+-class MoE at Q4 as the personal reference; W96; S server).

### 4.3 Scheduler and utility model

Organisation Deployment §7.2: priority classes, per-user fairness, prompt-cache-friendly fixed prompt layout, a resident small `utility` model for titles, summaries, reranking, compaction and non-reasoning harness calls, speculative decoding monitored by acceptance rate.

### 4.4 Model catalog

Curated list with pre-computed plans per profile; one-click download from Hugging Face (provisioning egress, separate from runtime egress); offline bundle import for air-gapped sites; licence text stored.

---

## 5. API

- `GET/POST /api/v1/...` JSON over HTTP for request/response; `wss://.../ws` for the event stream, document sync and streaming tokens. JSON on the wire; binary frames (Automerge sync messages, blobs) as WebSocket binary messages.
- OpenAPI generated from `localspace-proto`; the TypeScript client (`@localspace/api`) is generated in CI — no hand-written types on either side.
- Auth: session cookie (browser) or bearer token (desktop, API). OIDC in organisation mode; a local token in personal mode.
- Every mutating call carries an environment id and is permission-checked in Core; the client is never trusted.

---

## 6. Client (TypeScript)

### 6.1 Stack

React 19 + TypeScript + Vite as the rendering runtime and build tool; everything above them is in-house: `@localspace/ui` (components, theme tokens, docking layout, command palette, accessibility handled in our components), a small own store for app state, an own thin fetch/WebSocket client generated from `localspace-proto`. Content state is Automerge documents via `@automerge/automerge` with an own network adapter to Core and an own IndexedDB storage adapter. No Tailwind, no Radix, no Zustand, no TanStack, no dockview, no Monaco.

The shell contains: login, environment switcher, docking layout, the chat harness, the store panel, settings, the admin console (organisation mode). That is the whole base bundle — target ≤ 2 MB compressed.

### 6.2 Desktop shell: Tauri 2

Rust host process; system webview (WebView2 on Windows, WKWebView on macOS, WebKitGTK on Linux). Responsibilities: start Core in-process and `llama-server` sidecars, expose native file dialogs and GPU/hardware probing to the client through Tauri commands, auto-update, installers. The web bundle is identical to the one Core serves to browsers. No Electron, no bundled Chromium.

### 6.3 Harness surfaces: sandboxed iframes

A harness surface is an ES module bundle shipped inside the harness package (§7). The shell mounts it in an `<iframe sandbox="allow-scripts" ...>` on its own origin (`https://h-<harness-id>.<host>` in organisation mode; a custom `harness://` scheme in Tauri) with a strict CSP, so it has no access to the shell's DOM, cookies, storage or network. Everything goes through the bridge:

```ts
// @localspace/harness-sdk (runs inside the iframe)
const h = await connect();              // handshake with the shell
h.doc<BoardDoc>()                       // Automerge handle for this harness's document (replica, synced by Core)
h.theme                                 // tokens: colours, type scale, density
h.on("focus" | "resize" | "message", …) // shell events; "message" = harness/event from logic
h.send(bytes)                           // message to this harness's logic in Core (≤ 64 KB)
h.snapshot(): Promise<Blob>             // rendered image for the agent's `snapshot` tool
h.commands.register(…)                  // command-palette entries, keymap
```

The bridge is the only capability the surface has. A surface that needs a file, a fetch, a model call or another harness's document asks its logic, which asks Core, which checks the manifest. A closed panel unloads its iframe; nothing from an unopened harness is ever loaded.

Four surface kinds: `widgets` (a declarative JSON tree the shell renders itself, for panels without custom UI), `web` (the iframe bundle above — the normal case), `stream` (a `<video>` fed by a Tier B process in Core over WebRTC/WebCodecs), and `native` (a desktop-only overlay window owned by a Tier B engine). The last two, and the state-streaming variant of `web`, are how native GPU harnesses reach the screen — §6.4a.

### 6.4 What the field harnesses are built from — in-house

Two shared in-house libraries carry most of the weight, so each harness stays thin:

- **`@localspace/canvas`** — the 2D infinite-canvas engine: pan/zoom camera, spatial index (R-tree) with viewport culling, retained scene graph rendered on Canvas2D (WebGPU path later), hit-testing, selection and handles, snapping, text layout, freehand smoothing, undo through the Automerge document, export to SVG/PNG. Built once for the whiteboard, reused by the planning board, the sketcher's 2D mode, diagrams and charts.
- **`@localspace/view3d`** — the 3D viewport renderer on WebGPU (WebGL2 fallback): camera and gizmos, mesh/line/point batches uploaded from state buffers, instancing, picking, grid and lighting. It draws what native engines compute (§6.4a); it is not a game engine and stays small.

Both are written in TypeScript against the browser's own APIs (Canvas2D, WebGPU, WebGL2) — no Three.js, no tldraw, no Konva.

| Harness | Surface (own code) | Heavy work (own code, native in Core) |
|---|---|---|
| Whiteboard | `@localspace/canvas`: shapes, arrows, sticky notes, frames, text, freehand | none; Automerge doc |
| Planning board | `@localspace/canvas` + own board/column/card model, timeline view | none; Automerge doc |
| Sketching / CAD | `@localspace/canvas` 2D sketcher with an **own constraint solver** (Newton/Levenberg–Marquardt over geometric constraints); `@localspace/view3d` for the model | **own Tier B kernel in Core** in stages: mesh modelling and CSG first (in-house BSP/voxel-free mesh booleans), then sketch-extrude/revolve on a half-edge mesh, B-rep (NURBS surfaces, trimming, fillets) last — see §15 on why B-rep is a multi-year item; wgpu tessellation and display; exports `geometry.v1` |
| Physics simulation | `@localspace/view3d` viewport; scene setup UI | **own Tier B engine in Core** on wgpu compute (CUDA via `cudarc` only for NVIDIA-specific solvers): broad phase, narrow phase, constraint solver for rigid bodies; particle/SPH fluids; explicit FEM; deterministic stepping and recording; `simulation-result.v1` |
| Chat | shell-native React on the in-house component library | — |
| Data / tables | own virtualised grid on `@localspace/canvas` or DOM; own chart layer on `@localspace/canvas` | `table.v1` |
| Code | own editor (line model, syntax highlighting via tree-sitter grammars) — deferred; not needed for MVP | — |

The client's UI component library (buttons, inputs, menus, dialogs, docking layout, command palette, theme tokens) is also in-house: `@localspace/ui`. React remains the rendering runtime underneath it.

Rule: the surface renders and edits; anything above ~4 GB of memory or more than a few seconds of compute runs in Core.

### 6.4a Native GPU harnesses (physics, CAD, rendering at full performance)

For the harnesses where output quality is the product — a physics simulator, a CAD kernel, a renderer — the engine runs **natively in Core as a Tier B harness**, written in Rust against the GPU directly. This is where "Vulkan-level" performance lives, and it is the intended home for localSpace's own flagship harnesses.

**GPU API: wgpu, not raw Vulkan.** wgpu is Rust's native GPU abstraction over Vulkan, Metal and DX12 with compute shaders in WGSL; it gives Vulkan-class throughput on every OS with one codebase, and the *same* shaders run in the browser under WebGPU, so a harness can ship a lighter in-surface version of its engine for free. Drop to raw Vulkan (`ash`) only for a feature wgpu does not expose — hardware ray tracing, mesh shaders, Vulkan/CUDA interop — and isolate it behind the same trait. For NVIDIA-only solvers that need CUDA libraries (cuBLAS, cuFFT, existing CFD code), `cudarc` is the path; the harness declares `gpu.vendor = "nvidia"` and the catalog says so.

**Three ways the native engine reaches the screen**, chosen per harness and per mode:

| Path | How | Best for | Mode |
|---|---|---|---|
| **State streaming** (default) | Engine computes on the GPU in Core; each frame it writes a compact state buffer (positions, transforms, mesh deltas, tessellated display mesh) into shared memory (desktop) or a binary WebSocket frame (server); the surface renders it with WebGPU/Three.js | rigid-body and particle sims, CAD display meshes, anything where state is far smaller than pixels | both |
| **Video stream** | Engine renders the final image natively; hardware encode (NVENC/AMF/VideoToolbox, H.264/AV1); surface shows it in a `<video>` via WebRTC/WebCodecs; input forwarded back | ray-traced or volumetric rendering, fluids, anything pixel-heavy; server mode always | both, required on server |
| **Native overlay** | Engine renders into its own native window that the Tauri shell positions exactly over the panel rect; zero-copy, native input | maximum-fidelity desktop viewports (large CAD assemblies, real-time rendering) | desktop only |

The harness declares which paths it supports; Core picks by mode and hardware. Input events (pointer, keyboard, gestures) always flow surface → Core → engine with the same message shape, so an engine does not care which path is active. All three record their results into the harness document (`geometry.v1`, `simulation-result.v1`) so the agent, the ledger and other harnesses see the same artifacts regardless of how pixels were delivered.

**Isolation** is Tier B's: the engine is a separate OS process under a sandbox profile with a declared GPU reservation (`gpu = { vram_gb, exclusive }`), assigned from the harness GPU pool — never a model worker's GPU; on W32 it is the VRAM reservation in the placement plan.

**Consequence for the build order:** Tier B and the state-streaming path move into the MVP track (step 8 below) because physics and CAD are flagship harnesses, not add-ons. The video-stream and native-overlay paths follow once the first native engine exists.

### 6.5 Rendering and compatibility

3D surfaces use WebGPU where present (Chromium browsers, WebView2, Safari/WKWebView 26+) and fall back to WebGL2 — `@localspace/view3d` implements both backends behind one API (Linux WebKitGTK is the main WebGL2-only case). 2D canvases are Canvas2D and run everywhere. The SDK exposes `h.gpu` = `"webgpu" | "webgl2"` so a harness can pick shaders.

### 6.6 Memory

The client's webview costs 150–300 MB depending on OS; that is the platform's price and is stated plainly. The base shell is budgeted at ≤ 80 MB of JS heap idle; each open harness iframe is budgeted in its manifest (`[resources] surface_mb`) and the shell reads `performance.measureUserAgentSpecificMemory()` where available to enforce it; closed panels are unloaded, not hidden. Core stays ≤ 50 MB.

---

## 7. Harness package

```
io.localspace.whiteboard-1.4.0.hpack   (signed zip)
├── harness.toml          manifest: id, version, api range, capabilities, resources, contributes, dependencies, accepts/produces
├── logic.wasm            Component Model component: tools + context provider + event handlers (runs in Core)
├── ui/
│   ├── index.js          ES module bundle (React allowed; ships its own copy or uses the shell's shared runtime via import map)
│   ├── *.wasm            surface-side engines (Rapier, OpenCascade, planegcs …)
│   └── assets/
├── evals.json            scripted agent tasks with assertions
└── icon.svg
```

Manifest is the plugin spec's, with `views` entries of kind `widgets` | `web` | `stream` | `native`. Shared runtime: the shell provides React, `@localspace/ui`, `@localspace/canvas`, `@localspace/view3d` and the SDK through an import map so harnesses do not each ship their own copies; a harness may pin a version if it needs one. Logic components are AOT-compiled by wasmtime at install and instantiated on first call, dropped after 5 minutes idle.

Dependencies, lockfile per environment, registry protocol, channels, yank/deprecate: Plugin spec §17, unchanged.

---

## 8. Agent and harness contract (unchanged, restated)

Tools with budgets and `front_door`; context providers returning budgeted text with a `zoom` tool; dynamic tool exposure (focused / pinned / `find_capability`) with the grammar compiled per active set; the task ledger present on every turn; four communication mechanisms through Core (documents as bus, interface calls, events, agent handoff); specialists as sub-agents; one proposal branch per task across harnesses. Plugin spec §4, §9, §18.

The chat harness is where the agent lives: conversations, the ledger view, proposals to apply or discard, citations, the environment's network-mode indicator.

---

## 9. Data

Automerge documents for structured content (boards, sketches, plans, tasks), one per harness document, synced by Core to every open replica; blob documents (meshes, scenes, images) content-addressed and snapshotted before each write tool; the immutable version DAG with agent runs as branches; crypto-shredding for erasure. Plugin spec §10, Organisation Deployment §6 and §9. Using Automerge on both sides — Rust in Core, `@automerge/automerge` in the client — is the single biggest simplification in this rewrite: co-editing, offline replicas, undo and diffing are the library's, not ours.

---

## 10. Retrieval, network, identity, audit

Unchanged: ACL pre-filtered hybrid retrieval with citations (Organisation Deployment §6.4); three network modes and the gateway (§8); OIDC/SAML, SCIM, roles (§4); hash-chained audit with SIEM export (§10).

---

## 11. Efficiency rules that survive the rewrite

From Plugin spec §16: fixed prompt layout for prefix caching; speculative decoding; utility model routing; provider output cached by document hash; compaction; hybrid-MoE placement rules; AOT wasm; audit off the hot path; ACL as index pre-filter. Client-side replacements: iframes unloaded when closed; harness bundles cached by content hash (immutable URLs); Automerge incremental sync and compaction; virtualised lists; Three.js render-on-demand (`invalidate`) rather than a continuous loop; WebSocket binary frames for sync.

Budgets carried into CI: prompt-cache hit ≥ 80 %; ≥ 40 % of calls on the utility model; W32 100B+-MoE ≥ 15 tok/s; Core idle ≤ 50 MB; shell idle JS heap ≤ 80 MB; base bundle ≤ 2 MB compressed; harness panel open-to-interactive ≤ 1.5 s from cache.

---

## 12. Security model summary

Surfaces: iframe sandbox + CSP + separate origin; no ambient capability. Logic: wasmtime Component Model, capability-scoped, memory- and fuel-limited. Tier B native processes: OS sandboxes, admin-approved, post-MVP. Untrusted-content boundary around retrieved documents, web pages and tool results. All packages signed with the localSpace release key; org countersignature optional. Organisation Deployment §9.

---

## 13. Build order (MVP first)

1. `localspace-proto` + Core skeleton + axum server + generated TS client. Desktop (Tauri) and `serve` both boot to a login and an empty shell — day one.
2. llama.cpp sidecar supervision + placement planner + model catalog with one-click download. Gate: a 100B+-class MoE at Q4 runs on a W32 machine at ≥ 15 tok/s.
3. Chat harness: streaming, conversations, citations, tool loop with GBNF grammar, task ledger view.
4. Harness runtime: manifest, wasmtime logic components, iframe surfaces, bridge SDK, install/uninstall from a local registry.
5. **`@localspace/canvas` and the whiteboard harness as the first downloadable package** (own canvas engine, Automerge doc, ~12 tools, context provider, evals). Gate: install from catalog → agent puts a plan on the board → user edits it live at 60 fps with 5,000 shapes on screen → undo through the DAG.
6. Retrieval (tantivy + usearch, ACL filter) and document upload; network modes and gateway with `web.search`/`web.fetch`.
7. Multi-user `serve`: OIDC, workspaces, ACLs, Automerge sync between browsers, proposals for agent edits, audit log.
8. Tier B native runtime (sandboxed process, GPU pool, state-streaming path), `@localspace/view3d`, and the **own physics engine as the first native harness** (rigid bodies on wgpu compute; viewport on view3d); planning board harness on `@localspace/canvas`; the sketch → simulate → board handoff bench.
9. CAD harness, stage 1: 2D sketcher with the own constraint solver, extrude/revolve on an own half-edge mesh kernel with CSG, `geometry.v1` export into the physics harness. B-rep is a later stage (§15).
10. Registry service, signing, offline bundle, org catalog, entitlements.
11. Post-MVP: video-stream and native-overlay delivery paths; CUDA-only solvers; mistral.rs/vLLM workers; marketplace commerce.

Steps 1–5 are the MVP. Team pilots start after step 7; the physics and CAD harnesses (8–9) are what the research-lab pilots are sold on.

---

## 14. Adjustments to the other documents

- Plugin spec: §2–3 (egui Client, transports), §5 (surfaces and the egui ABI), §13 (tech stack) and §14 (build order) are replaced by §2, §5–7 and §13 here. §1.2's 50 MB now applies to Core only; the client's budget is §6.6.
- Organisation Deployment: `localspace serve` serves the JS bundle instead of a wasm client (§2, §3.1); "Client" throughout means the web client; the browser support note in §6.5 here applies. Everything else stands.
- Marketplace: the harness package gains a `ui/` bundle and the `web` surface kind (§7 here); the internal standards in its §8 add "use the shell's shared runtime via the import map; render 3D on demand; budget `surface_mb`". Everything else stands.

---

## 15. Risks specific to this stack

- **Building the product layer in-house is the schedule.** An infinite canvas that feels right is 2–3 engineer-months; a rigid-body engine with a stable constraint solver is 4–6; a 2D constraint solver 1–2; a mesh kernel with robust booleans 3–6; a full B-rep kernel (NURBS, trimming, fillets, robust intersections) is *years* and is the one item where "build it ourselves" should be revisited when the time comes — mesh-first CAD covers most of what an agent-driven sketch-to-simulation workflow needs. The in-house policy is right for the moat (canvas, physics, agent, harness runtime) and must be paired with a hard rule that every in-house library ships with a benchmark and a test suite from its first commit, or the rebuild costs twice.
- **Webview inconsistency.** WKWebView and WebKitGTK lag Chromium on WebGPU and some APIs; the SDK's WebGL2 fallback and a CI matrix across all three webviews are mandatory from step 4.
- **Two languages, one boundary.** The discipline is that no logic lives in the client that Core would have to trust. Generated types and a permission check on every mutating call keep the boundary honest; code review enforces it.
- **iframe performance ceiling.** Cross-origin iframes cost a process each in Chromium; twenty open panels is a lot of memory. The shell caps open panels per environment (default 6) and unloads the rest.
- **Hybrid MoE on W32 is still the technical bet.** Step 2 is the gate; nothing else in the plan matters if it fails.
