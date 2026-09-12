# localSpace — Harness Plugin System

Spec for the plugin core: how a module (whiteboard, physics sim, sketching tool, planning board) plugs into the app, how it is isolated, how agents discover and drive it, and how the same system runs as a personal desktop app and as an organisation's server. Targets strong hardware — machines that run large models and GPU simulations together (§1.1) — and is built to use that hardware as efficiently as possible (§16).

---

## 1. Terms

- **Environment** — a user's workspace: a set of installed Harnesses + a selected model + documents + layout, theme and agent policy. Environments are the unit of customisation and can be saved, shared and sold as templates (marketplace spec).
- **Harness** — one installed plugin. It contributes a surface (UI), a set of agent tools, and a context provider.
- **Core** — the Rust backend: model runtime, document store, version DAG, permission enforcement, agent loop, harness logic runtime.
- **Client** — the Rust GUI. Same code compiled to a native binary (desktop) or to WebAssembly (browser).
- **Front door** — the 1–3 tools a harness exposes when it is *not* the focused surface.

Rule: the agent never talks to a harness directly. Every call goes through Core, which enforces permissions, records the mutation, and returns the result.

### 1.1 Hardware target

Strong hardware, in two shapes. The product targets machines that run large models and GPU simulations together; average laptops are a later tier. Nothing below is allowed to make that later tier impossible — every budget, model assignment and placement is configuration, never a constant.

| Profile | Hardware | How 100B+ models run | Expect |
|---|---|---|---|
| **W32** — single-GPU workstation | 32 GB VRAM (RTX 5090-class), 64 GB RAM, ≥ 2 TB NVMe at ≥ 5 GB/s, 16+ cores with AVX-512 (AMX preferred) | **Hybrid MoE inference** (§11.1): attention, KV cache, shared experts and a hot-expert cache on the GPU; routed experts in RAM; NVMe streaming for models larger than RAM+VRAM | 100B–120B-class MoE at Q4: interactive. 235B-class at Q3/Q4: works, slow. Dense 70B: Q4 with layer offload, slow — MoE is the intended model class here |
| **W96** — multi-GPU workstation | ≥ 96 GB VRAM (2× RTX PRO 6000; or Apple M3/M4 Ultra ≥ 256 GB unified), ≥ 128 GB RAM | Fully GPU-resident up to ~90 GB of weights; hybrid beyond | 100B+ MoE at Q4/FP8 and 70B dense at FP8: fast. One GPU dedicatable to simulation |
| **S** — organisation server | ≥ 4× 80 GB-class GPUs, ≥ 512 GB RAM (deployment spec §12.1) | GPU-resident, tensor-parallel | everything, many users |

W32 is a first-class target, not a degraded mode: a Qwen3-family 100B+ MoE (80B-A3B, 235B-A22B and successors), gpt-oss-120b or GLM-4.5-Air-class model must run on it out of the box, chosen from the catalog with one click, with the placement decided by the planner (§11.1) rather than by the user.

Reference models the design is validated against (refresh quarterly): a **100B+-class MoE** at Q4 as the W32 reference (Qwen3-family 100B+, gpt-oss-120b); a **70B-class dense** at FP8 as the W96/S reference, each with a **draft model** for speculative decoding; a **large MoE** (235B-A22B, DeepSeek-V3-class) on S; a **VLM** sized to the profile (7–8B on W32, 72B-class on W96/S); a resident **utility model** (1.5–4B on W32, 3–8B otherwise) for calls that do not need reasoning (§16.1); a multilingual embedding model. `localspace doctor` identifies the profile, warns below W32, and refuses `serve` below S unless `--allow-below-floor` is passed.

### 1.2 Footprint — the app is 50 MB

The application's own memory is capped at **50 MB**: the native Client at idle, and Core at idle, each ≤ 50 MB of private resident memory. Model weights, KV and expert caches, memory-mapped documents and indexes, and harness processes are not "the app" — they are the user's chosen workload, shown separately and adjustably in the plan bar (§16.2) — but the shell that hosts them stays at 50 MB no matter how much is installed.

What makes that possible rather than aspirational:

- **Everything is a harness.** Chat, the whiteboard, settings, the store, the admin console — all are harnesses on the same contract, including the ones that ship in the box. Core contains only: the proto server, the DAG, the document store, the permission checker, the scheduler, the planner and the harness runtime. There is no feature in Core that a user cannot uninstall.
- **Nothing is resident that is not in use.** A harness's logic module is instantiated on first call and dropped after 5 minutes idle (its document stays in the store); its surface exists only while its panel is open. An environment with 30 harnesses installed and one open costs one harness of memory.
- **Files are mapped, not loaded.** Blobs, documents, the index and weights are memory-mapped; they live in the OS page cache, which is reclaimable and is not the app's footprint. Core never holds a second copy in its heap.
- **No embedded browser.** The Client is egui on wgpu; there is no webview, which alone is 100–300 MB in most "desktop" apps.
- **Budgets per harness.** The manifest declares `[resources] memory_mb = { logic = 32, surface = 16 }`; the runtime enforces both (wasm linear-memory limit, surface heap limit) and the store shows them. A harness over its declaration is killed and restarted, and the user is told.

Measured as private RSS (native) and wasm heap (browser Client), at idle with the reference environment installed, in CI on every release (§16.5).

Consequence for the agent design: tool budgets, context budgets and grammar sizes come from a **model profile**, selected per placement plan. The W32 profile is tighter (tool budget 2500 tokens, working set 16k) because every prompt token is paid for on a PCIe-bound machine; the S profile is looser (4000 / 24k). The focus-scoping in §9 applies to all of them.

---

## 2. Deployment topologies

One binary, two modes. Both are the same product; nothing is desktop-only or server-only.

| | **Personal** | **Organisation** |
|---|---|---|
| Command | `localspace` | `localspace serve --bind 0.0.0.0:8443` |
| Core runs | in the desktop process | on the org's server |
| Client runs | native window, same process | browser, Client compiled to wasm and served by Core |
| Users | one | many; each has their own Environment |
| Auth | none / OS user | org IdP (OIDC or SAML), session token |
| Models | loaded in-process on this workstation's GPUs; simulations on a separate GPU | loaded once on the server, shared by all users through a scheduler; simulations on dedicated GPUs |
| Harness logic | runs in Core | runs in Core (on the server) |
| Harness surfaces | run in the native Client | run in the browser Client |
| `serve` on `localhost` | also works: a single user can run Core as a local server and use the browser Client | — |

