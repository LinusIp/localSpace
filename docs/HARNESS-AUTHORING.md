# Writing a harness

A harness declares exactly three things. One missing and it is not agent-usable, so
Core rejects it at install rather than at first use.

The reference whiteboard in `harnesses/` is the worked example for all of this.

---

## 1. `harness.toml`

```toml
[harness]
id = "io.example.planner"        # reverse-DNS, immutable across versions
version = "1.0.0"
api = "^1.0"                     # the host harness-api range you built against
title = "Planning board"
publisher = "Example"
tier = "wasm"                    # wasm | native; native needs a native_reason

[capabilities]                   # default deny: anything absent is unavailable
fs = "none"                      # none | workspace | scoped:<subpath>
net = "none"                     # or { allowlist = ["tiles.example.com"], reason = "…" }
gpu = false                      # or { vram_gb = 24, exclusive = false }, native only
docs = "none"                    # none | acl
model = []                       # complete | structured | embed

[contributes]
views = [
  { id = "board", kind = "egui", module = "ui/board.wasm", placement = "main" },
  { id = "settings", kind = "widgets", placement = "side" },
]
tools = "tools.json"
context_provider = true
doc = "crdt"                     # crdt | blob
logic = "logic.wasm"
```

Nothing you do not declare is available to you. Widening capabilities in a later
version does not auto-install: the user is shown a diff of exactly what changed.

## 2. `tools.json`

```jsonc
{
  "name": "board.add_card",
  "summary": "Add a card to a column.",   // ≤ 25 words, linted at install
  "params": { "type": "object", "properties": { … } },  // ≤ 8 top-level properties
  "kind": "write",            // read | write | compute
  "front_door": false,        // at most 3 per harness; visible when unfocused
  "undoable": true,
  "confirm": "never",         // never | destructive | always
  "cost_hint": "instant"      // instant | seconds | long
}
```

Install fails if a summary is too long, a schema has too many properties, more than
three tools are front door, a `write` is neither undoable nor confirmable, a name is
not namespaced, or the descriptions together blow the profile's token budget. These
are not style rules — they are what keeps the model able to choose.

**Front doors are the two or three tools worth having in context when your harness
is not the one being looked at.** Pick a read that describes the state and the one
write a user is most likely to want.

## 3. The three exports

Generated from `wit/harness.wit` with `wit_bindgen::generate!`:

```rust
fn tools() -> String;                            // your tools.json
fn call(name: String, params: String) -> String; // {"ok", "result", "diff-summary"}
fn context(budget_tokens: u32, focused: bool) -> String;  // {"text", "expandable"}
fn view(view_id: String) -> String;              // a widget tree
fn event(view_id: String, payload: Vec<u8>) -> Vec<u8>;   // commands from a surface
```

`localspace-harness-sdk` provides the envelopes (`ToolResult`, `ContextBlock`), a
`BudgetedText` writer, and widget-tree helpers, so you are not hand-writing JSON.

### The document is the state

Read it with `doc-get`, edit the JSON, write it back with `doc-put`. Core reconciles
your JSON into the CRDT field by field — a two-word text edit stays a two-word change
in the history and over the wire — then commits it and pushes the patch to your
surface. Do not keep state in guest memory that the document does not describe: your
logic and your surface may be on opposite sides of a network.

### The context provider is the hard part

It is called before **every** model turn with the budget the current model profile
allows. Return a summary, not a dump:

```
board "Q4 planning": 1 frame, 40 shapes
frame f1 "Risks" (12 shapes)
  s3 sticky red at (40,40) "Supply chain lead times"
  …
selection, in full:
  s3 sticky red at (40,40) 130x110 "Supply chain lead times"
```

Three rules that matter more than they look:

- **Be expandable.** Return a summary and set `expandable`, and ship a `zoom` tool
  the agent can call for detail on one region. Never dump the whole document.
- **Say less when unfocused.** A pinned harness gets a much smaller budget; return
  one headline line and stop.
- **Be stable.** Core caches your output by document hash. If your text changes when
  the document has not — a timestamp, a random order — you break the worker's prefix
  cache for everyone.

Check what the model actually sees at 300, 600 and 1500 tokens with
`Request::PreviewContext`, which is what the whiteboard's own test does.

## 4. The surface

A view is declared in `harness.toml` with a `kind`. Two kinds run in the web
client and the desktop app (architecture v2 §6.3):

