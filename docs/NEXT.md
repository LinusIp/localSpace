# What to do next

Architecture v2.1 (2026-09-10) sets the order now: its build order is in
`docs/localspace-architecture-v2.md` §13, and `CLAUDE.md` sets how the work
is done. Steps 1 to 4 are done: the generated API and the React shell, the
`llama-server` sidecar with the model catalog, the chat harness with
streaming and conversations, and the harness runtime with iframe surfaces
and the bridge SDK (`docs/V2-PLAN.md` §8–11). Step 5, `@localspace/canvas`
and the whiteboard as the first downloadable package, is complete, with
three measurements open: its 60 fps number and the whiteboard's evals on the
reference model, taken on one W32 trip, and the workflow's first run once
the repository has a remote (`docs/STATUS.md`). Step 6 is next: retrieval
with the ACL pre-filter, document upload, and the network modes with the
gateway. Behind it, the 15 tok/s gate on a W32 machine, then OIDC (step 7). The items below are the seams inside Core that those
steps land on; they still hold, and their numbering is the older one.

Step 6, in the order approved on 2026-09-12 (`docs/DECISIONS.md`): 6.0 PNG
and SVG export as artifacts, with the interchange-types package; 6.1
citations in the proto and the chat; 6.2 documents as files; 6.3 upload;
6.4 extraction and chunking; 6.5 embeddings; 6.6 the index and
`docs.search`; 6.7 the gateway's gaps and the zero-connection job; 6.8 the
evals page and the `localspace` command, after which the egui client goes
to `archive/egui-client`. The spec changes it authorised are in deployment
§3.3 and §9.1 and plugin spec §16.1.

Step 5 leaves, in the order they come:

- **The W32 trip.** On the W32 machine, in `web/`:
  `node e2e/bench-canvas.mjs --profile w32 --shapes 5000 --out ../docs/gates/w32-canvas.json`,
  then commit the file; the workflow's manual `w32-gate` job checks it
  against 60 fps with 5,000 shapes on screen. On the same trip, the
  whiteboard's evals against the reference model, through the API until step
  6 gives evals a page and a command: load the model on the Models page,
  then send `{"run_evals":{"harness":"io.localspace.whiteboard"}}` to
  `POST /api/v1/request`. If they fail there, step 5 reopens for the tool
  descriptions, the front doors and the context provider.
- **The workflow's first run**, once the repository has a remote: commit the
  frame-time baseline it records as `web/packages/canvas/bench/baseline.ci.json`,
  and see its Linux builds of the Tauri shell and the egui client through,
  which have not been tried.
- **SVG and PNG export** as artifacts through Core (answer 4).

Small things step 4 left open, in the order they will matter:

- **Automerge in the client** landed in step 5 (decision of 2026-09-10):
  `@automerge/automerge` in the whiteboard frame over the bridge's sync
  channel to Core, every message that changes the document a commit, with a
  sync state per replica. Frames in the shell keep no IndexedDB copy
  (answer 2); a local replica comes back with detached surfaces, merged only
  after Core's snapshot. Whole-document JSON writes stay for blob documents.
- **A policy header on the shell page.** The harness origins carry a strict
  CSP; the shell's own `index.html` is served by `ServeDir` without one. It
  should name `frame-src` as the harness hosts and nothing else.
- **A memory budget per frame** when a browser exposes one. The SDK's
  `snapshot()` (v2 §6.3) now hands a surface Core's Automerge bytes.
- **Grants expire only by count.** Five hundred and twelve are kept; an
  organisation with more open surfaces than that wants an age limit and a
  revocation on logout.

---

## 1. Measure tool selection on a real model — the gate before anything else

H§15 names this as the whole bet, and H§14 step 8 makes it a gate. Everything below
is wasted if a reference model cannot drive one harness.

Point the Client at a real endpoint and run:

```bash
localspace evals io.localspace.whiteboard --harnesses harnesses
```

