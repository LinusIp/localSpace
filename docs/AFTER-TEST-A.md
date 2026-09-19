# After the laptop test

What was seen and deliberately not done before Friday 25 September 2026,
because it is neither needed for the test nor a security problem
(docs/DECISIONS.md, 2026-09-18, the answers after day 1). Newest first
within each part; an item leaves this list when it is built or decided
against, with a line in `docs/DECISIONS.md`.

## Due first, by ruling

- **The prompt as a system message and turns, not one user message.** The
  root cause of 2026-09-19's find: the whole prompt, conversation and all,
  reaches the engine as a single user message, which is not how small
  instruction models are trained to be addressed and is why they took the
  tool framing literally. The rewording of `prompt::SYSTEM` is a patch on
  this. Sending messages keeps the spec's order (and so the engine's prefix
  cache); it is measured with the evals per model size, and with the message
  script of the laptop test.
- **The model list as data.** A signed, versioned index that Core fetches
  from the registry or imports from a file; the compiled-in
  `models/catalog.json` and `models/gpus.json` move into it. The signature:
  `ed25519-dalek` is preferred over `ring`, decided with the item.
- **A pasted Hugging Face repo id**, with the same verdict before any
  download: **cut for the laptop test on 2026-09-19** (one to one and a half
  days against the half day it was allowed). The model card is untrusted
  data; `scripts/catalog-entry.mjs` shows what is read and what is not.
- **A licence-clean model for the 3B slot.** The default recommendation only
  offers models whose licence permits commercial use, so between the 1.5B
  and the 7B there is nothing to recommend (Qwen2.5 3B is under a research
  licence). Each candidate's naming and notice obligations get a careful
  read first.
- **A CI job that resolves every catalog entry's address** and compares its
  size and digest, so that a dead entry can never ship silently again
  (`scripts/check-catalog.mjs` is the check; two of five entries were dead on
  2026-09-19).

## Before the server test (with items 5 to 7)

- **An organisation's server does not start its model again after a
  restart.** On a person's own computer the window starts the model that was
  in use last; a server has no such window, and an administrator would load
  the model by hand after every restart. Core should do it as it starts.

## The package and the desktop app

- **A choosable folder for the models.** Many gaming laptops have a small C:
  and a large D:. Before the test there is the check: free space on the
  models' drive, said on the first run, a download refused before it starts.
- **The installer without its folder page.** It means owning more of the
  template than the one line owned today.
- **Our own build of llama.cpp** instead of the repackaged upstream release,
  with the optional CUDA variant beside it.
- **The Linux tarball** (due with the server test), and the engine pinned
  for Linux.
- **Signing**: `bundle.windows.signCommand` once the certificate exists;
  every executable and library, the engine's included; the publisher name
  becomes the certificate's subject.
- **Notices for what the binaries carry.** The package ships llama.cpp's and
  OpenMP's licences; the Rust and npm dependencies' notices are not
  gathered yet.
- **A Windows job object for the engine**, so that a crash of the app, not
  only a clean exit, takes `llama-server` with it.
- **"Delete the application data" under test.** The uninstaller's checkbox
  has no command-line switch, so the `package` workflow cannot tick it; it
  is checked by hand at the dry run.

## The conversation with a small model

- **On a fresh install the tools listed have nothing to act on**
  (`task.plan` "one step per harness", `find_capability` with nothing
  installed): fewer tools until something is installed.
- **The evals have no model in `localspace evals`** and a 300-second limit
  through the API, which the 7B with the old wording did not finish in. An
  evals run per catalog model, with its time, belongs in the record.
- **"Two stickies and an arrow between them"** fails on the 3B and the 7B
  with either wording: a third sticky instead of the arrow.

## The words a person sees

- **The vocabulary rule is not checked in CI**, although `docs/PILOT-1.md`
  §12 says it is: no script reads the shell's member-facing strings for the
  words a member must never see. Until it exists the rule is kept by hand
  (it caught "layers" in the first run's placement sentence on 2026-09-19).

## The estimate

- **CUDA's efficiency**, measured instead of taken as Vulkan's.
- **The fixed cost a token on the card** scales with the number of layers
  rather than being one constant; two models cannot tell the two apart.
- **The verdict lines** (15 and 5 tokens a second) and **the efficiencies**
  are provisional until the ten laptops' recorded speeds are in.
- **The reference tiers are still told from `nvidia-smi`** (`profile::Machine`):
  a workstation or server with AMD or Intel cards is "below the floor", is
  planned by `fit` like a laptop (which works), and in server mode meets the
  gate. The tiers should be told from the engine's device list too.
- **Other families in the catalog**, each after a real run through Core:
  Gemma 4 (E2B, E4B, 12B, 26B-A4B), Ministral 3 (3B, 8B, 14B), gpt-oss-20b,
  SmolLM3-3B, all Apache-2.0 and published ungated by ggml-org as of
  2026-09-19.
- **Integrated graphics through Vulkan.** They are planned as the processor
  today; on some machines the engine is faster on them than on the cores.

## From the review of 2026-09-13, still open

Invite tokens in GET paths and logs; the token-use race; unpurged lock and
session rows; a revoked session's socket living up to thirty seconds; export
documents keyed by content hash; one user's egress approval applying to
everyone.
