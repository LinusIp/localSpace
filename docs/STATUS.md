# Status

What exists, what is partial, and what is not built — mapped section by section to
the two specs. Written to be checkable: every "built" claim below is backed by code
you can run and, in most cases, by a named test.

`H§n` refers to the Harness Plugin System spec, `D§n` to the Organisation
Deployment spec.

Legend: **built** · **partial** (works, with a stated limit) · **not built**.

---

## Harness plugin system

| § | Subject | State | Notes |
|---|---|---|---|
| H§1.1 | Hardware profiles W32 / W96 / S | **built** | `profile.rs`. `localspace doctor` classifies this machine and refuses `serve` below the floor without `--allow-below-floor`. Every budget comes from a `ModelProfile`, never a constant. |
| H§1.2 | Footprint — the app is 50 MB | **partial** | `[resources] memory_mb = { logic, surface }` and `idle_unload` are declared, defaulted, validated and **enforced**: a wasmtime ResourceLimiter on the logic component and on the surface module. Over budget means stopped, reported, audited, and re-instantiated on the next call. Idle logic is unloaded after `idle_unload` and comes back with its document intact; every call path brings it online itself. The process weighs itself (`footprint`) and `bench` and `/metrics` print private RSS beside the 50 MB budget. **Half met.** Headless Core with a harness instantiated measures 5 MB private (18 MB working set) — inside its 50 MB. The desktop process is ~300 MB: wgpu, egui and the surface engine, i.e. the Client half, which is over; chat, settings and the store are not yet harnesses. Surface budgets are measured, not guessed: the whiteboard declares 32 MB against a zooming peak of 12 MB on an ordinary board and 21 MB on a text-heavy one, and a surface that breaks its budget is restarted once by itself. |
| H§2 | Two topologies, one binary | **partial** | `localspace` and `localspace-serve` share Core and the whole protocol. The server serves the API; the **browser Client bundle is not built** (see NEXT). |
| H§3 | Core / Client / two transports | **built** | `localspace-proto` is the only contract. `InProcess` moves typed values over channels; the server encodes the same types with postcard over a binary WebSocket. |
| H§4.1 | Surfaces | **built** | `widgets` and `egui` kinds both render. `stream` is declared and refused with a clear message. The reference `egui` surface carries real direct manipulation: tool palette, marquee and shift multi-select, dragging a selection, eight resize handles, snapping with alignment guides, stacking order, locking, duplicate, clipboard, freehand ink, text labels and keyboard shortcuts — with one commit per gesture rather than one per frame. |
| H§4.2 | Tools | **built** | `tools.json` parsed and linted at install: summary word budget, ≤ 8 params, ≤ 3 front door, no un-undoable unconfirmed write, total token budget. |
| H§4.3 | Context providers | **built** | Called before each turn with a real budget, cached by document hash, truncated on a line boundary with an explicit marker. |
| H§5.2 | Three surface kinds | **partial** | `widgets` + `egui` built; `stream` needs Tier B frame transport. |
| H§5.3 | The `egui` surface ABI | **built** | `hs_alloc` / `hs_init` / `hs_frame`, postcard, one `i64` return. The surface tessellates and ships `epaint::Mesh` (shapes are not serializable; meshes are). Native `SurfaceRunner` under wasmtime; conformance test renders the reference surface and checks the output paints. |
| H§5.4 | Surface ↔ logic | **partial** | The document carries state and messages carry commands, as specified. The document reaches the surface as a JSON projection (`GetDocJson`); Automerge **replica** sync is implemented and tested in Core but the browser Client that would use it is not built. |
| H§6 | Two runtime tiers | **partial** | Tier A (wasmtime, component model, WIT) is built and is what the reference harness uses. Tier B (subprocess, MCP JSON-RPC over stdio) is built as a transport — an existing MCP server is already a headless harness — but **the OS sandbox is not applied** (see below). |
| H§6 | GPU for Tier B | **partial** | `gpu = { vram_gb, exclusive }` parses and appears in the capability diff; there is no GPU pool or scheduler to honour it. |
| H§7 | Manifest | **built** | Default deny throughout. Capability widening re-prompts with a diff. `native_reason` required and surfaced. |
| H§8 | Network posture | **built** | Three modes; under `airgapped` the `web.*` tools are absent from the active set entirely. Allowlist, blocklist, intranet ranges, per-session quotas, tracking-parameter stripping, HTML→text. Fetched content is wrapped as untrusted. Search needs a configured backend. |
| H§8.3 | Harness egress | **built** | A harness gets a proxied fetch, never a socket, and only for hosts in its own allowlist. |
| H§9 | Tool exposure | **built** | Focused / pinned / touched ranking, front doors, `find_capability` over a BM25 index of installed tools, budget enforced by dropping whole harnesses lowest-first and saying so in the trace. |
| H§9 | Dynamic grammar | **built** | Real GBNF compiled from the active set (not the registry), cached in memory by active-set hash. Sent to backends that accept it; tool schemas are always sent. **Not yet cached on disk**, and tool-selection accuracy has **not been measured on a reference model**. |
| H§10 | State, mutation, DAG | **built** | Every write is a commit with parent, tool, params, doc hash and diff summary. Undo, redo, and whole-run drop. `crdt` documents are Automerge with field-level reconciliation; `blob` documents are content-addressed. |
| H§11.1 | Placement planner | **built** | Real arithmetic over a tensor map and a machine: GPU-resident core, hot-expert cache, RAM experts, NVMe streaming, KV precision, verdict, roofline tok/s. Reference maps for a 100B+ MoE at Q4 and a 70B dense at FP8. **The estimate is a roofline model, not a measurement**, and no backend consumes the plan yet. |
| H§11.2 | Inference for harnesses | **built** | `model.complete` / `model.structured` / `model.embed` are host imports, capability-checked, routed to the utility worker at background priority. |
| H§12 | Packaging and marketplace | **partial** | A Marketplace lists an offline bundle: description, publisher, tier and `native_reason` verbatim, capabilities in plain language, a capability diff against the installed version, and the agent-fit facts (tools, front doors, context provider, document kind, eval-case count). Install, remove and score all work from it, and a widened update waits for an explicit approval. `evals.json` and the eval runner report a pass rate per model. **No `.hpack`, no signing, no HTTP registry index, no org review queue.** |
| H§13 | Tech stack | **built** | As specified, except the inference backends (see H§11) and the browser target. |
| H§16.1 | Stable prompt prefix | **built** | Fixed layout, tool descriptions emitted in canonical order by harness id (never recency), provider output cached by document hash, working set bounded by profile. |
| H§16.1 | Utility model routing | **built** | Request classes and a router; harness calls and background work go to the utility worker. The split is counted. |
| H§16.1 | Short returns | **built** | Tool results carry `diff_summary`; provider blocks carry summaries plus a zoom tool. |
| H§16.2 | GPU/memory layout | **not built** | The planner describes the layout; nothing executes it. |
| H§16.3 | Client and transport | **partial** | The Client repaints only on input or when Core wakes it through the transport — never on a timer. An `egui` surface runs `hs_frame` only when input arrived, a document or message is waiting, or it asked for a repaint; a quiet host frame repaints last frame's cached meshes without entering the guest, and a document the guest already holds is not pushed at it again (both covered by `tests/surface.rs`). **No zstd on the wire, no brotli bundle, no hardware video encode** (there is no `stream` surface yet). |
| H§16.4 | Core hot path | **partial** | Tool routing, permission checks and DAG commits are synchronous in-memory operations on one thread per environment. **Wasm components are not AOT-cached at install**, and blobs are not memory-mapped. |
| H§16.5 | Budgets in CI | **partial** | `localspace bench` reports the budgets that do not need a loaded model, now including the app's private RSS against 50 MB and how many logic instances are resident. The model-dependent ones are stated as needing `serve`'s `/metrics`. **Not wired into CI.** |
| H§17 | Package management | **partial** | `[package] kind`, `[dependencies]` (version, optional, interface) and `[provides]` parse and validate. A resolver picks one version per package per environment, reuses an installed version that satisfies, binds interface dependencies to any provider, and names both dependents on a conflict. Install resolves first and pulls missing dependencies from the catalog in order. `environment.lock` — exact versions, blake3 content hashes, sources, interfaces — is a document in the DAG. **Not built:** component composition of libraries into dependents, the HTTP registry protocol, delta updates, yank/deprecate/channels, `.hpack`, signing. |
| H§18 | Inter-harness communication | **partial** | The task ledger lives in Core and is rendered into every prompt after the stable prefix, budgeted per profile; `task.plan` and `task.note` write it. Typed artifacts are DAG references pinned to a commit, registered only for declared `produces` kinds. A handoff names an `artifact`; Core checks `accepts` (a refusal names who does accept), reads the pinned version, and hands it over through `artifact-get`; both legs audited. Reference handoff whiteboard → `outline.v1` → planning board, tested end to end. **Not built:** interface calls, events/subscriptions, specialist sub-agents, converter harnesses, the three-harness bench. |

