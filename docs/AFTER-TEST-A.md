# After the laptop test

What was seen and deliberately not done before the laptop test of Monday
21 September 2026 (first planned for Friday 25),
because it is neither needed for the test nor a security problem
(docs/DECISIONS.md, 2026-09-18, the answers after day 1). Newest first
within each part; an item leaves this list when it is built or decided
against, with a line in `docs/DECISIONS.md`.

## Due first, by ruling

- **A busy first start recommends a model one size too small** (ruled
  2026-09-20: high priority; the measurement is not touched on the day of a
  dry run). The look copies memory three times and keeps the best, and on
  2026-09-20 all three were slow: 8.2 GB/s on a laptop that reads 18 to 21
  when quiet, moments after 4.4 GB had been written to its disk. The 7B
  fell under the line for "runs well" and the 1.5B was recommended, with
  the 7B already on the disk; a restart on the quiet machine read 20.2 and
  recommended the 7B. **This is not erring on the safe side: safe on speed
  is unsafe on answer quality.** By our own message script the 1.5B gets
  arithmetic wrong and invents facts, which is what moved the line from 15
  to 10 tokens a second. Nor is it exotic: a tester's first run comes
  moments after the installer wrote its files, on a gaming laptop with a
  launcher and a browser open. Monday's logs say how often
  (`copied at N GB/s`; `docs/test-a/RUNBOOK.md` says how to recognise it),
  and that number decides how it is built. Seen beside it, for whoever
  designs the fix: the recommendation is made once, from that one moment;
  a model already on the disk counts for nothing in it; and the same
  model's range moves between two looks ("about 6 to 8 words a second" on
  the first run, "6 to 9" once started), which two pictures side by side
  made plain. Nothing in the tester sheet asks a person to close other
  programs first: it is ours to solve.
- **The verdict does not stand out in its row: first on the design list**
  (ruled 2026-09-20). In Settings → Assistant, and behind "Choose a
  different model", a row is one grey paragraph: the verdict, the speed,
  where the model sits, the sentence on small models, the licence and the
  size of the download, all in one grey and one weight. The verdict is the
  single most important thing in the row, "the whole honesty claim", and
  it stands mid-paragraph. The first run's card does set it apart (green,
  on its own line), so the two places do not even agree. Seen on the
  landing pictures of 2026-09-20.
- **The prompt as a system message and turns, not one user message.** The
  root cause of 2026-09-19's find: the whole prompt, conversation and all,
  reaches the engine as a single user message, which is not how small
  instruction models are trained to be addressed and is why they took the
  tool framing literally. The rewording of `prompt::SYSTEM` is a patch on
  this. Sending messages keeps the spec's order (and so the engine's prefix
  cache); it is measured with the evals per model size, and with the message
  script of the laptop test.
- **What a read tool brings back never reaches the model** (plugin spec §10
  "Core returns {result, diff_summary} into context", §4.3 "a summary plus a
  `zoom` tool", §8.2). The model reads a tool's one-line summary and nothing
  of its result. For a tool that changes something the summary is the point;
  for one that reads, the result is: `web.fetch` ("fetched …, cached with a
  citation"), `web.search` ("8 result(s)"), the whiteboard's `canvas.list`
  and `canvas.zoom` ("5 shape(s) in full detail", of which the model sees
  no detail, while the prompt tells it to zoom when it needs some), the
  planner's `board.columns` and `board.zoom`. Found on 2026-09-19 when a
  model reported, in a fetched page's name, a temperature the page does not
  contain. **Until it is fixed no web tool is offered**
  (`WEB_RESULTS_REACH_THE_MODEL`, docs/DECISIONS.md, 2026-09-19, the answers
  after day 4); a board is small enough for its provider's summary to carry
  it, which is why the whiteboard's evals pass. The fix: a read tool's
  result, bounded by the profile's budget and wrapped as untrusted where it
  comes from outside, in what the model reads; then retrieval for what is
  larger. **With the web tools the third network mode returns** (plugin spec
  §8.1): until then the Client shows two true choices, *Offline* and
  *Online* (only to download a model), and says nothing of an assistant that
  asks before it goes online (2026-09-20).
- **What the rule "nothing is visible unless it works" took out, to come
  back when it works**: *Attach a file* and uploading your own files (with
  document search), *Documents* in the rail before a first document exists,
  the raw API's address under Advanced. *Sign out* stays out of a personal
  install for good.
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
- **Signing**: the path is built and rehearsed in CI with a throwaway
  certificate (`docs/BUILD.md`; 2026-09-19); what is left is the real
  certificate's own command and its provider's setup on the runner, a
  timestamp, and the publisher name becoming the certificate's subject.
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
- **The approval's words**: "Allow this environment to fetch from
  `www.space.com`?" is not how a person speaks. Nobody meets them while no
  web tool is offered.
- **The evals have no model in `localspace evals`** and a 300-second limit
  through the API, which the 7B with the old wording did not finish in. An
  evals run per catalog model, with its time, belongs in the record.
- **"Two stickies and an arrow between them"** fails on the 3B and the 7B
  with either wording: a third sticky instead of the arrow.

## The words a person sees, and the look

- **No walk in CI draws an answer**, so nothing would have caught the
  assistant's round mark squeezed to an oval beside every long answer
  (found on a picture of the release build and fixed on 2026-09-20; it was
  verified by measuring the mark in the packaged build). CI has no model,
  and both walks stop before a conversation. A walk needs an engine that
  answers one long paragraph without a model, and measures what is drawn.
- **The vocabulary rule is not checked in CI**, although `docs/PILOT-1.md`
  §12 says it is: no script reads the shell's member-facing strings for the
  words a member must never see. Until it exists the rule is kept by hand
  (it caught "layers" in the first run's placement sentence on 2026-09-19).

## The estimate

- **CUDA's efficiency**, measured instead of taken as Vulkan's.
- **The fixed cost a token on the card** scales with the number of layers
  rather than being one constant; two models cannot tell the two apart.
- **The verdicts' line** (the pace of reading at four words a second, read
  off the numbers shown since 2026-09-19; about 9 and 5.3 tokens a second)
  and **the efficiencies** are provisional until the testers' recorded
  speeds are in. The test
  `what_typical_computers_are_told_and_offered` shows what a change does to
  every typical computer.
- **A card the table does not know is planned as if it were the
  processor** (ruled for after the test, 2026-09-19): "an unknown card that
  reports 8 GB of memory is not a processor", and the processor's pace is
  pessimistic in a way that produces a wrong recommendation. The fix is a
  conservative bandwidth floor by the card's memory class, which settles
  the whole class instead of chasing a table that never ends. Until then:
  AMD's laptop parts and the RTX 50 laptop parts are in the table since
  2026-09-19, and the testers' own cards are checked against it when the
  screening answers arrive. Not in it still: Intel's Arc laptop parts, the
  professional cards (RTX A-series, Radeon Pro), and whatever comes next.
- **The estimate for a mixture of experts has never been compared with a
  run**: Qwen3 30B-A3B is said to run well on a 32 GB laptop from its
  active bytes alone. It is listed and is not the default until it has been
  through the message script.
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