Consequence for everything below: **harness logic and tools always run next to Core; harness surfaces always run next to the Client.** In personal mode those are the same process; in organisation mode they are separated by a network. The design has to work across that line, so it works in both.

---

## 3. Architecture — one Core, one Client, two transports

```
crates/
  localspace-proto     # Request / Response / Event types, serde. The only shared contract.
  localspace-core      # backend. No UI dependencies.
  localspace-client    # egui app. No IO except through `Backend`. Compiles to native and wasm32.
  localspace-desktop   # binary: core + client in one process, InProcess transport
  localspace-server    # binary: core + axum HTTP/WS API + auth + serves the wasm client bundle
  localspace-surface-sdk  # what harness authors compile surfaces against (pinned egui)
  localspace-harness-sdk  # what harness authors compile logic against (WIT bindings, MCP types)
```

The Client talks to Core through one trait:

```rust
trait Backend {
    async fn call(&self, req: Request) -> Result<Response>;
    fn events(&self) -> impl Stream<Item = Event>;
}
```

Two implementations: `InProcess` (channels) and `WebSocket` (tokio-tungstenite on native, `web-sys` in the browser). `localspace-proto` is the entire API. If a feature needs a call that is not in `proto`, it is added to `proto` — never as a desktop-only shortcut, or the server mode rots.

Per-user state lives in Core (documents, DAG, environment config, conversation history). The Client holds only view state: open panels, scroll positions, selection. A user who logs in from a different browser sees the same environment.

---

## 4. The three-part contract

Every harness declares exactly three things. A harness that skips any of them is not agent-usable.

### 4.1 Surface (for the human)

One or more panels rendered by the Client. Written in Rust, compiled to wasm, sandboxed. Detailed in §5. The surface has no filesystem, no network, no model access; it exchanges messages with its own harness logic through Core.

### 4.2 Tools (for the agent)

Typed actions with JSON Schema parameters. Each tool declares:

```jsonc
{
  "name": "canvas.add_shape",
  "summary": "Add a rectangle, ellipse or arrow to the canvas.",  // ≤ 25 words, linted at install
  "params": { /* JSON Schema, ≤ 8 top-level properties */ },
  "kind": "write",            // read | write | compute
  "front_door": false,        // true = visible even when unfocused
  "undoable": true,
  "confirm": "never",         // never | destructive | always
  "cost_hint": "instant"      // instant | seconds | long  (long ⇒ runs async, returns a job id)
}
```

Core rejects a package at install time if: a tool summary exceeds the token budget, more than 3 tools are marked `front_door`, or a `write` tool is not `undoable` and has no `confirm`.

### 4.3 Context provider (the part most plugin systems miss)

A function Core calls before each model turn:

```
context(budget_tokens, focus) -> ContextBlock
```

It returns a **text serialization of the harness's current state**, sized to the budget. Text is primary even though a VLM is always resident: it is cheaper, deterministic, diffable and citeable. A surface may additionally offer a `snapshot` tool that returns a rendered image for layout questions; it supplements the text, never replaces it.

Examples of what a good provider returns at a 600-token budget:

- *Whiteboard*: outline of frames → groups → shapes, with text content and rough coordinates; full detail for the selection.
- *Physics sim*: scene graph (bodies, joints, materials), solver settings, results of the last run.
- *Planning board*: columns, card titles, assignees, WIP counts.
- *Sketching/CAD*: layer list, named entities, constraints, current dimensions.

Providers must be **expandable**: return a summary plus a `zoom` tool the agent can call for detail on one region. Never dump the whole document.

---

## 5. Surfaces — Rust UI, sandboxed, native and browser

### 5.1 GUI framework: egui

Chosen because it is the one mature Rust GUI that (a) ships the same code natively and in the browser today, (b) is immediate-mode, which suits infinite canvases, boards and sketching, and (c) has a **serializable draw output** (`epaint` shapes), which is what makes sandboxed plugin surfaces possible at all — see 5.3. Rejected: iced (browser target lags, no sandboxed-plugin path), Slint (weak for custom canvas drawing, third-party licence in the critical path), Makepad (too small and unstable to build a platform on), gpui (native only), Dioxus/Xilem native renderers (not ready).

Client shell: `egui_tiles` for the dock/panel layout; `wgpu` renderer on both targets (WebGPU in the browser, WebGL2 fallback). Host ships a theme; surfaces inherit it.

Known cost: egui's text editing and document layout are basic. A rich-text or long-document harness will need its own text layout inside its surface. Accept this; the canvas-style harnesses are the product.

### 5.2 Three surface kinds

A harness picks one per view in its manifest.

| kind | What the harness ships | Rendered by | Use |
|---|---|---|---|
| `widgets` | a declarative widget tree (Column, Row, Text, Input, Button, Select, List, Table, Slider, Image …) + event handlers | Client, in host theme | forms, settings, lists, dashboards, panels for MCP-only harnesses |
| `egui` | a wasm module containing its own egui UI | Client paints the module's shape output into the panel rect | canvases, boards, sketching, anything custom |
| `stream` | nothing; a Tier B native process on the Core side renders frames | Client draws the frame into a texture slot; input events are forwarded back | physics viewports, 3D, CAD, video |

`widgets` is cheap to author and looks native everywhere. `egui` is full freedom. `stream` is for GPU work that can only happen next to Core.

### 5.3 The `egui` surface ABI