---

## Organisation deployment

| § | Subject | State | Notes |
|---|---|---|---|
| D§2–3 | One process, config, storage layout | **partial** | `localspace-serve` is one process with `--bind`, `--harnesses`, `--data`, `--web`. **There is no `localspace.toml`**; configuration is command-line only. |
| D§4 | Identity | **not built** | No OIDC, SAML, SCIM or sessions. The server keys an environment by a per-connection token and takes it at face value. This is the single largest gap for a real deployment. |
| D§5 | Tenancy | **partial** | Workspaces, environments and per-document ACLs exist in `acl.rs` and are enforced on every read, tool call and sync message. Only the personal workspace is created; there is no UI or API to make shared ones. |
| D§6.1 | Document ACL | **built** | Checked on read, write, retrieval listing and Automerge sync. A `view` member's outgoing changes are rejected server-side. |
| D§6.2 | Real-time co-editing | **partial** | Automerge sync is implemented and tested (two replicas converge). **No presence channel and no lease for blob documents.** |
| D§6.3 | Agents in shared documents | **partial** | Agent writes in a `proposal` workspace are recorded as a proposal over the run, and a run can be applied or discarded as one action. **Proposal branches are not isolated from other viewers' reads.** |
| D§6.4 | Ingestion connectors | **not built** | |
| D§7 | Model serving at scale | **not built** | Core routes to one external OpenAI-compatible endpoint. No supervised workers, no continuous batching, no scheduler queue, no per-user quotas. The `Router` and `RequestClass` are the seam these plug into. |
| D§8 | Governance | **partial** | Capability policy is enforced (`Policy::apply`), including refusing a capability and letting the harness run degraded. **No review queue, rollout groups, or org catalog.** |
| D§9.1 | Data protection | **not built** | No TLS termination, no at-rest encryption, no crypto-shredding. |
| D§9.2 | Isolation | **partial** | Tier A is wasmtime, capability-scoped and fuel-metered per call. Surfaces get no imports but the wasm-bindgen placeholders, which are stubbed with traps. **Tier B gets a cleared environment and a pinned working directory but no job object, landlock or seccomp** — it is off by default under organisation policy for that reason. |
| D§9.3 | Agent safety | **built** | Retrieved and fetched content is wrapped as untrusted and the loop does not honour instructions inside it. `always`/`destructive` confirmations cannot be pre-approved by a harness. Every agent action is a commit attributed to its run. A tool the model was never shown cannot be called. |
| D§10 | Audit | **partial** | Append-only, hash-chained, verifiable, reopened across restarts, with the specified record shape. **No syslog/CEF export and no retention policy.** |
| D§11 | Operations | **partial** | `/healthz`, `/readyz`, `/metrics`, `/api/v1/openapi.json`, structured logs. **No backup/restore, no HA, no runbooks.** |
| D§12 | Capacity | **partial** | `doctor` enforces the floor and reports the profile. **`bench` does not drive synthetic users.** |
| D§13–14 | Admin console, licensing | **not built** | |