The eval runner, the grammar, the active-set computation and the six whiteboard
cases are all built, and since the sidecar of v2 step 2 the number exists: **3 of
6 on Qwen2.5 0.5B Instruct Q4_K_M**, 26 s, on the review laptop (stickies placed
and the board read; a frame not created). The same 3 of 6 on 2026-09-10
through the API on the v2.1 stack, 3.4 s with the model already loaded: one
sticky, a frame and the read pass; three stickies, a title and an arrow
fail. Record it per model — the 3B and 7B
entries in the catalog are the next two, then the W32 gate — widen `evals.json`
toward the 5–20 cases the spec asks for, and add a second harness so
`find_capability` is exercised under a real tool count. The eval prompts share
the environment's transcript and commits today; a run should get its own.

**Seam:** `evals.rs`, `harnesses/whiteboard/evals.json`.
**Done when:** a pass rate exists for at least two models, and the malformed-tool-call
rate from `Metrics::malformed_rate` is under 0.5 %.

## 2. The browser Client

The one structural gap. `localspace-serve` already speaks the whole protocol; the
Client crate is written to compile for `wasm32-unknown-unknown`; the browser
`SurfaceRunner` is stubbed with an explicit error rather than silently doing nothing.

Three pieces:

- build the Client with trunk/wasm-bindgen and serve it from `--web`;
- implement `surface::imp::Runner` against the browser's `WebAssembly` API — the same
  three exports, the same postcard bytes, a small JS shim;
- switch the surface's document feed from `GetDocJson` to the Automerge replica sync
  that Core already implements and tests, so a board over a WAN sends changes rather
  than snapshots.

**Seam:** `crates/localspace-client/src/surface.rs` (the `#[cfg(target_arch = "wasm32")]`
module), `Core::handle`'s `DocSync` arm, `docs.rs::sync_message` / `receive_sync`.
**Done when:** the whiteboard runs in a browser against `serve` and two browsers
co-edit one board.

## 3. Identity, then shared workspaces

D§4 is the largest deployment gap and everything in D§5–6 depends on it. The ACL
model, workspaces, proposal branches and per-document checks are already built and
tested — they are simply always answered as one local user today.

Add OIDC (Authorization Code + PKCE) and a session store, map `group_claim` onto
`acl::Principal::Group`, and key the server's session map by the authenticated
subject instead of a per-connection token.

**Seam:** `localspace-server/src/main.rs::Server::session`, `acl::Identity`.
**Done when:** two users in different browsers see different environments, and a
`view` member's sync messages are rejected — which `AccessControl::check` already
does, given a real identity.

## 4. An inference worker that consumes a placement plan

The planner produces a plan; nothing executes it. Implement one `ModelWorker` that
loads weights per a `PlacementPlan` — `llama-cpp-2` first, because per-tensor device
overrides and CPU MoE kernels are the proven path on W32 today.

Then the H§16.5 numbers become measurable rather than estimated: replace the
planner's roofline with a micro-benchmark recorded at install, and report
prompt-cache hit rate, draft acceptance and decode tok/s on `/metrics`.

**Seam:** `model::ModelWorker` (one trait, already the only thing the router knows),
`planner::PlacementPlan`.
**Done when:** a 100B+-class MoE at Q4 loads on a W32 machine with one click and the
planner's verdict matches what actually happens.

## 5. Close the Tier B sandbox, or keep Tier B off

Today a Tier B child gets a cleared environment and a pinned working directory. That
is not a sandbox. Either add the OS-level isolation (Windows job object +
AppContainer, Linux landlock + seccomp, macOS sandbox profile) or leave Tier B
disabled in organisation mode, which is the current default.

Do this before the physics harness, not after: H§15 is right that Tier B is where
security actually breaks.

**Seam:** `runtime/native.rs::NativeHarness::spawn`.

## 6. Packaging and the registry protocol

Kinds, dependencies, the resolver and `environment.lock` exist (spec §17.1–17.2).
What is missing is the distribution around them: `.hpack` archives, signing with
the localSpace release key verified before `Registry::stage` returns, the HTTP
index (`/index`, `/packages/{id}/{version}`, `/search`, `/revocations`), delta
updates, yank/deprecate/channels, and — the real engineering item — linking a
`library` package into its dependents by Component Model composition. Today a
library is resolved, installed and locked, and then does nothing.