- `widgets`: your logic describes a tree of plain controls and the shell
  draws it; every click or edit comes back to your logic as an event. No
  code of yours runs in the client. Use it for settings, lists and forms.
- `web`: an ES module of yours runs in an iframe on an origin that is the
  harness's alone, with `@localspace/harness-sdk` as its only door.

`egui` and `stream` views still run in the egui client only, until the web
client reaches parity.

### A `web` view

```toml
views = [{ id = "board", kind = "web", module = "ui/web/index.js", placement = "main", title = "Board" }]
```

The directory holding the entry module is what the harness's origin serves,
and nothing else of the package: `ui/web/` here, with any assets you put
beside `index.js`, addressed by relative URL. The shell generates the page;
you ship no HTML. The page maps `@localspace/harness-sdk` for you:

```js
import { connect } from "@localspace/harness-sdk";

const h = await connect();           // resolves with the document in hand
render(h.doc());                     // your document, as JSON
h.on("doc", (doc) => render(doc));   // Core changed it: the agent, an undo, another user
h.write(next);                       // your edit; Core reconciles and commits it as `surface:<view>`
h.send({ hello: true });             // a message to your logic's `event` export, at most 64 KB
h.on("message", (bytes) => …);       // your logic's reply
h.on("command", ({ name, args }) => …); // the shell: `zoom` {value}, `fit`
h.theme;                             // the shell's colours and fonts, also set as `--ls-*` CSS variables
```

The types are in `web/public/harness-sdk.d.ts`. What the sandbox gives you is
exactly this: scripts, your own files, the bridge. The origin's
Content-Security-Policy allows no script but yours, no connection but to your
own files, and no frame parent but the shell that opened you. A fetch to the
API, to the network or to another harness's origin does not leave the frame;
if the surface needs data, it asks its logic, which asks Core, which checks
the manifest. Storage on your origin (`localStorage`, IndexedDB) is yours and
survives reopening; the document does not live there, it lives in Core.

A write is the whole document. Core diffs it against the Automerge document
field by field and commits only what changed, so writing the same document
back costs nothing, and the commit carries the user's name, not yours. A
write that another user or the agent races with merges; the `doc` event
that follows is the result, so render from it rather than from what you
sent.

### An `egui` view

Implement `Surface` from `localspace-surface-sdk` and call `export_surface!`:

```rust
impl Surface for Board {
    fn ui(&mut self, ui: &mut egui::Ui, state: &mut SurfaceState) {
        // state.doc is the document; set state.doc_dirty when you change it.
        // state.inbox holds messages; state.send() replies to your logic.
    }
}
export_surface!(Board);
```

A surface has no filesystem, no network and no model access. It gets the document
and one opaque message channel, and that is all. Keep it under 8 ms a frame: three
slow frames running earns a visible badge and a throttle — the Client then enters
it no more than twenty times a second for repaints it asked for itself.

Your egui's font atlas is capped at 1024 pixels a side, the smallest epaint
accepts, so it costs 4 MB at most rather than 16. Every distinct text size
rasterises a fresh set of glyphs into it, and each time it grows the surface
briefly holds more than two copies — so quantise sizes you derive from a zoom
(the whiteboard snaps to twelve sizes per doubling) instead of minting a new
one every frame. Texture pixels never travel through the frame: the host reads
them out of your memory and tells you when it has, through `hs_release`.

## 5. `evals.json`

Five to twenty tasks phrased the way a user would phrase them, each with an
assertion on the resulting document:

```json
{ "cases": [
  { "name": "three risks as red stickies",
    "prompt": "Put the three risks on the board as red stickies: supply chain, hiring, FX exposure.",
    "assertions": [
      { "path": "shapes", "min_len": 3 },
      { "path": "shapes.0.fill", "equals": "red" }
    ] } ] }
```

`localspace evals <id>` runs them against the model the environment actually has
loaded and reports a pass rate. Each case starts from a clean document, so cases
cannot depend on order. This is the real quality signal: a plugin a human can use
but a model cannot drive is a broken plugin here, and nothing else surfaces that.

## 6. `[resources]` — what you may cost

```toml
[resources]
memory_mb = { logic = 32, surface = 32 }   # enforced; shown in the store
idle_unload = "5m"                          # logic dropped after this idle time
```

Both limits are real. The logic budget is a wasmtime memory limiter on your
component; the surface budget is the same limiter on your surface module in the
Client. Grow past it and you are stopped, the user is told which budget you broke
and by how much, the event is audited, and your next call starts a fresh instance.
A surface that breaks its budget is restarted once by itself — its state is in
the document — and after that only when the user asks.