---

## Known limits worth stating plainly

- **No model is bundled.** Core talks to an OpenAI-compatible endpoint. Everything
  in the agent loop works without one except generating a turn — which is why the
  end-to-end tests drive the loop with a scripted worker and `evals` reports
  "no model loaded" honestly rather than passing vacuously.
- **The planner's tok/s is an estimate.** It is a roofline: bytes that must move
  per token over the bandwidth of the path they move over. It is deliberately
  conservative and is meant to be replaced by a measurement at install.
- **Tier B is not sandboxed by the OS yet.** It is disabled by default under
  organisation policy and requires an explicit approval of the package's
  `native_reason` in personal mode.
- **The browser Client is not built.** The Client crate is written to compile for
  `wasm32-unknown-unknown` and the browser `SurfaceRunner` is stubbed with an
  explicit error rather than silently doing nothing.
- **Retrieval does not exist.** `docs.search` is declared in the WIT and returns a
  clear "not available in this build" rather than an empty result set that would
  read as "nothing matched".

---

## How the claims above were checked

```bash
cargo test --workspace          # 205 tests
cargo test -p localspace-core --test whiteboard      # end-to-end, real harness
cargo test -p localspace-client --test surface       # surface ABI conformance
./target/release/localspace doctor
./target/release/localspace evals io.localspace.whiteboard --harnesses harnesses
```

