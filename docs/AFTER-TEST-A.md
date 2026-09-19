# After the laptop test

What was seen and deliberately not done before Friday 25 September 2026,
because it is neither needed for the test nor a security problem
(docs/DECISIONS.md, 2026-09-18, the answers after day 1). Newest first
within each part; an item leaves this list when it is built or decided
against, with a line in `docs/DECISIONS.md`.

## Due first, by ruling

- **The model list as data.** A signed, versioned index that Core fetches
  from the registry or imports from a file; the compiled-in
  `models/catalog.json` and `models/gpus.json` move into it. The signature:
  `ed25519-dalek` is preferred over `ring`, decided with the item.
- **A pasted Hugging Face repo id**, with the same verdict before any
  download, if it was cut on Tuesday 22nd; the model card is untrusted data.

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
