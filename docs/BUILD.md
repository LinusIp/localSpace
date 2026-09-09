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

## The web client and the desktop shell (architecture v2)

The client is TypeScript in `web/`, built with Vite; its API types are generated
from `localspace-proto`, never written by hand. The desktop app is Tauri 2 in
`crates/localspace-shell`: Core and the API server in one process on a loopback
port, the same web bundle in the system webview.

```bash
cargo test -p localspace-proto      # regenerates web/src/api/generated/ from proto
```

```bash
cd web && npm install && npm run build   # -> web/dist, about 70 KB gzipped
```

Serve it, personal mode, from the repository root:

```bash
cargo run --release -p localspace-server --bin localspace-serve -- --personal --harnesses harnesses --registry registry --data ~/.localspace --web web/dist
```

The server prints its token at start and writes it to `<data>/token`; the
browser asks for it once and keeps a session cookie. `--token` fixes it,
`--user` names the personal user, and without `--personal` the server runs in
organisation mode, where the one token stands for the operator until OIDC
(build order step 7).

The desktop shell needs `web/dist` to exist when it is built:

```bash
cargo build --release -p localspace-shell
```

Then `localspace-app --harnesses harnesses --registry registry --data ~/.localspace`
opens a window already signed in. `--web <dir>` points it at another bundle; an
installed app looks for `web/` beside its executable.

For a client development loop, `npm run dev` in `web/` serves the source with
hot reload and proxies `/api` and `/ws` to a server on port 8443.

The egui client (`localspace`) and its surface SDK stay in the tree until the
web client reaches parity, and the server still answers their postcard socket
at `/ws`.

On this machine Smart App Control has refused some freshly built debug
binaries and accepted release builds; if a new executable "cannot be run due
to an Application Control policy", build it in release.

## Models and the inference sidecar (architecture v2 §4)

Core starts `llama-server` itself, per model, on a loopback port, and turns the
placement planner's plan into its flags. It looks for the binary as
`--llama-server <path>`, then `LOCALSPACE_LLAMA_SERVER`, then
`<data>/engines/llama-server(.exe)`, then PATH. It never downloads an
executable: put a llama.cpp release build there yourself.

The catalog is `models/catalog.json`, compiled into Core; an organisation adds
or overrides entries with a `catalog.json` in the directory given by
`--models <dir>`. Downloads go to `<data>/models/` and are refused in an
air-gapped environment, where a file is imported in place from the Models
page instead. `cargo test -p localspace-core --test engine` exercises the whole
path against `fake_llama_server`, a test double this crate builds.
