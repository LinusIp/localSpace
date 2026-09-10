# Decisions

Every answered question and every decision made during the build, newest
first, with the date and the section of the specification it affects. Part of
the source of truth once written (`CLAUDE.md`, "Source of truth").

## 2026-09-10, answers to the step-5 report

- **Snapping** (answer 5; architecture §6.4). A move or a resize comes to
  rest on the edges and centres of the shapes and frames on screen when one
  is within 8 screen pixels, the nearest line first, axis by axis; a resize
  snaps only the edges its handle moves. The lines it rests on are drawn as
  guides while the gesture lasts. Alt held skips snapping, also when it is
  pressed or let go in the middle of a gesture. Grid snapping is off by
  default; the whiteboard's toolbar turns it on, and the grid (24 units, the
  dots the board draws) then takes an axis no shape is near. The setting
  lives in the frame and is not saved. Targets are the shapes on screen
  when the gesture begins, not the whole board, so a guide never leads off
  screen and the cost follows what is visible; arrows and ink strokes are
  not targets. `web/packages/canvas/src/snap.ts` with 11 tests; the gate
  walk drags a note in a real browser (`web/e2e/whiteboard.mjs`, step 7).
- **A typed note's text is one commit** (answer 10). A sticky note typed
  at creation is two commits, its creation and its text; the text is one
  commit however many keys it took, because the editor commits a text edit
  when the text editor closes (Escape, Ctrl+Enter, or focus leaving it),
  not per keystroke. The gate walk asserts exactly two commits for a
  29-character note (`web/e2e/whiteboard.mjs`, step 4).

## 2026-09-10, during the step-5 build

- **Undo, redo and a run drop are forward changes on a crdt document.**
  (architecture §6.1, §6.3; the DAG of plugin spec §7.) The DAG moves the
  document's head to the parent commit as before; the live Automerge
  document is no longer swapped for the parent's snapshot but reconciled to
  the parent's content, field by field, as one more change. Swapping the
  bytes undid nothing while a surface held a replica: the replica still had
  the undone change, the sync protocol sent it straight back and Core
  committed it again. A document with no changes yet still takes a snapshot
  as it is, history and all, which install and start rely on. Tests:
  `docs::tests::restoring_into_a_document_with_changes_is_one_more_change_not_a_replacement`,
  `crates/localspace-core/tests/sync.rs`.
- **Strings in the whiteboard document are Automerge's plain strings.**
  Core writes `ScalarValue::Str`; `@automerge/automerge` surfaces those as
  `ImmutableString`, so the surface reads them as JavaScript strings and
  writes the same kind back, and both ends hold one representation.
  Collaborative text (`Text`) is not used for shape labels at this step.
- **A sticky note typed at creation is two commits**, its creation and its
  text, as in any editor: the first Ctrl+Z takes the text, the second the
  note. (§13 step 5, "undo through the DAG".)

## 2026-09-10, answers to the step-5 questions

- **The tldraw surface is parked** on the branch `spike/tldraw`, a reference
  for bridge semantics only; nothing on `master` may import from it. The
  package, its dependency and lockfile and every tldraw reference are gone
  from `master`; the licence finding stays below. (architecture §1
  principle 4, §6.4.)
- **The shell's third-party UI dependencies go during step 5.** TanStack
  Query, unused, is dropped now. `@localspace/ui` replaces Tailwind, Radix,
  Zustand and any layout library; lucide-react is replaced by an own SVG
  icon set inside `@localspace/ui`. Markdown: the remark parser is
  infrastructure under principle 4 and may stay behind a wrapper; the
  rendering components must be ours, so react-markdown goes. (§6.1.)
- **`@localspace/canvas` scope for the gate:** camera, retained scene graph,
  R-tree culling, hit-testing, selection and handles, text, the seven
  whiteboard shape kinds, undo through Core. Snapping and SVG/PNG export
  follow inside step 5 after the gate is measured; the WebGPU path is
  deferred. The package ships its benchmark and test suite from its first
  commit (§15). (§6.4, §13 step 5.)
- **The 60 fps at 5,000 shapes gate** is measured on the W32 machine with the
  scripted benchmark page and recorded in the step-5 report and here. CI
  keeps a headless frame-time regression check against that baseline
  without asserting 60 fps. The review laptop's 7 fps display fault is an
  environment issue, not a product number. (§13 step 5.)
- **playwright-core is an approved dev-only dependency** for the end-to-end
  gate, on three conditions: it downloads nothing, it is never part of a
  production build or the base bundle, and it drives a browser already on
  the machine. (`CLAUDE.md`, "What you must never do".)
