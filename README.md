# localSpace

A local agent workspace where every capability comes from an installed **harness**.
A harness contributes three things and is not agent-usable without all three:

1. a **surface** for the human — a sandboxed wasm UI, or a declarative widget tree;
2. **tools** for the agent — typed actions with JSON Schema parameters;
3. a **context provider** — a text serialization of the harness's current state,
   sized to whatever token budget Core hands it.

The agent never talks to a harness directly. Every call goes through Core, which
checks permissions, validates the parameters, applies the confirmation gate,
records the mutation as a commit in a version DAG, and returns one short line to
the model.

One binary, two modes. `localspace` runs Core and the Client in a single process;
`localspace-serve` runs the same Core behind an HTTP/WebSocket API. They share the
whole protocol, so the two cannot drift.

---

## Quick start

```bash
cargo build --release
```

```bash
./target/release/localspace doctor
```

`doctor` identifies the machine, picks the model profile every budget comes from,
and reports what the reference models would actually do here — before you download
60 GB to find out.

```bash
./target/release/localspace --harnesses harnesses --registry registry --data ~/.localspace
```

That opens the desktop Client with the whiteboard installed, the planning board
offered in the Marketplace, and history persisted under `--data`. Without `--data`
everything is in memory.

To give the agent a model, open **Models** in the rail and point it at any
OpenAI-compatible endpoint — llama.cpp's server, LM Studio, mistral.rs, vLLM,
SGLang all speak it:

| field | example |
|---|---|
| endpoint | `http://localhost:1234/v1` |
| model id | whatever that endpoint reports |

---

## Commands

```bash
localspace                              # open the desktop Client
localspace doctor                       # hardware profile, and what will run on it
localspace bench                        # the efficiency budgets for this machine
localspace evals io.localspace.whiteboard   # a harness's agent-compatibility score
localspace call board.add_card '{"text":"write the migration plan"}'
localspace call canvas.add_sticky '{"text":"Supply chain","fill":"red"}'
localspace-serve --bind 0.0.0.0:8443 --harnesses harnesses --data /var/lib/localspace
```

`localspace call` takes exactly the path the agent takes — permission check, schema
validation, confirmation gate, DAG commit — so a script and an agent cannot
diverge.

Options: `--harnesses <dir>` (installed at start), `--registry <dir>` (a catalog the
Marketplace lists), `--data <dir>`, `--user <name>`, `--organisation`,
`--allow-below-floor`.

---

## Layout

```
crates/
  localspace-proto        the entire Client<->Core API; nothing bypasses it
  localspace-core         harness runtimes, permissions, DAG, agent loop, gateway
  localspace-client       the GUI; no IO except through `Backend`
  localspace-desktop      binary: Core + Client, in-process transport
  localspace-server       binary: Core + axum HTTP/WS
  localspace-surface-sdk  what an `egui` surface compiles against
  localspace-harness-sdk  what harness logic compiles against
wit/harness.wit           the Tier A contract
harnesses/whiteboard      reference harness #1: whiteboard, `egui` surface, 13 tools
registry/planner          reference harness #2: planning board, `widgets` surface, 9 tools
                          — an offline bundle the Marketplace installs from
docs/                     STATUS, BUILD, HARNESS-AUTHORING, NEXT
```

---

## What is actually built

`docs/STATUS.md` maps every section of both specs to what exists, what is partial,
and what is not built. Read it before planning work — it is written to be honest
rather than flattering.

Short version: the plugin core runs end to end. A wasm harness installs, its tools
are exposed to the agent under a per-turn budget, its surface runs sandboxed in the
Client, its context provider is called with a real token budget, every write lands
as a commit, and undo, redo and whole-run rejection all work. A Marketplace lists an
offline bundle and installs from it, holding any capability widening at an explicit
approval. 156 tests pass, including an end-to-end suite that drives both real
reference harnesses through Core.

Not built: the browser Client bundle, OIDC/SAML/SCIM, the inference workers
themselves (Core routes to an external endpoint rather than loading weights), the
Tier B OS sandbox, retrieval, and packaging/signing. Each is listed in STATUS with
what it would take.