The whiteboard suite installs the actual package — logic running as a wasm
component under wasmtime — and drives it through Core exactly as the agent does:
manifest, tool lint, Tier A runtime, JSON-into-CRDT reconciliation, commits, undo,
the confirmation gate, the context provider at three budgets, and a full agent turn
whose run is then dropped in one action.

---

## Architecture v2 (2026-09-09)

`localspace-architecture-v2.md` replaces the client, the surface ABI, the
inference backend and the build order. `docs/V2-PLAN.md` maps it onto the code.
Its build order, against the tree:

| Step | State | What exists |
|---|---|---|
| 1 proto + Core + axum + generated TypeScript; Tauri and `serve` boot to a login and an empty shell | **done** | `JsonSchema` and `TS` on every proto type; `cargo test -p localspace-proto` writes `web/src/api/generated/`; `/api/v1` with a token login, a session cookie or bearer, `POST /api/v1/request` for any request and `GET` routes for the common reads; `/ws/json` streaming events and answering requests under their id; `/api/v1/openapi.json` generated from the schemas; a React shell that signs in and shows the environment; `localspace-app` (Tauri 2) with Core and the server in-process on loopback, 47 MB private for that process |
| 2 llama.cpp sidecar, planner, model catalog | not started | the planner and the OpenAI-compatible worker with a `grammar` field exist |
| 3 chat harness | **done** except citations (step 6): the GUI, token streaming, persisted conversations | the main GUI in `web/`: chat with streaming deltas, tool calls inline as they happen, approval cards, the composer with stop; the right column with the active model, tools with switches, the context and recent changes; the Agents page with the task ledger, approvals, trace and the exact prompt the model will see; Tools with the active set, capability search and running a tool by hand; Models with an endpoint to connect; Data with the documents and the lock; History with undo, redo and drop-run; Library with install and uninstall; Settings; Help. Verified through the API: a hand-run `canvas.add_sticky` lands as a commit and in the document, and a chat message without a model gets the honest reply |
| 4 harness runtime with iframe surfaces and the bridge SDK | not started | manifest, logic components, install and uninstall exist; the `widgets` kind stays |
| 5 whiteboard as the first package, on tldraw | not started | logic, 20 tools, context provider, evals and the Automerge document exist |
| 6 retrieval, upload, gateway | partial | gateway modes and `web.*` tools exist |
| 7 multi-user `serve`, OIDC | partial | ACL, workspaces, proposals, audit exist; identity does not |
| 8–11 | not started | |

Measured on the reference laptop: the shell's host process 47 MB private; the
WebView2 processes 364 MB private together across 18 processes, above the
150–300 MB v2 §6.6 expects, to be watched as panels arrive. The base bundle is
70 KB gzipped against a 2 MB budget.

### v2 step 2, verified on the review laptop (2026-09-09)

Built: the model catalog (`models/catalog.json`, five entries from a 0.5B
smoke-test model to the 120B mixture of experts the W32 profile is planned
around) with the placement planner's verdict and estimate per entry for this
machine; downloads from Hugging Face on a Core thread with progress events,
refused when air-gapped; import of a file in place; the `llama-server`
sidecar supervisor (`core::engine`): a free loopback port, the plan as flags
(`-ngl`, `--n-cpu-moe`, KV precision, context), health polling until ready,
the worker installed in the router, restart on crash up to three times in
ten minutes, stop on request, a log tail; six new requests and two new
events; the Models page showing all of it.

