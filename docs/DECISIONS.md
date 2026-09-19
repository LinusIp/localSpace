# Decisions

Every answered question and every decision made during the build, newest
first, with the date and the section of the specification it affects. Part of
the source of truth once written (`CLAUDE.md`, "Source of truth").

## 2026-09-19, the answers after day 4 and their addendum: the laptop test is on Monday 21 September, and nobody stands beside the testers

The user's answers to the second evening report of 2026-09-19
(`8-localspace-answers-day4.md`) and the addendum that came with them
(`9-localspace-testA-remote-addendum.md`).

- **The test is Monday 21 September, unsupervised.** Ten people install on
  their own machines, in their own time, with an installer and a sheet and
  nobody to ask. The dry run is Sunday morning, on a machine that is not the
  builder's, from the CI artefact, following the sheet to the letter, with
  the message script run there and its output read, and a plain go or no-go
  before the end of that day. Monday morning: one clean package build from a
  head that has not changed since the dry run, then send. **The two extra
  days are not for new work**: no new model family, nothing from items 5 to
  7, nothing off the after-test list.
- **`web.fetch` is not offered until what it fetches can reach the model**
  (plugin spec §8.1, §8.2; reverses "`web.fetch` stays" of the answers after
  day 3, whose premise, the builder's, was wrong). "A tool that reports
  'fetched, cached with a citation' while the model never sees a word of the
  page does not fail honestly — it manufactures confidence." With it goes a
  check of every other tool for the same shape: a summary that implies the
  model saw what it never received. **Standing, from the user:** when a
  ruling rests on something later found not to be true, come back and
  correct it rather than build what was written.
  **Built, and the class found, not only the instance:** the model reads
  every tool's one-line summary and nothing of its result, so whatever a
  *read* tool brings back never arrives: `web.search` ("8 result(s)") as
  much as `web.fetch`, and, once they are installed, the whiteboard's
  `canvas.list` and `canvas.zoom` ("5 shape(s) in full detail") and the
  planner's two. On a fresh install only the two web tools are of that
  kind: `find_capability` says what it found and focuses it, so its tools
  arrive with the next turn, and `task.plan` and `task.note` claim nothing.
  So **neither web tool is offered, whatever the mode and whether or not a
  search service is set** (`WEB_RESULTS_REACH_THE_MODEL` in Core, false
  until a read tool's result can reach the model: first on
  `docs/AFTER-TEST-A.md`), and a call from the agent to one of Core's tools
  that is not on offer is refused before anything runs: no approval is
  raised and nothing is asked of the network. The person's own direct call
  path is untouched. **Spec narrowing, authorised:** plugin spec §8.1's
  `ask` and `online` modes offer no web tool for now.
- **The privacy sentence is "Nothing you type leaves your computer."**,
  true without qualification once no web tool is offered.
- **The card table gets AMD's current laptop range and the RTX 50 laptop
  range now**, each figure checked against the manufacturer's page, the
  lower one where a part has variants; the testers' own cards are added when
  the screening answers arrive, and the machine-shape test then says what
  each named tester will be offered. **Not this week:** a conservative
  bandwidth floor by memory class for a card the table lacks ("an unknown
  card that reports 8 GB of memory is not a processor"): new logic, after
  the test.
  **Entered the same day:** the six RTX 50 laptop parts, each as the
  "Memory Bandwidth" row of NVIDIA's own table gives it (5090 and 5080
  Laptop 896 GB/s, 5070 Ti Laptop 672, 5070, 5060 and 5050 Laptop 384; for
  the 5070 Laptop NVIDIA gives one figure for its two memory sizes), and
  twenty laptop parts of AMD's, RX 7900M to RX 6300M, each as the "Memory
  Bandwidth" line of its own page on amd.com gives it. AMD writes "up to"
  before every such figure and publishes no lower one, so there was no
  lower figure to take; the estimate's efficiency of 0.60 stands over it
  as over every card. AMD lists no laptop part of the RX 9000 series. An
  AMD laptop part is told from the desktop card of the same number by its
  letter (7600S, 7600M XT), not by the word "laptop". The machine-shape
  test has an 8 GB Radeon RX 7600S (offered the 7B, as an RTX 4060 Laptop
  is) and a 12 GB RTX 5070 Ti Laptop (the 14B). `models/gpus.json` is
  version 2.
- **The verdict's words are derived from the numbers shown beside them**,
  not from the raw estimate, so that the two can never disagree whatever a
  calibration does later: "Works — about as fast as you read · about 2 to 3
  words a second" was the one place where the product argued with itself.
  **Built:** one line, the pace of reading at four words a second, and the
  verdict is read off what is shown (`fit::Verdict::of_what_is_shown`):
  *faster than you read* when the least that is promised is above it (the
  low end of "about 5 to 7", or the one number of "at least 6"); *about as
  fast as you read* when the most that is shown reaches it ("about 3 to 4",
  "about 4 to 6"); otherwise *too slow for everyday use* ("about 2 to 3").
  A test walks every pair of numbers that can be shown, and every machine it
  tries through the whole of `fit`. The tokens-a-second lines are gone from
  the code; where the rule puts them is about 9 for "runs well" (ruled at
  10: a model estimated between 9 and 10 shows the same "about 5 to 6" or
  "5 to 7" as one at 10 and cannot be told apart on screen) and 5.3 for
  "works" (ruled at 5). **What moved on the typical computers:** the 14B on
  the 4 GB development laptop and the 7B on a computer with slower memory
  and no card are *too slow*, as their "about 2 to 3 words a second" says;
  and **a card the table does not know, on a laptop with faster memory, is
  now offered the 7B**: its floor reads "at least 6 words a second", which
  is faster than a person reads. With slower memory the floor is "at least
  3" and the default stays the 1.5B, until the card is in the table.
- **The catalog's field is `exercised_on`, not `script_run`**: it records
  that a model went through the script, not that it did well; the 1.5B went
  through it, answered badly, and is still the default where there is no
  graphics card.
- **`app.log` replaces standing over a shoulder.** The sheet asks each
  tester to send the file back, with its exact path. Whatever is cheap and
  answers a question that would otherwise be guessed at is logged, a line
  an event: how long a download took and at what rate, whether a model was
  found here or fetched, the plan of layers and every step back from it, the
  time to the first word, every error a person was shown. **Nothing of what
  a person typed or was answered, and nothing is ever sent by the product:**
  the tester attaches the file.
  **Built:** one tap where every event passes writes to the log what Core
  itself says of the engine, the models, the plan and the computer (the
  plan of layers, the engine's flags at every start and every step back
  from a plan, the warm-up, the time to ready) and every warning and error
  a person was shown; never a line of a harness's own, which may quote a
  document. New lines: which Windows and which processor; a download's
  beginning (how much was already here), its end (how long, at what rate)
  or why it stopped; a file found on the computer and taken as the
  published one; and the measure of every answer, never a word of it: the
  time to the first piece, the tokens, the rate while writing, the size of
  the prompt, the tools asked for. The person's folder is written `~` in
  every path.
- **Found by reading the package workflow's log, where the two new lines
  stood: the build a person installs had never measured its memory.** On
  the CI runner the release build logged "copied at 1344816.4 GB/s" and
  "estimated at 2407216.2 tokens a second": the measurement looked at one
  byte of its copy, and the optimiser of a release build removed the copy.
  No debug build ever showed it, and every build run on the development
  laptop was a debug build. On a tester's computer everything that runs
  from system memory would have been promised thousands of words a second.
  Both buffers are now held opaque to the optimiser; a figure no memory
  does (above 400 GB/s, under 0.5) is never planned with: a careful 8 GB/s
  stands in and the log says so; and the package workflow reads the figure
  out of the installed release binary and out of the app's own log, and
  refuses one that no memory does or one that had to be replaced.
- **Smart App Control is a go or no-go for each tester.** Whoever's
  screening answer says it is on gets a signed build or does not take part;
  nobody is sent an installer that Windows will refuse. The sheet shows the
  SmartScreen dialog as it is, with the words to click, says beforehand that
  the app is not signed yet and that this is expected of a test build, and
  names whom to contact. If a certificate arrives by Sunday, the packaging
  must be ready to use it that moment.
- **The sheet is the product now**, and the dry run tests it as much as the
  build: every step in order, a picture where Windows interrupts, and what
  a slow step looks like so that a pause of four minutes reads as normal.
  **A results form** of ten short questions collects what only a person can
  tell; everything measurable comes from the log.
- **The USB sticks are out** for this test (nobody can hand one over; each
  tester downloads over their own connection). `scripts/stick-list.mjs` and
  the recognition of a copied file stay in the product for a company that is
  air-gapped; no more time goes into them before the test. The end-to-end
  time is still measured at the dry run and decides nothing any more.
- **Five to seven usable results of ten is a normal outcome** of an
  unsupervised test and is planned for.

## 2026-09-19, the answers after day 3: six rulings, one addition, and the order of the next day

The user's answers to the evening report of 2026-09-19
(`7-localspace-answers-day3.md`). Two corrections of the builder's stand:
a fresh install's first prompt is 1,052 tokens, not 2,600, and the 3B's
licence is said in the licence's own words ("Free for research and
evaluation only, not for commercial use."), not as "personal use".

- **1. The middle verdict's words change** (the provisional verdicts of the
  answers after day 1): "Works — about as fast as you read". The old words,
  "slower than reading pace", were false across most of their band and stood
  on one line with "about 7 to 9 words a second". The four labels are one
  scale of reading pace: *Runs well — faster than you read*; *Works — about
  as fast as you read*; *Too slow for everyday use*; *Will not fit on this
  computer*.