**Seam:** `catalog::candidates` (an index file beside the directory scan),
`registry::Registry::stage` (signature check first), `registry::instantiate`
(composition before instantiation).

## 6a. The rest of inter-harness communication

The ledger, typed artifacts and the document-as-bus handoff are built and tested.
Still to do from spec §18.2–18.4: **interface calls** (`harness:call` by interface,
routed by Core to any provider — the WIT import and the capability), **events**
(`harness:subscribe` to `doc.changed(type)` or a topic), **specialist sub-agents**
(a per-harness skill run in its own context, returning artifacts and a summary),
**converter harnesses** when types do not match, and the three-harness release
bench (sketch → simulate → board) — which needs a third harness.

**Seam:** `wit/harness.wit` (`host` interface), `Core::resolve_handoff`,
`agent::run_loop`.

## 6b. Meeting the 50 MB budget

Spec §1.2 caps the Client and Core at 50 MB private RSS each. `bench` prints the
number: headless Core is 5 MB private with a harness instantiated, inside budget;
the desktop process is roughly 300 MB, so the Client half is what is over. The path the spec itself
lays out: make chat, settings, the store and the admin console harnesses on the
same contract so Core holds only the proto server, the DAG, the document store,
the permission checker, the scheduler, the planner and the runtime; memory-map
blobs and documents instead of holding them in the heap; and measure what wgpu and
the two wasmtime engines cost at idle before deciding what to do about them.
Budgets and unloading are in place; the shell is what is heavy.

**Seam:** `footprint::Footprint` (the measurement), `Registry::unload_idle`.

## 7. Efficiency items with a measurable payoff

In rough order of value per hour:

- **AOT-compile wasm components at install**, cached by module hash. Today every
  process start JIT-compiles both whiteboard modules — the largest share of the
  desktop app's startup CPU, and glacial in a debug build where Cranelift itself
  is unoptimised. (`registry::instantiate`, `surface::Runner::load`.)
- **Cache compiled grammars on disk** by active-set hash — the in-memory cache
  already exists and is keyed correctly.
- **zstd above 4 KB** on the WebSocket. One place, both directions.
- **Compaction**: fold old turns into a DAG-backed summary on the utility worker.
  The working set is already bounded; today the fold is a marker rather than a
  summary. (`prompt::render_conversation`.)

## 8. A third reference harness

The planning board (`registry/planner`) already exercises `find_capability` across
two harnesses, the pinned-harness budget path, and the `widgets` surface end to end.
The one still missing is a physics sim: it proves `blob` documents, `stream` surfaces
and Tier B together — but it depends on items 5 and 2.

---

## Smaller things worth fixing when you are next in the area

- `bench` does not drive synthetic users; it reports only what needs no model.
- Blob documents are not memory-mapped; reads copy.
- The audit log has no syslog/CEF export and no retention.
- There is no `localspace.toml`; the server is configured entirely on the command
  line, which will not survive a real deployment.
- Default sticky placement can land a card on top of a frame's title.
- The whiteboard still has no grouping, rotation, connector rerouting, images,
  comments, presence cursors or templates. Multi-select, resize, snapping,
  z-order, locking, ink and the clipboard are in; the rest is not.
- The Marketplace lists a directory. It does not yet verify a signature, so in an
  organisation it should point only at a bundle the security team assembled.
- Found and fixed: programmatic focus and maximise (`SwitchToThisWindow`,
  `ShowWindow`) delivered stray input to the surface — once an empty sticky with
  its editor open, once every shape on a board deleted while the frame survived,
  which is exactly Ctrl+A then Delete. The runner forwarded keyboard on hover.
  It now forwards keyboard only while the canvas holds keyboard focus, which it
  gets by being clicked, and drops any document write until Core has handed over
  the real document. An untouched launch never lost anything, before or after.
- The task ledger lives in Core's memory for the life of the process. Artifacts
  carry across turns, not across restarts; a `localspace call` from the CLI is one
  process, so an artifact it registers is gone by the next call. Persisting the
  ledger as a DAG document is the obvious next step.