The surface is a plain `wasm32-unknown-unknown` module (not a WASI component — browsers can't run components without a transpile step, and a surface needs no IO). Exports:

```
hs_alloc(len) -> ptr
hs_init(cfg_ptr, cfg_len)                     // fonts, theme, pixels-per-point, initial doc snapshot
hs_frame(in_ptr, in_len) -> out_ptr, out_len  // in:  RawInput + doc patches + messages from logic
                                              // out: FullOutput (shapes, textures_delta, cursor, repaint request)
                                              //      + doc changes + messages to logic
```

Encoding: `postcard` (serde, compact binary). The Client only calls `hs_frame` when the panel is visible and either input arrived, a patch arrived, or the surface requested a repaint. The Client namespaces the surface's texture ids so its font atlas and images cannot collide with the host's.

The same `.wasm` file runs in both Clients: on native through `wasmtime`, in the browser through the browser's own `WebAssembly` API via a small JS shim. One implementation of a `SurfaceRunner` trait per target, identical behaviour.

Version coupling: `localspace-surface-sdk` re-exports a pinned egui/epaint; the shape schema version is part of `harness-api`. A surface compiled against SDK 1.x runs on any Client with `harness-api` 1.x.

Frame budget: a surface that exceeds 8 ms per frame three frames running gets a visible "slow" badge and is throttled, so one bad plugin cannot stall the Client.

### 5.4 Surface ↔ logic

Surface and logic are separate modules that may be on opposite sides of a network. They share the harness document (§10) — Core holds the authoritative Automerge doc, the surface holds a replica, patches flow both ways over the `Backend` stream. Beyond the doc, they get one message channel (`harness/event`, opaque bytes, ≤ 64 KB per message). Design a harness so the doc carries state and messages carry only commands; that is what keeps it correct under latency.

---

## 6. Runtime tiers (harness logic)

Two tiers, one protocol. The agent cannot tell them apart. Both run next to Core.

| | **Tier A — WASM** (default) | **Tier B — Native process** |
|---|---|---|
| Runtime | wasmtime, Component Model + WASI p2 | OS subprocess, JSON-RPC over stdio |
| Isolation | capability-based, no ambient authority | OS sandbox (job object / seccomp+landlock / sandbox profile) |
| Gets GPU / native libs | no | yes |
| Approval | user install | **admin approval + signature required** |
| Surface kinds | `widgets`, `egui` | `widgets`, `egui`, `stream` |
| For | boards, sketching, notes, docs, calculators, connectors | physics engines, 3D renderers, CAD kernels, CV pipelines |

Default Tier A. A package asking for Tier B must justify it in the manifest with a `native_reason` string shown verbatim in the install dialog and the admin console.

GPU for Tier B: a native harness declares `gpu = { vram_gb = 24, exclusive = false }`. Core assigns it a GPU from the pool reserved for harnesses (`[harnesses] gpus` in config), never a model worker's GPU. Exclusive harnesses (a large CFD or rigid-body run) get the whole device and queue behind each other; non-exclusive ones share via the driver. On W96 the harness pool is the second GPU. On W32 there is one GPU: the harness pool is a **VRAM reservation** in the placement plan (default 6 GB, user-adjustable per environment), the planner shrinks the hot-expert cache to make room, and a harness that needs more than the reservation asks for a **temporary re-plan** — the model's expert cache is evicted for the run and rebuilt afterwards, which the user sees as "simulation running, model slower". The model is never unloaded.

**Wire protocol: adopt MCP**, extended with three methods — `harness/context`, `harness/view`, `harness/event`. Consequence: any existing MCP server is already a headless harness (tools, no surface, or a `widgets` panel), and harness authors can use existing MCP SDKs. Do not invent a new RPC schema.

In organisation mode a Tier B process runs on the server, per user session, under a resource quota (CPU, memory, one GPU slice). The server refuses to start a Tier B process when the quota is exhausted and tells the user.

---

## 7. Manifest

`harness.toml` at package root.

```toml
[harness]
id = "io.localspace.whiteboard"      # reverse-DNS, immutable
version = "1.4.0"
api = "^1.2"                          # host harness-api semver range
title = "Whiteboard"
publisher = "localSpace"
tier = "wasm"                         # wasm | native
# native_reason = "..."               # required when tier = "native"

[capabilities]
fs = "workspace"                      # none | workspace | scoped:<subpath>
net = "none"                          # none | allowlist (see §8)
gpu = false
spawn = false
clipboard = "on-user-action"
docs = "acl"                          # retrieval via Core only, ACL enforced
model = ["complete", "embed"]         # inference the harness may request

[resources]
memory_mb = { logic = 32, surface = 16 }  # enforced; shown in the store
idle_unload = "5m"                        # logic instance dropped after this idle time

[contributes]
views = [
  { id = "board",    kind = "egui",    module = "ui/board.wasm", placement = "main" },
  { id = "settings", kind = "widgets", placement = "side" },
]
tools = "tools.json"
context_provider = true
doc = "crdt"                          # crdt | blob
file_types = [".lsboard"]

[model_hints]
prefers = ["tool-use", "ctx>=16k"]    # surfaced by the installer when picking a model
```

Rules:

- **Default deny.** Anything not declared is unavailable.
- `net = "none"` is the default and expected value. The product is offline-first.
- An update that **widens** capabilities does not auto-install. It re-prompts, showing a capability diff.
- `docs = "acl"` never gives the harness the index. It gives it a `docs.search(query)` Core call whose results are already filtered by the caller's ACL, with citations attached.

---

## 8. Network posture — offline by default, online on demand

The product is offline-first, but an agent doing research needs the web. Both are supported by making egress a **Core-owned, environment-level** concern. No harness ever opens a socket itself. In organisation mode, egress is the server's egress — a browser Client never fetches anything but the app itself.

### 8.1 Environment network modes

Set per environment; in organisation mode the admin sets the ceiling and the user can only go stricter.

| Mode | Behaviour |
|---|---|
| `airgapped` | No egress at all. `web.*` tools are absent from the agent's tool set entirely, so the model never proposes a search it can't run. Retrieval falls back to the local corpus. |
| `ask` (default) | `web.search` / `web.fetch` exist; each first use per domain per session raises an inline approval in the conversation. Approvals are logged. |
| `online` | Egress allowed without prompting, still restricted to the admin domain allowlist and still fully logged. |

The mode is visible in the Client at all times — a single indicator, because a user in a bank needs to know at a glance whether this environment can talk to the internet.

### 8.2 The gateway

One Core component is the only thing with a socket. It provides two agent tools:

```
web.search(query, recency?, site?) -> [{title, url, snippet}]
web.fetch(url, mode: "text"|"raw") -> {content, fetched_at, url}
```

It enforces: domain allowlist/blocklist, per-session request and byte quotas, timeouts, HTML→text extraction, and a strip of tracking parameters. Search goes to a configurable backend (a self-hosted SearXNG instance for organisations; a public search API for personal use). Fetched content is **cached into the workspace as a cited document**, so the same page is not re-fetched, it enters retrieval alongside internal documents, and every claim the agent makes from it carries a citation with a fetch timestamp.

Fetched web content is data, never instruction. The gateway wraps it in an untrusted-content boundary and the agent loop does not honour directives found inside it.

### 8.3 Harness egress

A harness that genuinely needs a specific host (a map tile server, a materials database) declares:

```toml
[capabilities]
net = { allowlist = ["tiles.example.com"], reason = "map tiles for the site plan" }
```

The request still goes through the gateway — the harness gets a proxied fetch, not a socket. Admin policy can refuse the capability outright, in which case the harness installs and runs with that feature disabled rather than failing.

### 8.4 Provisioning egress is separate

Downloading a model from Hugging Face or a harness from the registry is **provisioning**, not runtime egress, and is governed by its own setting. An air-gapped site keeps runtime at `airgapped` forever while still provisioning by importing an offline bundle prepared on a staging machine. Never gate model downloads on the same switch that controls whether the agent can browse.

---

## 9. Tool exposure — the scaling problem

Twelve harnesses × fifteen tools is 180 tools. Even a 70B-class model degrades past a few dozen tools in context, and every description is paid for on every turn. Exposure is dynamic, computed per turn:

1. **Focused harness** — the surface the user is on, or the one the last tool call touched: *all* its tools.
2. **Pinned harnesses** — user-pinned, plus any the current conversation has touched: *front-door tools only*.
3. **Everything else** — not in context. Reachable through one always-present Core tool:

```
find_capability(need: string) -> [{harness, tool, summary}]
```

which does a local embedding search over installed tool descriptions and, on the next turn, promotes the matching harness to focused.

Build the constrained-decoding grammar (GBNF / JSON-Schema-constrained) **from the active set each turn**, not from the full registry. Cache the compiled grammar keyed by the active-set hash — the set changes rarely, so this is nearly free.

Budget: total tool-description tokens in context ≤ 4000 for the reference 70B-class profile (a model profile carries this number; a future small-model profile would set ~1500). The installer lints against it; the runtime truncates by dropping the lowest-ranked pinned harness first and says so in the trace.

---

## 10. State, mutation and the DAG

Every agent-initiated write is a commit. This is what makes agents safe to hand a physics scene or a client's board.

```
agent → Core (permission check, confirm gate)
      → harness tool call
      → harness mutates its document
      → harness emits change event
      → Core writes a commit to the version DAG (parent, tool call, params, doc hash)
      → surface replica receives the patch, repaints
      → Core returns {result, diff_summary} into context
```

`diff_summary` is what the model sees — "added 3 shapes, moved 1" — not the whole new document state. The context provider handles state; tool results handle deltas.

Document storage, two options the harness picks in its manifest:

- `doc = "crdt"` — Core-provided Automerge document. Gets free undo, granular diffs, replica sync to the surface over the network, and later multi-user editing. Use for boards, sketches, outlines, tasks.
- `doc = "blob"` — opaque content-addressed blob (blake3), Core snapshots before each write tool. Use for heavy binary state: physics scenes, meshes, textures. Surfaces of blob harnesses are normally `stream`.

Undo is Core-level and uniform: DAG revert, regardless of harness. An agent run is one branch; "reject the agent's changes" is a branch drop, not 40 undos.

---

## 11. Inference

### 11.1 Placement planner and hybrid MoE inference

Every model load goes through a planner. Input: the model's tensor map (per-tensor sizes, MoE layout: layers, experts per layer, active experts, shared experts), the machine (VRAM per GPU, RAM, NVMe bandwidth, PCIe generation and width, CPU ISA and cores, NUMA), and the request (context length, batch, quality floor). Output: a **placement plan** — where every tensor lives, KV cache precision, expert-cache size, prefetch policy, estimated tok/s and first-token latency — plus a verdict: `resident`, `hybrid`, `streaming`, or `does not fit`. The catalog shows the verdict and the estimate per model *for this machine* before the download button.

Placement rules for a MoE on W32 (32 GB VRAM, 64 GB RAM), in priority order:

1. **GPU-resident always**: embeddings, attention (Q/K/V/O), norms, router, shared experts, the LM head, and the **KV cache** at Q8 (FP8 where supported). For a 100B+-class MoE at Q4 this is ~8–14 GB.
2. **Hot-expert cache on GPU**: the remaining VRAM, minus a reservation for the utility model, the VLM if loaded, and the harness pool (§6), holds the most-used routed experts. Expert usage statistics are persisted per model and warm the cache on the next load; steady-state hit rate is on `/metrics`.
3. **Routed experts in pinned RAM**, computed either on the GPU after an async PCIe copy (prefetched for the next layer as soon as the router for the current layer has fired) or **on the CPU** with AVX-512/AMX kernels when the CPU is faster than the copy — the planner picks per machine from a micro-benchmark run once at install. Active parameters are what matter: an A3B model is CPU-comfortable, an A22B is copy-bound.
4. **NVMe streaming** for the part of the expert set that does not fit in RAM: memory-mapped, page-cache-backed, with the same prefetch. Works; the planner labels it `streaming` and shows the honest tok/s.
5. **Dense models** on W32 use layer offload (first N layers on GPU, rest on CPU) and are labelled `hybrid` with their tok/s; the catalog steers W32 users to MoE.

Multi-token verification runs the same experts for several tokens at once, so **speculative decoding is more valuable on hybrid than on resident** — the expert copy is amortised over the draft length. The draft model lives fully on the GPU.

Backends that implement placement plans: `llamacpp` (per-tensor device overrides, CPU MoE kernels — the proven path on this hardware class today, used through `llama-cpp-2`), `mistralrs` (device mapping, ISQ) as it matures for MoE offload, and the internal `native` executor (Rust, candle/cudarc-based) planned to own the hot-expert cache and prefetch logic once the two above have shown what the ceiling is. The plan format is the same for all three; a backend that cannot honour a plan says so and the planner re-plans for the next backend.

W96 and S use the same planner; there the verdict is usually `resident` and the plan is a tensor-parallel layout.

### 11.2 Inference for harnesses

A harness never bundles a model. With `model` capability it calls Core:

- `model.complete(prompt, opts)`
- `model.structured(schema, prompt)` — grammar-constrained, so the harness gets valid JSON back
- `model.embed(texts)`

Core routes to whatever the environment has loaded, using `model_hints` to warn on mismatch. This keeps one set of resident models instead of each harness pulling its own; on W32 the VRAM budget is a single shared plan and every gigabyte a harness takes is an expert-cache gigabyte the model loses.

Organisation mode adds a **scheduler** in front of the model: continuous batching, per-user fairness, a queue with visible position, and per-org quotas. Agent loops of different users interleave on one resident model. If the org wants more than one model resident, that is a second server process behind the same scheduler, not a Core concern.

Hugging Face one-click: the model catalog is a Core concern, not a harness one. Install flow = pick from a curated list (repo id + quantization + a measured tok/s estimate for the detected hardware) → download GGUF/safetensors → register in the environment. In organisation mode only admins do this. A harness may *request* a model at install ("this works best with a vision model") but cannot download or execute one itself.

---

## 12. Packaging and marketplace

Package: signed zip, `.hpack` — manifest, logic (wasm component, or native binary per platform), surface wasm modules, icons, `evals.json`.

- **Signing**: every package is signed with the localSpace release key (sigstore or minisign), hash pinned in the registry index. Unsigned packages install only in developer mode.
- **Registry**: an HTTP JSON index for connected installs; an **offline bundle** (a folder or single archive of packages + index) for air-gapped sites. Air-gapped import is a first-class path, not an afterthought — banks and medical centres will use it as the only path.
- **Org catalog**: admin console defines the allowlist of harness ids/versions, which capabilities are grantable at all, and whether Tier B is permitted. Employee installs resolve against the org catalog only.

**Agent-compatibility score.** Every package ships `evals.json`: 5–20 scripted tasks phrased as a user would ("put the three risks on the board as red stickies"), each with an assertion on the resulting document. Core can run them against the actually installed model and show a per-model pass rate on the store page. This is the store's real quality signal — a plugin that a human can use but a 7B model cannot drive is a broken plugin here, and nothing else will surface that.

---

## 13. Tech stack

| Concern | Choice |
|---|---|
| Language | Rust everywhere: Core, Client, SDKs, reference harnesses |
| Client GUI | egui + eframe, `egui_tiles` docking, `wgpu` (WebGPU / WebGL2 in browser), AccessKit |
| Client targets | native (Windows, macOS, Linux) and `wasm32-unknown-unknown` via trunk/wasm-bindgen |
| Server | axum, tokio, tokio-tungstenite; serves the wasm Client bundle; OIDC via openidconnect, SAML via samael |
| Client↔Core protocol | `localspace-proto` serde types; postcard over WebSocket; in-process channels on desktop |
| Tier A runtime | wasmtime + Component Model, interfaces in WIT |
| Surface runtime | wasmtime (native Client); browser `WebAssembly` API (web Client) |
| Tier B transport | subprocess, JSON-RPC 2.0 over stdio, MCP schema |
| Tool schemas | JSON Schema draft 2020-12 |
| Structured docs | automerge (Rust), sync protocol over the Backend stream |
| Blobs + DAG | blake3 content addressing, redb (or SQLite) for the commit graph |
| Inference | one internal worker trait + placement planner (§11.1). Backends: llama.cpp via `llama-cpp-2` (hybrid MoE on W32, Apple Metal), mistral.rs (Rust, multi-GPU, continuous batching, paged attention — default on W96/S), internal `native` executor (Rust, candle/cudarc) for the hot-expert cache once proven; vLLM / SGLang / TensorRT-LLM as optional external workers in `serve` mode — Core has no code dependency on them |
| Constrained decoding | GBNF grammar compiled per active tool set |
| Retrieval | Core-owned: fastembed or candle embeddings + usearch/HNSW, ACL filter applied pre-ranking |
| Web search backend | SearXNG (self-hosted) or a search API, behind the gateway |
| Sandbox (Tier B) | Windows job object + AppContainer; Linux landlock + seccomp; macOS sandbox profile |

---

## 14. Build order

1. `localspace-proto` + Core skeleton + `Backend` trait with both transports. Desktop and `serve` both boot and show an empty Client — **from day one**, so the two modes never diverge.
2. Manifest parser, permission enforcement, `harness-api` v1 (WIT).
3. Tier A logic runtime, tool routing, DAG commits, undo.
4. `widgets` surfaces; then the `egui` surface ABI with the native `SurfaceRunner`.
5. Context provider protocol + budget/ranking logic + `find_capability`.
6. Reference harness #1: whiteboard (`crdt` doc, `egui` surface, ~12 tools, 2 front-door). Proves the context provider and the surface ABI together.
7. Browser `SurfaceRunner`; whiteboard running in a browser against `serve`. Automerge replica sync over WebSocket.
8. Dynamic grammar from the active tool set; measure tool-selection accuracy on the reference models (100B+ MoE on W32 in hybrid mode, 70B dense on W96/S).
8a. Placement planner + hybrid MoE path on a real W32 machine (RTX 5090, 64 GB): a Qwen3-family 100B+ MoE at Q4 loads with one click and holds the W32 budgets in §16.5. This is a gate, not a nice-to-have.
9. Auth (OIDC), per-user environments, inference scheduler.
10. Tier B: subprocess transport + sandbox + admin approval + `stream` surface. Reference harness #2: physics sim (`blob` doc, GPU).
11. Packaging, signing, offline bundle import, org catalog.
12. Eval runner and store scoring.

Steps 1–8 are the product. Everything after is distribution and scale.

---

## 15. Known risks

- **Models orchestrating multi-tool work is the whole bet.** 70B-class models make it far more likely to hold than the small-model case, but harness tool sets are novel to them; focus-scoping and front doors are the mitigation. Validate at step 8 before building the marketplace.
- **Hybrid MoE inference on W32 is a moving target.** The techniques (expert offload, hot-expert caching, CPU expert kernels, prefetch) are proven in llama.cpp and ktransformers but each new model family changes the tensor layout; the planner and the backend abstraction exist so a new family is a planner update, not a rewrite. Keep the `native` executor behind the same trait until it beats llama.cpp on the W32 bench.
- **W32 is PCIe-bound and RAM-bound; everything else competes with the expert cache.** The VLM, the utility model, the draft model and simulations all take VRAM the planner would otherwise give to experts. The budgets in §16.5 are the referee; when they cannot all be met the planner tells the user which model to drop rather than silently slowing down.
- **Profiles limit the market less than a single floor did**, but still: below W32 is unsupported for now. Budgets, profiles and placement are configuration, so a smaller tier later is a profile plus a plan, not a redesign.
- **Rust-only GUI means one skill set on the harness team.** Every surface is Rust and egui. Since all harnesses are in-house this is a hiring constraint rather than an ecosystem one; `widgets` surfaces cover most panels without custom drawing, and the SDK ships a whiteboard and a form as copyable templates. Accept the trade: the sandboxing and the native/browser parity are only possible because of it.
- **The `egui` surface ABI is novel.** Shape-output-over-the-boundary is a known technique but not a packaged one; expect to own it. Pin epaint's schema aggressively and write a conformance test that renders a fixed surface on both runners and diffs the pixels.
- **Catalog breadth is a headcount problem.** With every harness written in-house, the number of fields the product serves is bounded by the harness team's throughput. The mitigations are structural: shared libraries and interchange types (§17–18) so each new harness is thin, `widgets` surfaces for anything that is not a canvas, and specialists so a harness can be shipped with a narrow first tool set and grown.
- **Tier B is where security actually breaks**, and in organisation mode it is a process on the org's server. Off by default at the org level; expect regulated buyers to keep it off, so the flagship harnesses should be Tier A wherever physically possible.
- **Context providers are the hard authoring task**, harder than the UI. Ship a reference implementation and a test tool that shows the author exactly what the model sees at 300/600/1500 tokens.

---

## 16. Efficiency — design rules

Strong hardware is not a licence to waste it; a 70B model's decode step is the most expensive operation in the system and everything is arranged around not paying for it twice. Four rules, then where each applies.

1. **Pay per change, not per turn or per frame.** Nothing is recomputed, re-sent or re-rendered unless its input changed.
2. **Cache by content hash, everywhere.** If two things have the same hash they are the same thing; DAG, blobs, grammars, provider output, compiled wasm, KV prefixes.
3. **Big model only when it matters.** A resident small model takes the calls that do not need reasoning.
4. **Efficiency is a tested property.** Budgets in CI, metrics in production, regressions fail the build.

### 16.1 Inference (the dominant cost)

- **Stable prompt prefix.** Prompt layout is fixed in this order: system prompt → model profile → active tool descriptions → context-provider blocks → conversation. Each segment changes rarely, so the worker's prefix/radix KV cache hits on nearly every turn. Tool descriptions are emitted in a canonical order sorted by harness id, never by recency, so the prefix survives focus changes. Target: prompt-cache hit ≥ 80 % of prompt tokens on turn 2+.
- **Prefix caching in the worker** is mandatory for a backend to qualify (mistral.rs, vLLM, SGLang all provide it); KV cache in FP8.
- **Speculative decoding.** Each chat worker carries a draft model (`draft = "llama-3.2-3b"` for Llama 3.3 70B; EAGLE-style heads where the backend supports them). Expected 2–3× decode throughput at identical output; verified by the bench, disabled automatically if the acceptance rate drops below 60 %.
- **Chunked prefill + continuous batching**, so a 60k-token retrieval prompt does not stall everyone else's decode.
- **Utility model.** A resident 3–8B model (`role = "utility"`) handles: `find_capability` reranking, conversation titling, tool-result summarisation, context compaction, ingestion chunk cleanup, and any harness `model.complete` call whose `model_hints` do not demand reasoning. The scheduler routes by request class; the big model sees only conversation turns and agent steps. Expected share of calls on the utility model: 40–60 %.
- **Constrained decoding is an efficiency feature**, not only a safety one: a malformed tool call costs a full generation plus a retry turn. Grammars are compiled once per active-set hash and cached on disk.
- **Short returns.** Tool results carry `diff_summary`, never full state; provider blocks carry summaries with a `zoom` tool. Both are budgeted per model profile and linted at install.
- **Compaction.** Conversations older than N turns are folded into a DAG-backed summary by the utility model; the working set the big model sees is bounded (default 24k tokens) regardless of conversation length. The full history stays in the DAG for the user.
- **Provider output is cached by document hash.** A context provider is not re-run when its document has not changed since the last turn; unchanged blocks are also byte-identical, which is what keeps the prefix cache warm.
- **Embeddings** run batched on the utility/embedding GPU (on W32, on the CPU by default, so the expert cache stays whole); the index stores int8 vectors, searched directly, and reranks the top 200 with full precision. A binary pre-filter is deferred until a measurement asks for it — a p95 over budget, or a corpus past a million chunks (amended 2026-09-12, `docs/DECISIONS.md`). Chunks are embedded once per content hash — a re-ingested unchanged file costs nothing.

### 16.2 GPU and memory layout

W32 (hybrid MoE) rules — these decide whether a 100B+ model is interactive or a slideshow:

- **The PCIe bus is the budget.** Every routed expert that misses the GPU cache costs a copy; the planner's job is to minimise copies per token. Hot-expert cache first, prefetch second, CPU compute third — and the choice between copy and CPU compute is measured on the machine, not assumed.
- **Expert usage is persisted** per model and per workload (chat vs agent steps differ); the cache is pre-warmed at load from those stats, so the first minute is not the slow minute.
- **Layer-ahead prefetch**: as soon as layer *L*'s router fires, the experts for layer *L* are requested; the copy for layer *L+1* overlaps *L*'s attention. Pinned host memory, dedicated copy stream, no synchronous transfers on the decode path.
- **Batch the copy**: speculative decoding and prompt prefill both push several tokens through the same expert per copy; the planner sets the draft length from the measured copy/compute ratio rather than a constant.
- **Weights are memory-mapped from NVMe** with huge pages where available; RAM is the page cache, not a second copy. Loading a 60 GB model is bounded by NVMe read speed, and a second load is instant.
- **NUMA-aware**: expert tensors are placed on the NUMA node nearest the GPU's PCIe root; CPU expert kernels are pinned to that node's cores.
- **KV cache at Q8/FP8** and a working set bounded by the model profile (§1.1): on W32 the context is the other thing that eats VRAM the experts want.
- **Everything else is a VRAM reservation in the same plan**: draft model, utility model, VLM, harness pool. The planner shows the plan as one bar; the user drags reservations and sees the tok/s estimate change.

Multi-GPU (W96, S) rules:

- Tensor-parallel groups live on NVLinked GPUs only; `doctor` refuses a TP group across PCIe.
- Weights are memory-mapped from NVMe and shared between worker restarts via page cache; a worker restart on a 70B model is seconds, not minutes.
- Simulation GPUs are never shared with model workers (§6), so neither side ever evicts the other. On a personal workstation the second GPU is the simulation GPU; if there is only one GPU the harness pool is a VRAM reservation on it, declared up front, and the model is sized to what remains.
- Tier B processes stay warm for 15 minutes after their last call, then exit; their surfaces reconnect transparently on the next call.

### 16.3 Client and transport

- egui is repainted **only on demand**: input, an incoming patch, an animation the surface asked for. Idle Client CPU and GPU are near zero; this is measured.
- An `egui` surface runs `hs_frame` only when visible and dirty; shape output for an unchanged frame is not re-sent to the renderer. Fonts and textures are uploaded once.
- `stream` surfaces use **hardware video encode** (NVENC/AMF, H.264 or AV1) at a bitrate adapted to the panel size, encode only while the panel is visible, and pause when the document is idle. On the desktop the same surface bypasses encoding entirely: the Tier B process renders into a shared GPU texture the Client draws directly (DXGI/DMABUF/IOSurface).
- Automerge sync sends only new changes, with periodic compaction so a long-lived board does not grow its history unboundedly in memory; the Client keeps a compacted replica and fetches history on demand.
- Wire format is postcard over binary WebSocket with per-message zstd above 4 KB. In desktop mode there is no serialisation at all: `InProcess` moves typed values over channels.
- The wasm Client bundle is served brotli-compressed, cache-forever with content-hashed names; a release ships a new hash, not a cache purge.

### 16.4 Core and harness runtime

- Blobs are content-addressed and memory-mapped; a document version that shares blocks with the previous one shares them on disk and in memory. Reads never copy.
- Wasm components are **AOT-compiled at install time** (cranelift, cached by module hash) and instantiated from a pooling allocator; a tool call into a Tier A harness is microseconds, not a JIT.
- Tool routing, permission checks and DAG commits are synchronous in-memory operations on one thread per environment; the only I/O on the hot path is the append to the commit log.
- The audit log is written by a separate task from a bounded channel; the hot path never waits on the SIEM.
- Retrieval applies the ACL filter as a pre-filter on the index (a bitmap per user, cached per session), so a query touches only visible vectors rather than filtering after ranking.

### 16.5 Budgets enforced in CI and production

| Metric | Budget | Where enforced |
|---|---|---|
| Prompt tokens per agent step, reference harness | ≤ 6k (S/W96), ≤ 4k (W32) | CI (bench replay) |
| Prompt-cache hit rate, turn 2+ | ≥ 80 % | CI + `/metrics` alert |
| Calls routed to the utility model | ≥ 40 % | `/metrics` |
| Malformed tool calls | < 0.5 % | CI + `/metrics` |
| First-token latency, interactive, sized load | p95 ≤ 3 s (S), ≤ 4 s (W32, 8k prompt) | CI bench + alert |
| Decode tok/s per GPU vs bench baseline | ≥ 90 % | alert |
| **W32, 100B+-class MoE at Q4, hybrid**: decode | ≥ 15 tok/s single stream, interactive verdict | CI on the reference W32 machine |
| **W32**: hot-expert cache hit rate, steady state | ≥ 70 % | CI + `/metrics` |
| **W32**: synchronous PCIe copies on the decode path | 0 | CI (profiler assertion) |
| **W32**: model load from warm page cache | ≤ 10 s | CI |
| **W32**: draft acceptance rate | ≥ 65 % | CI + `/metrics` |
| Client idle CPU / GPU | < 1 % / 0 % | CI (headless Client) |
| **Native Client idle private RSS**, reference environment | ≤ 50 MB | CI + `/metrics` |
| **Core idle private RSS**, reference environment, models excluded | ≤ 50 MB | CI + `/metrics` |
| Browser Client wasm heap, idle | ≤ 50 MB | CI |
| Harness over its declared `memory_mb` | 0 tolerated — killed, restarted, reported | runtime |
| `hs_frame` time, reference whiteboard | ≤ 4 ms p95 | CI |
| Sync bytes per edit, whiteboard | ≤ 2 KB | CI |
| Tier A tool-call overhead (host → wasm → host) | ≤ 50 µs | CI |
| Stream surface bitrate at 1080p, idle document | 0 | CI |

A regression against any budget fails the release. `localspace bench` reports the same table for a real deployment so an org can see where its hardware actually lands.

---

## 17. Package management

The registry is a package manager in the Unity Package Manager sense: a large and growing catalog of harnesses — all written by localSpace — resolved per environment, from localSpace's registry by default and from scoped registries (an org's intranet mirror, an org's private namespace for commissioned harnesses) where they exist. First-party authorship does not make dependency management optional: a hundred harnesses sharing geometry, table and drawing libraries need exactly the same resolver, lockfile and interfaces as an open ecosystem, and they are cheaper to get right now than to retrofit.

### 17.1 Manifest additions

```toml
[dependencies]
"io.localspace.types.geometry" = "^1.2"      # library package: shared data types
"io.localspace.mesh-viewer"    = { version = "^2", optional = true }   # another harness
"localspace.geometry.v1"       = { interface = true }                 # any provider of this interface

[provides]
interfaces = ["localspace.simulation.v1"]     # WIT interfaces this harness implements

[package]
kind = "harness"                              # harness | library | types | template | model-pack | skill | theme
```

- **Library packages** (`kind = "library"`, `"types"`) have no surface and no tools: a wasm component other harnesses link against at install time via Component Model composition. One copy per version per machine; a geometry library used by twelve harnesses is loaded once.
- **Interface dependencies** name a WIT interface, not a vendor. The resolver satisfies them with any installed harness whose `[provides]` lists it, or offers the store's providers. A CFD harness depends on `localspace.geometry.v1`, not on a particular sketching tool.
- Ids are reverse-DNS: `io.localspace.*` for the catalog, `<org-domain>/*` for commissioned private listings.

### 17.2 Resolution and lockfile

Each environment has `environment.lock`: every package, exact version, content hash, source, and the interface bindings chosen. Installing the same environment (or template) on another machine or for another employee replays the lockfile byte-for-byte; the lockfile is a document in the DAG, so a change to the environment is a diff. Resolution is semver with a single version per package per environment (no duplicate majors — a conflict is reported with the two dependents named and the store offers the versions that would satisfy both).

### 17.3 Registry protocol

A documented HTTP API (`/index`, `/packages/{id}/{version}`, `/search`, `/revocations`), content-addressed package storage so mirrors and offline bundles are byte-identical, **delta updates** between versions, and signed indexes. Sources per environment: `registry` (default localSpace), scoped registries by namespace prefix, `path` and `git` for development (developer mode only; never in a lockfile that is published). A local content-addressed cache means a package downloaded once is never downloaded again for any environment on that machine; the org mirror does the same for the whole company.

### 17.4 Lifecycle and compatibility promise

localSpace can **yank** a version (blocks new installs, existing stay), **deprecate** a package with a replacement pointer the store surfaces, and ship on `stable`/`beta` channels. `harness-api` majors are supported for 24 months after the next major ships so that organisations pinning old versions keep working; deprecations are announced one minor ahead with lint warnings in `localspace dev check`. Because Core and every harness are built by the same team, the API can evolve faster than an open platform's — but the lockfile means an org never gets an unplanned change.

---

## 18. Inter-harness communication and shared context

A task that spans harnesses — sketch a bracket, simulate it, put the results on the planning board — has to feel like one task, to the user and to the agent. The rule: **context lives in Core, never inside a harness.** Harnesses hand each other data through Core, and the agent carries one ledger across all of them.

### 18.1 The task ledger

Every agent run has a task object in Core:

```
Task {
  goal,                       // what the user asked
  plan: [Step {harness, intent, status}],
  artifacts: [{id, type, doc_ref@commit, summary, produced_by}],
  notes,                      // agent scratch: decisions, open questions
  citations
}
```

It is rendered into the prompt as one compact block (~600 tokens, budgeted by the model profile) that is present in **every** turn regardless of which harness is focused. When the focused harness changes (§9), the tools change; the ledger does not. That is the shared context: the agent in the physics harness knows the bracket it is simulating is `art_3`, produced by the sketching harness at commit `b7f1…`, with the summary the sketching harness's context provider wrote for it. Artifacts are DAG references, so the handoff pins an exact version and costs nothing to pass — no data is copied, only a hash.

### 18.2 Four ways harnesses talk, all through Core

| Mechanism | Use | Capability |
|---|---|---|
| **Documents as the bus** | A produces a typed document; B imports it by reference. The normal path: `sketch.export_geometry() → art_3`, `physics.import(art_3)` | `docs` (already required) — B must declare `accepts = ["geometry.v1"]` |
| **Interface calls** | A calls a tool on any provider of an interface: `call("localspace.geometry.v1", "boolean_union", …)` | `harness:call = ["localspace.geometry.v1"]` — by interface, never by vendor id |
| **Events** | B subscribes to `doc.changed(type = geometry.v1)` or a named topic; a board harness updates a card when a simulation finishes | `harness:subscribe = [...]` |
| **Agent handoff** | The agent itself moves the task from step to step (below) | none — this is the loop |

Every one of these is a Core operation: permission-checked, ACL-checked, audited, and recorded in the DAG when it mutates anything. A harness never gets a handle to another harness's memory.

### 18.3 Typed artifacts

Interchange types are packages (`kind = "types"`), versioned and owned by localSpace — `geometry.v1` (B-rep and mesh), `drawing.v1` (2D vector), `table.v1`, `image.v1`, `pointcloud.v1`, `simulation-result.v1`, `outline.v1` (boards, notes, plans), `timeline.v1`, plus domain-specific ones as the catalog grows. Since one team owns both the types and every harness that uses them, the rule is strict: a harness never invents a private format for data another harness could plausibly consume. A harness declares `accepts` and `produces`. Core uses these to: validate a handoff, suggest the next harness ("this is `geometry.v1`; the physics harness accepts it"), find or install a **converter** harness when types do not match, and tell the agent which tools take which artifact, so the model does not guess at formats.

### 18.4 Agent handoff — one task, several harnesses, in order

The agent loop plans across harnesses and executes step by step:

1. **Plan**: the agent writes `plan` into the ledger with the intended harness per step, using each harness's front-door summaries and `find_capability`.
2. **Focus**: for each step Core focuses the step's harness — full tools in, other harnesses' front doors only, the ledger always present.
3. **Execute**: tool calls produce artifacts; each is appended to the ledger with the producing harness's own summary of it (the context provider is reused for this, so summaries are consistent).
4. **Hand off**: the next step's harness receives the artifact by reference; its `import` tool validates the type and returns a `diff_summary`.
5. **Finish**: the ledger's artifacts are the result set the user sees, each opening in its harness at the pinned version.

Two refinements keep this efficient and correct. **Specialist sub-agents**: a harness may ship a `specialist` (a skill: prompt, preferred tools, examples). Core can run a step as a sub-agent that sees only that harness's tools plus the ledger, returns its artifacts and a summary, and is then discarded — the parent agent's context never fills with another harness's tool chatter, and a 30-tool harness costs the parent nothing. **Proposal branches per task**: all mutations of one task go to a single DAG branch across every harness touched, so "discard the agent's work" drops one branch even when four harnesses were involved, and "apply" merges it once.

### 18.5 What the harness team has to do per harness

Declare `accepts`/`produces`, implement `import`/`export` tools for those types, write a context provider that produces good artifact summaries, and ship a specialist when the harness has more than ~10 tools. Nothing else; the ledger, routing, permissions and branching are Core's. The SDK's `localspace dev run` shows the ledger exactly as the model sees it during a multi-harness task, and the release bench runs a three-harness handoff for any harness that accepts or produces a type — the handoff is what decides whether the catalog feels like one product.

Build order: §17 lands with step 11 (packaging); §18.1–18.3 with step 6 (the whiteboard is the first `outline.v1` producer) and the handoff loop with step 8, validated by a reference three-harness task (sketch → simulate → board) that is part of the release bench.
- **Latency between surface and logic in organisation mode.** A harness designed as if surface and logic share memory will feel broken over a WAN. The doc-carries-state rule (§5.4) is the discipline; the SDK should include a latency-injection mode for testing.
