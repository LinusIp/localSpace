# Building

## The workspace

```bash
cargo build --release
cargo test --workspace
```

Nothing outside the workspace is needed for that. `harnesses/` is deliberately
excluded from the workspace: those crates target wasm and have their own lockfiles.

## The reference harness

One command builds every harness's files from their sources and lays each
package out as its `.hpack` will hold it (architecture v2.1 §7), in
`dist/hpack/<id>-<version>/`. `--in-place` also puts the built files where
`--harnesses harnesses`, `--registry registry` and the tests look for them:

```bash
rustup target add wasm32-wasip2
node scripts/hpack.mjs --in-place
```

`scripts/harnesses.json` records where each harness's sources live; the
layout does not depend on it. What the script does, by hand:

A harness's logic is a wasm component and a `web` view is an ES module bundle,
each built with its own tools.

```bash
rustup target add wasm32-wasip2
```

**Logic** — a wasm *component*, so it can import the host interface defined in
`wit/harness.wit`:

```bash
cd harnesses/whiteboard-logic
cargo build --release --target wasm32-wasip2
cp target/wasm32-wasip2/release/whiteboard_logic.wasm ../whiteboard/logic.wasm
```

A harness whose views are all `widgets` (like `registry/planner`) has no second
artefact at all: the shell renders the tree.

**Web surface** — the board the web client and the desktop app show (v2.1
§6.3, step 5): `@localspace/canvas` against the same document, held in the
frame as an Automerge replica, built with Vite into the package's `ui/web/`
directory. It is a few kilobytes: the canvas engine, the UI library, React
and Automerge come from the shell through the import map.

```bash
cd web && npm install
npm run build              # the shell, and the libraries under dist/_localspace/
npm run build:whiteboard   # -> harnesses/whiteboard/ui/web/
```

The build output is not committed, like the wasm artefacts. Without it the
package's `web` view is refused at install, which is the honest failure.

Then:

```bash
localspace --harnesses harnesses
```

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

The workspace is heavy to build on a 16 GB machine: seventeen integration-test
executables each link Core, wasmtime, Automerge and redb, and one linker can
take gigabytes. So `.cargo/config.toml` caps cargo at four jobs and the `dev`
profile carries line-table debug info only (a full-debug target is 59 GB; with
line tables, 11 GB). Run the one or two suites you are working on, for
instance `cargo test -p localspace-core --test roles`; leave
`cargo test --workspace` to `scripts/check.sh` in WSL (below). If an
editor runs rust-analyzer on the same checkout, give it its own target
directory (`rust-analyzer.cargo.targetDir: true`), or the two builds keep
invalidating each other. Never run a build beside a test server and a
headless browser. `cargo clean` when the target directory has grown stale.

See [PERFORMANCE.md](PERFORMANCE.md): `LOCALSPACE_PERF=1` prints per-second frame
costs and gaps, `LOCALSPACE_PERF_SPIN=1` measures the ceiling of the display path,
and the `gpu:` line names the adapter in use.

## Checking before a push

Until the runner allowance resets in October, `ci` runs only when started by
hand, and nothing runs on GitHub's computers otherwise (docs/DECISIONS.md,
2026-09-24). Every push is checked first on this machine, in WSL, by
`scripts/check.sh`: the jobs of `.github/workflows/ci.yml`, step for step
(formatting, lint and types; the web tests, build, sizes and frame times;
clippy and every test; the two browser walks and the audit; the message
script against the test engine), on a clean copy of the commit about to be
pushed, cloned into `~/localspace-check` and built on WSL's own disk.
**Nothing is pushed that it fails on**, and every report carries the commit
and its last line. It runs in Linux, where Smart App Control has no say.

```bash
wsl -d Ubuntu-24.04 --cd "C:\path\to\localSpace" -- bash -lc scripts/check.sh
```

`--w32-gate` adds the W32 canvas gate, as `ci`'s input of the same name
does; a commit other than HEAD can be named. The first run builds
everything, debug and release (about 25 GB on WSL's disk, which grows on
C:); later runs are incremental.

Setting WSL up is the machine owner's, once, since it needs an administrator,
a restart and a password: in PowerShell as administrator,
`wsl --install -d Ubuntu-24.04`, restart Windows, open *Ubuntu 24.04* from
the Start menu and choose a user name and password; then, in Ubuntu, in this
repository (`cd /mnt/c/path/to/localSpace`),
`bash scripts/wsl-setup.sh`, which says what it fetches and from where.

