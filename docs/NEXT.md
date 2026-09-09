# What to do next

Architecture v2 (2026-09-09) sets the order now: its build order is in
`docs/V2-PLAN.md` §5, step 1 is done, and step 2 — the `llama-server` sidecar,
the placement plan turned into its flags, and the model catalog with download —
is next, with the 15 tok/s gate on a W32 machine behind it. Steps 3 to 5, the
chat harness, iframe surfaces with the bridge SDK, and the whiteboard on tldraw,
complete the MVP. The items below are the seams inside Core that those steps
land on; they still hold, and their numbering is the older one.

---

## 1. Measure tool selection on a real model — the gate before anything else

H§15 names this as the whole bet, and H§14 step 8 makes it a gate. Everything below
is wasted if a reference model cannot drive one harness.

Point the Client at a real endpoint and run:

```bash
localspace evals io.localspace.whiteboard --harnesses harnesses
```

The eval runner, the grammar, the active-set computation and the six whiteboard
cases are all built. What is missing is the number. Record it per model, then widen
`evals.json` toward the 5–20 cases the spec asks for and add a second harness so
`find_capability` is exercised under a real tool count.

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
- A surface's memory ratchets by 4 MB each of the first few times its font
  atlas is rebuilt, then settles (`tests/surface.rs` prints the curve). The
  doubling chain of the atlas fragments dlmalloc's heap so the next 4 MB does
  not fit the hole the last one left. A global allocator with size classes, or
  an epaint that can start its atlas at full height, would take the whiteboard's
  text-heavy peak from 21 MB to about 13. The base of 7 MB is mostly egui's
  default fonts in the data segment; shipping one font would cut it by 1.5 MB.
- The Library does not yet show a surface's live memory against its budget,
  though the runner now measures it (`SurfaceRunner::memory_bytes`).
- The whiteboard surface draws only what is on screen, but it still clones
  and z-sorts the whole shape list from the document every frame: 1.95 ms of
  a 4.6 ms frame on a thousand-note board, against 2.6 ms for drawing the
  sixty visible notes (`tests/surface.rs`, the thousand-sticky test prints
  both). It clones because the pointer handler mutates the document while the
  list is in use. Borrowing the list and separating the read phase from the
  mutation phase in `canvas()` would make that cost proportional to what is
  visible.
