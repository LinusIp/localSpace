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
slow frames running earns a visible badge and a throttle.

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

---

## An MCP server is already a harness

Tier B speaks MCP-shaped JSON-RPC over stdio. An existing MCP server answers
`tools/list` and `tools/call`, ignores the three `harness/*` extensions, and Core
describes it generically. Add `harness/context` when you want it to carry state into
the model's turn properly.
