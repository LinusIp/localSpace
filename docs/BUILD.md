# Building

## The workspace

```bash
cargo build --release
cargo test --workspace
```

Nothing outside the workspace is needed for that. `harnesses/` is deliberately
excluded from the workspace: those crates target wasm and have their own lockfiles.

## The reference harness

A harness has two wasm artefacts, built for two different targets, because they are
two different kinds of thing.

```bash
rustup target add wasm32-wasip2 wasm32-unknown-unknown
```

**Logic** — a wasm *component*, so it can import the host interface defined in
`wit/harness.wit`:

```bash
cd harnesses/whiteboard-logic
cargo build --release --target wasm32-wasip2
cp target/wasm32-wasip2/release/whiteboard_logic.wasm ../whiteboard/logic.wasm
```

**Surface** — a plain wasm *module*, because a browser cannot run a component
without a transpile step and a surface needs no IO at all. A harness with a
`widgets` surface (like `registry/planner`) has no second artefact at all — the
Client renders the tree:

```bash
cd harnesses/whiteboard-surface
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/whiteboard_surface.wasm ../whiteboard/ui/board.wasm
```

Then:

```bash
localspace --harnesses harnesses
```

### Why the surface still imports something

`egui` depends on `web-sys` unconditionally on `wasm32`, so the linker emits
wasm-bindgen placeholder imports whether or not any of that code is reachable. The
Client stubs exactly those with traps and refuses every other import, so a surface
that genuinely tries to call into the browser fails loudly and one that does not —
the normal case — never touches them. `crates/localspace-client/tests/surface.rs`
asserts that no other import appears.

## Running the server

```bash
localspace-serve --bind 127.0.0.1:8443 --harnesses harnesses --data ./data
```

It refuses to start below the supported hardware floor unless you pass
`--allow-below-floor`, which is logged. Without a built web bundle it serves a page
explaining how to get one; the API at `/ws` works either way.


### The second reference harness

The planning board is `widgets`-only, so it is one artefact:

```bash
cd harnesses/planner-logic
cargo build --release --target wasm32-wasip2
cp target/wasm32-wasip2/release/planner_logic.wasm ../../registry/planner/logic.wasm
```

It lives under `registry/` rather than `harnesses/` so it is *offered* by the
Marketplace rather than installed at startup. That is the whole difference between
the two directories:

```bash
localspace --harnesses harnesses --registry registry
```

`--harnesses` is the environment's installed set; `--registry` is a catalog to
install from. An offline bundle is just a `--registry` directory copied across.

## When it feels slow

See [PERFORMANCE.md](PERFORMANCE.md): `LOCALSPACE_PERF=1` prints per-second frame
costs and gaps, `LOCALSPACE_PERF_SPIN=1` measures the ceiling of the display path,
and the `gpu:` line names the adapter in use.

## Notes for this machine

- On Windows with Smart App Control enforcing, a freshly linked `.exe` is sometimes
  refused with "permission denied" on first run. Copying the binary to a new name,
  or a clean rebuild, clears it.
- Keep `CARGO_TARGET_DIR` short (e.g. `C:/lst`). Long paths bite the wasm builds.