Measured, with llama.cpp build b10869 (Vulkan, Windows) placed under
`<data>/engines/` and Qwen2.5 0.5B Instruct Q4_K_M downloaded through the
catalog (491,400,032 bytes, the real size correcting the estimate):

| | |
|---|---|
| Load to ready | 16 s, resident, `-ngl 999`, quantised KV, 16,384 context |
| Prompt processing | 862 tokens/s on the RTX 3050 Ti |
| Generation | 87 tokens/s |
| One chat turn, 3,722-token prompt | 4.5 s |
| Whiteboard evals | 3 of 6 passed in 26 s: stickies placed, the board read; a frame not created |

The GUI showed the model in the top bar, the pill went to Ready, the Active
Model card read "Running". Not yet done: the 15 tok/s gate, which needs a W32
machine and the 120B model; the utility model and speculative decoding of
§4.3. The sidecar tests in `tests/engine.rs`, against a fake engine, could
not be run here because Application Control refused their executable; they
are written to run wherever a new unsigned executable may execute.

### v2 step 3, the chat harness (2026-09-09)

Done: the shell of the chat harness in the web client (the main GUI: the
conversation, tool calls inline, approvals, the composer with stop, the
ledger view on the Agents page); **token streaming**: the worker reads the
model's server-sent events and the agent forwards each piece as an
`assistant_delta`, held back only while the model is producing a tool call in
the grammar's shape (measured over the JSON socket: 27 deltas for a
128-character answer, the first 3.6 s in, which is the 3,700-token prompt
being processed); **conversations**: several, switchable, deletable,
renamable, persisted as `conversations.json` under the data directory, the
transcript always the current one, evals kept out of them, a conversation
list beside the chat. Not yet: citations, which arrive with retrieval in
step 6; the tool loop and the GBNF grammar were already there.

### v2 step 4, the harness runtime with iframe surfaces (2026-09-10)

Done: the `web` view kind in the manifest (`kind = "web"`, `module` an
`index.js` inside the package; anything else is refused at validation);
Core's `GetSurfaceFile`, which serves only the directory holding that entry
module and refuses `..`, absolute paths, drive letters and directories
(tested against seven escapes); Core's `WriteDoc`, a surface's whole
document as JSON, reconciled into the Automerge document and committed as
`surface:<view>` by the user only when something changed, with the patch
pushed to every client; the server's harness origins, one per harness at
`h-<slug>.localhost:<port>` (or a wildcard an organisation owns, via
`--surface-hosts`), answered by `Host` header before any other route, opened
by `POST /api/v1/surfaces` into a grant carried as the first path segment of
everything the frame loads (`/s/<token>/index.js`; no cookie, because a
browser withholds cookies from a third-party frame, which the harness frame
is to the shell), a generated page whose import map is admitted by a
per-response nonce, and a Content-Security-Policy on every response
(`default-src 'none'`, scripts and connections from the origin only,
`frame-ancestors` the one shell that opened the view); the bridge SDK,
`@localspace/harness-sdk`, embedded in the server and typed in
`web/public/harness-sdk.d.ts`; the shell's panels beside the chat (at most
six, kept mounted behind their tabs), the iframe host that answers the
surface's hello with the document and forwards Core's patches, writes and
messages, the `widgets` kind rendered from the logic's tree, and an honest
notice for `egui` and `stream` views; zoom in the top bar acting on the
active web panel as a command; Open buttons per view on the Tools page;
the whiteboard's own small `web` view, a plain ES module, to prove the loop
before tldraw. Tests: 2 in the manifest and registry for the rule and the
escapes, 5 in the server for slugs, hosts, the grant path and the policy, 1
API test walking the grant, the page, the module, the SDK, four escapes, a
missing grant and a foreign origin. The api test exe and a release-profile
Core test exe ran on this laptop; the debug Core test exe was refused by
Smart App Control as before. Two things the tests could not show and the
browser did: an inline import map is a script under CSP and needs the nonce,
and a `SameSite` cookie set by the frame's first response is never sent with
its module fetches, which is why the grant moved into the path.

