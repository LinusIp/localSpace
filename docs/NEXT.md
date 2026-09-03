# What to do next

Ordered by what unblocks the most, not by what is easiest. Each item says where the
seam already is, so none of these is a rewrite.

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

## 6. Packaging

The Marketplace already reads an offline bundle, shows what a package would be
allowed to do, and holds a widened install at an approval. What is missing is the
distribution around it: `.hpack` archives, publisher signing verified before
`Registry::stage` returns, an HTTP index for connected installs, and the org review
queue with pinned versions and rollout groups.

**Seam:** `catalog::scan` (an index file alongside the directory scan),
`registry::Registry::stage` (signature verification before anything is parsed).

## 7. Efficiency items with a measurable payoff

In rough order of value per hour:

- **AOT-compile wasm components at install**, cached by module hash. A tool call
  should be microseconds, not a JIT. (`registry::instantiate`.)
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
