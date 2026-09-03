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
| H§16.5 | Budgets in CI | **partial** | `localspace bench` reports the budgets that do not need a loaded model. The model-dependent ones are stated as needing `serve`'s `/metrics`. **Not wired into CI.** |

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
cargo test --workspace          # 164 tests
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