- **Rust standards apply to new code; existing modules migrate when
  touched.** The edition 2021 to 2024 switch is one dedicated commit with no
  other change. `clippy::unwrap_used` is a warning now and denied in every
  new crate; `#![deny(unsafe_code)]` goes on every crate without `unsafe`
  today; the five existing `unsafe` blocks get a `// SAFETY:` comment each
  and stay. `println!` in the CLI binaries' own user-facing output is
  legitimate but goes through one output module; everything else uses
  `tracing`. (`CLAUDE.md`, "Coding standards".)
- **The marketplace spec is the fourth document** in the hierarchy,
  authoritative for the catalog, entitlements, package types and the org
  catalog; plugin spec §12 is not a substitute. It is in `docs/`.
- **Today's spec-neutral work is committed now** as four conventional
  commits: install persistence; the commit flag and write numbering; the
  shell defaults; the harness-origin policy fix.
- **Automerge comes into the client in step 5** for the whiteboard document:
  `@automerge/automerge` with the own network adapter to Core and the own
  IndexedDB storage adapter (§6.1). Whole-document JSON writes cannot meet
  the 5,000-shape gate. Write numbering stays for blob documents. Core holds
  the authoritative Automerge document for the whiteboard from this step on.
- **The `native` view kind is reserved now** in `localspace-proto`, so a
  manifest declaring it parses and fails installation with a clear "not
  supported" error rather than a parse error; the implementation waits for
  step 8. (§6.3, §13 step 8.)
- **The step-5 plan is approved** with the amendments above folded in:
  Automerge for the whiteboard document is in scope; `@localspace/ui` covers
  only what the shell needs today (tokens, buttons, inputs, menus, dialogs,
  icons, docking); the canvas package has zero runtime dependencies; undo
  remains Core's; the selection stays uncommitted (`commit: false`); the
  benchmark page lands in the first canvas commit.

## 2026-09-10

- **Architecture v2.1 received; tldraw stopped.** (architecture §1 principle 4,
  §6.1, §6.4, §13 step 5.) The revised architecture builds the product layer
  in-house: `@localspace/canvas`, `@localspace/ui`, an own store and fetch
  client; no tldraw, Tailwind, Radix, Zustand, TanStack, dockview or Monaco.
  The tldraw surface built earlier the same day is superseded. The coding-agent
  prompt was adopted as `CLAUDE.md`; the questions it raises against the
  existing code are open (see the step-5 report of the same day).
- **Finding, for the record (superseded by v2.1):** tldraw 5.4.1's licence
  allows unlicensed use in development environments only and enforces it in
  the SDK: on a production build the editor hides itself after five seconds;
  it also calls `cdn.tldraw.com` for a watermark tracker. Both are reasons
  it could not have shipped under the deployment spec's no-outbound rule.
- **Catalog installs persist.** (plugin spec §12, architecture §13 step 5
  "installed from the catalog, not bundled".) A package installed from a
  catalog is copied under `<data>/installed/<id>` and loaded again at start;
  uninstall removes the copy; a package loaded in place with `--harnesses`
  is left alone. Its document is restored from the DAG on install.
- **A surface write may skip the commit.** (architecture §6.3.) `WriteDoc`
  gained `commit: bool` (default true); the bridge's `write(doc, { commit:
  false })` moves the document and notifies every client without a history
  entry, for state such as the selection. Writes are numbered (`seq`) and
  every document the shell hands a surface carries `written`, the number of
  the surface's last write Core had taken in, so a surface can drop a
  document read before its own write landed.
- **Only chat in the box by default.** (architecture §1 principle 3.) The
  desktop shell without flags installs nothing, offers `registry/` beside the
  executable (or `registry/` and `harnesses/` in a checkout) as the catalog,
  and keeps the user's data under `%LOCALAPPDATA%\localSpace`
  (`$XDG_DATA_HOME/localspace`, `~/.localspace`).

## 2026-09-09

- **v2 stands, in full and as written.** (architecture, whole.) Replaces the
  egui client, the egui surface ABI, the inference backend and the build
  order of the plugin spec; Core stays Rust.
- **tldraw for the whiteboard surface**, v2 §6.4's first choice, licence to
  be budgeted before step 5. *Superseded on 2026-09-10 by v2.1.*
- **The egui client stays in the tree until the web client reaches parity.**
  The server keeps answering its postcard socket at `/ws`.
- **The main GUI** is the chat-centred layout the user handed over: a rail
  (Chat, Agents, Tools, Models, Data, History, Library; Settings, Help), the
  chat in the middle, a right column with the model, tools, context and
  recent changes. The product name stays localSpace.

## 2026-09-08

- **Keep egui; do not rewrite in Floem.** Superseded for the client by v2 on
  2026-09-09; the egui client remains only until parity.
- **The display-driver fault on the review laptop** (AMD integrated GPU,
  problem code 31, panel on the Basic Display Driver) is below every stack
  and is the user's to repair; no system setting is changed by the agent.