In October `ci` runs once by hand on the head, as the check from a clean
computer, and whether automatic runs come back is decided then.

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
`--user` names the personal user. Without `--personal` the server runs in
organisation mode, where accounts sign in with an email and a password:
`--bootstrap-admin you@example.com` makes the first administrator on a server
with no accounts and prints their one-time link (valid 24 hours), and every
other account is made by an administrator in the app. `--public-url` is the
address users open, for the links the server prints.

The desktop shell needs `web/dist` to exist when it is built:

```bash
cargo build --release -p localspace-shell
```

Then `localspace-app --harnesses harnesses --registry registry --data ~/.localspace`
opens a window already signed in. `--web <dir>` points it at another bundle; an
installed app looks for `web/` beside its executable.

For a client development loop, `npm run dev` in `web/` serves the source with
hot reload and proxies `/api` and `/ws` to a server on port 8443.

The whiteboard's egui surface was retired on 2026-09-11. The egui client
(`localspace`) and its surface SDK stay until the web client can do what they
do, as `docs/DECISIONS.md` lists; the server still answers their postcard
socket at `/ws`.

On this machine Smart App Control has refused some freshly built debug
binaries and accepted release builds; if a new executable "cannot be run due
to an Application Control policy", build it in release.

## Models and the inference sidecar (architecture v2 §4)