Seen live in the browser against `localspace serve` on 2026-09-10: Tools →
"Board (web)" opens the panel beside the chat; the frame on
`h-io-localspace-whiteboard.localhost:8443` says hello, receives the board
and draws its sticky; zoom in the top bar reaches the frame as a command
(110% shown in both); a `write_doc` through the API makes one `surface:web`
commit by the user ("added 1, changed 4 field(s)"), writing the same document
again makes none, and the frame redraws with the second sticky from the
`doc_patch` that follows. The one hop not driven by hand is a click inside
the frame: the review browser's automation does not reach into a
cross-origin frame, so the surface's own "Add sticky" was exercised only by
reading its code; it posts through the same function as the hello that was
seen.

### v2.1, and step 5 under way (2026-09-10)

The architecture was revised to v2.1 the same day (`docs/V2-PLAN.md` §11):
the product layer is built in-house, so step 5 is `@localspace/canvas` and
the whiteboard as the first downloadable package, not a surface on tldraw.
The tldraw surface built that morning is parked on `spike/tldraw`; it had
proved the bridge end to end in a real browser and found that the SDK
refuses production use without a key and calls its vendor's CDN.

Landed on `master` before the canvas work, each verified where the review
machine let a test binary run: catalog installs are copied under the data
directory, come back after a restart and go on uninstall, with the document
restored from the DAG (2 tests); a document write without a commit and
numbered writes on the bridge, closing a race in which a document read
between two writes deleted the user's fresh edit (1 test); the desktop shell
installs nothing by default and keeps the user's data under
`%LOCALAPPDATA%\localSpace`; `data:` is allowed in a harness origin's
connect-src; the `native` view kind is reserved in the contract and refused
at validation with a reason (1 test); the workspace warns on
`clippy::unwrap_used`, every crate denies `unsafe_code`, the five unsafe
blocks are documented; the CLI's output goes through one module; TanStack
Query is gone; `cargo run -p localspace-proto --bin export-ts` writes the
bindings where a fresh test executable is refused.

Landed later the same day, as the step's own work: `@localspace/canvas`
(`web/packages/canvas`, no runtime dependency: camera, retained scene graph
over an R-tree, hit-testing, selection and handles, text layout, the seven
shape kinds, the tools, level-of-detail rendering; 19 node tests; a
benchmark page with a headless runner and a laptop baseline);
`@localspace/ui` (tokens, controls, overlays, docking, an own icon set, a
store on `useSyncExternalStore`) with the shell moved onto it and off
Tailwind, Radix, Zustand, lucide-react and react-markdown; React, the
canvas, the UI library and Automerge served once per harness origin through
the import map; the replica channel in the SDK and Core, sync messages both
ways, every message that changes the document a `surface:sync` commit by
the user; the whiteboard's web surface (8 KB) on the canvas with the
document as an Automerge replica; undo, redo and run drop as forward
changes, so a replica follows them instead of sending the undone change
back; the edition 2024 switch as its own commit; the gate walk as a browser
test.

Verified on the review laptop (2026-09-10), a 0.5B model loaded:

| check | result |
|---|---|
| `node e2e/whiteboard.mjs`: install from the catalog, the board on its own origin shows Core's document, the agent's note lands and the frame shows it, a note typed in the frame lands as the user's `surface:sync` commit, Ctrl+Z twice and Ctrl+Shift+Z twice through Core's history with the frame following, zoom from the shell's bar reaches the frame | PASS; the agent added 1 shape |
| `node e2e/bench-canvas.mjs --profile laptop`, 5,120 shapes at 1600×900 | pan and drag p95 7.0 ms at both zooms, which is the runner's pointer cadence; paint p95 2.9 ms with all 5,120 drawn and 3.0 ms at reading zoom; 142.9 fps at the worst p95 |
| the whiteboard evals on Qwen2.5 0.5B Instruct, through the API | 3 of 6, as in step 2 |
| `cargo test --release -p localspace-core --lib docs::`, `--test sync`, `npm test` | 11, 2 and 19 pass |
| gzipped: the shell bundle, canvas, ui, the whiteboard surface, the Automerge library | 110 KB, 14 KB, 5.6 KB, 3.5 KB, 1.6 MB |

Not done: the 60 fps at 5,000 shapes gate on the W32 machine, which is not
available here; the frame-time regression check in CI, as the repository
has no CI yet; snapping and SVG/PNG export, which follow the gate
measurement by decision; the IndexedDB storage adapter for the replica,
raised as a question instead; the Automerge library's size, the vendor's
full build with its WebAssembly inlined as base64.