- **2. A model may only be the default if it has been run through the
  message script on some machine.** A property of the catalog, permanent,
  not a patch for the test: "the failure mode is not a bad model, it is an
  untested one arriving in front of a stranger." Found by running the real
  fit over typical laptops: any laptop with 32 GB of memory was offered
  Qwen3 30B-A3B by default, a model that has never been started through
  Core anywhere. It stays listed and can be chosen; so does gpt-oss 120B;
  the rule covers the next family too.
  In the catalog the rule is an entry's `script_run` (renamed `exercised_on`
  by the answers after day 4), the day it last went
  through the script (the builder's name for it, open to a better one): the
  four Qwen2.5 entries read on 2026-09-19 carry it, an entry that does not
  say is never the default, an imported file never says, and a test holds
  every entry that says so to a section of its own in
  `docs/test-a/MESSAGE-SCRIPT.md`. The catalog is version 4.
- **3. The line for "runs well" moves from 15 to 10 tokens a second**, on
  the measurement that moved it: on the 4 GB card the 7B runs at 13 tokens
  a second (about ten words a second, two and a half times the pace of
  reading) and is reliably good on the message script, where the 1.5B, the
  default there under the old line, answers "17 × 24 = 388" and "France has
  two capitals". The 15 was a guess and provisional; the 10 is measured.
  Two conditions: every machine shape is probed again after the change, the
  ones without a graphics card included, and nothing else may silently
  become "runs well" that should not. **What the probe found**, over
  thirteen typical computers through the engine's own device line and the
  card table (kept as the test `what_typical_computers_are_told_and_offered`):
  one default changes, the 4 GB RTX 3050 Ti laptop's, from the 1.5B to the
  7B ("Runs well — faster than you read · about 7 to 9 words a second", 12.5
  tokens a second estimated, 13 to 16 measured). Three verdicts go from
  "works" to "runs well" and none of them is a default: the 3B on a
  computer without a card (11 to 14 tokens a second), and the 30B-A3B on a
  32 GB computer without a card (14, from an estimate no run has checked).
  A 6 or 8 GB card keeps the 7B, a 12 or 16 GB card the 14B, a computer
  without a card the 1.5B (the 7B reaches 5 to 8 tokens a second there), an
  older 4 GB card (GTX 1650) the 1.5B. Even the low end of the range shown
  at the line is 5.6 words a second, above the pace of reading, so "faster
  than you read" is true of the whole band, and the middle band is 3.75 to
  7.5 words a second, "about as fast as you read". **Found on the way:**
  the card table has no AMD laptop parts and no RTX 50 laptop parts; such a
  card is used and promised only the processor's pace, so its laptop is
  offered the 1.5B where the card would carry the 7B. Put to the user: the
  testers' cards from the screening, each added with its published
  bandwidth. **The principle underneath: prefer
  the more reliable model once a model is fast enough to read along with.
  Speed above reading pace has sharply diminishing value; correctness does
  not.**
- **Found while taking the pictures again, and fixed: the memory was
  measured once, at the busiest moment of a first start.** The look took its
  one 100 ms sample of the copy rate last, two seconds in, while the window
  was opening and a model copied from a stick was being checked: 13 GB/s on
  a laptop that copies at 19, which put the 7B under the new line and the
  1.5B in its place, on some starts and not on others. Being busy can only
  make memory look slower, never faster, so the look now takes three
  samples with its other steps between them and keeps the best (19.5, 20.8
  and 20.9 GB/s over three such starts; the 7B each time). And the log now
  keeps what the look found and what the first run recommended with its
  estimate, two lines, so that a recommendation that surprises someone on
  the day can be explained from `app.log` alone; nothing in them is about
  a person.
- **4. `web.search` is not offered until a search service is set** (plugin
  spec §8.1 and §8.2; answer 24). §8.1 takes the web tools away when
  airgapped "so the model never proposes a search it can't run", and the
  same reason holds while no service exists: ten to twenty seconds of
  waiting for "nothing found". **Spec narrowing, authorised:** in `ask` and
  `online` mode `web.fetch` exists as before, and `web.search` exists once a
  search service is configured.
- **5. A reply that is nothing but a tool call is read as the call**, in the
  model's own shape (`name`, `arguments`) as well as Core's (`tool`,
  `params`): only when the entire reply is a call to a tool actually on
  offer. It is the first thing cut if the day runs out.
  **Built the same day, and needed more than was thought:** in a second run
  of the message script the 14B wrote its call as text twice in nineteen
  turns, not once (the translation again, and the code), both times
  `task.note`, once spread over several lines.
  `model::parse_bare_call` reads such a reply as the call when the entire
  reply is one object with `name` and `arguments` and nothing else, and the
  name is a tool on offer in that very request, with its dot or as the
  engine is given it; every worker's reply passes it in the router. The
  agent loop already held back a reply that begins with a brace, so nothing
  of it is shown. Through the real 14B afterwards: the translation and the
  code both come back in words.
- **Found by the second runs, put to the user and not decided: what a
  fetched page says never reaches the model.** With `web.search` gone the
  14B reaches for `web.fetch` on a plain question (the Moon's distance:
  "Allow this environment to fetch from `www.space.com`?"), and the 7B for
  `find_capability` on the weather. A tool's result comes back to the model
  as its one-line summary; the page was to come back through retrieval
  (plugin spec §8.2), which is not built. A local stand-in page said "23
  degrees with thick fog … the word of the day is marmalade"; with the
  fetch allowed, the 7B answered that the page says "sunny with a high of
  22°C … the word of the day is serendipity". The builder's statement in the
  evening report, that `web.fetch` works without a search service, was
  wrong in the sense that matters, and ruling 4's "`web.fetch` stays" rests
  on it.
- **6. The test is in English only.** All ten testers write in English; no
  language is added to the message script and no time goes into other
  languages. The German and Russian findings stay in the record as a known
  limitation. The tester sheet asks for English and says other languages
  are not part of this test; one tried anyway is an observation to write
  down, not a failure to fix.
- **A. The verdicts say nothing about quality, and one sentence will**: the
  tiny band carries "Small models answer quickly but get things wrong more
  often.", wherever such a model is recommended or listed. Not a second
  scale: one sentence, attached to the band.
- **B. The order:** `web.search` hidden; the default-eligible rule; the line
  at 10 with the new words and the quality sentence in one pass, and the
  probe of every machine shape; the catalog checked again; the two pictures
  taken again; the tool call as text if the day has room. Thursday is the
  dry run on a machine that is not the builder's, and nothing else: the
  message script is run once there too, and its output read.
- **C. Smart App Control** is flipped by the user if it blocks the package,
  at once when told; the builder never changes it. **An organisation's
  server that does not start its model again** stays on the list for the
  server test, untouched this week. **The USB stick** carries the models of
  every band a tester may be recommended, the 7B for 4 GB cards included.

## 2026-09-19, the message script: what the recommended models answer, and a word that reached a person

Section C of the answers after day 2, begun the same day: a fixed script of
seventeen messages and a three-turn conversation, through every model a
tester can be recommended (`scripts/message-script.mjs`; the answers and a
person's reading of them are in `docs/test-a/MESSAGE-SCRIPT.md`).

- **The 7B is the first model that is reliably good on the script** (16 of
  19 turns; its misses are ungrammatical German and an invented detail in
  Russian). **The 1.5B, the default on a 4 GB laptop since the licence rule,
  is fast and unreliable**: 17 × 24 = 388 in one run and 408 in the other, a
  date right in one run and wrong in the other, "France has two capitals"
  in both. **The 0.5B is a last resort only**: it repeats questions back and
  drops a conversation's thread at the first follow-up, which bears out the
  ladder rule. **The 14B gave the best answers of the four** (16 of 19
  turns, nothing factual wrong, correct German inside its translation and
  correct Russian, real trips from Berlin); on the development laptop it is
  what the app says it is, too slow for everyday use (answers in 3 to 60 s).
- **Put to the user, not decided:** whether the default on a 4 GB laptop
  should be the 1.5B (forty words a second, by the rule "the largest that
  runs well") or the 7B (about ten words a second measured, which the app
  calls "works"). And whether the middle verdict's words can stay: "Works,
  slower than reading pace" covers 5 to 15 tokens a second, which is about 4
  to 11 words a second, where a person reads about 4: only the bottom of the
  band is slower than reading.
- **Two things the 14B showed, put to the user and not decided.** It reached
  for `web.search` three times (the Moon, the weather, a cost), the 7B once:
  the tool is offered whenever the mode is not `airgapped` (plugin spec
  §8.1) and has no search service behind it until the user sets one (answer
  24), so each reach is ten to twenty seconds for nothing. §8.1 gives the
  reason the tools are absent when airgapped, "so the model never proposes
  a search it can't run", and the same reason would take `web.search` out
  while no service is set; `web.fetch` works without one and would stay.
  And once in nineteen turns its tool call came back from the engine as
  text, bare JSON without the tags the engine's reading of the model's
  format looks for, and Core showed it to the person as the answer:
  `{"name": "task.note", "arguments": …}`. Core already reads a reply of
  its own grammar's shape as a call (`tool` and `params`,
  `model::parse_grammar_call`); the model's own shape it does not. Both
  are in `docs/AFTER-TEST-A.md` until the user says otherwise.
- **A word no member may be shown reached a person through the model.**
  Asked to shorten a sentence, the 1.5B answered "I don't have a harness to
  use for this task": it had read "(no harness is focused)", a rule about
  "each harness" and two tool summaries that named harnesses, none of which
  means anything on a fresh install. The word is out of what the model
  reads: the state says "(nothing is open)", the rule says "what is open is
  described further down", `find_capability` searches "what is installed"
  and `task.plan` plans "one step per tool". The spec's layout of the prompt
  is untouched. The 1.5B still declines that one sentence, now in ordinary
  words; five other rewriting requests were done well. The vocabulary rule
  is about what a person reads, and what the model reads is what it says.
- **Asked for today's weather**, the 1.5B says it cannot fetch it, and the
  7B reaches for `web.search`, which on a fresh install has no search
  service behind it and fails before any address is built: nothing leaves
  the computer, and the person is told after nine seconds that nothing was
  found. A tester will ask this within minutes; noted for the sheet.

## 2026-09-19, the warm-up: a person's first message starts as fast as their second

Answer 4 of the answers after day 2, built within its deadline and under its
three conditions.

- **What it is.** Once a model has loaded and sits where it should, and
  before anyone is told it is ready, the engine is sent the part of the
  prompt every first turn begins with (the instructions, the profile, the
  tools, the state: `Prompt::prefix`), with the same tools beside it as a
  turn sends, and asked for one token. The engine keeps what it read, and a
  turn's prompt continues from there with the ledger and the conversation.
- **Inside the existing wait.** It happens on the engine's supervisor thread
  between "the model answers" and "ready", which the person is waiting
  through anyway; nothing new is shown.
- **Silent and never fatal.** It is bounded at 90 seconds; when it fails or
  runs out of time nothing is said and nothing stops, and the first message
  is that much slower. A stop that arrives meanwhile wins. The fake engine
  proves both halves in CI: the read comes before "ready", and a read that
  fails leaves no notice and a model that is ready all the same.
- **Measured with the real engine on a fresh data folder (Qwen2.5 1.5B).**
  With the card: the stable part is 1,052 tokens, read in 0.6 s inside the
  wait; the first message ("Hello!") then needed 34 tokens and was answered
  in 0.1 s. With the card hidden, as on a laptop without one: read in 5.4 s
  inside the wait (194 tokens a second), and the first message was answered
  in 0.6 s where it would have taken about six.
- **A correction to the record of the same day:** the "2,600 tokens" of the
  first turn was measured on the development data, where the whiteboard and
  the planner are installed and their tools are in the prompt. On a fresh
  install, which is what a tester has, the first turn's prompt is 1,052
  tokens.
- If a person installs a tool between the load and their first message the
  tools in the prompt change and the read is not reused; nothing is lost but
  the time it saved.

## 2026-09-19, a file is the model's by its SHA-256: downloads, models copied in, and the default's two rules

What the answers after day 2 asked for by Tuesday, except the warm-up: the
checksum, the detection of files that are already there, the licence rule
and the ladder of the recommendation. With it, what the run of the Friday
flow on the development laptop taught.

- **`sha2` 0.10** is the direct dependency (it was compiled in already
  through other crates; no new crate enters the tree).
- **"Installed" means: here, finished, and the published file.** A file of
  which the catalog gives a SHA-256 is the model's only once that digest has
  been found to be its own, whether it was downloaded or arrived by other
  means. A file is read through once: what was found is kept in
  `verified.json` beside the models (its length, its time of modification,
  its digest), and it is read again only when length or time has changed. A
  model of which the catalog gives no digest (an organisation's own entry,
  an imported file) is held to being here, as before.
- **After a download** the file is compared with the published digest before
  it takes its name (`verifying`); one that differs is removed and refused:
  the right length is not the right file.
- **Files that were already there** (the ruling: models travel on a stick or
  a share into the models folder, and the app notices them) are looked at
  whenever the catalog or the computer is asked about, on a thread of its
  own, one look at a time: so also when they are copied in while the app is
  open. The entry shows `verifying` meanwhile and `checked` tells the clients
  to ask again. **What is something else under a model's name does not count
  and is left alone**, with a notice that says so; the download the person
  then asks for replaces it. **A file shorter than the published one may
  still be arriving and is never touched**: the download is refused with a
  sentence that says to wait for the copy or delete the file. Asking for a
  model whose published file is already here fetches nothing at all, and the
  check of the disk's room counts what is here.
- **The default recommendation only offers a model whose licence permits
  commercial use** (`commercial_use` in the catalog; an entry that does not
  say is never the default), and says nothing of what it passed over. Every
  entry carries its **licence in words a person can act on**
  (`license_words`), shown on the first run and in Settings. The words are
  written from the licence itself: Apache-2.0 is "Free to use, also for
  commercial use."; the Qwen Research licence defines non-commercial as "for
  research or evaluation purposes only", so Qwen2.5 3B says "Free for
  research and evaluation only, not for commercial use." and not "personal
  use", which that licence does not grant.
- **The ladder has a lowest rung worth standing on.** A model of under a
  billion parameters is offered only where nothing larger so much as works:
  a larger model that works comes before a smaller one that runs well. On
  the development laptop the default is now **Qwen2.5 1.5B** ("about 35 to
  50 words a second"); the 3B runs well there too and is passed over for its
  licence.
- **"First run" means that no model has been chosen on this computer**, not
  that no model's file is here: with models copied in beforehand a file is
  "here" within seconds, and the first run of 2026-09-19's first version was
  skipped for an empty chat that said "No model". Core now **remembers the
  model that was started last** (for the computer, not for a person), the
  window **starts it again as it opens**, and the first run is shown until
  one was chosen. Before this, every start of the app ended at "No model —
  choose one in Settings".
- **The first run keeps its one deliberate click.** A model that is already
  on the computer is not started by itself: the page says "It is already on
  this computer: nothing to download." and its button says **Start**. A
  tester has the sentence about their computer to read and write down, and
  the guide says "accept it".

**The Friday flow, run on the development laptop** (a fresh data folder;
0.5B, 1.5B and 7B placed in its models folder beforehand; the real engine):
the first run appeared after 3.3 s with the 1.5B already verified; Start; the
chat had its model 5.7 s later; nothing was fetched. Closed and opened again:
the chat after 2.8 s, its model back by itself at 7.9 s. **Every entry that
ships was checked against Hugging Face the same day** (`scripts/check-catalog.mjs`:
ten files of seven entries answer at their pinned addresses with the
catalog's sizes and digests); it is run again before Thursday.

**For the server test, not built:** an organisation's server does not start
its model again after a restart (the window that does it is a person's own);
it belongs with items 5 to 7.

## 2026-09-19, the answers after day 2

The user's rulings on the four questions of the second daily report, two
additions to the plan for the laptop test, and a re-ranking of its risks
("answers after day 2").

1. **The reworded system prompt stays** (`prompt::SYSTEM`, 2026-09-19), with
   its measurements in this record. The 0.5B falling from 3 of 6 to 1 of 6 in
   the whiteboard's evals is accepted: it is recommended only where nothing
   larger fits, and there plain chat that works is worth more than tool calls
   that half work. **The ladder is checked:** nobody lands on the 0.5B when
   the 1.5B fits; a slightly slower 1.5B is preferred to a 0.5B that cannot
   use a tool. **The root cause is written down so that it is not lost:** the
   whole prompt reaches the engine as a single user message, and not as a
   system message and turns, which is why a small model takes the tool
   framing literally. The rewording is a patch on a structural problem, and
   the structural fix heads `docs/AFTER-TEST-A.md`.
2. **`sha2` is approved** as a direct dependency. A file held only to its
   published size is not verified: a truncated or corrupted download of
   exactly the right length passes. With it comes a second job that matters
   more than the checksum (below).
3. **The default recommendation only ever offers a model whose licence
   permits commercial use.** A standing product rule, not a decision for one
   Friday: a company running localSpace on a research-licensed model is a
   liability the default handed them. Two conditions on how it shows: the
   licence appears **in words a person can act on** ("Free for personal and
   research use, not for commercial use"), not as a licence's name; and the
   recommendation **does not explain what it skipped**, it recommends the
   best licence-clean model that fits. On the development laptop that is the
   1.5B and not the 3B, a real step down in quality, which is **not** fixed
   this week by adding a model family: the candidates each carry naming and
   notice obligations that deserve a careful read. A licence-clean model for
   the 3B slot is a first item after the test. (The user's caveat, kept: not
   legal advice; every licence that ships is read properly before a
   commercial launch.)
4. **The warm-up of the engine's cache after a load is built, with a
   deadline and conditions:** it lands by the end of Tuesday 22 September or
   is cut; it happens **inside the existing "getting ready" wait**, never as
   a new wait; and a warm-up that fails is **silent and never fatal**: the
   first message is a few seconds slower and that is all.

**Confirmed:** the pasted Hugging Face repo id is **cut** for the laptop
test (estimated at one to one and a half days against the half day it was
allowed), and other model families **wait**. Both are on the after-test list.

**Ten people downloading at once is the likeliest way the day fails**
(2.6 MB/s at this site; a venue's one pipe divided ten ways). So the models
are **pre-staged**, and the app notices them: **at startup the models folder
is scanned, and a file with a catalog entry's name and its SHA-256 marks
that entry installed, with no download.** The same verification as after a
download, pointed at a file that arrived by other means. On the day the
models travel on a USB stick or a local share into
`%LOCALAPPDATA%\localSpace\models`; a tester whose machine wants a model that
is not on the stick falls back to downloading, which still works. It lands
before the tester sheet's screenshots, because it changes what Friday looks
like. The digests of every model a tester might be recommended are in the
catalog.

**Answer quality is ranked level with Smart App Control**, not third: four
messages were a thin sample. **Before Thursday a fixed script of about
fifteen realistic messages runs through each recommended model, and the
results are recorded**: a greeting, "What can you do?", a factual question,
a short email, something to summarise, a little arithmetic, a translation, a
"make this shorter", a question in a tester's own language if one will be
used, something deliberately vague, and **a three-turn conversation whose
follow-up depends on the previous answer**, because a growing context on a
4 GB card is where things fall over. The same script on each band. It ranks
above any further model family: a model that fails it is found on Wednesday,
when the answer can still be "recommend the next one up".

**Notes, as rulings.**
- A dead catalog entry must not be able to come back: a CI job that resolves
  every entry's address is for after the test; **a one-off check of every
  entry that ships is done before Thursday** (two dead out of five says
  check them all).
- **Smart App Control on the development machine:** it stays on, but the dry
  run matters more than the experiment. If on Wednesday or Thursday it
  stands between the builder and a working package, it may be turned off
  without asking first, and that day's report says so. *The builder's note:
  changing a security setting of Windows is left to the user even so; if it
  comes to that the builder says so at once and the user flips it.*
- The rule for hybrid laptops is confirmed by an explicit check: the card of
  its own is preferred, not whichever device is listed first.
- The daily package run stays.

**The remaining days:** the checksum, the detection of files already
present, and the warm-up if it fits, by Tuesday; the message script, the
check of the catalog and the screenshots on Wednesday; the dry run on a
machine that is not the builder's on Thursday; the test on Friday. Nothing
of items 5, 6 or 7 until the laptop test is done. The same report each
evening.

## 2026-09-19, plain chat did not work on the models a laptop runs: the system prompt's opening

Found while verifying item 3 with a real chat turn, and **the largest risk
to the laptop test found so far**: on a fresh install, with nothing from the
Store, the models the first run recommends could not hold a plain
conversation through Core. **Built the same day on the builder's judgment,
because the test is "install, the app recommends a model, and they chat";
put to the user for confirmation or reversal.** It is one constant
(`prompt::SYSTEM`); the layout the plugin spec fixes (§16.1: system prompt,
model profile, tools, context, conversation) is untouched.

- **The cause.** The prompt opened: "You are the agent inside localSpace.
  You act by calling the tools listed below, which are the only capabilities
  you have." A 100B-class model reads past that; a 3B or 7B takes it
  literally. On a fresh install the tools listed are five bookkeeping and web
  tools (`find_capability`, `task.note`, `task.plan`, `web.fetch`,
  `web.search`), and the whole prompt, conversation included, reaches the
  engine as one user message.
- **Measured before, on a fresh data folder** (engine b10869, the
  development laptop). Qwen2.5 3B: "Hello!" was answered with the ledger's
  own bookkeeping text; "Say hello in five words" with two
  `find_capability` calls and no reply; a question about Australia's capital
  with tool syntax written out as text; "Explain how a heat pump works" took
  26 s and returned the context's format. Qwen2.5 7B: "Hello!" got "task run
  … completed without a goal specified"; the capital question was answered
  correctly after 52 s of tool calls; the heat pump question ended in tool
  calls and no reply.
- **The change.** The opening now says that answering in plain words comes
  first ("a greeting, a question you can answer from what you know,
  something to write, explain or translate needs no tool at all, and your
  reply is simply the answer"), that the tools are for what only they can
  do, that `find_capability` is for when a tool would have to do the thing,
  and that the ledger's and the conversation's format is never repeated in a
  reply. The other rules are word for word what they were.
- **Measured after, same folder, same messages.** 3B: four proper answers
  in 0.2 to 2.4 s. 7B: four proper answers in 1.1 to 7.6 s.
- **What it costs where tools are wanted** (the whiteboard's own agent
  evals, through the server):

  | Model | Old wording | New wording |
  |---|---|---|
  | Qwen2.5 0.5B | 3 of 6 (recorded 2026-09-10) | **1 of 6** |
  | Qwen2.5 3B | 5 of 6 | 5 of 6, the same case failing |
  | Qwen2.5 7B | did not finish within the API's 300 s | 5 of 6 in 291 s |

  The case that fails on both wordings is "two stickies and an arrow between
  them" (a third sticky instead of the arrow). The 0.5B, told to answer in
  words, stops reaching for tools: it is recommended only where nothing
  larger fits, and with the 1.5B now in the catalog that is a rare computer.
  The browser walk with the agent on and the 7B loaded still passes: asked
  for a note on the board, the agent put it there through the whiteboard's
  tool.
- **What this does not fix, and what it suggests.** The prompt still reaches
  the engine as one user message rather than as a system message and turns,
  which is not how small instruction models are trained to be addressed; and
  on a fresh install tools such as `task.plan` ("one step per harness") have
  nothing to act on. Both are for after the test, with evals per model size
  to steer by (`docs/AFTER-TEST-A.md`).

## 2026-09-19, loading is not evidence of fitting: the look after a load, and what was measured

Item 3 of the build order for the two tests (the instructions for the
builder, §3.3; the answers of 2026-09-18, §C and the approval after day 1).
Measured on the development laptop: the RTX 3050 Ti Laptop with 4 GB (3,962
MiB, 49 MiB held by other programs), engine b10869 through Vulkan, and
Qwen2.5 7B at Q4_K_M, 4.4 GB in two files, at the app's context of 8,192.

| Layers on the card | The card's own memory | "Shared" | Generated |
|---|---|---|---|
| 0 (the processor alone) | | | 9.5 tokens a second |
| 15 | 2,752 MiB | 41 MiB | 13.2 |
| 18 | 3,193 MiB | 41 MiB | 15.1 |
| 20 | 3,476 MiB | 41 MiB | 15.9 |
| 22 | 2,795 MiB | **1,022 MiB** | **7.0** |
| 26 | the engine exits while loading | | |

- **The failure the answers warned of is real and is worse than described:**
  with 22 layers the model loads, a gigabyte of it spills into system
  memory the card reaches over the bus, and it generates slower than with no
  card at all. Nothing in the engine's output says so.
- **Its signature is unmistakable.** A load that holds keeps 41 MiB in
  "shared" graphics memory at every number of layers; one that spilled keeps
  the overflow there. The line is drawn at 256 MiB. (The worry that a
  partly-offloaded model keeps its system-memory half in "shared" memory was
  unfounded for this engine: it does not.)
- **Both ends are answered, before anyone is told the model is ready**
  (`engine::AfterLoad`, asked by the supervisor when a start is over). A load
  that **spilled** gives back the layers the overflow amounts to and one
  more, so that one more start settles it (1,022 MiB is seven of this
  model's layers: 22 becomes 14). An engine that **gave up while loading**
  gives back a quarter of its layers, never fewer than two, down to the
  processor alone. At most five starts, then whatever happened is accepted
  or reported as before. A stop that arrives meanwhile wins. The ruling said
  "a layer at a time"; the measurement says how many layers the overflow is,
  and a start of this model costs five seconds, so it is taken in one step.
- **What a load taught is kept** while Core runs: the most layers that
  model may be given here. The verdicts use it, so a model that had to give
  layers back shows the speed of where it now sits ("the speed estimate
  adjusted accordingly"), and the next load starts from it. It is not
  written to disk: the card may be freer tomorrow.
- **Windows' figures are read once per start**, on the supervisor's thread
  and never on Core's: about 1.3 s, while the person is waiting for a load
  anyway. Elsewhere there are no such figures and none are needed: a card
  that is asked for too much refuses, which is the second case.
- **The plan is careful by about five layers on this card** (15 planned, 20
  measured to hold: 13.2 against 15.9 tokens a second). Left so: a modest
  model that works is the ruling's measure of success, and the ten laptops
  say whether the margin can shrink.
- **The estimates against the measurements:** the processor alone, 8.1
  estimated and 9.5 measured; 15 layers, "about 6 to 8 words a second" said
  and 9.9 measured. Both promise less than was delivered.

**Verified as the item asks, "on a machine with less VRAM than the model
needs"**, by a test that is ignored where there is no such machine
(`a_real_card_smaller_than_the_model_ends_with_a_plan_that_holds`): Core was
told the card had 8 GB, planned all 29 layers, the engine gave up, 22 were
tried, 1,021 MiB had spilled, 14 held, and the model was announced ready
after 15 seconds, with the catalog saying "Works, slower than reading pace ·
about 6 to 8 words a second · About half of it fits in the graphics memory".

## 2026-09-19, downloads that continue, and a catalog made of Hugging Face's own facts

The first part of the reduced item 4 of the build order for the two tests
(the instructions for the builder, §3.4: "Downloads resume after an
interruption and verify a checksum on completion"; tiny and small entries
first).

- **Two of the catalog's five entries could not have been downloaded.** The
  7B model's single file name never existed (the repository holds it in two
  parts), and the 120B model's three parts had been replaced upstream by one
  file. Found on 2026-09-19 when the approved 7B download answered 404. So:
- **Every entry is pinned to the commit of its repository** its facts were
  taken at (`revision`), and files are fetched from that commit, not from
  `main`: a repository that renames or replaces a file breaks nothing, and
  the digests keep matching.
- **An entry's machine-made fields are Hugging Face's own facts**, gathered
  by `scripts/catalog-entry.mjs`: the files of one quantisation with their
  exact sizes and SHA-256 digests (the repository's listing), the layers and
  the KV size (the base model's `config.json`), the commit. It reproduced the
  layers and KV sizes the hand-written entries had. Everything read is treated
  as data: names, numbers and digests are copied, nothing is executed, a
  model card's prose is never read, and a gated repository is refused. The
  title, the licence with its address and the notes stay a person's to write.
  `verify` in an entry is what each finished file must be.
- **Added, of the family that has run through Core:** Qwen2.5 1.5B (1.0 GB;
  Apache-2.0; for a computer without a card of its own) and Qwen2.5 14B
  (8.4 GB in three files; Apache-2.0; for a 12 GB card and up). Other
  families (Gemma 4, Ministral 3, gpt-oss-20b, all Apache-2.0 and published
  ungated by ggml-org) each need a real run through Core first — their chat
  templates, their tool calls, their thinking modes — and at this site's
  2.6 MB/s that is a download apiece: they follow as time allows, and "four
  models instead of eight" is the ruling's own measure of what may give.
- **A download continues where it stopped.** A file arrives as
  `<name>.part` and takes its name only when it is whole. The part is kept
  when a download stops; the next one asks the server for the rest
  (`Range`), and begins again only if the server sends everything anyway. A
  dropped connection is picked up after five seconds, until six tries in a
  row have brought nothing (any progress starts the count again). Across a
  restart of the app the catalog says `paused` with what is here, the pages
  say "N% is already here" and offer to continue, and the check of the
  disk's room counts only what is still to come. Every path is bounded: a
  part longer than the file is begun again once, and a file that does not
  come to its published size is removed and refused, never installed.
- **Proved with a real interruption:** the 0.5B model through the app from
  Hugging Face, the server killed at 121.6 MB, restarted (`paused`,
  121,620,628 of 491,400,032 bytes), continued from that byte, finished at
  exactly the published size, and its SHA-256, computed outside the app, is
  the published one.
- **The checksum is recorded and not yet checked by the app.** Verifying a
  SHA-256 takes a hash function, and the workspace names none for it
  (`blake3` is there; Hugging Face publishes SHA-256). `sha2` is the standard
  crate and is already compiled in through other crates, but it is a new
  direct dependency: **asked of the user on 2026-09-19**. Until then a
  finished file is held to its exact published size, and no page claims more.

## 2026-09-19, the first run: what Core decides, and what was measured with the real engine

The second half of item 2 of the build order for the two tests, with the
part of item 3 that is the engine's flags (the instructions for the builder,
§3.2 and §3.3). The check after a load, the rest of item 3, follows.

- **Which computers `fit` plans for.** A workstation or server of the
  reference tiers (W32, W96, S) keeps the placement planner, so the W32 gate
  and its measurement stay untouched. **Every other computer** — every
  laptop of the first test, and a single-GPU server of the second — is
  planned by `fit`, from what the computer was found to be: the verdict in a
  person's words, a range of words a second, and where the model sits, on
  every catalog entry **before any download**. The protocol carries them as
  `verdict_label`, `speed` and `placement` beside the verdict's id
  (`runs_well`, `works`, `too_slow`, `will_not_fit`).
- **No jargon where a person reads.** The placement sentence says "About
  half of it fits in the graphics memory; the rest runs from system memory,
  which is slower", not how many layers: the layer count is for support
  (`localspace doctor`, the trace). The vocabulary rule of `docs/PILOT-1.md`
  §12 caught it; that rule is still kept by hand
  (`docs/AFTER-TEST-A.md`).
- **The engine's flags from the fit:** the context, the number of layers on
  the card as the engine counts them, and the one device by name
  (`-c 8192 -ngl 37 --device Vulkan0`), so that a hybrid laptop never puts
  the model on the processor's own graphics; with nothing on the card,
  `--device none`. **No `999` is left anywhere**: the planner's path says
  every layer by its number too. A model whose numbers are not known is left
  to the engine's own fitting (`-ngl auto --fit on`). The computer is looked
  at again at the moment of a load, since the card may hold more or less than
  when the verdicts were shown.
- **The context on such a computer is 8,192 tokens**: what the small
  profile's working set needs, and a KV cache a laptop's card can hold
  beside the weights.
- **The model to start with** is the largest that runs well; failing that
  the largest that works; failing that the smallest that fits at all. Only
  what can be fetched or is already here, and never one the disk has no room
  for.
- **Room on the disk is checked before the first byte**, on the drive the
  models go to, asked at that moment and not taken from the first run's
  figure: a model's size and 1 GiB beyond it. The refusal is one sentence
  that names the place: "Qwen2.5 7B Instruct needs 4.4 GB and drive C: has
  2.1 GB free. Make room there and try again."
- **The first run** is the administrator's question (`DescribeComputer`;
  the one user of a personal workstation is its administrator, and in an
  organisation the computer that matters is the server, so a member's window
  never asks). On a person's own computer Core begins looking as it starts,
  on a thread of its own, so the window's first question does not wait two
  or three seconds for PowerShell. The page shows the sentence, what follows
  from it, the recommended model with its verdict, speed, placement and size,
  the room on the drive, and one button; "Choose a different model" and "Not
  now" lead on. The browser walk meets the page on its fresh server, checks
  the sentence, and goes past it.
- **The hardware floor in personal mode is a statement** (answer 3 of
  2026-09-18): `serve --personal` on a small computer logs that localSpace
  runs a model sized to it and starts; without `--personal` the gate and
  `--allow-below-floor` stay as they were.

**Measured on the development laptop with the real engine (b10869),
through Core:** qwen2.5-3b started with `-c 8192 -ngl 37 --device Vulkan0`
and no key on its command line; it held **2,218 MiB of the card's own memory
and 35 MiB shared**, against 2,551 MiB the plan allowed for (the plan errs
on the careful side: the working buffers were 80 MiB where it allows 256,
and the token embeddings stay in system memory). A real chat turn generated
at **40.5 tokens a second, 30 words a second**; the first run had promised
"about 20 to 30 words a second". **The first turn's prompt is 2,600 tokens**
(the instructions and the tools), which took 3.1 s before the first word on
this card (842 tokens a second); on a computer without a usable card that
wait is the longest of the day, and only the first turn pays it, because the
engine keeps the prompt. Put to the user as a finding, not built.

**A constraint of the development machine:** Smart App Control now refuses
the Rust toolchain's own linker for wasm components
(`wasm-component-ld`), so harness logic cannot be rebuilt there; the files
built on 2026-09-10 are current (their source has not changed) and CI builds
them in every run. The setting stays on (answer 3 of the day before).

## 2026-09-18, the answers after day 1

The user's rulings on the six questions of the first daily report, two
approvals, and direction for the four days left ("answers after day 1").

**Direction.** The day gained on item 1 is kept, not spent: items 2 to 4
depend on hardware the builder does not have and on a calibration still
being derived. The two fixes made unasked (the console windows, the engine's
key) were right; from here, **what is neither needed for Friday nor a
security problem goes on a list for the week after**
(`docs/AFTER-TEST-A.md`). The `package` workflow **runs at least once a
day** until Friday, so that its cache is never cold when a rebuild matters.

1. **The folders on Windows.** The program goes to
   `%LOCALAPPDATA%\Programs\localSpace`, the standard place for a per-user
   install; the person's data stays in `%LOCALAPPDATA%\localSpace` (the
   ruling of 2026-09-09 stands), and that is the folder "Delete the
   application data" removes. No bundle identifier anywhere a person can
   see: `io.localspace.app\data`, built the day before on the builder's
   recommendation, is withdrawn. *What it took:* tauri-cli's stock installer
   puts a per-user program in `%LOCALAPPDATA%\<product>` and offers no
   setting for it, so `packaging/windows/installer.nsi` is a copy of
   tauri-cli 2.11.4's template (upstream SHA-256 `20f4ecc7…fa079`) with
   **one line changed**, the default folder; and
   `packaging/windows/hooks.nsh` makes the uninstaller's checkbox remove
   `%LOCALAPPDATA%\localSpace`, through the hook the template offers for it.
   The copy is re-taken when the tauri-cli pin moves.
   **Free disk is checked on the drive the models go to**, stated on the
   first-run screen, and a download that will not fit is refused **before it
   starts**, with a sentence that names the drive. A choosable model folder
   is the proper fix and waits until after the test.
2. **WebView2 is not downloaded by the installer** (`skip` stays): Windows
   11 always has it, and a download during the install is a network call in
   a product that sells not making them. The condition: when it is missing
   the app's dialog says what to install in plain words and has a button
   that opens Microsoft's page. The Windows version is among the screening
   questions, so a Windows 10 machine is handled deliberately.
3. **Smart App Control stays on** on the development machine: it is the
   only machine there is with it on, and a signed build has to be proven
   against it. The retries it costs are accepted.
4. **The installer's folder page is acceptable.** "No configuration
   questions" meant no ports, no model choices, no server addresses: nothing
   that takes a judgment the person cannot make. Removing the page is polish
   for after the test.
5. **Four verdicts, provisional.** *Runs well — faster than you read* from
   15 tokens a second; *Works, slower than reading pace* from 5 to 15; *Too
   slow for everyday use* below 5, shown and never hidden; *Will not fit on
   this computer* when memory cannot hold it. **The numbers are provisional:
   Friday's ten laptops calibrate them**, and revising them afterwards is the
   expected thing, not a reversal. (They replace the two-way line at 10 of
   the day before.)
6. **The publisher is `localLabs`** until the certificate is issued, then
   the certificate's legal subject exactly.

**Approved: live graphics memory from Windows' performance counters**,
which corrects the instruction to "compute from measured free VRAM" on its
facts (the engine's figure is a per-process budget). They are read **once at
the first run and once after a model loads**, never on anything a person
waits for. The plan before a load takes the **smaller** of the per-process
budget and the card's total less what the counters show other processes
holding, and subtracts the margin from that. **Approved: the engine's
key**, with its negative test kept.

**An unknown card is planned carefully and says so:** "GPU detected,
capability unknown — starting carefully". It gets no figure that cannot be
supported: the speed shown for it is what the processor alone would do, as
"at least", and the recommendation follows from that. A tester with a modest
model that works is a success; one with a confident wrong estimate is not.

## 2026-09-18, what the computer is and how fast a model will be: the measurements behind item 2

The first half of item 2 of the build order for the two tests (the
instructions for the builder, §3.2; the answers of the same day, §C).
Measured on the development laptop: Ryzen 7 6800H, 16 GB DDR5, Radeon
graphics in the processor, an RTX 3050 Ti Laptop with 4 GB, engine b10869.

- **The graphics cards are the engine's own list**
  (`llama-server --list-devices`), not `nvidia-smi`: it covers every vendor
  through Vulkan, names the device the engine is then pinned to with
  `--device`, and needs no new dependency. On the hybrid laptop it lists the
  discrete card only. A processor's own graphics are told from a card by
  name and planned as the processor; a card that is not in
  `models/gpus.json` is "GPU detected, capability unknown — starting
  conservatively" and planned at 128 GB/s; no device is a machine that runs
  on its processor. Nothing in detection fails.
- **The engine's "free" figure is a budget, not a measurement.** It stayed
  at 3,367 MiB while another process held 2.2 GB of the same card. Live use
  on Windows comes from the performance counters, without administrator
  rights: `\GPU Adapter Memory(luid…)\Dedicated Usage` before a load,
  `\GPU Process Memory(pid_<engine>…)\Dedicated Usage` and `Shared Usage`
  after it, with the adapter's LUID found by name under
  `HKLM\SOFTWARE\Microsoft\DirectX`. A large *shared* figure for the engine
  is the spill into system memory the answers warn of. That is item 3's
  verification after load.
- **Memory bandwidth of cards is data** (`models/gpus.json`, the makers'
  published figures, matched whole words, the longest name first, a laptop
  part only against a laptop entry), compiled in beside the model list and
  moving into the index with it. **The system memory's speed is measured**
  at detection, by copying on up to eight threads for a tenth of a second:
  19 to 21 GB/s on this machine.
- **The speed estimate** is a roofline, part by part: the active bytes on
  the card divided by the card's bandwidth times an efficiency, plus a fixed
  cost a token, plus the active bytes in system memory divided by the rate
  memory is read at. Calibrated on four measurements (tokens a second
  generated, `llama-bench`): qwen2.5-0.5b Q4_K_M 105.6 on the card and 85.8
  on the processor; qwen2.5-3b Q4_K_M 42.5 on the card, 25.0 with half its
  layers there, 18.6 on the processor. They give **Vulkan 60 % of the
  published bandwidth and 5.2 ms a token besides**, and **the processor
  reading at 2.0 times the measured copy rate**. The estimate reproduces all
  four within 15 %, which the tests hold it to. **CUDA is not measured**: it
  is given Vulkan's figure, so a CUDA build placed by hand is never promised
  more than what was measured. One machine is one calibration: the ten
  laptops' recorded speeds are the second.
- **What a person sees is never that number**: it is a range of *words* a
  second (three words to four tokens), from three quarters of the estimate
  to the estimate, each rounded down (whole words under ten, fives under
  fifty, tens above): "about 20 to 30 words a second".
- **The three verdicts:** *will not fit* when what stays in system memory
  exceeds it, less a reserve (the larger of 4 GB and a quarter) for
  everything else; *runs well* from 10 tokens a second, about twice the
  speed of reading; *runs slowly* below, shown with its number.
- **The margin on the card** is the larger of 384 MiB and 8 % of the card,
  taken from the smaller of the budget and what is measured free, with
  256 MiB for the engine's working buffers (80 MiB measured for a 3B model
  at 8,192 tokens; to be measured again with the 7.6B). Layers are counted
  as the engine counts them, the repeating layers and the output layer, and
  the engine is given that number, never 999.

## 2026-09-18, the Windows package: what is in it and how it is built

Item 1 of the build order for the two tests (the instructions for the
builder, §3.1; the answers of the same day, 2 and §D).

- **The engine's pin.** llama.cpp release **`b10869`** (commit
  `30b6a755e29692e8bc8e072885325716a2fee70f`), asset
  `llama-b10869-bin-win-vulkan-x64.zip`, 35,757,044 bytes, SHA-256
  `e5506b8beb008e9214f368d3ac22fec3634d1bf3ece7e1dfd81f94ec2c3d45ba`: the
  digest GitHub publishes for the asset, and the archive the measurements of
  2026-09-18 were made with. `scripts/engine.json` holds the pin;
  `scripts/fetch-engine.mjs` checks size and digest before it unpacks and
  exits non-zero on a mismatch, with nothing unpacked (a one-byte change to
  the archive was tried). Only the Windows asset is pinned: the Linux one is
  pinned when the tarball is built, for the server test.
- **What is kept of it:** `llama-server` and the libraries it loads, every
  CPU variant, the Vulkan backend, OpenMP with its licence; 24 files,
  98.6 MB. Left out: the other command-line tools, and the RPC backend and
  its server, because a network backend has no place in a product whose only
  socket is the gateway. llama.cpp's MIT licence, taken from the tag, ships
  in `licences/`.
- **Where Core looks for the engine:** a flag, `LOCALSPACE_LLAMA_SERVER`,
  `<data>/engines`, then `engine/` beside its own executable, then PATH. One
  placed by hand comes before the package's, so a faster build put under
  `<data>/engines` wins without touching the install.
- **The installer** is NSIS through `tauri-cli` 2.11.4 (pinned), per user
  (no administrator prompt), from the stock template: welcome, the folder
  with its default, finish. The folder page is the one page that shows a
  choice; removing it means owning a copy of the template, which is not this
  week's work. **The portable zip** holds the same files and the app.
- **The desktop app** (the desktop answers of 2026-09-12, built now because
  they come before an installer): one running copy, a second launch bringing
  the first to the front (`tauri-plugin-single-instance`); a start that fails
  says so in a dialog with where the log is (`tauri-plugin-dialog`), and Core
  is made at launch so that an unopenable data folder is that dialog and not
  an error on the first message; a release build logs to
  `<data>/logs/app.log`; and closing the app unloads the model
  (`Running::shutdown`), because a process that exits runs no destructors and
  a `llama-server` left behind keeps the graphics memory.
- **The `package` workflow** builds both artefacts on a Windows runner, by
  hand or by a `v*` tag (a Windows runner costs double, and `ci` stays the
  judge of a commit), and proves there what a runner can: the silent
  install lays out every file; the engine and the command line run from
  where they were put, with no GPU; the app makes its data folder, starts,
  serves its client, opens its window; a second launch gives way; the
  uninstaller leaves the data. It cannot prove what SmartScreen and Smart
  App Control do, how a real GPU behaves, or what a person sees: the dry
  run's.
- **Not signed.** Signing is a configuration change when the certificate
  exists (`bundle.windows.signCommand`), and covers every executable and
  library in the package, the engine's included.
- **Verified by the first `package` run** (35369269827, commit `0eb8b24`,
  Windows Server 2025 runner, every step green at the first attempt): the
  installer (`localSpace-0.1.0-0eb8b24f-windows-x64-setup.exe`, SHA-256
  `4ce8e9f1…ec941`) and the zip (`832bbeec…fb9a`), 102 MB together; the
  silent install laid out every file; the engine ran from the installed
  folder and, on a machine with no GPU and no Vulkan loader, listed no
  device and exited cleanly; the installed app made its data folder, served
  its client one second after starting and had its window open eight
  seconds later; a second launch gave way; the uninstaller left the data.
  A cold run takes 81 minutes (the command line 24, tauri-cli 18, the
  harnesses and the app 30). **Not verified by it, and owed to the dry
  run:** SmartScreen and Smart App Control on the unsigned files, the
  installer's pages as a person clicks through them, the app on a real GPU,
  and a machine without WebView2.

Built on the builder's recommendation and **put to the user as questions**
the same day, each one line to change:

1. **The person's data moves** from `%LOCALAPPDATA%\localSpace` (the ruling
   of 2026-09-09) to `%LOCALAPPDATA%\io.localspace.app\data`, because the
   per-user installer puts the *program* in `%LOCALAPPDATA%\localSpace`, and
   program and data must not share a folder. The new place is the
   application's own data folder, beside the webview's, and exactly what the
   uninstaller's "Delete the application data" removes. No migration: only
   the development machine has data at the old place.
2. **WebView2 is not downloaded by the installer** (`webviewInstallMode:
   skip`): the other modes call Microsoft during the install, or add well over
   100 MB to every download for a runtime Windows 11 always has. When it is
   missing the app says so in its dialog.

## 2026-09-18, the plan for the two tests, and the nine answers

The requirement changed on 2026-09-18: *the software must run a model on any
hardware* (the instructions for the builder, second version, which replace
the DGX Spark plan of the same day). The builder's four answers went back
with measurements from the development laptop; the user's rulings on them
("answers, and the plan for the two tests") are recorded here before any of
the build order's code.

**The plan.**

- **Test A, ten gaming laptops in personal mode, is Friday 25 September**,
  Monday 28 the slip day. **Test B, a server with thin desktop clients, is
  Wednesday 30 September or Thursday 1 October.** If only one fits, Test A
  wins: ten people are booked, and it is the direct proof of the governing
  requirement.
- **Order:** item 1 (the Windows installer with the pinned upstream engine),
  item 2 (the hardware check as the first run), item 3 (computed layer
  offload), the reduced item 4 (tiny and small entries, the honest verdict,
  downloads that resume and verify). **A dry run on Thursday 24th:** the
  finished package installed on a Windows machine that is not the
  development machine, from the artefact and the instructions the testers
  get; half a day, with whatever it finds. Items 5, 6 and 7 do not start
  until Test A is done.
- **No slack.** If something slips, what gives is the number of testers or
  the number of catalog entries, never the honesty of the verdict and never
  the packaging.
- **A report at the end of each day:** what landed, what moved, whether
  Friday is still real; if it stops being real, that is said on Tuesday, not
  Thursday.

**Cut until after the tests:** whiteboard group one; the signed index, the
registry fetch and the publishing mechanism; the Linux tarball, which moves
to Test B; building llama.cpp ourselves.

**A deferral with a date, not a change of direction:** the model list stays
compiled into Core for Test A. "The catalog is data, not code" stands. Making
it a signed, versioned index that Core fetches or imports is **due as the
first item after Test A** (from 2026-09-28). The pasted Hugging Face repo id
is kept only if it costs half a day or less on top of the reduced item 4;
the builder decides on the estimate and says which way it went; if cut, it
comes right after the index.

**The nine answers.**

1. **Test B's server** is assumed to be x86-64 Linux with an NVIDIA GPU, and
   that is all that is built for. No aarch64 work. If it turns out to be a
   DGX Spark, that is an explicit change and Test B moves.
2. **The engine in the installer** is the upstream llama.cpp Vulkan release,
   repackaged, pinned by SHA-256: the exact tag and digest are recorded
   here with the code, the digest is verified at build time, and a mismatch
   fails the build. Our own build comes after the tests; in-house first is
   sequenced, not abandoned. The repackaged binaries carry our signature
   like our own.
3. **The hardware floor** becomes a plain-words statement in personal mode,
   never a refusal: say what the machine can expect and let the person go
   on. In server mode the gate stays, with `--allow-below-floor`: an
   administrator putting a company on an inadequate box deserves a stop.
4. **The certificate** is a cloud signing service, not a USB token (Azure
   Trusted Signing if eligible, a cloud-HSM OV certificate otherwise), so CI
   can sign. The user is obtaining it; it is on the critical path.
5. **`rcgen` and `tokio-rustls` are approved** for the self-signed
   certificate and native TLS (item 6).
6. **The pin lives in the app's Rust side**, which holds the pinned
   connection while the WebView talks to it over loopback. Installing a
   certificate machine-wide on an employee's computer is refused outright.
7. **The index signature** is decided with the item; when it comes back,
   `ed25519-dalek` is preferred over `ring` (pure Rust, no C toolchain,
   cross-compiles cleanly).
8. **Downloads:** the 7.6B model (4.7 GB) for item 3's verification against
   the 4 GB card, yes; the CUDA build, no: it would change no decision this
   week.
9. **Dates:** as above.

**On the findings.**

- **The speed estimate carries a measured efficiency factor per backend**
  (Vulkan, CUDA, CPU), not one constant: qwen2.5-3b Q4_K_M on the RTX 3050
  Ti Laptop reached 42.5 tok/s, 46 % of the bandwidth roofline, so an
  uncalibrated estimate promises twice what people get. **What is displayed
  is rounded down and shown as a range or a floor**, never as one number
  with a decimal. The layer-share model for partial offload (25.9 predicted,
  25.0 measured) is good enough to ship; the whole-GPU case is calibrated
  the same way.
- **Loading is not evidence of fitting.** On Windows a Vulkan allocation
  spills into shared system memory instead of failing, and the model then
  crawls. Item 3 computes from *measured free* VRAM (3,367 of 3,962 MiB on
  that laptop, not the 4 GB on the box), then **verifies after load** that
  the VRAM in use matches the plan, and backs off a layer at a time when it
  does not.
- **The GPU device is pinned explicitly** on hybrid laptops; a machine with
  only an integrated GPU takes the conservative path and is told so
  plainly.
- **macOS is out of scope for both tests;** Metal is the answer when it
  arrives.

**Smart App Control is the biggest risk to Test A.** Where it is on, an
unsigned executable is refused outright, with no "run anyway". So: the
certificate is the real fix and signs everything, the repackaged engine
included; **a portable zip is built as well as the installer**; a one-page
instruction sheet covers the SmartScreen click-through and what to do on a
hard block; the testers are asked in advance for GPU, VRAM, RAM and whether
Smart App Control is on. On the development machine the setting is the
user's decision (it cannot be turned back on without reinstalling Windows);
until then the installer is verified in CI and the parts that stay
unverified are named.

**The deployment guide** is `docs/DEPLOYMENT-GUIDE.md`, the user's document,
and it is the **target state**, not a description of today: the list of
mismatches at its head is the definition of the gap. The guide follows the
build: no command is implemented merely because the guide names it, and when
an item lands the guide changes in the same commit. It supersedes the draft
in `docs/PILOT-1.md` §9. One correction to the instructions: no
`[[models.worker]]` entry exists; what exists, and what Test B uses, is an
administrator pointing Core at an external endpoint at runtime.

## 2026-09-18, the seven questions of the review, decided by the user

The fresh-eyes review of 2026-09-13 left eight questions; the user answered
seven of them on 2026-09-18 (the instructions for the builder, §5). Items 5,
6 and 7 are done before Phase B; the rest fold in where they sit. None of
them is built by this commit: this is the record.

1. **Break-glass for changing another workspace's members and document
   access.** An administrator may; it always requires a typed reason; it is
   audited as `workspace.break_glass`, the same mechanism that covers
   entering a workspace, extended to membership and ACL edits
   (`SetMember`, `RemoveMember`, `SetDocumentAccess`, the agent-writes
   setting). No silent administrator override anywhere.
2. **Proposal-mode branches.** Keep what exists: an agent's edits to a shared
   document land where a person applies or discards them; shared workspaces
   default to proposal mode, an owner may set direct. No richer review
   interface before the pilot. (The review's finding that today's proposals
   commit to the shared head and are dropped by rewinding stays on the list;
   the decision is about scope, not about that mechanism being right.)
3. **Per-address lockout and a request-rate limit.** Both. The lockout per
   address on repeated failed sign-ins is thirty failures in fifteen
   minutes, locked fifteen, which the directory already does; added to it, a
   per-session rate limit on the chat and tool endpoints so one client
   cannot monopolise the engine. Both audited.
4. **A read-only account may export a board.** Viewing includes exporting
   what one can already see; an export is not a write and creates no shared
   state. `ProduceArtifact` stays ungated by role, and gets the test that
   says so.
5. **A Content-Security-Policy and HSTS on the shell origin.** Both, now,
   before real browsers on a real network reach it, which is the server
   test. Strict: `default-src 'self'`, `wasm-unsafe-eval` only where the
   surfaces need it, no third-party origins; HSTS on the TLS origin.
6. **The audit log's head is anchored outside its files.** The current
   chain head is written to a separate small file an administrator can
   read, so truncating the log is detectable and not only tampering within
   it; `localspace audit verify` compares.
7. **Commits carry the real user, not `"user"`.** The audit story and
   per-user undo both depend on it.

Still open from the review, not among the seven: the lows that remain
(invite tokens in GET paths and logs, the token-use race, unpurged lock and
session rows, a revoked session's socket living up to thirty seconds,
export documents keyed by content hash, one user's egress approval applying
to everyone).

## 2026-09-14, builds on the laptop: four jobs, line tables, one suite at a time

The user's machine went to 100% disk with memory at the ceiling during the
review's test runs: cargo ran one rustc or linker per core, each taking
gigabytes while it linked one of the seventeen integration-test executables,
and Windows paged the rest. Four changes, by the user: `.cargo/config.toml`
caps cargo at four jobs; the workspace's `dev` profile carries line-table
debug info only, the same setting CI has had since 2026-09-13 (59 GB to
11 GB of target); the whole workspace is not tested locally any more, only
the suites being worked on, with CI doing the rest warm in about six
minutes; and an editor's rust-analyzer gets its own target directory. The
stale target directory, 95 GB, was removed. `docs/BUILD.md` says the same.

## 2026-09-13, the fresh-eyes review of who sees what, and what was fixed at once

Before Phase B builds retrieval's ACL pre-filter on it, the code that
decides who sees what was reviewed as its own task by a reviewer who had
not written it: the access control, the directory (argon2, lockout,
sessions, links), the audit chain, the exposure of tools, the request
dispatch, the server's cookies and origins, the settings. Twenty-seven
findings came back; the five high ones were confirmed by reading the code
paths again. The ones that violate what the documents already say (every
mutating call permission-checked in Core, `CLAUDE.md`; shared state the
administrator's, deployment §4.3; a tightened document never wider than
its workspace, §6.1) were fixed the same day, each with a negative test.
The ones that are decisions are questions to the user, listed at the end.

Fixed:

- **A surface message to harness logic wrote without a check.** The logic
  may hand back a document; it was applied and committed whoever asked.
  Now the caller must hold `view` to reach the logic at all and `edit` for
  the document to change; a read-only account is refused as for any write;
  refusals are audited as `document.write` denied. (`roles.rs`: a `view`
  member's message carrying a whole document changes nothing.)
- **History was everyone's.** `GetHistory` returned every commit of every
  workspace with the tools' arguments in them. It now returns the commits
  of documents the caller holds `view` on, and the environment's lock only
  to administrators.
- **Undo, redo and dropping a run reached anyone's document.** Undo and
  redo now act on the most recently changed document in the caller's own
  workspace that they may edit, never the environment's lock; dropping a
  run requires `edit` on every document the run touched; a read-only
  account does neither.
- **Anyone could point the model at any URL.** `SelectModel` is an
  administrator's request, the endpoint goes through the gateway's check
  like any other egress, and the choice is audited.
- **Shared state had no administrator's gate.** In one place before the
  dispatch, the requests that change or read what is shared by everyone
  (network mode, install, approve, uninstall, enable, model download, load,
  unload, import, select, the engine log, the context preview, evals) are
  the administrator's; the one user of a personal workstation is its
  administrator, so nothing changes there. The Store shows members no
  Install and the network pane no choice, saying who decides.
- **A member removed from a workspace kept a tightened document's level.**
  The level held on a document is now the workspace's level capped by the
  tightening, never the tightening alone; "everyone in the workspace" in a
  tightening means the members, and is refused as a member itself.
- **Smaller ones.** An account has at least one role (an empty list was a
  full member). A presence window is keyed by its person together with its
  name, so nobody evicts another's window by guessing the name. A failed
  sign-in verifies a decoy hash when there is no account or no password, so
  its timing says nothing about who exists. A package id is lower-case
  reverse-DNS and never a path (`../..` was a valid id before). The session
  id is read from the cookie or the Authorization header only; the query
  form is the WebSocket upgrade's alone. A trusted-proxy range of every
  address is refused at start.

Held sound by the review, for the record: session ids and their rotation,
the cookie's flags, argon2id and its parameters, one-time links, break-glass
scope and its clearing, the isolation of per-user state, surface origins and
their CSP, CSRF posture, the audit chain within and across files, the
settings whitelist, and the offline administration commands.

Open, as **questions** to the user (each a design decision the documents do
not settle, or a change beyond a day):

1. Should an administrator need break-glass to change another workspace's
   members and document access (today only entering it needs a reason)?
2. Proposal mode: the agent still commits to the shared head; the spec's
   branch the requester sees first — Pilot 1 or later?
3. Lockout and rate limiting: per account and address instead of per
   account, and a request-rate limit in the server layer before Core.
4. May a read-only account export a board it can see (producing an artifact
   document)?
5. A Content-Security-Policy and HSTS on the shell origin (deployment §9.2
   names them; the harness origins have theirs).
6. The audit log: anchor its head outside the files so a truncated tail is
   caught, and fail an action when its record cannot be written.
7. Commits carry the author's kind, not the user; audit surface writes and
   document reads by user.
8. The lows the review lists that remain: invite tokens in GET paths and
   logs, a token-use race, unpurged lock and session rows, a revoked
   session's socket living up to thirty seconds, export documents keyed by
   content hash, one user's egress approval applying to all.

## 2026-09-13, the organisation's name, seats, cursors, and Phase A's gate (commit 10)

The user's answers to the three questions of the board and People work, and
the walk that closes Phase A's gate:

- **`[organisation] name` is a setting** (answer 1; deployment §3.3 gains
  the key). Set at install: the install guide's minimal file opens with it.
  It is shown on the sign-in page, in the browser tab's title
  ("localSpace · Meridian Bank"), on the invitation page and on the People
  page ("Who can sign in to localSpace at Meridian Bank."). When it is not
  set the phrase is dropped, not replaced: "Who can sign in to localSpace."
  and a plain "localSpace" tab, because "here" reads like a placeholder
  someone forgot. An empty value is refused at start like any other bad
  setting. The name travels in `GET /api/v1/auth/mode`, whose answer is now
  a contract type, `proto::AuthMode`, generated for the client like `Me`.
- **No seat cap in Pilot 1** (answer 2). The People page says "3 people",
  never a number the product cannot enforce. When the entitlement of the
  marketplace spec (§5) is built, its seat field stays in the data model so a
  cap later is a display change, not a schema change; nothing of it exists
  in code yet, so nothing was added.
- **Named cursors stay in Phase C** (answer 3); presence landed early and the
  cursor channel is the remaining half.
- **Phase A's gate runs in CI: `web/e2e/org.mjs`.** On an organisation server
  started on an empty data directory whose settings name the organisation:
  the first administrator is made from the link in `first-admin-link.txt`;
  two more people are invited, a member and a read-only account; each of
  the three starts in a personal workspace of their own; the member signs
  in through the sign-in page, which names the organisation, as does the
  tab; the administrator and the member open the same shared board in two
  browsers and each sees the other on it; a note made and typed by one
  reaches the other and a change comes back; the read-only account is
  refused from the API and from the board itself, in the shared workspace
  and on her own personal board; the member, lowered to "can view" on the
  workspace, is refused the same way by the access level; and the audit log
  holds each of those under the right person, denials included. The CI job
  then stops the server and runs `localspace audit verify` over the log.
- **A read-only account changes nothing through a surface either.** The
  walk found that `WriteDoc` and `DocSync` looked only at the access level,
  so a read-only account could edit the board of its own personal
  workspace, which it owns. Both now refuse a viewer before the level is
  looked at, audited as `document.write` denied with the same one sentence
  the tools give, and `roles.rs` has the negative test. Producing an
  artifact (an export) is not gated by role; whether a read-only account may
  export a board it can see is a **question** for the user.

## 2026-09-13, the board and the People page as built against the app screens (Phase A, commit 9, continued)

The user sent the board and the Admin → People artboards again: "the
canvas must look like this and the team section too". What changed, and
what each thing on the screen is backed by, because every claim on a
screen has to be true:

- **The board's bar.** White, 56 px, a hairline under it: back, the
  board's name, "All changes saved" or "Saving…", and on the right the
  network chip, the people on the board, Export and Share. The zoom left
  the bar for the board itself.
- **The people on the board are presence, not decoration.** Core keeps who
  has which board open (`Request::Presence { board, peer }`,
  `Event::Presence { board, people }`,
  `crates/localspace-core/src/presence.rs`). A window of the shell
  announces the board it shows every twenty seconds and nothing when it
  leaves; a window quiet for forty-five seconds is forgotten
  (`Config.presence_ttl_ms`); a window may announce only a board it may
  read; only the people on a board are told who is on it; the same person
  in two windows counts once; a board in another workspace is another
  place. The avatars are those people in order of arrival, their initials
  in the colour the People page gives them. A personal workstation shows
  none: there is nobody else. Another person's cursor with their name (the
  artboard's "Bek") needs a cursor channel and stays in Phase C.
- **Share is a link and who can open it.** In a shared workspace the
  dialog says everyone in the workspace can open the board and how many
  people that is, and copies `/board/<harness>`, which the shell opens
  after sign-in (`web/src/App.tsx`). It gives nobody new a way in and says
  so. A personal workspace has no Share: it takes no members.
- **Export** in the bar asks the surface for a PNG or an SVG by command;
  the surface renders and hands the file to Core as before, and the shell
  says what was exported. The surface's own copy of that note went, for
  the package's size limit (below).
- **Inside the frame the tools float.** A palette on the left (select,
  sticky note, rectangle, ellipse, connector, text, pen, frame; panning is
  the space bar, the middle button or H), the zoom control bottom right,
  and a bar above the board for the selection (colours, lock, bring to
  front, delete) only while something is selected. Undo and redo are
  Ctrl+Z and Ctrl+Shift+Z: the artboard has no buttons for them, and the
  package's size limit left no room for any. The board fits itself to the
  frame once the frame has a size; a frame opened in a hidden tab has none
  at first, and a fit to nothing was the smallest zoom there is.
- **The notes and the connectors.** A note is a pastel card with no border
  and a soft shadow, 196 × 108 by default; its first line is its title in
  bold and the rest its body (`Scene.stickyLayout`); the colours are the
  artboard's (yellow #FBE8A6, blue #CDE3F5, red #F6D2CF, green #D9EFDF;
  amber and grey in the same key). A connector between two shapes leaves
  the middle of the side that faces the other and curves
  (`Scene.arrowCurve`), grey #9A9DA1 at 2.2 px with a 9 px head; towards
  a free end it is straight. The selection is a 1.6 px green outline five
  pixels out with corner handles; the page is dotted. The SVG export draws
  the same, and the tests of the canvas package say so.
- **The board's text is Figtree.** The frame's stylesheet now carries its
  fonts beside it under `_localspace/` (`web/scripts/build-libs.mjs`), the
  only place a harness origin serves shared files from; the canvas
  measures and draws in the same face, and draws again when the font
  arrives.
- **The whiteboard package is 1.3.0**: its web surface changed, and a
  package whose content changes gets a new version (2026-09-11). Its
  bundle first measured 4.7 KB gzipped against the 4.5 KB limit of
  2026-09-10 (`web/size-limits.json`); with the duplicate export note gone
  it is 4.57 KB, inside the limit, which stays where it was.
- **People (Admin).** No side column: People and Workspaces are two quiet
  tabs above the heading; Search is a button that opens the field; the
  rest of the table was the artboard's already. Two things the artboard
  says that the product cannot say truthfully stay out: "at Meridian Bank"
  needs an organisation name the configuration does not carry, so the
  line reads "here"; "4 of 20 seats used" needs a seat cap Pilot 1 does
  not have (2026-09-13, the shell as built), so the count reads "N
  people". Both are questions to the user, still open.

## 2026-09-13, CI: what fifteen red runs were, and the pipeline that replaces it

Every run of the workflow since the first push failed, all of them in the
rust job, each between 29 and 56 minutes except run 13. The causes, read
from the logs (`gh run view <id> --log-failed`) rather than guessed:

- **Run 13, the fast one (2 min): rustfmt.** `cargo fmt --check` on
  `crates/localspace-core/tests/preferences.rs`, a file formatted after it
  was staged. Fixed in 16acda1.
- **Run 12 (56 min): the runner ran out of disk during `cargo test
  --workspace`.** The job shows no failed step because the runner itself
  could no longer write its log ("No space left on device" from the
  runner's worker, in the job's annotations), 37 minutes into the tests
  step, after clippy had passed. Runs 2 to 11, 14 and 15 are the same cause
  wearing three faces: `No space left on device (os error 28)` from rustc
  and cargo while building the tests (runs 2, 5, 9); `ld terminated with
  signal 7 [Bus error]` while linking a test or the `localspace` binary,
  the linker writing a memory-mapped output on a full disk (runs 2, 5, 11,
  15); and the runner's own log failing, which leaves the job without a
  failed step (runs 3, 4, 6, 12, 14).
- **Run 1 (29 min), the only run that reached the tests: three tests in
  `crates/localspace-core/tests/engine.rs` assumed a GPU.** They built
  their Core with `Config::personal`, which detects the machine; the runner
  has none, the planner's verdict for the stub model was "does not fit"
  instead of "resident", and `LoadModel` was refused. They now describe a
  W32-class machine (a 32 GB GPU, 64 GB of RAM, 16 cores) and the W32
  profile in the test, so the verdict is the same on every host; what they
  test is the sidecar path, not the planner.
- **Not the cause: Tauri's system libraries.** With the five packages the
  job installs, clippy over the whole workspace, shell included, passed on
  Linux in run 12 (16 minutes), and no run reports an error from the
  shell's build; the failures came after it, out of disk. The shell stays on
  Linux. The split of 2026-09-11 (Core and the server on Linux, the shell on
  a Windows runner) remains the fallback if it ever stops building there;
  Core and the server never leave Linux.

What changed, in the order asked:

- **Cheap before expensive.** A `check` job (rustfmt over the workspace and
  the harness crates; oxlint; `tsc -b` for the app, and the type checks of
  the canvas package and the whiteboard surface) gates `web` and `rust`,
  and `e2e` needs both. Every job has a timeout: check 10, web 30, rust 60,
  e2e 45 minutes. The check job's commands take under a minute on the
  build machine; a commit that fails them costs about two minutes of CI.
- **A warm build.** `Swatinem/rust-cache` replaces `actions/cache` in the
  rust and e2e jobs: the first action in the workflow not written by
  GitHub, on the user's instruction, pinned to commit 6323deb (v2.9.2)
  rather than a moving tag, with `cache-on-failure` so a red test still
  warms the next run. It caches the workspace target and the two harness
  crates' targets.
- **Disk.** The test build carries line-table debug info only
  (`CARGO_PROFILE_DEV_DEBUG=line-tables-only`; `panicked at file:line` and
  backtraces stay readable). A full-debug target of this workspace measures
  59 GB on the build machine, 13 GB of it debug-info files and 23 GB
  libraries, with 17 integration-test executables that each link Core and
  its engines (wasmtime, Automerge, redb). The rust job also removes
  toolchains the runner image ships and this build never uses (Android,
  .NET, Haskell, CodeQL, cached Docker images: about 20 GB) before
  building, and prints the disk after the build so the margin is known.

- **A superseded run is cancelled.** `concurrency` per branch with
  `cancel-in-progress`: a second push while the first is still building
  stops the first. The gate is about the head of the branch.

- **The first green runs.** Run 17 (2026-09-13, 83da035) was the first
  with a green rust job on Linux, cold: clippy 16 minutes, the tests 32,
  the job 53 against its 60-minute timeout; the disk step freed 24 GB and
  the line-table build left 26 GB spare with an 11 GB target. Its browser
  walk failed on the walk's own reading of a fresh board, which has no
  shapes yet (fixed, 6bfebd1). Run 18, on the cache run 17 saved, was the
  first green run: check 21 seconds, rust 6 minutes 16 seconds, web 67
  seconds, the walk 3 minutes 25 seconds, about ten minutes in all. The
  rust timeout stays at 60 minutes: a cold build needs most of it, a warm
  one a tenth.

Green CI is commit 10's gate (`docs/PILOT-1.md`, Phase A): nothing in
Phase B starts on top of unverified commits.

## 2026-09-13, the shell as built against the app screens (Phase A, commit 9)

The UI reference is the user's design canvas "localSpace App Screens"
(seven 1440×900 artboards: sign in; first run; an answer with sources;
the whiteboard; the Store; Settings; Admin → People), sent on 2026-09-13
with the instruction to follow it. Directives 1–3 and 5 and the desktop
additions of 2026-09-12 hold; the screens are how they look.

- **Palette, type and shape from the screens:** page `#FAF9F8`, rail
  `#F4F2EF`, ink `#1C1E20`, one green `#1D7A55`; Figtree, vendored
  into the bundle under the SIL Open Font License (`web/public/fonts`),
  never fetched from anywhere; radii 9 and 14. The tokens stay in
  `@localspace/ui`'s stylesheet, and the surfaces read them through the
  `--color-*` names, so the whiteboard's chrome follows.
- **The rail** is 248 px with the mark, New chat, Chats, the workspace's
  pages (each installed tool with a page, Documents, Store) and Admin
  (organisation administrators only), Settings and Help at the bottom. It
  collapses to 56 px of icons with Ctrl/Cmd+B or the toggle, and the
  choice is kept with the user on the server (`GetPreferences` /
  `SetPreference`, a fixed set of keys, `rail_collapsed` the first). On
  the board it is icons by default and opens for that visit only.
- **The top bar carries two things always:** the network as a word
  (Offline; Online, asks first; Online) and, only when the assistant is
  not ready, why (Starting up…; Reconnecting…; No model — choose one in
  Settings / ask your administrator), plus the person's initials. No
  URL, port, endpoint, token count or model filename appears anywhere a
  member can see; those live under Settings → Advanced, which a member of
  an organisation cannot open.
- **The chat** opens on a greeting by first name and the time of day,
  three things to try (each puts words in the box; the third is "Plan on
  the board" when the whiteboard is installed, "Explain something"
  otherwise), the composer with Attach a file (present and disabled,
  saying that files come with document search), the assistant's human
  name as a chip that opens Settings → Assistant, and the footnote naming
  where answers come from. Tool calls show as what they did, in Core's
  words, never a tool's name. Sources under an answer wait for retrieval
  (Phase B): nothing is drawn that is not there.
- **The board** fills the page; the harness's surface is the board; the
  shell adds the title (the document's own when it has one), "All changes
  saved" / "Saving…" from the writes in flight, the zoom, and the chat as
  a drawer on the right. Presence and Share come with Phase C.
- **The Store** lists the catalog's tools as icon, name, one sentence and
  one button; what a tool is made of sits behind Details; a widened
  capability is asked about in a dialog, not a browser prompt; nothing is
  listed that is not in the catalog.
- **Settings** has General, Assistant, Network, Tools and About, and
  Advanced as one collapsed row. Assistant lists the models on the server
  by their catalog titles with a reason each (the answer to the open
  question of 2026-09-13: **the catalog names them**; an administrator's
  own names and descriptions can come with the admin pages of Phase C);
  whoever may add a model does it there in plain words. Advanced holds
  the endpoint form (personal mode only), the engine and its log,
  diagnostics (the turn's tools, the task ledger, the prompt preview, the
  trace), history with undo/redo/drop, the environment lock, and running
  a tool by hand.
- **Sign in** is email and password in an organisation, the token on a
  personal computer opened by hand (the app signs its own window in). A
  one-time link at `/invite/<token>` sets a password; the first
  administrator's asks for name and email too. **Admin → People** is the
  table of the screen with the roles as words (Administrator, Member,
  Can view only), "Invitation sent" until a password is set, and a menu
  per person: role, a new link, unlock, sign out everywhere, disable.
  **Admin → Workspaces** makes shared workspaces and sets each member's
  level in words. The count line says "N people": there is no licence and
  no seat cap in Pilot 1, so no "of 20". Administrators see a banner while
  sign-in is by local accounts.
- **Help** is a page of four short answers and where to turn ("Ask your IT
  team"); a fuller guide comes with Phase D.
- The gate walk (`web/e2e/whiteboard.mjs`) now drives the Store tile and
  the Whiteboard item in the rail, and passes.

## 2026-09-13, the one binary and its settings file (Phase A, commit 8: four answers)

Deployment §3.1–3.3, §4.1, §9.1; Pilot 1 (`docs/PILOT-1.md`) answer 27 and
the install guide. Answered on 2026-09-13, as recommended, with the fourth
changed.

- **A new crate, `localspace-cli`, owns the `localspace` binary** (answer 1;
  deployment §3.1 promises one binary by that name). It carries `serve`,
  `doctor`, `bench`, `evals`, `call`, `admin` and `audit`, and links no
  GUI toolkit: a headless Linux server does not carry one. The egui crate
  keeps a binary name of its own, `localspace-desktop`, until it is
  archived; the Tauri shell stays `localspace-app`.
- **Everything a deployment sets lives in `localspace.toml`**, under the
  names of deployment §3.3 (answer 2). Flags are limited to `--config`,
  `--insecure`, `--allow-below-floor`, `--personal` and `--token`, and a
  flag overrides the file. The file is `--config`'s, else
  `/etc/localspace/localspace.toml` when it exists (on Windows
  `%ProgramData%\localSpace\localspace.toml`), else defaults.
- **A `tls = { cert, key }` value refuses to start** (answer 3): native TLS
  termination is not built yet, and a key that is set, believed and
  ignored would serve a company's documents in plaintext. The message
  names the supported path: "TLS termination is not built in yet; put
  localSpace behind a reverse proxy and set `tls = "behind-proxy"` and
  `trusted_proxies`." The same rule holds for every key: **a key or value
  this release does not honour is refused at start, never ignored** — an
  `oidc` provider, `at-rest` encryption, a non-local audit sink, an online
  registry, a retention or limit the code does not enforce. Unknown keys
  are errors that name the key and the file.
- **The first administrator's link is minted by the server itself**
  (answer 4, changed from the plan's `admin bootstrap` after start).
  When `serve` starts in organisation mode and there are no users at all,
  it mints a one-time link, logs it, and writes it to
  `<storage.root>/first-admin-link.txt`, readable only by the service user
  (mode 0600 on Linux; the data directory's own permissions on Windows),
  and deletes the file once the link is used. The link asks for the
  administrator's name, email and password, since the server knows none
  of them yet. The install guide becomes "start the service, open the link
  from that file" — no second command. There is no authenticated admin
  channel to a running server in Phase A: that is new privileged surface
  for one rare operation. `localspace admin bootstrap` stays as the
  **offline** path for minting a new link with the service stopped, and
  password recovery for a locked-out administrator,
  `localspace admin reset-password --email`, runs offline too — a few
  seconds of downtime is acceptable for a break-glass operation, and the
  running server's surface stays the HTTP API alone. Both commands take
  `--config` (the file names the data directory) or `--data <dir>`; both
  refuse, naming the fact, while the server holds the database.
- **As built (commit 8, second of three).** The link is a one-time token
  like any other, marked as the first administrator's and bound to no
  account; `GET /api/v1/auth/invite/{token}` answers `first_admin: true`
  so the page asks for a name and an email as well as a password, and
  `POST /api/v1/auth/set-password` takes them; a user's own link ignores
  them. Minting again while there is still nobody kills the earlier link,
  so only the newest — the one in the file and the log — opens. The file's
  text names the 24 hours and what to do after them. Accepting the link
  deletes the file; a start with accounts already there deletes a stale
  one. `admin bootstrap` and `admin reset-password` audit what they did
  as the actor `operator` and, without a settings file, print the link
  without an address in front of it and say so. The `--bootstrap-admin`
  flag is gone.
- **As built (commit 8, third of three).** `doctor`, `bench`, `evals`,
  `call` and `audit verify` moved from the egui crate into the one
  binary; the egui crate keeps the GUI alone under `localspace-desktop`.
  Every command reads the same settings file as `serve`. `bench` and
  `evals` run on a scratch Core in memory, with the harnesses installed
  from the catalogs the file names (`[harnesses] catalogs`) for that run
  only, so a measurement or a test never touches the data; a widened
  capability is approved for the run, and the output says so. `call`
  acts on the data the settings name, as the organisation's operator or,
  with `--personal`, as the workstation's user, and meets a running
  server the way the admin commands do. Command operands (`evals
  <harness>`, `call <tool> [json]`, `--email`, `--data` for the
  offline commands) are the command's own, not deployment settings; the
  five flags of answer 2 remain the only settings on the command line.

## 2026-09-13, the audit writer as built (Phase A, commit 7)

Deployment §10.1 ("append-only, hash-chained, daily rotated, verifiable
with `localspace audit verify`") and §16.4; Pilot 1 Phase A.

- **One file per UTC day**, `audit/YYYY-MM-DD.jsonl`, the day taken from
  the record's own timestamp. UTC, not local time, so a file's name means
  the same on every machine and the cut does not move with daylight
  saving. The chain runs across files: the first record of a day carries
  the hash of the last record of the day before, and `verify` walks the
  files in name order, one line at a time.
- **The writer never opens an older file than the last it wrote to.** A
  clock stepped back across midnight, before or after a restart, puts
  records in the current day's file with their own timestamps, so the
  files stay in chain order and `verify` stays true.
- **A writer thread behind a bounded channel** of 4,096 records. Core's
  thread computes the chain (the blake3 of a record is microseconds) and
  hands the record over; the writer serialises and appends, one write per
  record, unbuffered, so a crash loses nothing the writer had reached. When
  the writer is 4,096 records behind, an append waits rather than drops:
  the local file is the record, and §16.4's rule that the hot path never
  waits is about the SIEM, which is not in Pilot 1. A record the disk
  refuses is counted (`write_failures`) and reported through `tracing`;
  the chain in memory continues, so the failure is visible and the next
  restart continues from what is on disk.
- **The file from before the cut**, `audit/audit.jsonl`, is read first and
  never written again; a log that has one continues its chain from its
  last record. Nothing is rewritten or renamed: an append-only log is not
  migrated by editing it.
- **Ids and fields stay as they were** (`a<12 digits>`, `ts_ms`): the hash
  is over a record's serialised bytes, so a renamed field would break every
  chain already on disk. The spec's example (`ts` as ISO time, ULID ids) is
  a presentation; the verify walk and the export of §10.2 can render
  either.
- **A log kept in memory** (tests, `--ephemeral`) keeps its records in
  memory; a log on disk keeps none there, so Core's memory does not grow
  with the audit. `records()` on a log on disk writes out what is pending
  and reads the files back.
- **`localspace audit verify --data <dir>`** prints the count, the files
  and the first and last day when the chain is intact, and exits 1 naming
  the file and line of the first broken or unreadable link. It lives in
  the `localspace` binary with `doctor` and moves with it into the one
  binary of commit 8.
- Tests: the audit unit tests (records go to the file of their day and the
  chain runs across days; a record edited in an older file is found with
  its file and line, and a deleted one breaks the next file's first link; a
  record stamped before the current day stays in the current file, before
  and after a restart; the old file is read first and continued from;
  three times the queue's depth of appends all land; a line that is not a
  record is reported with its place; days are named in UTC).

## 2026-09-13, roles and tool exposure as built (Phase A, commit 6)

Deployment §4.3 (the Viewer row: "read-only in granted workspaces; can
chat with documents but the agent has no write tools") and §6.2; plugin
spec §9; Pilot 1 Phase A.

- **A viewer is a read-only account.** A caller whose roles hold `viewer`
  and nothing above it (`Caller::is_viewer`; roles add up, so a viewer who
  is also a member is a member) is shown only the tools of kind `read`,
  Core's own included: `find_capability`, `web.search` and `web.fetch`
  stay, `task.plan` and `task.note` go. A call to anything else — from
  the client, the agent or an eval — is refused before any harness runs,
  with one sentence, "Your account can read here but not change anything."
  (`READ_ONLY_REASON`), and audited as `tool.call` denied with
  `why: read-only account`. The workspace level the viewer holds does not
  raise this: an `edit` membership given to a viewer reads.
- **`compute` tools count as changes for a viewer.** A read-only account
  is shown and allowed `read` only; `compute` needs a member's account.
  (For a member with a `view` membership, below, `compute` stays, as it
  changes no document.)
- **A member who may not change a board is shown none of its writing
  tools** (§6.2, where a `view` member's outgoing changes are rejected).
  Per installed harness, Core asks the same check a write would go through
  — on the workspace's document for that harness once it exists, on the
  workspace's membership before that, since the document inherits it when
  it is made — and hides that harness's `write` tools from the active set
  when the caller holds less than `edit`. Their account is not read-only,
  so Core's ledger tools and other harnesses' writes stay. The refusal on
  a call is the document check that already existed. Exposure follows the
  caller across workspaces and levels on their next request; break-glass
  shows an owner's tools inside.
- Why exposure and not only refusal: a model shown a tool it may not use
  proposes it and is refused, which reads as the product failing; tools it
  is not shown it does not propose (plugin spec §9 is about what the model
  sees). Refusal stays as the enforcement, exposure as the courtesy, and
  the two agree by construction (both ask `AccessControl::check` or the
  role).
- Tests: `crates/localspace-core/tests/roles.rs` (a read-only account is
  shown and allowed only the tools that read, with its refusals audited; a
  member who may not change a board sees none of its writing tools, before
  and after the board exists, and sees them again when given `edit`, in
  their own workspace, and under break-glass) and the exposure unit tests
  (read-only account; read-only harness; the builtin kinds agree with the
  builtins shown).

## 2026-09-13, workspaces and access as built (Phase A, commit 5)

Deployment §5, §6.1 and §6.3; Pilot 1 (`docs/PILOT-1.md`) Phase A.

- **Who makes a workspace, who owns it.** Only an administrator creates a
  shared workspace (`CreateWorkspace`), and the creator owns it. Members
  are added one by one, each with a level (`view`, `comment`, `edit`,
  `owner`), by one of its owners or an administrator; a member holding
  less than `owner` is refused. A personal workspace takes no members at
  all: it is one person's. Ids are `ws_<uuid>`; personal ones stay
  `ws_<user>`.
- **Agents propose in shared workspaces** (§6.3): a shared workspace is
  created with `agent_writes = proposal`, a personal one with `direct`;
  an owner or administrator changes it (`SetAgentWrites`).
- **A document follows its workspace live.** A document with no ACL of its
  own answers to its workspace's members as they change: adding, changing
  or removing a member takes effect on the next request, with nothing to
  re-sync. `SetDocumentAccess`, by the document's owner or an
  administrator, gives it an ACL of its own, and that ACL can only tighten:
  a level above what the workspace gives the same principal, or a
  principal the workspace does not have, is refused with the reason;
  `None` clears the tightening and the document follows its workspace
  again. This is §6.1's "tightened, never loosened beyond its workspace"
  as a property the store enforces, not a convention.
- **Break-glass** (§5 roles, §6.1): an administrator who is not a member
  goes into a workspace with a reason or not at all; a member goes without
  one. The entry is audited as `workspace.break_glass` with the reason.
  While inside they hold `owner` on that workspace's documents,
  tightened ones included, through the same check as everyone else (there
  is no bypass; the check grants it), and every audit record they make
  there carries the reason in `actor.break_glass`. It ends the moment
  they go to a workspace they are a member of, and it does not survive a
  restart: after one they land in their personal workspace.
- **Where a user was is remembered.** The workspace a user was last in is
  kept per user in the database and reopened after a restart if they may
  still be in it; otherwise their personal one opens.
- **One board per workspace and harness.** The first request for a
  harness's document in a workspace materialises the workspace's one board
  for it; every member with `edit` writes to the same document, a
  `view` member reads it and is refused writing tools with a one-sentence
  reason, and a non-member is refused the document and cannot select the
  workspace. Removing a member ends their way in on their next request.
- **`ListWorkspaces`** shows a member their workspaces with the level they
  hold and which one they are in; an administrator sees every workspace.
  `EnvironmentState` names the workspace by id as well as by name, so the
  client can act on it without matching names.
- Tests: `crates/localspace-core/tests/workspaces.rs` (a shared workspace
  is one board for its members and none for others, including break-glass
  and removal; tightened but never loosened, with the refusals audited as
  `document.access`; members and the remembered workspace survive a
  restart; a personal workspace takes no members) and the ACL unit tests
  (break-glass is `owner` in that workspace and nothing elsewhere).

## 2026-09-12, two additions to Phase A's shell commit: the desktop

- **The desktop build looks like a finished application** (architecture
  §6.2): its own window with the localSpace name and icon, Core started by
  the app itself, and **no URL, port or endpoint visible anywhere in the
  interface, in either mode** — the model card, which printed
  `http://127.0.0.1:65334/v1`, included. The technical id of a model may
  appear under Advanced; where a model is served from does not. Endpoints
  and ports stay in the logs and in `localspace doctor`.
- **Every demo before December runs on the desktop shell**, so the shell is
  reported on honestly and brought to the state a non-technical user opens
  on a workstation: the report and the plan for it are in the session of
  2026-09-12 and folded into `docs/PILOT-1.md`.
- **The desktop, answers 2 to 7.** The "connect to a model server" form
  stays under Settings → Advanced in personal mode — something the user
  types, not machinery shown at them — and is absent in organisation mode.
  The installer ships llama.cpp's **Vulkan** build, which runs everywhere
  at a real fraction of CUDA's speed, right for a first launch; the CUDA
  build is a one-click download under Settings → Model when an NVIDIA GPU
  is detected, provisioning egress like a model (plugin spec §8.4). The
  installer is an **NSIS `.exe` through `tauri-cli`**; MSI when an
  organisation asks to deploy by policy. `tauri-plugin-dialog` and
  `tauri-plugin-single-instance` are approved, and if the week gets tight
  the visible failure and the single-instance guard come **before** the
  installer: an app that opens nothing, or opens twice and dies on a
  database lock, is the worst thing that can happen in front of a
  prospect. **Windows only** for Pilot 1: the organisation server runs
  `serve` headless with a browser client, so no AppImage or `.deb`; Linux
  CI still matters for Core and the server. **Signing:** the certificate
  process is under way; the first demos run unsigned, the pilot install
  runs signed; the bundler is configured so that adding the certificate is
  a configuration change, not a rework, and the install guide says
  SmartScreen warns until it is in place.

## 2026-09-12, answers to Phase A and the shell, and a fifth directive

- **The migration is proved against a real v1 database** (review of
  2026-09-12). A data directory the previous binary wrote — commit
  `5db9fcf`, the last before the schema version — is checked in under
  `crates/localspace-core/tests/fixtures/v1/` with a note on how it was
  made, and `tests/migration.rs` migrates it: the board under its old
  name with its three stickies, its history live enough to undo through,
  both exports byte for byte with their hashes, the conversation, and the
  schema stamped; and it proves the v1 file is copied before the first
  write and never written to, by making it read-only and hashing it
  before and after. Nothing generated by the migration code stands in for
  the real file.
- **Directive 5 — an employee logs in, picks a model, and types. Nothing
  else.** Models get human names and a one-line reason to pick one — "Fast
  · good for quick questions", "Balanced · the everyday choice", "Most
  capable · slower, for hard work" — with the technical id only under
  Advanced. In organisation mode the admin sets the default, and a setting
  decides whether members may switch at all; when they may not, there is
  no picker on screen. First run after login is an empty chat with the
  cursor in the box and three example prompts: no wizard, no tour, no
  setup step. A vocabulary rule for everything a member can see: no token,
  context window, embedding, harness, tier, wasm, commit, DAG,
  entitlement, gateway, sidecar, quantisation; say tools, or the tool's
  own name; say version or change, not commit — "harness" is our word,
  not theirs. Every error says what happened in one plain sentence and
  what to do about it ("Your assistant isn't ready yet. Ask your
  administrator."), never a code or a stack. Empty states are written: an
  empty chat, an empty Documents page, an empty Store each tell a
  first-time user what to do. The wording pass is part of Phase A's shell
  commit, not polish for later: renaming things after pilot users have
  learned them is worse than naming them right once. The rule is kept by a
  check in CI over the member-facing strings of the shell.
- **Passwords** (approved 2026-09-13, with three additions): at least twelve
  characters and no composition rules, the right modern choice; new
  passwords are screened against an embedded list of the most common ones
  and a match is refused — the one check that prevents compromise, and it
  works offline; up to 256 characters are accepted, past the 64 the spec
  asks for, with spaces and any Unicode, counted as characters and never
  silently truncated; and no forced expiry. The embedded list was assembled
  offline from the well-known most common bases and their usual suffixes
  (`crates/localspace-core/src/common-passwords.txt`, about 6,500 entries of twelve or
  more characters); a canonical list of the most
  common passwords replaces it when one can be supplied, and the loader
  takes any newline-separated file.
- **Identity, as built on 2026-09-12** (Phase A, commit 4): a session id
  and a one-time token are 32 random bytes as hex, stored as their blake3;
  the session cookie is `ls_session`, httpOnly, SameSite=Strict, `Secure`
  behind TLS, with a `Max-Age` of the session's life; a one-time link is
  `<public url>/invite/<token>`; the first administrator comes from
  `--bootstrap-admin <email>` on the serve binary until `localspace admin
  bootstrap` exists (commit 8), and only while the server has no accounts;
  an open socket rechecks its session every thirty seconds and closes with a
  word when it has ended; the address on the audit and the lockouts is the
  connection's until `trusted_proxies` arrives with the settings file.
- **Phase A, answers 1 to 10, as recommended:** `clap` with the derive API;
  argon2id at 64 MiB, 3 iterations, 1 lane, a 16-byte salt, PHC strings,
  hashed on a blocking thread so a login cannot stall the runtime; the
  lockout as specified (five per account with doubling, thirty per IP in
  fifteen minutes, records in the database, admins see locked accounts and
  clear them, the message never says whether the account exists); sessions
  of 32 random bytes stored hashed, hard expiry at `session_ttl`, the same
  id as the bearer; the ledger and conversations in the database per user
  and workspace; `doc_<uuid>` ids with the oldest document as the shown
  board and the v1→v2 migration after a copy; the database renamed
  `db/localspace.redb` in that migration; events routed per user, a
  document's change to every connected user with `view` on it; the admin
  command line limited to `bootstrap` and `user reset-password`;
  break-glass with a required reason, audited, in Phase A.
- **The shell, answers 1 to 11:** the rail's order as proposed, with "New
  chat" a button at the top rather than a list item; ten recent chats of
  the current workspace and "All chats" with a search; canvas-first when a
  board is open, the chat a drawer; **Advanced is admin-only in full in
  organisation mode** — a member never reaches the endpoint, port, token
  budgets, context preview, active set, engine log or trace, not even by
  digging — and exists for the user, collapsed, in personal mode; the
  status pill says "Ready", "Thinking…", "Waiting — 2nd in line",
  "Starting up…", and "No model — ask your administrator" in organisation
  mode against "Choose a model" linking to the picker in personal mode;
  the network indicator's member-facing label is **"Offline"**, not
  "Air-gapped" — `airgapped` stays the configuration value and the word in
  the admin popover, where the security reviewer wants to see it; a Store
  card leads with an icon, the title, one line of what it does for the
  user, and Install, with publisher and version small and tier, memory,
  capabilities, eval count and source registry behind a Details expander;
  the rail state remembered server-side per user; Ctrl+B and Cmd+B; undo
  belongs to the document being looked at; nothing pre-seeded anywhere,
  CI included.

## 2026-09-12, four directives on the shell and the canvas

Given after seeing the running app: the right panel showed an endpoint, a
port, a context size, tool counts, a token budget and "removed 1
field(s)" — machinery, none of it the user's work, which is what makes an
app feel like a wrapper. A lawyer at a bank should never learn what a token
budget is. Nothing below changes Phase A's gate or the December date.

- **Directive 1, hide the machinery** (plugin spec §8.1 on the indicator).
  The right panel leaves the main screen. Active model, tools and their
  counts, context, recent changes, the endpoint and the port move into
  Settings under an Advanced section a curious admin opens and an employee
  never does. Two things stay visible, as a compact strip, not a panel:
  the network-mode indicator, because the spec requires it always visible
  and it is the promise the product sells; and a small status pill — the
  model is ready, or the request is queued at position N.
- **Directive 2, collapsible chrome and work-shaped navigation.** The left
  rail gets a collapse control and a keyboard shortcut, to icons and then
  away, remembered per user. It lists the user's work: their chats, the
  harnesses they have installed (the Whiteboard as an openable item once
  it is there, not under "Tools"), their documents — Documents, not Data —
  and one entry that opens the Store. Models, tool permissions and the
  diagnostics go into Settings. The agent's ledger and artifacts sit
  beside the conversation that produced them, not on an Agents page; undo
  history belongs to the document being looked at, not to a global
  History page.
- **Directive 3, the Store is real and the whiteboard comes from it**
  (architecture principle 3). A Store page with cards, descriptions, what
  each harness does, an Install button and an installed state, even while
  it serves a local registry folder in the pilot. The whiteboard is not
  pre-seeded: the user installs it and watches it appear. An org admin can
  pre-install for employees; the Store stays visible.
- **Directive 4, the canvas earns "Miro-like"**, in order of what each
  contributes: navigation that feels weightless (trackpad and wheel pan
  and zoom with momentum, space-drag, zoom to fit and to selection, a zoom
  control that is not just a number); marquee selection and multi-select
  that moves and resizes as one bounding box with handles; connectors that
  bind to shapes and reroute when a shape moves; text that edits in place
  instantly and sticky notes that fit their text; a right-click menu with
  copy, paste, duplicate, delete, z-order, align and distribute; images
  pasted or dropped onto the board; live cursors with names.
- **Where they land.** Directives 1 to 3 are shell work measured in days
  and fold into Phase A's shell commit, which already opens the shell for
  login, the admin page and the workspace switcher. Directive 4 goes into
  Phase C beside presence: canvas depth and live cursors together are what
  make a pilot user say "this is our Miro", and apart they touch the same
  code twice.
- **What already exists of Directive 4**, so the plan counts only the gap:
  wheel zoom at the pointer and pan, space-drag, zoom to fit, marquee
  selection, multi-select moving as one, arrows bound to shapes that
  follow them (`scene.endpoints`), in-place text editing, z-order, align
  and distribute as agent tools. Missing: momentum, zoom to selection and
  a zoom control, multi-select resizing as one box, notes that grow with
  their text, the context menu and its actions in the UI, images, live
  cursors.

## 2026-09-12, answers to the Pilot 1 plan

`docs/PILOT-1.md` is approved: four phases, about three months of building
to a pilot-ready build in early December, the partner's week after it. The
user plans the pilot conversations around that and holds, dated: the W32
trip, the private repository before Phase A's gate, and the partner with
its hardware before Phase D — both settled by mid-November.

- **Identity first** (answer 1): retrieval built on a single-user Core is
  retrieval built twice.
- **Behind a proxy only, for Pilot 1** (answer 2; deployment §9.1): Caddy in
  the guide terminates TLS; native TLS is a Phase D stretch. **Spec
  addition, authorised:** the server refuses to start when it binds
  anything but loopback without TLS or `trusted_proxies` set, unless an
  explicit `--insecure` flag is passed, which is logged loudly. Nobody
  serves a company's documents in plaintext by forgetting a key.
- **Passwords are argon2id** (answer 3), through the `argon2` crate; the
  parameters are recorded here when chosen, so they can be reviewed rather
  than discovered.
- **OIDC through the `openidconnect` crate** (answer 4), which covers the
  details the failure modes live in.
- **The artefact is a tarball** (answer 5): the binary, the web bundle, the
  registry folder and the unit file; embedding the bundle waits for the
  release pipeline.
- **Backup is the single `.tar.zst` of deployment §11.3** (answer 6), with
  the `tar` and `zstd` crates.
- **Team mode on a W32-class box is the planning floor** (answer 7;
  deployment §12.1): one interactive stream, ten users. If the partner's
  machine is better, the profile rises, not the scope.
- **Admin pages from Phase A** (answer 8); `localspace admin bootstrap`
  stays the command-line path, since the first admin cannot create
  themselves through a page they cannot log into.
- **Presence is ephemeral events over the event stream** (answer 9), never
  in the DAG.
- **Shared workspaces default to `proposal`** (answer 10; deployment §6.3),
  and an owner may set `direct`.
- **A local user's first password comes from a one-time link** (answer 11):
  single-use, expiring in 24 hours, regenerable by an admin; no password
  passes through an admin.
- **One board per workspace per harness is shown; many are held** (answer
  12): the data model carries several documents per harness per workspace
  from Phase A, and the UI shows one until a list view is asked for.
- **The audit's IP comes from `X-Forwarded-For` only behind
  `trusted_proxies`** (answer 13), else from the socket.
- **`doctor` warns about an unencrypted volume and shows a banner** (answer
  14); the partner accepts it in writing on the acceptance list.
- **Two additions to Phase A, because local accounts are new attack
  surface:** repeated failed logins are rate-limited and locked out per
  account and per IP, with the attempts audited; and every live session of
  a user is invalidated when their password is reset, their role changes or
  an admin disables them, so a revoked employee does not keep working
  because a tab stayed open.
- **The provenance refinement of 6.0 stands** — Core fills in the document
  and the head commit, because the surface knows its board and not Core's
  history — as a better design than the one specified.

## 2026-09-12, answers to the 6.0 plan

- **One types package, and every kind is declared** (answers 8 and 9;
  plugin spec §18.3, §17). `io.localspace.types` 1.0.0 in `registry/types/`
  declares `outline.v1`, `image.v1` and `svg.v1` in its `types.toml`, each
  with a title, a MIME type, a file extension and the fields an artifact of
  it carries. An install is refused when `produces` or `accepts` names a
  kind no installed types package declares, so a package depends on the
  types package that declares its kinds and the dependency installs it;
  the whiteboard is 1.2.0 and the planner 1.1.0 for that. A package loaded
  from a directory at startup has its dependencies resolved from the
  catalog the same way, and is set aside with a notice when they cannot
  be. An artifact carries `fields`, checked against its type's required
  list, and `file {name, mime, bytes}` when it is a file, which is where
  the shell shows a Download.
- **An export is the selection when there is one, else the whole board**
  (answer 1); a selected frame brings its contents.
- **PNG geometry** (answer 2): two device pixels per board unit, the longer
  side capped at 8,192 px by lowering the scale, 24 units of padding, the
  board's background colour, no grid dots.
- **SVG content** (answer 3): the same padding and a background rectangle;
  text as `<text>` lines wrapped exactly as the canvas wraps them, in
  `system-ui, -apple-system, Segoe UI, Roboto, sans-serif`, with the font
  size and the line height written on every `<text>` so the file does not
  reflow where it is opened; no embedded fonts, scripts or external
  references.
- **File names** (answer 4): the board's title slugified, or `board`, then
  the first seven characters of the commit, then `.png` or `.svg`. Core
  fills in the document and the head commit when a surface names neither,
  and checks them when it does: a surface knows its board, not Core's
  history.
- **Where an export lives until 6.2** (answers 5, 6 and 13; deployment
  §3.4). Its bytes go through the DAG's content-addressed blob table, as a
  blob document's do; a `documents` table in the same redb holds the
  record, so an export is listed after a restart without being loaded into
  memory; its document id is `blob:<blake3>`, so identical bytes are one
  document and each export is its own commit with its own provenance. An
  export is not a change to the board and is not in the board's history:
  its commit is on the export document, as `surface:export` by the user,
  with `{kind, name, document, commit}`; undoing it empties the export
  document while the artifact's pinned commit stays readable. The document
  listing comes forward from 6.2, with the `Harness` and `Export` sources;
  6.2 adds `Upload` and `Web`, the index state, and blobs as files.
- **The route's body limit is 200 MiB for now** (answer 7; deployment
  §3.3), the spec's default for `max_upload_mb`, a constant until
  `localspace.toml` carries the key.
- **Download** (answer 10) is offered three ways: a notice when the export
  lands, the artifact's row on the Agents page, and the document's row on
  the Data page. Always as an attachment with `nosniff`, never inline on
  the app's origin.
- **"Copy as PNG" is not in 6.0** (answer 11), though the manifest's
  `clipboard = "on-user-action"` would allow it.
- **`[server] clamav`** (answer 12; deployment §3.3) is `tcp://host:port`
  or `unix:/path`, as the spec now reads.

## 2026-09-12, answers to the step-6 plan

- **Export moves to the front of step 6, as 6.0** (the plan's 6.9; plugin
  spec §18.3). The interchange-types package it creates is a dependency of
  anything that produces artifacts, so it exists before retrieval writes
  web-cache documents. The order is now: 6.0 export and the types
  package; 6.1 citations in the proto and the chat; 6.2 documents as
  files; 6.3 upload; 6.4 extraction and chunking; 6.5 embeddings; 6.6 the
  index and `docs.search`; 6.7 the gateway; 6.8 the evals page and the
  `localspace` command.
- **Spec change, authorised: encryption at rest is its own step, and the
  index is not encrypted** (the plan's spec issue; deployment §9.1 against
  architecture §3 and plugin spec §1.2, §16.4). An encrypted index cannot
  be memory-mapped: a decrypting layer would hand tantivy and usearch
  their bytes from Core's heap, and any real corpus would break the 50 MB
  budget. Step 6 builds blobs and the index unencrypted. Encryption at
  rest becomes its own step, scheduled before the first enterprise pilot,
  with this design: per-document keys encrypt the blob and the stored
  chunk text, so destroying a key crypto-shreds the document; index terms
  and vectors stay unencrypted and memory-mapped; shredding deletes the
  document's chunks from the index, which it can do because the index is
  mutable and the DAG is not; volume encryption covers the index at rest,
  and `doctor` checks for it. Deployment §9.1 says so now.
- **The step-6 gate** (answer 1; architecture §13 names none; deployment
  §16). All of: the ACL suite; in `airgapped`, no `web.*` tool in the
  trace and zero packets under capture while the positive control passes;
  in `ask`, one prompt per domain per session, the approval logged;
  `online` inside the allowlist; an audit record for every gateway
  request; the browser upload-and-cite test; the retrieval eval. CI
  measures `docs.search` p95 on a synthetic corpus of 100k chunks, failing
  at double its baseline, and Core's idle private memory with that index
  open, failing above 50 MB.
- **The retrieval eval marks someone else's homework** (answer 2). The
  corpus is the four spec documents; 30 questions, each paired with the
  section that answers it, phrased as a user would ask and never reusing a
  section's heading words; 5 more the corpus does not answer, which pass
  only when nothing is cited confidently. It passes when the right section
  is in the top 5 for at least 27 of the 30 with bge-m3; it runs on the
  laptop now and again on the W32 trip.
- **tantivy and usearch at their newest release, pinned exactly**
  (answer 3; architecture §3), recorded here when added. If usearch's C++
  build turns painful on the runner, that is reported, not worked around:
  a pure-Rust HNSW is the fallback and the user's call.
- **Spec change, authorised: the binary pre-filter is deferred** (answer
  4; plugin spec §16.1). The int8 HNSW is searched directly and the top
  200 reranked in f32. At 100k chunks it answers in well under a
  millisecond, and the binary stage buys nothing while costing recall and
  a rescoring path. It is added when a measurement demands it: a p95 over
  budget, or a corpus past a million chunks. §16.1 says so now.
- **Ranking** (answer 5): the top 50 by BM25 and the top 50 by vector,
  merged by reciprocal rank with k = 60, the top 8 to the model. All four
  numbers are configuration, not constants, and the source of each of the
  8 is logged so the eval can attribute its failures.
- **Chunks** (answer 6): split at headings, then paragraphs, about 512
  tokens with 64 of overlap; the locator is the heading path and the page.
  Tokens are counted by `llama-server`'s `/tokenize` at index time; four
  characters per token is only the fallback when no engine is up, because
  bge-m3 is multilingual and the heuristic undercounts Russian and Uzbek
  badly enough to truncate chunks at the model's window.
- **The embedding model** (answers 7 and 8; deployment §3.3): bge-m3 as
  `gpustack/bge-m3-GGUF` at Q8_0, with `role = "embedding"` in the
  catalog; its size, checksum and licence are recorded from the real
  download. It takes a VRAM reservation in the plan when it fits beside
  the chat model and runs on the CPU otherwise, shown in the plan bar
  either way; on W32 the CPU is the default, so the expert cache stays
  whole.
- **`find_capability` searches embeddings when bge-m3 is loaded and BM25
  when it is not** (answer 9; plugin spec §9).
- **`docs.search` is a Core tool, always present** (answer 10; plugin spec
  §8.1, whose "retrieval falls back to the local corpus" assumes the agent
  can search it). Retrieved text is not pasted into every prompt, which
  would break the stable prefix of §16.1.
- **Citations are `[n]` markers the model writes, validated by Core**
  (answer 11; deployment §6.4): numbered per task in the order Core handed
  the sources over; Core checks every marker the model emits against that
  set and strips or flags unknown ones, because models invent citation
  numbers and an invented one is worse than none. The markers become
  links, and a list of sources sits under the answer; web sources show
  when they were fetched.
- **Upload** (answers 12 to 21; deployment §3.3, §6.1, §9.1): the file is
  the request body, streamed to disk, no multipart; an Upload button on
  the Data page and the chat's Attach button, which uploads the file and
  names it in the message; `owner` alone may delete, and the uploader is
  the owner; one commit per upload referencing the file by hash, so
  History shows it and undo removes it; text, Markdown, CSV, JSON, HTML,
  PDF, DOCX, PPTX and XLSX are extracted, everything else stored and
  listed as "not indexed", and there is no OCR; `pdf-extract` 0.12 for
  PDF, `zip` and `quick-xml` for the Office formats; extraction runs in a
  short-lived child process of the same binary with a deadline, because
  PDF parsers are where untrusted input meets fragile code; a ClamAV
  scanner that is configured and unreachable refuses uploads, and the UI
  says why; blobs are streamed, not memory-mapped, until harness surfaces
  open blob documents in step 8.
- **Settings live in `localspace.toml`** (answer 22; deployment §3.3),
  holding only the keys step 6 uses under the spec's names: `[server]
  max_upload_mb`, the `[network]` keys, `[models] embedding`. **Spec
  addition, authorised:** `[server] clamav`, the scanner's socket, is
  added to §3.3.
- **The ceiling in personal mode is `online`, the default mode `ask`**
  (answer 23; plugin spec §8.1): there is no admin above the user.
- **No search backend by default** (answer 24; plugin spec §8.2): SearXNG's
  JSON API at a URL the user sets; nothing is contacted until they set
  one.
- **Zero connections are proved under packet capture, with a mandatory
  positive control** (answer 25; deployment §16): a Linux CI job runs the
  stack in a network namespace whose only way out `tcpdump` watches; the
  airgapped script must produce zero packets, and the same script in `ask`
  mode with approval must produce exactly the expected connections to a
  local fake server, so a capture that sees nothing because it is broken
  can never pass as a clean run.
- **Gateway quotas, deliberate now** (answer 26; plugin spec §8.2): 100
  requests and 64 MiB per session, a 20 s timeout, and 5 MiB per page.
- **The `localspace` command is the server crate's binary** (answer 27;
  deployment §3.1): `serve`, `doctor`, `bench`, `evals` and `call`, no
  GUI; `localspace-serve` goes.
- **The Evals page** (answer 28) sits under Library → the harness → Evals
  and runs against the loaded model; each run is kept in `db/` with the
  model, the date, the cases passed and the malformed-call rate.
- **One types package, `io.localspace.types`** (answer 29; plugin spec
  §18.3), in `registry/types/` with `kind = "types"`; its `types.toml`
  gives each type its MIME type, extension and required fields: `image.v1`
  (PNG) and `svg.v1`, each with `document` and `commit`; `outline.v1`,
  which the whiteboard already produces.

## 2026-09-11, answers to the step-5 closing report

- **The CI frame-time baseline comes from the first run, on a pinned
  runner** (answer 3). The workflow names its runners by a pinned label,
  `ubuntu-24.04`, never a moving alias. The benchmark keeps the runner's
  label in the baseline it records, and a run on another runner is not
  compared against it: the check fails and asks for a baseline recorded on
  that runner. CI fails only at about double the baseline
  (`--tolerance 1.0`), because a shared runner is noisy and the performance
  gate is the W32 number. The baseline the first run records, uploaded as
  an artifact, is committed as it comes.
- **CI tries Linux first, and Core and the server never leave it**
  (answer 4). Core and the server build and test on Linux whatever else
  happens, because that is what organisations deploy. If the Tauri shell
  cannot build on the Linux runner, the rust job splits into Core and the
  server on Linux and the desktop shell on Windows; the server crates are
  never excluded from Linux.
- **Automerge's WebAssembly has a fixed size limit** (answer 7). The other
  limits stay at their size on 2026-09-10 plus about a quarter. The
  WebAssembly file of Automerge 3.4.1, a vendor artifact, is held at exactly
  its gzipped size, 1,128,416 bytes, and named with its version in
  `web/size-limits.json`, so any change to it fails the build until the
  limit is updated with the version, on purpose. The script around it keeps
  a quarter's headroom.
- **The egui whiteboard surface is retired** (answer 5). The crate, its view
  in the whiteboard's manifest, its build, and the tests that used it,
  including the egui client's conformance test against it, are gone. The
  package is 1.1.0, its tools, document and web view unchanged: a package
  whose content changes gets a new version. The surface had broken
  unnoticed because nothing built it. **The egui client waits.** What it can
  do that the web client cannot: run a harness's evals and show the pass
  rate per model; `localspace doctor`, which the plugin spec (§1.1) and the
  deployment checklist (§16) name; `localspace bench`, which the checklist
  names; and `localspace call`, a tool run from the command line, which the
  web client's Tools page does in the browser. The first three become
  step-6 tasks, and `archive/egui-client` waits until they are done.
- **Step 5 is complete, with three measurements open** (answer 1;
  `CLAUDE.md`, "Definition of done for the MVP"; architecture §13 step 5).
  The open items are the canvas at 60 fps with 5,000 shapes on the W32
  machine, the whiteboard's evals, and the workflow's first run. The eval
  gate moves: the definition of done's "evals pass" is measured against the
  reference model on the W32 machine, on the same trip as the frame-time
  number, and not against the 0.5B model, whose 3 of 6 describes the model
  rather than the harness. If the evals fail on the reference model, step 5
  reopens for the tool descriptions, the front doors and the context
  provider.
- **Snapping targets stay the shapes on screen, captured when a gesture
  begins** (answer 2): a guide pointing at something the user cannot see is
  worse than no guide.
- **`svg.v1` for SVG exports, with their provenance** (answer 6; plugin spec
  §18.3). PNG exports are `image.v1` and SVG exports `svg.v1`, each
  registered with its MIME type and extension, and both carry the id of the
  document and the commit they were rendered from, so an exported picture
  traces to the board state that produced it. The interchange-types package
  the answer names does not exist yet: no `kind = "types"` package has been
  built, and interchange types are names in manifests. How to create it is
  a question in the step-6 plan.

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
- **A sync state per replica** (answer 7; architecture §6.1). Core keeps an
  Automerge sync state per document per replica, not one per document. The
  shell names each frame's replica (`peer`: random, one per frame and per
  reload) and sends the name with every `DocSync`; Core answers each
  replica under its own state with `DocPatch { doc, peer, message }`, and
  the shell hands a patch only to the frame of that name. A frame that
  closes or reloads sends `DocSyncEnd`. Past 32 replicas of one document,
  the one heard from longest ago is dropped: a live replica answers every
  change it is sent, so that is one that has gone. Readers of the JSON
  projection (a surface that takes JSON, the egui client) follow the new
  `DocChanged { doc }`, which Core emits on every change; `DocPatch` is for
  replicas only. Tests: `crates/localspace-core/tests/sync.rs` (two windows
  in step, a closed frame sent nothing more, readers of the JSON told of
  every change); the gate walk edits one board from two browser windows
  (`web/e2e/whiteboard.mjs`, step 9). Identities at step 7 add attribution
  on top.
- **Automerge's slim build in frames** (answer 8; architecture §6.1, §6.3).
  The library the shell serves to harness frames is
  `@automerge/automerge/slim`, which loads its WebAssembly from
  `_localspace/automerge_wasm_bg.wasm` beside it on the harness origin and
  compiles it as it streams in; the full build carried the same
  WebAssembly inside the script as base64. The frame's policy is
  unchanged: `'wasm-unsafe-eval'`, `connect-src 'self' data:`, and no host
  named but the shell's in `frame-ancestors`, which a unit test now holds.
  The server test asserts the file comes as `application/wasm` under the
  policy, and the libraries build fails without it. Gzipped, the script
  went from 1,635 KB to 16.6 KB beside a 1,123 KB WebAssembly file: a
  frame's first load of Automerge from 1,635 KB to 1,140 KB.
- **No IndexedDB storage for frames in the shell** (answer 2; §6.1). A
  frame receives Core's snapshot at every connect, so a copy kept in the
  browser adds nothing but a way for a stale one to push old content back.
  When a local replica returns, for detached or offline browser surfaces at
  step 7 or later, it is only ever merged after Core's snapshot has been
  applied, never pushed to Core first, so a stale local copy cannot bring
  old content back. This replaces the storage-adapter half of the
  2026-09-10 answer on Automerge below.
- **The user's selection stays in the frame** (answer 3; §6.3). It is local
  and never written. The document's `selection` stays the agent's:
  `canvas.select` sets it without a commit and the frame follows it. An
  ephemeral presence channel on the bridge, with identities, comes at step 7.
- **Export leaves as artifacts through Core** (answer 4; plugin spec
  §18.3): PNG as `image.v1`, SVG as an artifact type of its own, so an
  export lands in the workspace and the task ledger, and downloads happen
  from the shell. Copying to the clipboard from the frame is allowed under
  the manifest's `clipboard = "on-user-action"`. The frame's sandbox gets
  no `allow-downloads`. Export itself is still to be built.
- **Package layouts from a script** (answer 1; architecture §7, plugin
  spec §12). The whiteboard's web surface stays in `web/surfaces/whiteboard`
  for now. `node scripts/hpack.mjs` builds each harness's files from
  wherever their sources live and lays the package out as its `.hpack`
  will hold it, in `dist/hpack/<id>-<version>/`: `harness.toml`,
  `logic.wasm`, `ui/`, `evals.json`, the tools file, and an icon when there
  is one. Where the sources live is recorded in `scripts/harnesses.json`, a
  repository detail the layout does not depend on. The script fails when a
  file the manifest names is missing from the layout; `--in-place` also
  puts the built files in the manifest's own directory, which
  `--harnesses`, `--registry` and the tests load. Zipping and signing a
  layout into an `.hpack`, and where surface sources live, are the
  packaging step's (§13 step 10).
- **CI** (answer 6; `CLAUDE.md`, "How you work"). `.github/workflows/ci.yml`
  runs on every push and pull request. **web**: lint, both typechecks, the
  node tests, the shell's build and the whiteboard's, the bundle sizes
  against `web/size-limits.json`, the canvas frame times against the CI
  baseline. **rust**: rustfmt over the workspace and the harness crates;
  clippy over every target with `unwrap_used` forced to a warning and
  everything else an error; the workspace's tests, with the harness
  packages built first by `scripts/hpack.mjs --in-place` so that no test
  skips for want of them. **e2e**: the server built, the whiteboard
  installed from the catalog into a fresh data directory, and the gate
  walked in the runner's Chrome without an agent. A manual run adds
  **w32-gate**, which checks the measurement recorded in
  `docs/gates/w32-canvas.json` and puts it on the run's page. The runners
  are ubuntu-latest, with the Tauri shell's system libraries from apt; the
  only actions are GitHub's own. Size limits: the shell's is the
  architecture's 2 MB; each package's is its size on 2026-09-10 plus about
  a quarter. The frame-time check compares against
  `web/packages/canvas/bench/baseline.ci.json`: until that file is
  committed, a run records one as an artifact and warns. The repository
  has no remote yet, so the workflow has not run.
- **rustfmt** (answer 9): the default configuration, applied in one commit
  with nothing else in it, over the workspace and the three harness crates;
  CI checks it.

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