Declare what you measure, and no more: the store shows these numbers. For an
`egui` surface, measure while zooming text. The whiteboard sits at 5 MB with an
ordinary board and peaks at 12 MB zooming it; a board with five large headings
peaks at 21 MB. The default of 16 is for surfaces that draw little text. The
test that produces those numbers is `crates/localspace-client/tests/surface.rs`.

Your logic instance is dropped after `idle_unload` without a call. Nothing you kept
in guest memory survives that — your document does. Keep state in the document.

## 7. Artifacts and handoff — talking to other harnesses

Context lives in Core, never inside a harness. Harnesses hand each other data by
reference, through Core, and the agent carries one ledger across all of them.

```toml
[contributes]
produces = ["outline.v1"]     # kinds your tools may register
accepts  = ["outline.v1"]     # kinds your tools may import
```

**To produce:** write the data into your own document, then return
`"artifact": {"kind": "outline.v1", "summary": "3 items from board Risks"}` in a
tool result. Core pins it to the commit that call made and puts it in the ledger as
`art_N`. Only kinds in `produces` are accepted; anything else is refused in the
trace. The whiteboard's `canvas.export_outline` is the worked example.

**To accept:** take an `artifact` parameter. Before your tool runs, Core resolves
it, checks the kind is in your `accepts` (a wrong target is refused naming a right
one), and reads the producer's document at the pinned version. Inside the call,
`artifact-get(id)` returns `{id, kind, summary, produced-by, commit, content}`.
You never see what the producer's document became afterwards. The planning board's
`board.import_outline` is the worked example.

Interchange types are `name.vN`, owned by localSpace, and strict: never invent a
private format for data another harness could plausibly consume.

## 8. `[package]`, `[dependencies]`, `[provides]`

```toml
[package]
kind = "harness"               # harness | library | types | template | model-pack | skill | theme

[dependencies]
"io.localspace.types.geometry" = "^1.2"                       # a library package
"io.localspace.mesh-viewer"    = { version = "^2", optional = true }
"localspace.geometry.v1"       = { interface = true }         # any provider

[provides]
interfaces = ["localspace.simulation.v1"]
```

One version per package per environment. Installing you installs what you depend
on first, from the catalog, in order; an installed version that satisfies your
requirement is reused. A conflict is refused naming both dependents. Everything
ends up in `environment.lock`, a document in the DAG, with a content hash per
package. Depend on interfaces, not vendors, wherever you can.

---

## An MCP server is already a harness

Tier B speaks MCP-shaped JSON-RPC over stdio. An existing MCP server answers
`tools/list` and `tools/call`, ignores the three `harness/*` extensions, and Core
describes it generically. Add `harness/context` when you want it to carry state into
the model's turn properly.

---

## What the reference whiteboard's surface does, and why

It is worth reading `harnesses/whiteboard-surface` before writing your own, because
two of its choices are not obvious and both matter:

**One gesture, one commit.** A drag mutates `state.doc` every frame so the preview
is live, but only sets `state.doc_dirty` when the gesture *ends*. Setting it every
frame would put sixty commits in the history for one drag, and undo would become
useless. Anything continuous — dragging, resizing, drawing ink — should follow this
shape.

**Draw what is on screen, and no more.** Immediate mode rebuilds the shape
list every frame, so every draw pass first tests the shape's screen rectangle
against `painter.clip_rect()` and skips the rest; on a thousand-note board
with sixty visible, a frame costs what sixty cost. The background dot grid is
one mesh of quads rather than 2,600 tessellated circles, and note text is laid
out at a wrap width snapped to 8 px so egui's galley cache survives a zoom.
The Client's `tests/surface.rs` measures all of this.

**The schema is the only contract.** The board's array-taking tools accept `ids`,
never a bare `id`, because `required: ["ids"]` means Core refuses the other form
before the harness runs and the grammar built from that schema will not let a model
emit it. A harness being quietly lenient about something the schema forbids is
unreachable code that misleads whoever reads it next.

The editing it implements — tool palette, marquee and shift multi-select, dragging a
whole selection, eight resize handles, snapping with alignment guides, stacking
order, locking, duplicate, clipboard, freehand ink, text labels, and keyboard
shortcuts — is all document mutation. There is no second model to keep in step, and
every one of those edits is undoable and mergeable because Core reconciles the JSON
into the CRDT field by field.