Core starts `llama-server` itself, per model, on a loopback port, and turns the
placement planner's plan into its flags. It looks for the binary as
`--llama-server <path>`, then `LOCALSPACE_LLAMA_SERVER`, then
`<data>/engines/llama-server(.exe)`, then `engine/` beside its own executable
(what a package carries), then PATH. One placed by hand comes before the
package's, so a faster build put under `<data>/engines` wins. Core never
downloads an executable: in a checkout, put a llama.cpp release build there
yourself, or let `node scripts/fetch-engine.mjs --out <data>/engines/llama-server`
fetch the pinned one (the script empties the directory it is given, so give
it that one and not `<data>/engines`, where the engine's logs are).

The catalog is `models/catalog.json`, compiled into Core; an organisation adds
or overrides entries with a `catalog.json` in the directory given by
`--models <dir>`. Downloads go to `<data>/models/` and are refused in an
air-gapped environment, where a file is imported in place from the Models
page instead. `cargo test -p localspace-core --test engine` exercises the whole
path against `fake_llama_server`, a test double this crate builds.

## The Windows package (the installer and the portable zip)

Until October the package is built on this laptop, and only when a release
is being prepared (docs/DECISIONS.md, 2026-09-24): the `package` workflow
(`.github/workflows/package.yml`, started by hand or by a `v*` tag) stays
for later, and keeps what it builds one day only, to be downloaded here:
the repository is public, and anything public goes out only as a GitHub
Release when the founder decides (2026-09-25). What ten people run is still what a commit produced: the build is
of a commit that `scripts/check.sh` passed, **the file the release check runs
on is the file the testers get**, and its SHA-256 goes into the report. The
steps, on Windows:

```bash
(cd web && npm ci && npm run build)
cargo build --release -p localspace-cli
cargo install tauri-cli --version 2.11.4 --locked
node scripts/package.mjs          # -> dist/windows/: …-setup.exe, …-portable.zip, SHA256SUMS.txt
```

`scripts/package.mjs` lays the package out in `dist/package/` (the web
client, the Store's catalog through `scripts/hpack.mjs`, the command line,
the licences, and the engine), builds the NSIS installer with
`cargo tauri build --config tauri.bundle.conf.json` in
`crates/localspace-shell`, and zips the same files with the app as the
portable copy. `--stage-only` stops after the layout.

**The engine is llama.cpp's own Vulkan release, pinned.**
`scripts/engine.json` names the release, the asset, its size and its SHA-256;
`scripts/fetch-engine.mjs` downloads it (or takes `--from <archive>`), checks
size and digest **before unpacking**, and fails the build on a mismatch. Only
the files the engine needs are kept. Moving to another release means changing
the pin and reading the keep list again; the script says when a listed file
is missing.

**When the pin moves**, before a build with the new engine reaches anyone:
- the keep list is read again (above);
- `scripts/message-script.mjs` runs on every model in the catalog, each on a
  fresh data folder, and the answers are read: moving the pin is a
  regression event for every catalog model (docs/DECISIONS.md, 2026-09-23);
- its last step, Continue, is read for every catalog default
  (docs/DECISIONS.md, 2026-09-24). An answer is stopped after thirty words
  and carried on: the join must read as one sentence, the answer must not
  begin again, and `app.log` must not say "continue: the engine did not
  begin its reply …". Today's engine says the handed words again, exactly,
  and Core drops them; a new pin may not, and then Core drops nothing and
  says so in that line;
- how the engine divides `-c` between its slots, read from its `/props`
  and `/slots` under `--parallel 1`, `--parallel 2` and no `--parallel`
  (docs/DECISIONS.md, 2026-09-24): b10869 splits `-c` evenly under
  `--parallel N`, shares one pool of `-c` between all slots without it or
  with `--kv-unified`, and Core's `--parallel N -c N×C` promise rests on the
  first.

The installer is per-user (`%LOCALAPPDATA%\Programs\localSpace`, no
administrator prompt) and asks nothing but the usual folder page. It does not
download WebView2: Windows 11 always has it, and when it is missing the app's
dialog says what to install and opens Microsoft's page. The person's data is
in `%LOCALAPPDATA%\localSpace`, which the uninstaller removes only when
"Delete the application data" is ticked. The app writes `logs\app.log`
there in a release build, since it has no console.

tauri-cli's stock installer puts a per-user program in
`%LOCALAPPDATA%\<product>`, which is where the data lives, and has no setting
for it. So `packaging/windows/installer.nsi` is a copy of tauri-cli 2.11.4's
template with that one line changed (its header says from which upstream
file), and `packaging/windows/hooks.nsh` makes the uninstaller's checkbox
remove the data folder. Take the copy again when the tauri-cli pin moves.

**Not signed yet, and ready to be.** Until the certificate exists,
SmartScreen warns and Smart App Control, where it is on, refuses the
installer and the zip's executables outright. The path a certificate takes
is built and rehearsed: `scripts/package.mjs` reads
`LOCALSPACE_SIGN_COMMAND`, what signs one file, as JSON in the form tauri's
`bundle.windows.signCommand` takes,
`{"cmd": "…\\signtool.exe", "args": ["sign", "/fd", "SHA256", …, "%1"]}`.
With it set, **every executable and library of the package is signed**, the
engine's and the command line's too (Smart App Control judges each file a
program loads, not only the installer), and tauri signs the app, the
installer and the uninstaller with the same command. The command is never
printed, since its arguments may hold a secret. `gh workflow run package.yml
-f sign=rehearsal` makes a throwaway certificate on the runner, signs with
it, and refuses the build if one installed program file, or the installer,
is not signed by it. When the real certificate arrives: whatever its
provider needs on the runner (a tool, a login), and
`LOCALSPACE_SIGN_COMMAND` from a secret; nothing else changes. A
certificate's signature also wants a timestamp (`/tr <the provider's
address> /td SHA256`), which the rehearsal leaves out.

What the workflow proves on a clean Windows runner: the installer runs
silently and lays out every file; the engine and the command line run from
where they were put, with no GPU (the CPU fallback's first step); the app
makes its data folder, starts, serves its client and opens its window; a
second launch gives way to the first; the uninstaller removes the program and
leaves the data. What it cannot: SmartScreen and Smart App Control, a real
GPU, a Windows in another language, and what a person sees. Those belong to
runs on real machines, before a build ships:

- **A dry run on a machine that is not the builder's**, from the link a
  person will get, following the sheet to the letter
  (`docs/test-a/RUNBOOK.md`).
- **Smart App Control on**: the development laptop has it on, and the
  release build is installed and started there.
- **A Windows that speaks another language** (ruled 2026-09-23): the release
  build runs at least once on a Windows whose answers are not in English,
  and the development laptop answers in Russian. Read the first lines of
  `app.log`: the operating system's name, the system memory, every graphics
  card and what other programs hold of it must all be there. The build Test
  A shipped told such a Windows it had "1 GB of system memory" and offered
  only the smallest model. A build checked only on English Windows has not
  been checked for the people it is for.
