# Decisions

Every answered question and every decision made during the build, newest
first, with the date and the section of the specification it affects. Part of
the source of truth once written (`CLAUDE.md`, "Source of truth").

Since 2026-09-25 the business decisions (plans and phases, estimates and
dates, pilots and customers, the market, the tests with people and their
results) are kept in a private log, `LinusIp/localSpace-internal`; this one
keeps the technical decisions. What it held of them moved there that day, and
a day that held both says so in one line at its end. `docs/AFTER-TEST-A.md`,
`docs/TEST-A-TESTER-SHEET.md`, `docs/test-a/RUNBOOK.md`,
`docs/test-a/SHOT-LIST.md` and `docs/PILOT-1.md`, named in older entries,
moved with them.

## 2026-10-26, the security mailbox is the main reporting channel

The founder's ruling of 26 October.
- **`localspace.security@gmail.com`** is the main way to report a security problem, in `SECURITY.md` and in the website's `security.txt`.
- **GitHub's private vulnerability reporting** stays as an extra while this repository is public. It works only on public repos, so before this repository goes private it moves to the public plugin-interface repo, and both files are updated.

## 2026-10-26, how security problems are reported and handled

The founder's ruling of 26 October: a written vulnerability-handling process now, ahead of the rest of the supply-chain work (the egress and supply-chain tasks of the workplan).
- **`SECURITY.md`** says how to report a problem privately (GitHub's private vulnerability reporting), what happens next and when, and what is covered. The engine's own problems go to llama.cpp.
- **The time aims:** confirm receipt within 5 working days; a severity within 10 more; critical fixes as soon as we can; for high-severity issues, "we aim to fix high-severity issues within 30 days and will tell you if it takes longer" (the founder's wording); the rest in the next release; an advisory when the fix is out, within 90 days of the report.
- **Only the newest release and preview** get fixes.

## 2026-10-26, text files in chat (0.2.0)

Internal plan 38, F1 and a smaller F4. It is part of the frozen 0.2.0 scope on `release/0.2`, so it isn't behind a switch (the switch rule covers later work on `main`). A fresh-eyes review of the first version found 27 problems, all fixed here. Two more checks of the fixes found 11 and then 12 smaller ones, also fixed.

- **What is taken:** any file that isn't a picture goes to Core, whatever its name (a `README` or a `Dockerfile` too). Core reads it as text or refuses it. The file chooser offers pictures, text and code files as a hint only.
  - **Read as they are:** UTF-8 (with or without a byte-order mark) and UTF-16 (with a mark, or found by its zero bytes).
  - **Nearly UTF-8 counts as UTF-8:** a character cut at the end, or a stray byte, when the file has more good characters beyond ASCII than invalid sequences. Without this, the whole file would be read as Windows-1251.
  - **Anything else is read as Windows-1251,** the usual encoding of Russian and Uzbek Cyrillic files on Windows.
  - **Refused as not text:** a file with a zero byte anywhere in it, or with more than 1% control characters in its first 65,536. Escape doesn't count, so a coloured log is text.
  - **Also refused:** an empty file, and one over 2 MiB. A large file is told "too large", with its size to one decimal ("2.1 MB"), only when its start reads as text. Anything else, like a PDF or a zip, is told it isn't text. The composer says both at once, without sending the file. It judges by the name first, then the type: "too large" unless they say the file is something else, such as a PDF, an archive, an Office file, a model, a database, audio or video. A name the chooser offers wins, since Windows gives some text files other types (`.ts` as video, `.csv` as Excel). Core judges by the bytes.
  - **A folder dropped or pasted** is never sent.
- **No new library.** The Windows-1251 table is ours (`text_file.rs`), since `encoding_rs` would be a new dependency.
- **Kept as UTF-8,** as `<id>.txt` beside the pictures, and deleted with its chat.
  - **The id is the hash of the file's name and bytes,** so two files that read the same stay two, each with its own name. The same file attached twice goes once.
  - **Taking a message's files is all or nothing:** if one can't move into the chat, those already moved go back. A file the chat already has from an earlier message is never moved, so nothing it uses is ever taken back.
  - **Served back** as `text/plain; charset=utf-8`, with `nosniff`, as a download, never shown as a page.
- **How the prompt counts:** `prompt::tokens_of` counts by the pieces the models' tokenizers cut text into (Qwen, Gemma).
  - **A token each:** a digit, the space before a digit, the Uzbek ʻ and ʼ, and a line break.
  - **A word** is at least a token, one for every four bytes of it.
  - **A run of punctuation** is one token, or more when it is long.
  - **Never less** than the old estimate of a token for every four bytes.
  - **Spaces before a figure:** the last space is a token, and the others one more for every twenty (the models' vocabularies hold runs of spaces as single tokens). A no-break space counts its two bytes, as Gemma reads it.
  - **Digits beyond ASCII** (Arabic-Indic, full-width) count a token for each of their bytes.
  - **Why:** the old count read a table of figures as a third of its real size or less.
  - **Measured against the real tokenizers** of Qwen2.5, Qwen3.5 and Gemma 4 (their vocabularies, through the engine's own tokenizer):
    - tables of figures count at 1.0 to 1.25 times their real size;
    - English prose about 1.4 times;
    - Russian 1.5 to 1.9 times;
    - code and Markdown 1.2 to 1.5 times;
    - **Uzbek Latin prose counts short, at 0.76 to 0.87 of its real size, and so does Uzbek Cyrillic on Qwen2.5 (0.89).**
  - **What that means:** a long Uzbek chat on a laptop can go over the engine's 8,192-token context before the divider moves. This was so before text files too: the old count was shorter still. It is a question of the context budget (plan 41), and the founder decides it.
  - **The same count everywhere:** the conversation's costs, its trimming, and the cut of a file.
- **How much the model reads:**
  - **The budget:** the text files of one message share half the working set, less the person's own words and each file's header.
  - **Each file takes what it needs, or an equal share of what is left,** so a long file never leaves a short one nothing.
  - **A longer file is cut by the same count,** at a line end when there is one in the last tenth of what fits.
  - **The cut is decided once,** when the message is sent, and kept with the file, so every later prompt reads the same part.
- **The divider matches what the model reads:** the chat's `read_from` is worked out on the chat as the model reads it, files and all (`agent::with_text_files`). Only the part of each file the model reads is read from the disk for this. Settings → "This turn" shows the files the same way.
- **The person's request is always read:** when a turn's own tool calls fill the working set, the request is kept with the newest calls that fit beside it, so the model never works on without it. If not even the newest call fits beside it, the working set holds, and the newest calls are read as before.
- **Never cut silently:**
  - **The model is told** when only the start fits, or when there was no room for a file at all.
  - **The person sees how much was read:** "the model read the first 23% (about 5,000 of 22,000 words)". Below 1% the figure has one decimal, below 0.1% it says "<0.1", and it is never "0%". The word counts are left out when a file has fewer than 100 words, or when the share of words read and the share of characters read differ by more than twice. A long line of data, or a picture inside a Markdown file, can make them differ.
  - **Russian agrees with the number:** "из 22 001 слова".
  - **A file read none of** says so in its own sentence.
- **Four attachments a message,** pictures and files together. Only pictures are refused for a model that can't read them.
- **The desktop window takes dragged files:** Tauri's own drag-drop handler is off (`disable_drag_drop_handler`), since on Windows it keeps drops from the page. A file dropped outside the message box doesn't open in place of the app. A pasted file is taken like a dropped one.
- **Later:**
  - **counting with the engine's `/tokenize`** instead of the estimate. This matters most for Uzbek, which is counted short (above);
  - PDF and DOCX (preview.3).
- **Walked in CI:** `web/e2e/text-files.mjs`, with the test engine that can't read pictures.
  - Core refuses a `.txt` that isn't text.
  - A Windows-1251 file is taken; added again, it goes once. A file with no extension is taken too.
  - Both reach the engine as words, and the first saves back as the same words in UTF-8.
  - A 1.5 MB file says how much was read.
  - Deleting the chat removes the files.

## 2026-10-26, the language chosen in Settings is remembered (0.2.0)

The founder's ruling of 25 October: fix it on `release/0.2`, with a test, before the 4 November cut-off.

- **The bug:** the 18 October entry says the choice is "kept as the `language` preference". Core accepted only `rail_collapsed` (`PREFERENCE_KEYS`), so choosing a language in Settings showed "`language` is not something the shell remembers", and the choice was gone after a restart. It was in preview.1. The language shown still followed Windows.
- **The fix:** `language` joins `PREFERENCE_KEYS`.
  - **What it accepts:** "system" or a short language tag (letters, digits and hyphens, starting with a letter, at most 35 characters). A language added to the client later needs no change in Core.
  - **What it refuses:** anything else, with "`…` is not a language".
- **Tested** (`tests/preferences.rs`):
  - the choice is kept across a restart;
  - malformed values are refused, and nothing is stored.


## 2026-10-26, no open-source licence: the `sdk/` Apache exception is withdrawn

The founder's ruling of 26 October. It replaces the 25 October entries on the `sdk/` folder.

- **Nothing in this repository is open source.** It's visible on GitHub, all rights reserved. The root `LICENSE` says so again, without the exception.
- **`sdk/` is removed,** with its `LICENSE`, `NOTICE` and README, and so is `scripts/check-sdk-boundary.mjs` with its CI step, which only guarded the Apache folder.


## 2026-10-25, the plugin manifest is `plugin.toml`; plugins are `.lspkg` files

The owner's ruling of 25 October.
- **`plugin.toml`, not `localspace.toml`,** which is already the Server's settings file.
- **Packed plugins are `.lspkg` files,** to match `.lsassistant` (internal plan 48), in place of the spec's `.hpack`. The harness-plugin spec's §12 is amended when the plugin kit lands.
- `sdk/README.md` says so now.

## 2026-10-25, the plugin SDK's folder is Apache 2.0 now; the rest waits for the separation

The owner's ruling of 25 October.

- **Internal plan 42's order stands.** The personal app goes Apache 2.0 after the organisation code is separated, as part of the Server work. Until then the repository stays "All rights reserved", and nothing calls it open source.
- **The exception: `sdk/`,** the plugin SDK of internal plan 47, has its own Apache 2.0 `LICENSE` and `NOTICE` now, so developers can build plugins. Its contents arrive with the plugin kit's PRs.
- **The root `LICENSE`** names the holder, Abdulloh Shavkatjonov, in place of "localSpace", and the `sdk/` exception.
- **Nothing in `sdk/` may come from outside it.** No path leaves the folder, and no dependency on the app's crates (`localspace-proto` included, since it still carries organisation types) or its web packages. `scripts/check-sdk-boundary.mjs` checks this in CI; tried against a tree that breaks both rules, it fails on each.
- **Code signing of the Windows installer is postponed until after funding.** The plugin key of the marketplace (internal plan 48) is separate from it.

## 2026-10-25, 0.2.1: three themes, one set of tokens, the translation system, two tracks

The owner's rulings of 24–25 October. The technical brief is `docs/design/0.2.1-THEMES-AND-TRANSLATION.md`.

- **Three themes,** each in light and dark, plus "Same as Windows":
  - **indigo and amber** (the default);
  - violet and magenta;
  - teal.
- **The tokens** are one `--ls-*` set:
  - `data-palette` picks the colours, and `data-theme` or `prefers-color-scheme` the mode;
  - every theme passes WCAG AA, checked in CI;
  - the choice is stored as a preference;
  - text and buttons use separate accent roles: `--ls-accent` for text, `--ls-accent-fill` with `--ls-on-accent` for buttons.
- **The typeface is Onest,** for its Cyrillic. Its Uzbek oʻ and gʻ (U+02BB) are checked before it ships.
- **The translation system:**
  - every visible string goes through it, with one file per language and a test for missing keys;
  - Core sends codes with parameters, not English sentences;
  - `lang` follows the language shown.
  - No language comes first; we start with English, Russian and Uzbek because we can check them.
- **Work runs in two tracks.** Track A (0.2.0 on `release/0.2`, then Core and the agent on `main`) and track B (translation, design, plugin kit) work in parallel.
  - Every new feature sits behind a switch that's off by default (`themes`, `translation`, …).
  - CI and the browser walk run with the switches on and off.

## 2026-10-24, the picture check logs other signed-in users' memory

The founder's ruling of 2026-10-24, after the preview check:

- **In 0.2.0, only the log line.** `Hardware` gains `ram_other_users_mib`: on Windows, the working set of every program in a session that is neither ours nor the services' (0), so another person signed in to the same computer.
  - The hardware facts in the log say it: "memory 15613 MiB (7806 free; 3457 MiB held by other signed-in users' programs)".
  - The picture check writes one `app.log` line with the free memory it saw, and that figure.
  - Elsewhere it's `None`, and the line says "an unknown amount".
- **Why:** on the owner's laptop, a second signed-in user with Chrome open held about 3.4 GB. Windows counts that as taken, so the 4B's picture file was left out once; **Try again** then worked.
- **The full fix is designed and ready if testers need it.** It counts what Windows can free for another signed-in user's idle programs as available, on a desktop Windows, capped by the page file's free space, with the 1,536 MB margin unchanged. Its tests are written in the plan of 2026-10-21.
- **Tested:** `hardware::tests::the_log_says_what_other_signed_in_users_programs_hold`.

## 2026-10-21, the preview's notes after the owner's check

The owner checked `v0.2.0-preview.1` (build `920b7123`) and approved it. Two known issues were added to its notes, on the release page and in `docs/release-notes/v0.2.0-preview.1.md`:

- **The app is mostly in English for now;** only the newest messages are in Russian and Uzbek. The full translation is planned for 0.2.1.
- **Pictures may ask to close other programs when there is enough memory.** On the owner's laptop, a second signed-in Windows user (with Chrome open) made the 4B's picture check refuse once; **Try again** then worked, and kept working with Chrome reopened. The check (`fit::picture_file_place`, internal plan 38) is to be loosened for 0.2.0, by counting another user's memory sensibly or by a smaller margin. How is a question for the owner before it's built.

## 2026-10-21, public previews: `v0.2.0-preview.N`

The owner's ruling of 2026-10-21: `0.2.0-preview.1` goes out publicly before the release check of 11–12 November.

- **A third kind of tag,** beside a release (`v0.2.0`, after the release check) and a release candidate (`v0.2.0-rc.N`, a test of the path, deleted after the check): **a preview, `v0.2.0-preview.N`.** It's meant for people to try, and kept until 0.2.0 replaces it.
- **The code's version stays 0.2.0.** `scripts/check-version.mjs` accepts the preview's tag for it, as it does an rc's. So the files are named `localSpace-0.2.0-<build>-windows-x64-setup.exe` and `…-portable.zip`, with `<build>` the commit's first eight characters.
- **`package.yml` publishes a preview as a pre-release,** never with the release check's box. Its notes, `docs/release-notes/v0.2.0-preview.N.md`, must have a line beginning `**Preview.**`, or nothing is published.

## 2026-10-21, the repository tidied before it is opened (internal plan 42 §4)

The owner's rulings of 2026-10-21.

- **`.mailmap`:** the commits made as `Khafizullo <locallabs.io@gmail.com>` were written by Abdulloh Shavkatjonov under a friend's account saved on the build machine. They show under the author's own name. **No history is rewritten.**
- **LinusIp is the owner's own GitHub account,** so its commits (pull-request merges and edits made on GitHub) are mapped to the same name. From 2026-10-21 new commits are made as `Abdulloh Shavkatjonov <locallabs.io@gmail.com>` (the clones' local git config).
- **"Meridian Bank" became "Example Bank"** in the organisation walk, the CI and local checks, and the settings test, so no real bank's name is used as a test organisation. Older entries of this log keep the old name, as they were written.
- **`docs/test-a/MESSAGE-SCRIPT.md` moved to the internal repository.** It's the laptop test's record of what each model answered to the fixed messages, with a person's reading of it, and is internal material. The test that compared it with the catalog now checks the catalog alone: every model said to have been through the script names the day, as a date.
- **The empty `docs/measure/2026-10-10/FOUNDER-READ-4B-RU-UZ.md` is deleted;** the real one is in the internal repository.

## 2026-10-21, a full chat is trimmed in big steps (plan 41 §2)

The owner's yes of 2026-10-21, with the plan's recommendations. The context stays at 8,192 tokens in 0.2, and the compacter is for 0.2.1.

**Before:** Core gave the model "the newest turns that fit" (`prompt::turns`). Once a chat was full, nearly every message pushed the oldest turn out and changed the prompt's start, so the engine read the whole chat again on every message: about 12 s before the first word on the founder's laptop (the 24-message runs of 2026-10-20).

- **`prompt::read_from`:** while what is read fits the profile's working set, the start stays where it is and the engine reuses what it read. When it no longer fits, the start jumps **in one step** to the first message of the person's after which **at most half** the working set is left. The engine reads that once and is fast again for the next several messages.
- **It begins with the person's message** (some templates refuse one that begins with an answer). **The newest message of the person's is always read**, however long.
- **Nothing is stored.** The rule is replayed over the chat as it grew, so the same messages always give the same start. Every caller of `prompt::build` reads from it: the agent, the stable part and the context preview. Old chats and the old app are unaffected.
- **A picture counts as 400 tokens** (`PICTURE_TOKENS`; the engine counted 383 for a question with one picture, its system part included).
- **Counting** stays bytes / 4. Cyrillic is two bytes a letter, so a Russian chat trims early, never late. The engine's own count comes with text files (plan 38 F4).
- **`Response::Transcript` carries `read_from`.** The client shows a divider before that message: "Earlier messages are no longer read by the model. They stay here for you." It has Russian and Uzbek versions, which the founder is to check. Every message stays in the chat and in the record.
- **Tested:** `prompt.rs`:
  - a chat that fits is read from its start;
  - a full one moves in one step to at most half and then stays for ten more pairs;
  - the newest is always read;
  - pictures count;
  - `build` reads from the step.

## 2026-10-21, Help → Send feedback; links open in the person's browser

The owner's ruling of 2026-10-21.

- **Help has a "Send feedback" row.** Its link opens localSpace's Telegram group, `https://t.me/localSpace_discussion`, **in the person's browser.** Nothing is sent from the app: no report, no log, no address. The preview's release notes carry the same link.
- **The desktop window hands any link the person clicks that would open a new window to their own browser.** That covers this one and links in answers. It does so **only for `https`**; anything else is refused (`on_new_window` in `localspace-shell`). The app itself fetches nothing, and the "no outbound connection" rule holds, since it's the browser that connects, at the person's click. Before this, such a click opened a bare WebView2 window inside the app.

## 2026-10-21, the third-party notices travel with the package

An audit on 2026-10-21 found the Windows package shipping only llama.cpp's licence and libomp's. The MIT, BSD, ISC, Unicode, OFL, MPL and CDLA notices of what it links and bundles were missing. They are owed whatever localSpace's own licence is. The owner asked for them in the public preview.

- **`scripts/third-party-notices.mjs`**, written in-house (no new tool), writes `licences/THIRD-PARTY-NOTICES.txt` into the package at packaging time. It covers:
  - every Rust crate linked into `localspace-app.exe` and `localspace.exe`: `cargo metadata` for Windows, normal edges only, never through a proc-macro. That's 417 crates;
  - the JavaScript the web client ships, the production closure of `web/` and its packages: 74 packages;
  - **the KaTeX fonts, under SIL OFL 1.1** (not the package's MIT), with their copyright lines and Reserved Font Names;
  - components listed by hand in `packaging/licences/notices.json`, ten of them:
    - the engine (llama.cpp b10869);
    - the libraries built into its DLLs, fetched from llama.cpp's sources at its commit `30b6a75` with the owner's yes of 2026-10-21: cpp-httplib, nlohmann/json, stb_image (MIT chosen), miniaudio (public domain or MIT-0), sheredom's `subprocess.h` (Unlicense), BoringSSL 0.20260903.0 (Apache-2.0);
    - **the engine's built-in web UI with the 360 libraries its lockfile bundles,** each with its own licence file, fetched from unpkg at the locked version. Four MIT packages ship no file and get the MIT text with their authors; DOMPurify's Apache-2.0 option is taken. The Node-only `@napi-rs/canvas` binaries are left out, since they never reach a web page. localSpace doesn't use this UI, but it is inside `llama-server-impl.dll`;
    - **Microsoft's WebView2 loader**, linked into `localspace-app.exe`: the BSD-style licence and Microsoft's own notices from the SDK it comes from (`Microsoft.Web.WebView2` 1.0.3650.58, the NuGet package).
- **Each licence text is written once,** with every component it covers: 299 distinct texts.
- **A crate that ships no licence file** gets the standard text of the licence it declares, with its authors as the holders. Of an "OR", the permissive choice is taken: MIT, then Apache-2.0, then BSD-3-Clause, then MPL-2.0. That covers 21 crates, among them `ittapi` (BSD-3-Clause, not GPL-2.0), `automerge`, `ts-rs` and parts of wasmtime. The standard texts are kept in `packaging/licences/texts/`, copied from crates that carry them.
- **MPL-2.0 crates** (through Tauri) are listed with where their source is, as that licence asks of a binary.
- **CI** (`check`) runs `--check`: a shipped dependency with no licence text fails the build. The package step lists the notices file among what it must contain.
- **When the engine is upgraded,** its vendored libraries and its web UI's list must be fetched again for the new commit. The engine's entry in `scripts/engine.json` and the `engine/` texts change together.

## 2026-10-19, at most 4 context checkpoints for one person

The owner's answer (a) of 2026-10-19. After `-np 1 --cache-ram 256` the engine still grew by about 150 MB a message. The likeliest cause is its context checkpoints: 32 per slot by default, the saved states a model like Qwen3.5 resumes a prompt from.

- **For one person** (`engine::slot_flags`): `--ctx-checkpoints 4`. An organisation's server keeps the engine's default.
- **Measured before merging,** 2026-10-20, on the founder's laptop under the laptop rules (Silent, Chrome closed), through `localspace serve --personal`. It was a 24-message chat in three languages, a second chat, then back to the first and back to the second. The readings are in `D:\localSpace\check\np-run\out-cp`.
  - **Before** (`f4b5beb`): the engine's working set grew about 100 MB a message, to 3,366 MB at message 16, then stayed flat.
  - **After:** it **levels off at message 2, at 2,010 MB**, and stays there to message 24. That's 1.35 GB less at most.
  - **Speed:** the first word came after 0.3–1.2 s and the answers ran at 7–21 words a second, the same as before.
  - **At the context limit** (8,192 tokens, reached at message 22): one wait of 11.7 s while the start of the chat is trimmed, then 0.7 s again. Before, every message from then on waited about 12.4 s.
  - **Going back to the older chat:** 12.3 s for the first word (12.6 s before). With one slot the engine reads the long chat again; its saved state is over 256 MiB, too large for the cache. Going back to a short chat: 0.7 s.
- **Tested:** `engine.rs`, the flags; `tests/engine.rs`, the engine starts with them.

## 2026-10-19, the engine's slots and caches, and the database's

The owner's answers of 2026-10-19, after the test build freed about 0.3 GB on a restart with the same programs open. The engine had been started with its own defaults (its logs said `n_slots = 4`), with a prompt cache in system memory of up to 8 GiB, into which idle slots are saved.

- **For one person** (the desktop app, `serve --personal`, the kit; `engine::slot_flags`): `-np 1 --cache-ram 256`. That's one slot, since one answer is written at a time, and at most 256 MiB of earlier prompts kept in system memory. Checkpoints stay at the engine's default until measured.
- **An organisation's server keeps the engine's defaults** for now, for people asking at once. **Open:** the planner should size its slots and cache from the machine.
- **The database keeps at most 64 MiB of its file in memory** (`store::CACHE_BYTES`); redb's default is 1 GiB. A person's file is under a megabyte.
- **Measured before merging,** 2026-10-19, on the founder's laptop under the laptop rules (Silent; Chrome, Telegram and the Claude app open), through a real `localspace serve --personal`. The script and the readings are in `D:\localSpace\check\np-run\out`; the internal log has them.
  - **Before** (build `bd25567`): `n_slots = 4, n_ctx_slot = 8192`. The engine grew by about 100 MB with every message and never gave it back: working set 1,683 → 3,137 MB over 12 messages. The 3 GB rule stopped the run in the second chat.
  - **After** (`9bc5aea`): `n_slots = 1, n_ctx_slot = 8192`, so **each chat still gets 8,192 tokens**.
    - About 0.5 GB less right after the load (4,326 MB private, against 5,484 MB less the 641 MB picture file the "after" load left out).
    - The engine now skips a prompt state over 256 MiB rather than keeping it.
    - The same speed: 14–20 words a second, and the first word after 0.3–1.2 s.
  - **Still growing**, about 150 MB a message. It's most likely the engine's context checkpoints, 32 a slot by default, which aren't limited yet. The next step is to measure `--ctx-checkpoints`.
  - **Not measured:** the wait for the first word on going back to an older chat. Both runs were stopped by the 3 GB rule before the switches, because too little memory was free on that laptop with those programs open.
- **Tested:** `engine.rs`, the flags for each topology; `tests/engine.rs`, the engine on the ordinary laptop starts with them.

## 2026-10-19, what the picture-file check leaves free: the chat counted once, 1.5 GB while pictures are read

The owner's answer (b) of 2026-10-19. The test build had left the 4B's picture file out on the founder's laptop with Chrome and the Claude app open, 0.5 GB short. The 2.4 GB chat allowance isn't counted twice in that rule: it was measured on top of the fit's part, picture file included. But the measurements say no rule that keeps 3 GB free keeps the file on that laptop with a browser open.

- **The rule** (`fit::picture_file_place`): the file stays in system memory when what is free at the load holds:
  - the fit's system-memory part, picture file included;
  - a chat's 2.4 GB, once;
  - what reading pictures adds, 600 MiB (`PICTURE_READING_MIB`; the largest rise measured while the 4B read three pictures on 2026-10-18, 554 MiB, rounded up);
  - **1.5 GB still free** (`PICTURES_STILL_FREE_MIB`).
- **On the founder's laptop** with Chrome open (about 5.8 GB free at the load) the 4B now keeps its file. While it reads pictures, about 1.7 GB stays free.
- **The card's rule and the message are unchanged;** `needs_mib` is the shortfall against this sum.
- **Tested:** `models.rs`, `the_4b_keeps_its_picture_file_on_a_16_gb_laptop_with_a_browser_open` (16 GB, a 4 GB card, 5,825 MiB free); `fit.rs`, left out below the line with the exact `needs_mib`.

## 2026-10-19, the model suggested for pictures: never larger, and one that keeps its picture file here

The owner, after the test build: the app had offered Gemma 4 E4B, which is larger than the 4B, to download.

- **Core chooses it now, at the load** (`Catalog::picture_model`), from the computer as found then, and the environment carries it (`picture_model`). The client no longer chooses one.
- **The rule:**
  - a model shown in the app, with a picture file;
  - **never larger than the one in use** (by parameters);
  - one that would keep its picture file here by the same rule as the one in use (`fit::picture_file_place`).
- **Which one:** one already on this computer comes first. Failing that, one the disk has room for, offered as a download. Of those, the largest.
- **None qualifies:** the app shows only "Close other programs" and "Try again". The 4B has no smaller model with a picture file in the app (Gemma 4 E2B and E4B have more parameters), so it never gets a suggestion.
- **Tested:** `models.rs`, `the_model_suggested_for_pictures_is_never_larger_and_keeps_its_picture_file_here`; `tests/engine.rs`, no suggestion where no other model reads pictures.

## 2026-10-18, the version is 0.2.0; a tag must name it

- **The version is 0.2.0,** raised from 0.1.0 in the two places it is set:
  - the workspace's `Cargo.toml`, which every crate inherits, and with it what
    the app logs, what its API reports and the About screen;
  - the shell's `tauri.conf.json`, which the installer's version
    information and the package's file names come from.

  The test release `v0.2.0-rc.1` showed the gap: its files were named
  `localSpace-0.1.0-…`. Not raised: the harnesses' own versions, the internal
  web packages', and the manifest test fixtures, none of which is the app's
  version.
- **`scripts/check-version.mjs`** says the two files agree and, given a tag,
  that the tag names their version: `v0.2.0` for 0.2.0, or `v0.2.0-rc.N` for
  one of its release candidates. It runs:
  - in `ci`'s `check` job, on every push;
  - in `package` before a tag's package is built;
  - again before a tag is published.

  Tried by hand: the tag `v0.2.0` against the code's 0.1.0, then `v0.1.0`
  and `v0.2.0-beta` against 0.2.0, each refused; `v0.2.0` and `v0.2.0-rc.2`
  against 0.2.0, each accepted.

## 2026-10-18, where the picture file goes, by the fit (plan 38, P5)

The owner's answers of 2026-10-18, as built (`fit::picture_file_place`; `Core::picture_file_place`):

- **On the card** only when every layer of the model is on the card and the file plus 256 MiB of working room (`PICTURE_FILE_WORK_MIB`) fits there too, within the card's usable memory. A picture never costs a layer.
- **In system memory** otherwise, when the memory free at this load holds the model's system-memory part (the file with it), a chat's 2.4 GB (`IN_A_CHAT_MIB`) and 3 GB still free (`STILL_FREE_MIB`). Free memory is looked at just before the load, with our own engine stopped. Total memory isn't used here, so closing programs really helps.
- **Left out** when neither holds. The engine starts without `--mmproj`, so the model reads no pictures. Core keeps how much more free memory would have kept the file, `needs_mib`: what the three above add up to, less what is free.
- **The reference tiers** (W32 and up, planned by the planner, with a card of 32 GB and more) always put it on the card. Below them, a model whose numbers aren't known keeps it in system memory, as before.
- **The kit's word comes first:** `set_vision_place` still puts it on the card or the processor for the kit's two passes.
- **The fit counts the file where it goes** (`Fit::with_picture_file`): on the card it moves out of system memory onto the card, with its working room; left out, it leaves system memory.
- **Said, never a silent slowdown:**
  - the environment carries `picture_file_left_out_mib`, so the composer says it as soon as a picture is attached;
  - a message sent with pictures anyway is refused with `PictureFileLeftOut { needs_mib }`;
  - the message is the approved "Pictures need about {gb} GB more memory than is free now. Close other programs, or switch to {model} for pictures.";
  - where no other model here reads pictures, "Pictures need about {gb} GB more memory than is free now. Close other programs, then try again." is shown instead (new; its Russian and Uzbek await the founder's check);
  - {gb} is rounded up to one decimal, in the language's own figures.
- **"Try again"** (new string, in all three languages) asks for the model in use again. A model that runs without its picture file is the one exception to "asking again for the running model starts nothing": it is loaded again, with free memory looked at anew.
- **Tested:**
  - `fit.rs`: the card on a 16 GB card, system memory on the 4 GB laptop, never the card for a model partly on it or with no card, left out with the exact `needs_mib`, and the file counted where it goes;
  - `tests/engine.rs`: on a laptop with a full card and 4 GB free, the model starts without the file, the environment and the refusal both say what's needed, and asking again loads it again. On W32 the file goes on the card, and the kit can still put it on the processor;
  - `attachments.test.ts`: the message with and without a model to switch to, in all three languages, and the gigabytes figures.

## 2026-10-18, the kit asks about a person's own pictures (`--pictures`)

- **`localspace measure --pictures <folder>`** (the owner, 2026-10-18, option (a); test tooling ahead of the plan for pictures and files). It loads the model, then asks about each picture in the folder instead of running the script:
  - each question goes in a fresh chat, with thinking off, through Core's own picture path (`send_picture`), so the app's language instruction applies;
  - the questions are in a text file beside each picture, with the picture's full name and `.txt` added (`page.jpg.txt`), one per line;
  - a picture with no questions beside it is asked "What is in this picture?";
  - the results file gets a section, "The pictures asked about", with the time, the time to the first word, the reading and writing figures and the answer, for each question.
- **Only PNG and JPEG are sent.**
  - Core labels every picture `image/png`, and the engine's image reader decodes PNG and JPEG by their bytes. It cannot read WebP.
  - Any other picture is written down as not asked, with why, and the run goes on.
  - What WebP needs in the app is a question for the plan for pictures and files.
- **A model without a vision file is refused** before anything is asked.
- **`--chat <file>`** holds one chat instead of running the script. Each non-empty line is sent in turn, in the same chat, with thinking off. It reads memory and speed over a long conversation, for the `--no-mmap` re-measure the owner asked for (10 or more messages). The results file gets "One chat, message after message".
- **The results file is written after every answer, Continue and picture,** not only at the end. A run stopped from outside, such as a heat or memory stop by the laptop's watch, keeps what it had. Until the last write, the file says the run was still going (the owner, 2026-10-18, after a stopped picture run lost five answers).
- **Tested** in CI's kit job against the test engine (`scripts/measure-fake.sh`): a PNG with two questions beside it (one in Russian), a PNG with none, and a WebP. The results must show all three, the WebP as not asked, and no script.

## 2026-10-18, pictures go only to a model that reads them (plan 38, P4)

- **Core holds to it.** A message with pictures is refused with `CannotReadPictures` when the loaded model's engine was started without a picture file (`--mmproj`). Core never trusts the client on this. A message sent while no model is loaded is kept as before. A model that reads pictures says so in the catalog (`reads_pictures`: it has a picture file).
- **The composer says it at once,** before sending: with pictures attached and a model that can't read them, it shows "{model} can't read pictures." and turns Send off. Beside that it offers a model that can, as a button: "Switch to {model} for pictures" when one is on this computer, or "Download {model} for pictures" when not.
- **The model offered** (`lib/attachments.ts`, `pictureModel`) reads pictures, isn't the one in use, runs well or works here and isn't tight on memory. Of those, one already on this computer comes first, and then the largest. When none qualifies, there's no button.
- **Tested:**
  - Core: `tests/engine.rs`, a model with no picture file refuses a picture (the forbidden case beside P1's allowed one);
  - client: `attachments.test.ts`, the model offered and the new refusal in all three languages.

## 2026-10-18, attaching pictures in the composer (plan 38, P3)

- **Three ways in:**
  - the paperclip button opens the file chooser (PNG, JPEG, WebP, several at once);
  - dragging pictures onto the composer, which says "Drop the picture here";
  - pasting a screenshot (Ctrl+V) into the message box.

  Anything else is refused with the strings table's words, and more than 4 pictures with "Up to 4 pictures can go with one message".
- **Each picture goes to Core as soon as it is added** (`POST /api/v1/attachments`). The composer shows it as a small square from the file on the person's own computer, faded until Core has taken it, with a button to remove it. Send waits until every picture is taken. A refusal from Core is said in the person's language (`lib/attachments.ts`, `refusalText`).
- **When the message goes,** its pictures go with it as ids. If Core refuses them, the words and pictures come back to the composer with the reason.
- **In the sent message** the pictures show above the words, from their own address (`GET /api/v1/attachments/{id}`). A click opens one larger in the app's own dialog, never in another program.
- **Tested:**
  - `attachments.test.ts`: which files are pictures, the most per message, and every refusal in all three languages;
  - a browser walk in CI (`web/e2e/pictures.mjs`) attaches the kit's picture through the file chooser and sends it. It checks the engine got it as a picture with the words, the message shows it and it's served under its type with nosniff, and that deleting the chat leaves it neither served nor on the disk.

## 2026-10-18, a model is offered first only where it leaves room to spare

The owner's rule, from the runs on the founder's laptop (16 GB, 4 GB card, Silent mode, `--load-mode none --no-host`):
- the 4B with its picture file used about 3.2 GB of system memory in a chat;
- the 9B with its picture file about 5.1 GB.

In Windows® mode with Chrome open, the 9B fell to 1.1 GB free while loading.

- **The rule** (`fit::fits_with_room`): a model's memory, plus Windows' reserve, plus 3 GB for a browser and other programs, plus 3 GB still free, must fit within the computer's memory. A model's memory is the part the fit puts in system memory (its picture file included) plus 2.4 GB for what a chat adds; 2.4 GB is the largest measured, the 4B's. Windows' reserve is the larger of 4 GB and a quarter of the memory.
- **`recommend` chooses among the models that leave room,** where any does. The 9B stays the 4 GB tier's decided default, and it is passed over wherever it doesn't leave room. Where no model leaves room (8 GB, or no card), the choice stands as it was.
- **Each catalog entry says `memory_tight`** when the model would load but leaves no room to spare. The app then shows the approved note: "Needs most of this computer's memory: close other programs, such as your browser, before you use it." It's in English until the strings table (plan 38, B1) brings Russian and Uzbek.
- **What it gives on the typical computers** (`what_typical_computers_are_told_and_offered`):
  - **16 GB with a 4 GB card** (the founder's laptop, or a GTX 1650): **the 4B**. The 9B is listed with the note.
  - 16 GB with a 6 GB or 8 GB card: the 9B, since most of it sits on the card. This row is to be checked on the RTX 5080.
  - 24 GB and 32 GB: the 9B.
  - 8 GB, and computers with no card: the 4B, as before, with the note where it is tight.

## 2026-10-18, pictures kept with the chat, and made ready for the model (plan 38, P1 and P2)

The owner approved plan 38's picture work, its interfaces, and these values: 30 MB and 60 megapixels at most, 1,280 pixels on the longer side, only the shrunk copy kept, and at most 4 pictures in one message.

- **Making a picture ready** (`crates/localspace-core/src/picture.rs`, using `image` 0.25 with only its PNG, JPEG and WebP readers; approved 2026-10-18):
  - A file over 30 MB is refused before it is read. A picture over 60 megapixels is refused from its header, before it is decoded.
  - A phone photo is turned the way its camera says.
  - The longer side comes down to 1,280 pixels.
  - A JPEG stays a JPEG (quality 85). Anything else becomes a PNG; that includes WebP, which the engine can't read.
- **Keeping it with the chat** (`crates/localspace-core/src/attachments.rs`):
  - Only the copy made ready is kept, named by its hash: under `attachments/<person>/pending/` until it is sent, then under `attachments/<person>/<chat>/`.
  - **It is deleted with the chat.** What was added but never sent is cleared when Core next starts.
  - A person reaches only their own folder. An id is only ever 32 lower-case hex digits, so a request can't reach outside it.
  - The pictures Core held in memory, lost on restart and never deleted, are gone.
- **The API:**
  - `ChatMessage.attachments` records what was attached and is saved with the chat.
  - `SendMessage` takes the attachments' ids.
  - `AddAttachment` (the bytes) answers `Attached` or `AttachmentRefused`. `GetAttachment` (an id) answers the bytes.
  - A refusal comes in parts (`AttachmentRefusal`: too large, too many pixels, not a picture, unreadable, too many, not found), so the client can say it in the person's language.
- **The routes:**
  - `POST /api/v1/attachments?name=` carries the raw body, up to 31 MB.
  - `GET /api/v1/attachments/{id}` returns the picture in place, under the PNG or JPEG type Core gave it, never sniffed. Someone else's attachment, or none, gets the same 404.
- **The prompt** reads the chat's pictures from storage, and sends each one under its own type (`data:image/jpeg` for a photo). It no longer labels everything `image/png`.
- **Tested:**
  - unit tests for every format, the limits, the turning and the shrinking;
  - unit tests for keeping, sending, deleting, another person's id, an id that isn't one of Core's, clearing at start, and five pictures refused;
  - prompt and engine-request tests with one picture and with two;
  - Core tests: a picture survives a restart, is fetched by its person, and is deleted with the chat; a text file and an unknown id are refused.
- **Built and tested by CI.** The laptop builds in Silent mode only. In Windows® mode a 2-job build reached the 90 °C stop in about 40 seconds.

## 2026-10-18, messages in Uzbek, Russian and English (plan 38, B1)

The owner's ruling: a language setting and a strings table for the new messages, with the rest of the app later. The language follows Windows' display language by default.

- **`web/src/lib/strings.ts`** holds every new message in English, Russian and Uzbek (Latin, with ʻ in oʻ and gʻ). It's written by us, with no library, and the founder checks the Russian and Uzbek. The first strings:
  - the approved memory note;
  - the six reasons a picture isn't taken (too large and too many pixels both suggest a normal-size photo or a screenshot);
  - "can't read pictures", with "Switch to …" and "Download …" for pictures;
  - "pictures need about … GB more memory";
  - the attach controls.
- **The language shown:** the person's choice in Settings → General → Language (Same as Windows / English / Русский / Oʻzbekcha), kept as the `language` preference. Without a choice, it's Windows' display language as the webview reports it (`navigator.languages`): Uzbek or Russian when it's one of those, English otherwise. `useStrings()` gives the messages in that language.
- **The rest of the app stays English in 0.2.0,** and moves into the table later.
- **Tested** (`strings.test.ts`, in `npm test`):
  - Windows' language is followed, and a choice wins over it;
  - every message exists, non-empty, in all three languages;
  - the numbers and the model's name reach the words;
  - the memory note reads exactly as approved in all three.

## 2026-10-18, the engine reads the model file instead of mapping it (`--no-mmap`)

- **Core starts the engine with `--no-mmap`** on every computer below the reference tiers (`engine::fitted_flags`). The owner approved it for 0.2.0, before rc.2.
  - Mapped, the whole model file stays in system memory even when most of it is on the card.
  - Measured on the founder's 16 GB laptop with a 4 GB card, Chrome open, the engine alone with Core's settings:
    - the 4B with its picture file used 3.6 GB mapped and **1.6 GB read** (21.6 against 23.0 tok/s on one short answer, 8.6 against 7.6 s to load);
    - the 9B with its picture file used **4.0 GB read**, with 4.24 GB left free and 9.7 tok/s. Mapped, it couldn't load at all: free memory fell to 1.36 GB.
- **Through Core, `--no-mmap` alone misled the check after loading.** Read, the engine keeps what stays on the processor in pinned memory the card can reach, such as the word table. Windows counts that as shared graphics memory, and `look_after_load` took it for a spill: it stepped the 4B down from 31 layers on the card to 0, and the model ran on the processor, with the processor reading at 89.9 °C. **The flags are therefore `--load-mode none --no-host`.** `--load-mode none` is the pinned engine's current name for not mapping (`--no-mmap` is deprecated there), and `--no-host` turns that pinned buffer off. The engine alone with these: the 4B uses 1.6 GB at 23.6 tok/s, the 9B with its picture file 4.2 GB at 8.7 tok/s.
- **The planner's flags for the reference tiers keep the mapping.** Streaming experts from the disk depends on it.
- **The owner's rulings on the default, from the same measurements:**
  - On 16 GB with a 4 GB card, **the 9B stays the default.** It meets the rule with room to spare, and it gave the fewest wrong answers.
  - "Switch to Qwen3.5 4B for pictures" is offered when a picture is attached and the 9B is slow with it. The threshold for "slow" will be proposed from the 9B's picture times once Core uses `--no-mmap`.
  - On 8 GB, where no model meets the rule, the 4B is offered with the "close other programs" note.
  - The note shows only where the rule says memory is tight.
  - The rule itself, and the note in three languages, come in their own PR.
- **Re-measured under the laptop rules** with Chrome open: a chat of ten or more messages on the 4B and on the 9B, the speed, and whether the laptop stays responsive (memory still free, nothing paged to disk).

## 2026-10-18, Gemma 4 E2B and E4B shown; the small Qwen3.5 stay hidden; the Uzbek Moon question reworded

The owner's rulings of 2026-10-17 on the six small hidden entries (the 0.2 plan, item 2), after their runs on the founder's 4 GB laptop (package `96efd0e8`, engine b10869 on Vulkan, Silent mode, NVIDIA driver 617.42; the results in the internal repository under `docs/measure/2026-10-17/`).

- **Gemma 4 E2B and Gemma 4 E4B are shown** (`hidden` removed from their entries in `models/catalog.json`).
  - They went through the whole script with no loops, no crashes and no answer in the wrong language.
  - Every sum, and the picture, is right.
  - E4B answered the Uzbek (Cyrillic) question in Latin script.
  - Both misread the Uzbek Moon question (below).
  - Speeds while writing: E2B 45 tok/s, E4B 20 tok/s.
- **They have no exercise record, so they are never offered first.** `recommend` only offers a model that has been measured: one measured on the tier itself, or, where none has been, the best measured one. A shown entry without a record is listed with the verdict its fit gives it, and is chosen only by the person.
- **Qwen3.5 0.8B and 2B stay hidden for 0.2.**
  - The 0.8B loops in Uzbek (four answers, two with no answer at all), and loops until it is cut off when thinking.
  - The 2B repeats a block in one Uzbek answer, gives the train's arrival as 11:15 instead of 12:15 with thinking off, and puts the Moon at 75,000 km.
- **Ministral 3 8B is not yet decided.** Its run was stopped at the 3 GB memory rule while it loaded. It runs again with the founder's other programs closed. Gemma 4 12B waits for a 16 GB machine.
- **The Uzbek Moon question is reworded** in the kit's script (`crates/localspace-cli/src/measure.rs`):
  - It read "Oy Yerdan qancha uzoqlikda joylashgan?". "Oy" alone is both the Moon and a month, and both Gemma models took it as a question about a place they couldn't identify.
  - It now reads "Yer bilan Oy orasidagi masofa qancha?" ("What is the distance between the Earth and the Moon?"). The Earth beside "Oy" says which one is meant.
  - Its expected fact is unchanged (about 384,400 km).
  - Results recorded before this date used the old wording.
  - **Rerun of that question alone, on both models,** with thinking off (a development build of the kit, the same engine, Silent mode):
    - **E4B now answers right:** "o'rtacha masofa taxminan 384 400 kilometr".
    - **E2B still misreads it,** now as a question about itself ("Men siz bilan jismonan qandaydir masofada emasman"). For E2B the miss is the model's, not the wording's. It is shown all the same: one wrong fact, with no loop and no wrong language.
- **Tested:** `an_entry_that_has_not_passed_its_checks_is_hidden_from_the_app_and_kept_for_measure` now also asserts that the two Gemma entries are in the app's list and the two small Qwen3.5 are not.

## 2026-10-18, maths in chat answers, rendered by KaTeX

What the 2026-10-10 ruling ("Maths display (KaTeX) is in 0.2") is built as
(the owner's plan for 0.2, item 3; architecture v2.1 §6 for the client).

- **Maths is found before the Markdown parse** (`web/src/lib/maths.ts`).
  Remark would read `x_1` inside a formula as emphasis and `\(` as an
  escaped bracket. So each formula is lifted out first, and a placeholder of
  private-use characters is left in its place. The renderer puts KaTeX's
  rendering where the placeholder is. No `remark-math` or other new
  dependency: KaTeX is the only one, as ruled.
- **What counts as maths** is what models write:
  - `$$ … $$` and `\[ … \]` on a line of their own (display);
  - `\( … \)` within a sentence;
  - `$ … $` within a sentence, as Pandoc reads it: no space inside either
    mark, and the closing mark not followed by a digit, so that
    "$5 and $10" stays two prices.

  Code, fenced or inline, is never touched. An answer cannot forge a
  placeholder: the private-use marks are stripped from it first.
- **KaTeX is fetched with the first formula,** not with the shell
  (`web/src/components/Maths.tsx`). Until it is ready, or if it cannot be
  loaded, the formula's source shows as code. `katex.render()` writes into
  an element that React never fills, with `throwOnError: false` (a formula
  it cannot read shows in red as written), `trust: false` and
  `strict: "ignore"`.
- **woff2 fonts only.** A Vite plugin removes the woff and ttf alternatives
  from KaTeX's stylesheet before it is bundled, so those fonts are never
  emitted.
- **Its own size row:** `web/size-limits.json` holds KaTeX's script,
  stylesheet and fonts apart from the shell's 2 MB, since they are not part
  of the base bundle. CI first measured them at 333.2 KB gzipped (the
  shell itself at 128.1 KB), and the limit is that plus about a quarter,
  426,500 bytes.
- **The walk covers one formula** (`web/e2e/maths.mjs`, in CI's `e2e` job).
  The test engine, given a stub that says `writes maths`, answers with a
  display formula and an inline one. The walk checks that:
  - both are rendered by KaTeX;
  - no dollar mark is left;
  - the words around them are kept;
  - every font fetched is woff2.

  Unit tests for the extraction run with `npm test` (`src/**/*.test.ts`,
  added to the test script).
- **Built and tested by CI only** (the owner, 2026-10-17): after the
  founder's laptop froze under a model run, nothing heavy runs on it until
  its health check after the fixes is clean. This branch was written by
  editing files, and its first build, lint and test runs were CI's.

## 2026-10-18, publishing to localSpace-releases, after the release check only

- **`package.yml` publishes** (the owner, 2026-10-17). A `v*` tag builds the
  package as before, and keeps it 14 days instead of one, for the release
  check. Nothing is published by the tag itself.
- **After the check has passed on that package,** the founder starts the
  workflow by hand with `publish_tag` and `release_check_passed`. The
  `publish` job then:
  - refuses unless the tag is a version (`v0.2.0`) and the box is ticked;
  - finds that tag's successful package run, at the tag's own commit;
  - downloads **the very files the check ran on**, and checks them against
    their `SHA256SUMS.txt`;
  - requires `docs/release-notes/<tag>.md`;
  - creates the release in `LinusIp/localSpace-releases` with the installer,
    the portable zip, `SHA256SUMS.txt` and the notes.
- **The token:** `RELEASES_TOKEN`, made by the founder, a fine-grained token
  with Contents read and write on `localSpace-releases` only. It is read by
  the publishing step and nothing else.
- **A release** (`vX.Y.Z`) is published only with the box ticked, and goes
  out as the latest.
- **A release candidate** (`vX.Y.Z-rc.N`) is published only as a
  pre-release, without the box: it tests the path before the check. Its notes
  must say "Test release: not for use, deleted after the check." It is
  deleted from `localSpace-releases` after the check, so that the first
  public release is 0.2 (the owner, 2026-10-17 and 2026-10-18).
- **Refused, each tried in a hand run that published nothing:**
  - a release tag without the box;
  - a release candidate with the box, which would make it a full release;
  - a tag that is neither.
- **No environment approval is used:** GitHub's free plan has none for a
  private repository, and this one is to become private. The gate is the
  hand start and the box, after the check.
- **Signing:** 0.2 ships unsigned to testers. The rehearsal signing stays in
  the workflow, and an OV certificate is the founder's later decision.

## 2026-10-18, Qwen2.5 retired; a tier without its own measurement takes the best measured model; the measured note on bigger cards

- **Qwen2.5 is retired, not removed** (document 34; the owner, 2026-10-17).
  - **In the catalog:** `"retired": true` on its five entries, kept for the
    tests.
  - **`Catalog::recommend`** never offers a retired entry first.
  - **Core's list (`model_catalog`)** leaves a retired entry out unless a
    checked copy is on the computer. A copy that is listed reads "Older
    model, no longer offered. You can keep using it or delete it."
    (`models::RETIRED_WORDS`, verdict `retired`). It stays loadable and
    deletable, and is never deleted for the person.
  - **A download of a retired entry is refused:** "… is an older model and is
    no longer offered."
  - **Full removal comes in 0.2.1.**
- **A tier with no measurement of its own takes the best measured model that
  fits there** (document 36 §4). Without this, 8 GB, 16 GB and no-card
  computers would have been offered nothing first once Qwen2.5 was retired.
  The typical-computers table now offers the Qwen3.5 9B to every computer
  with a card of 4 GB or more, and the 4B where there is no card, the
  processor's own graphics, or an unknown card with slow memory.
- **On a bigger card, a model measured on a smaller tier that fits wholly on
  the card** is shown "Runs well: at least about N words a second (measured
  on a 4 GB laptop)". It is the kit's median writing speed there, in words a
  second, as a lower bound, and never as if measured on the person's own
  computer.
  - **The record:** an exercise record's new `writing_tok_s`: the 9B's of
    2026-10-15 (9.2) and the 4B's of 2026-10-08 (38.7), so 7 and 29 words a
    second.
  - **Elsewhere,** the estimate stays as before.
- **The fit now counts the vision file in system memory**
  (`Shape::ram_only_bytes`, from the entry's own check of the file). Core
  always starts the engine with `--mmproj … --no-mmproj-offload`, so the 9B's
  0.9 GB vision file sits in system memory whatever goes to the card. It
  counts towards fitting, not towards the speed. The room left for Windows is
  unchanged: the larger of 4 GB or a quarter of the memory.
  - **Checked as the owner asked:** a laptop with the same 4 GB RTX 3050 Ti
    and 8 GB of memory is offered the **4B**, since the 9B "will not fit"
    there. It is a new row of the typical-computers table.
- **The measured note stands only where this card's own estimate is at
  least the measured speed.** A bigger card is not always a faster one.
  Without this, "16 GB of slower memory and an 8 GB card the table does not
  know" showed the 9B as "at least about 7 words a second (measured …)" while
  offering the 4B first. A card the table does not know is promised only what
  the processor would do, and is never told more.
- **Tests:** the rule tests keep their Qwen2.5 fixtures, un-retired in each
  test's own catalog (`offered_again`). The tests about typical computers
  take the new defaults. New tests cover an older model never offered first,
  the measured note and where it is never shown, and the older model in
  Core's list with its download refused.

## 2026-10-17, the NVIDIA edition approved for 0.2.1; "stopped" after the save; localSpace moves to D:

- **The NVIDIA edition is approved for 0.2.1, on CUDA 12.4.** It meets the
  reading clause on the RTX 3050 Ti (reading +55 to +58 %, writing +2 to
  +5 %). Its minimum driver is 551.61, per NVIDIA's Table 4. The app suggests
  it only when the card and the driver can both run it. CUDA work starts after
  0.2 ships.
- **The measurement it rests on, with its exact settings** (2026-10-16):

  | What | Setting |
  |---|---|
  | The laptop | ASUS TUF Gaming A15, Ryzen 7 6800H, RTX 3050 Ti Laptop GPU (4 GB, compute capability 8.6), 16 GB of memory, on mains power |
  | Driver | NVIDIA 566.07, unchanged throughout |
  | Armoury Crate | the window: Performance; the registry: 2 |
  | Engine | llama.cpp b10869 both ways |
  | Vulkan files | `llama-b10869-bin-win-vulkan-x64.zip`, as packaged |
  | CUDA files | `llama-b10869-bin-win-cuda-12.4-x64.zip` (254,079,631 bytes, SHA-256 `e5035058…f7bf`) and `cudart-llama-bin-win-cuda-12.4-x64.zip` (391,443,627 bytes, `8c79a9b2…e1d6`), unpacked together (1.1 GB) |
  | Placement, chosen by Core | the 9B `-c 8192 -ngl 15`, on `Vulkan1` and on `CUDA0`; the 4B `-c 8192 -ngl 31` on `Vulkan1`, `-ngl 30` on `CUDA0` |
  | Writing and the first word | the kit's whole speed script through Core, thinking off, temperature 0.3. Vulkan: package `e317f6d`, 2026-10-15. CUDA: package `96efd0e8` with `LOCALSPACE_LLAMA_SERVER` pointing at the CUDA engine, 2026-10-16 |
  | Reading | a 3,000-token message cut by the engine's own tokenizer, `/completion` with `cache_prompt: false` and `n_predict: 1`; one warm-up, then the median of three, on each engine started with the placement Core chose for it |
  | Between parts | three minutes' cooldown; no throttling (at most 92.9 °C, never under the base clock while busy) |

  The scripts (`run-cuda.sh`, `read3k.mjs`) and the results are in the
  internal repository's `docs/measure/2026-10-16/cuda/`.
- **"Stopped" stays after the save in 0.2.1.** It is checked again in
  document 37's capacity test, with many people and a busy disk.
- **Everything of localSpace's moves to D:** (`D:\localSpace\` with
  `models\`, `check\`, `repos\` and `build\`; Ubuntu to `D:\WSL\`). The
  founder cleaned C: by hand; C: is for his other work. At least 15 GB stays
  free on D: and 30 GB on C:. Nothing new of ours is written to C:. Moving
  means copy, check the sizes (and the SHA-256 of model files), then remove
  the C: copy; `cargo clean` on C: is allowed, since the build is rebuildable.
- **0.2.1: the person chooses where models are kept** ("Keep models on: drive
  D:"). Existing models are moved with a check, and each drive's free space is
  shown.

- **A lesson, corrected the same day: the "layer" is the Claude app's
  packaged-app redirection of `AppData`, not the sandbox.**
  - **What it is:** the Claude desktop app is a packaged Windows app. Windows
    redirects its writes under `%LOCALAPPDATA%` and `%APPDATA%` into the
    package's own folder,
    `%LOCALAPPDATA%\Packages\Claude_pzs8sxrjxfjjc\LocalCache\`.
  - **Who it applies to:** everything the coding agent starts inherits it,
    "outside the sandbox" included: the shell, PowerShell, cmd, `mklink`, and
    the app itself. Those programs see the real folder merged with the
    redirected one, and a write lands in the redirect.
  - **Only the founder's own PowerShell**, started from the Start menu, sees
    the real `AppData`. `%TEMP%` and `D:` are not redirected.
  - **How it was found:** the backup folder showed at two paths, its real one
    and one under the package's `LocalCache`. Then the founder looked from his
    own PowerShell. On the real disk `%LOCALAPPDATA%\localSpace` was still a
    plain folder of three files with no chats. The junction and the
    rename-to-backup had happened only in the redirect.
  - **Where his chats lived:** in the redirect, from the first start on
    2026-09-19 to the last one this morning (06:20 UTC). The real folder's own
    log begins today at 06:55 UTC, the founder's first start from outside
    Claude, which found nothing ("first run").
  - **What it affected:**
    - his Qwen3.5 4B and his `engines` folder, removed, in the redirect, as
      the first version of this entry recorded;
    - one start at 11:09 that rewrote his token and added lines to his log;
    - the junction and the backup, made only in the redirect.

    His chats were safe in the copy `D:\localSpace\app-data` (checked, 11
    chats, 76 messages).
  - **What the founder did, in his own PowerShell:** he copied that folder to
    `D:\localSpace\app-data-try`. He made the real `%LOCALAPPDATA%\localSpace`
    a junction to it, and his chats show in the app. Kept for now:
    `D:\localSpace\app-data`, `D:\localSpace\app-data-founder` (the real C:
    folder of three files) and the real
    `%LOCALAPPDATA%\localSpace.backup-2026-10-17`.
  - **The rule since (the owner, 2026-10-17):** for any change to the
    founder's real files under `AppData`, the coding agent writes the exact
    PowerShell commands and he runs them himself. The agent may read and
    report, but changes nothing there. A junction is removed only with
    `(Get-Item <path>).Delete()`, never with `Remove-Item`.
  - **No reported figure is affected:** the 30 measurement results name 60
    model files, all with the official SHA-256. Every kit run used its own data
    folder under `%TEMP%`, which is not redirected.
  - **What the founder's earlier sessions were:** all 18 app starts from
    2026-09-19 to 2026-10-17 06:20 ran inside the redirect. Among them were the
    Test A dry run, his test of `aeb31fa` on 2026-10-10, and his sessions of
    2026-10-15 and 16. They were launched from inside Claude: an installer or
    the app opened from Claude, whose "start the app" then inherited the
    redirect.
    - **Unaffected:** what he saw and judged (answers, speeds, the screens),
      since the program and the engine were the same.
    - **Never exercised on his laptop until today:** the app with the real
      `AppData`, that is a first start from the Start menu after an install
      from Explorer, and the uninstaller's "delete the application data", which
      removes the real folder and not the redirect.
    - **The release check therefore installs and starts the app from Explorer
      and the Start menu, never from a file opened in Claude.**
  - **Windows PowerShell 5.1 skips paths over 260 characters without a
    word.** Three Qwen2.5 14B files (8.4 GB) in a deep scratchpad folder
    survived the founder's clean-up for that reason. Commands for him use
    `-LiteralPath "\\?\C:\..."`.

- **The Qwen3.5 9B is confirmed as the 4 GB default for 0.2:** the
  founder's 10-minute try of `96efd0e8` (started from the Start menu, the real
  `AppData`) found its speed fine and its Uzbek and Russian answers good. No
  veto.
- **The move to D: is finished.**

  | | Free | What of ours is there |
  |---|---|---|
  | C: | 174.7 GB of 389.7 | the repositories on the Desktop; this session's scratchpad under `%TEMP%\claude` |
  | D: | 23.3 GB of 86.2 (the limit is 10) | `localSpace\models` 33.6 GB, `check` 3.5 GB, `build` 2.1 GB, `app-data-try` 6.2 GB (the founder's data and his 9B), `app-data` and `app-data-founder` (copies kept for now); `WSL\Ubuntu-22.04\ext4.vhdx` 17.5 GB |

  - **Ubuntu:** compacted from 29.9 GB to 15.4 GB and moved by the founder
    with `wsl --manage Ubuntu-22.04 --move D:\WSL\Ubuntu-22.04`. A
    `cargo check --workspace` in `~/ls-build` then passed in 6 min 43 s.
  - **The real `%LOCALAPPDATA%\localSpace`** is the founder's own junction to
    `D:\localSpace\app-data-try`. The redirected junction is removed.
  - **A full workspace build on D:** adds about 11 GB and still keeps D:
    above 10 GB.

## 2026-10-16, the language rule in the prompt; CUDA as an NVIDIA edition; document 37's design rulings

- **Both system prompts now say "Answer in the language and alphabet the
  person writes in."** (`prompt::LANGUAGE`, in `SYSTEM` and `SYSTEM_PLAIN`,
  with a test). It stays in the product whatever the retest shows. Measured on
  2026-10-15: Gemma 4 E4B answered the Uzbek Moon question in English five
  times of six and drifted from Cyrillic Uzbek into Latin; the Qwen3.5 4B wrote
  one answer in Azerbaijani.
- **"Mixing" for the 4 GB rule:** a single word with letters from the other
  alphabet is a typo; an answer in, or drifting into, another language is
  mixing. One retest, fixed in advance: the Russian and Uzbek language set
  again for the Qwen3.5 9B, Gemma 4 E4B and Qwen3.5 4B with the instruction,
  read by the same three readers. The decision itself is in the business log.
- **Near-repeating loops** (lines that vary slightly, as the Ministral 3 8B's
  did on 2026-10-15): the 2,048-token answer limit covers them in 0.2; a wider
  loop guard comes in 0.2.1.
- **CUDA, for 0.2.1, ships as a second installer, not a download inside the
  app:** the standard edition (Vulkan) and an NVIDIA edition (llama.cpp's
  b10869 CUDA 13.3 build and NVIDIA's runtime files inside, Vulkan beside it as
  the fallback), both from `localSpace-releases`. NVIDIA's licence lets its
  runtime files go out only inside an application (CUDA Toolkit EULA §1.1.1(3),
  §1.1.2, Attachment A §2.6); its licence goes into `licences/`. When the app
  finds a suitable NVIDIA card and driver it suggests the NVIDIA edition, which
  the person downloads and installs like an update: **no download from inside
  the app**, so the only outbound connection stays the model download the
  person asks for. If the CUDA engine fails to start or crashes, Core starts
  the Vulkan engine, unasked, and logs why.
- **CUDA becomes the default on a card only where it is clearly faster:** at
  least 15 % faster writing and not slower reading or to the first word, or at
  least 30 % faster reading with writing no slower. Measured on the founder's
  RTX 3050 Ti (the 4 GB default and the 9B; first word, writing, reading a
  3,000-token message), and the RTX 5080 laptop when it is available.
- **Document 37's design, ruled (built after 0.2):**
  - **Roles: three.** Owner (one per installation), Admin (people, models,
    tools) and Member. No role can read anyone's chats. The code's `Admin` and
    `Member` keep their meaning; `Owner` is new; `Viewer` is to be mapped
    (asked).
  - **Built-in tools:** on the server edition the documents tool and the code
    runner ship built in, switched off until an admin enables them. The desktop
    base install stays as it is.
  - **MCP servers** run as separate processes with only the access the admin
    grants: named folders, and network only to named hosts. Until that sandbox
    exists they are admin-installed only, off by default, and marked on screen
    "runs with full access".
  - **The code runner:** Python inside the existing WebAssembly sandbox
    (WASI): no network, no files outside its folder, time and memory limits.
    Its size is reported before it is added.
  - **The action log:** for reads, the user, tool, time, outcome and how many
    items were read, never their names; for actions that change something,
    also the target (a file path or a record id). Never chat text, prompts or
    document contents. People see on screen that actions are logged.
  - **Web tools:** the ruling that hid them (`WEB_RESULTS_REACH_THE_MODEL`) is
    lifted for the server edition only: off by default, switched on by an
    admin, every use visible to the person.
  - **Partners may build environments** later, signed and reviewed. This
    supersedes the marketplace spec's "one author" principle (§1).
- **`docs/STATUS.md` corrected:** the tool-call grammar is compiled but not
  sent to the engine (H§9 is partial, not built).
- **The Qwen3.5 9B is the 4 GB tier's default** (`defaults` in
  `models/catalog.json`, no longer hidden, its 4 GB run of 2026-10-15
  recorded under `exercised`). The rule fixed on 2026-10-11:
  - at least 8 tokens a second and the first word within 5 seconds: all
    four pass (the 9B: 9.2 tokens a second, 0.5 s);
  - no loops: the Ministral 3 8B is out;
  - no mixing, as read above: after the retest with the language
    instruction, Gemma 4 E4B still answered an Uzbek question in English and
    switched two Cyrillic answers to Latin, and the 9B mixed in none of 35 by
    any of the three readers;
  - among those left, the fewest wrong answers: the 9B, 8 to 11 of 35.

  The tests that named the 4B as the default now name the 9B. Those that
  need a hidden entry use Gemma 4 31B. On a 4 GB card the 9B is partly in
  system memory; where it neither runs well nor works, the rule below the
  default still applies.
- **Later the same day, the roles in full:** `Viewer` maps to `Member`, with
  read-only access given per document (the access levels already allow it).
  The Owner is the first administrator, created at install; ownership passes
  only by the Owner's own action; no Admin can disable, demote or remove the
  Owner. A lost Owner account is recovered only from the server machine
  itself, by a local command run by someone with administrator access to that
  machine, and the recovery is written to the action log.
- **The documents tool and the code runner are built once, for the server**
  (document 37, step 3), switched off there until an admin enables them. On
  the desktop they become optional first-party add-ons from the Store once it
  reopens; nothing of them is in the desktop base install. There is no maths
  checker: the agent checks calculations by running them in the code runner.
- **The local API:** the server's OpenAI-compatible endpoint, with accounts,
  in document 37's step 1; the desktop's after step 3. **The desktop's
  listener amends deployment spec §8 ("the gateway is the only socket") when
  it is built, on these conditions:** off by default and switched on by the
  person; `127.0.0.1` only; every program that asks is shown and approved once;
  and the screen says plainly that this approval is a courtesy, not
  protection against software already on the computer.

- **The NVIDIA edition takes CUDA 12.4, not 13.3** (ruled after the
  measurement below).
  - **Why:** 12.4 runs on far older drivers, and on the older cards (GTX 10
    series, Pascal) that CUDA 13 drops.
  - **When to move:** to 13.x only if a later measurement shows it clearly
    faster on newer cards, by the same rule.
  - **Minimum driver, from NVIDIA's own release notes:** CUDA 12.4 GA needs
    **551.61 or newer on Windows** (550.54.14 on Linux; Table 4,
    "Corresponding Driver Versions"). With minor version compatibility, any
    12.x driver of 525 or newer (Table 3). CUDA 13.x needs 580 or newer
    (Table 3).
  - **Measured:** CUDA 13.3 does not start on the founder's driver, 566.07
    (`failed to initialize CUDA`); 12.4 does.
- **The NVIDIA edition is suggested only when the card and the driver can
  both run it.** When the card can and the driver is too old, the app shows
  one plain line, at most once, and the person can dismiss it: "A newer NVIDIA
  driver would let you use the faster NVIDIA edition." No link is opened and
  nothing is downloaded.
- **Vulkan against CUDA 12.4 on the RTX 3050 Ti** (driver 566.07, llama.cpp
  b10869 both ways; Core chose the placement).
  - **Writing:** +5 % on the 9B (9.2 to 9.7 tokens a second), +2 % on the 4B
    (38.6 to 39.2).
  - **The first word:** the same or faster.
  - **Reading a 3,000-token message:** +58 % on the 9B (6.85 s to 4.32 s), +55 %
    on the 4B (3.08 s to 1.98 s).
  - **The verdict:** under the rule (at least 15 % faster writing with
    nothing slower, or at least 30 % faster reading with writing no slower),
    **CUDA is clearly faster on this card, by the reading clause.**
  - The figures and the method are in the internal repository's
    `docs/measure/2026-10-16/cuda/`.

- **A timed Linux build of `other-labs` (`acb8b1b`) in WSL**, Ubuntu 22.04,
  `.wslconfig` 6 processors, 6 GB memory, 4 GB swap. Rust 1.99.0, Node
  24.19.0. CI's settings (`CARGO_INCREMENTAL=0`, line tables only) and CI's
  commands, `-j 4`, in a clone on Ubuntu's own disk.

  | Step | Time |
  |---|---|
  | `cargo fetch` | 21 min, slowed by WSL's connection to crates.io |
  | the web bundle and harness packages | 2.1 min |
  | clippy, clean | 5.2 min |
  | the tests, clean build and run | 11.1 min |
  | the whole suite again, already built, `--no-fail-fast` | 2.0 min, all passing |
  | after a one-line change in `localspace-core`, the tests rebuilt | 1.9 min |
  | the desktop app (`localspace-shell`) | 4.3 min |

  - **The connection:** WSL resolved `index.crates.io` to IPv6 first and its
    NAT stalled on it, so index requests took 15 to 20 s or timed out against
    2 to 4 s from Windows. `CARGO_HTTP_TIMEOUT=120`, `CARGO_NET_RETRY=10` and
    `CARGO_HTTP_MULTIPLEXING=false` for that run got through; nothing in WSL
    or Windows was changed.
  - **Peak memory:** 3.6 GB used, with 0.2 GB of swap, under the 6 GB cap.
  - **Disk:** `target` 14 GB, the crate cache 1.4 GB, `web/node_modules`
    150 MB.
  - **WSLg:** the app starts and opens its window ("the window is open", the
    web client served). It finds no graphics card in WSL, so it recommends
    the 1.5B.
  - **Missing packages:** none.
  - **One timing failure:** `a_stop_mid_answer_keeps_what_came_marked_as_stopped`
    (`crates/localspace-core/tests/turns.rs`) took 8.7 s to reach "stopped"
    against its 2-second limit, on the first run of the tests right after the
    14 GB build, with C: 93 % full. (Not during compiling: `cargo test`
    compiles everything before it runs a test.) **It did not happen again in
    58 stops:**
    - 5 ms idle;
    - 5 to 15 ms with all six processors busy;
    - 5 to 15 ms with 2 GB files written over and over;
    - 5 ms with the whole file's 15 tests running together;
    - 5 to 46 ms with 1.5 GB written and flushed to disk (`sync`) over and
      over.

    **The path:** a Stop shuts the engine's socket at once, so the words end
    at once (`stream::Stop::stop`). The turn is announced "stopped" only
    after Core's thread has saved the chat (`agent::keep` → redb commit,
    which waits for the disk; no transaction sets a durability) and closed
    the run (`agent::end`, `finish_run`, then `TurnChanged`). A disk that
    stalls therefore delays the "stopped" state and every other request on
    Core's one thread: measured, a forced flush took it from 5 ms to 46 ms.
    The 8.7 s once is most likely the host's disk catching up after the
    build, on a nearly full drive, but that is not proven. No change to the
    product or the test is made; the 2-second limit stays.

## 2026-10-15, what CI runs when

The owner's ruling, so that the repository can become private and stay inside
the free plan's 2,000 minutes a month:

- **On every push and pull request:** `check` (formatting, lint, types),
  `rust` (clippy, tests) and `web` (tests, build, sizes, frame times), as
  before.
- **On pull requests, on merges to `main` and by hand only:** `e2e` (the
  end-to-end walk) and `measure` (the kit against the test engine), with
  `if: github.event_name != 'push' || github.ref == 'refs/heads/main'`. A
  push to a branch with a pull request open still runs them, through the
  pull request.
- **The Windows package** (`package.yml`): by hand before a merge to `main`,
  for a check, and for a release tag; it never ran per push.
- **Why, in minutes** (the last 30 days, before the change): `ci` about 20
  billed minutes a push over 115 runs, about 2,300; the package about 13
  minutes a run, counted twice for Windows, over 38 runs, about 1,000; about
  3,300 together. After it: about 9 minutes a branch push, the full 20 only on
  pull requests and `main`, and the package only before a merge or a release,
  about 1,200 a month at the same pace.
- **What does not change:** nothing reaches a tester until `ci` is green on
  that exact commit, the walk and the kit included (they run on the pull
  request and on `main`), and the release check has passed.

## 2026-10-15, the laptop back from its service: its temperatures after it

Document 36, section 2, steps 1 and 2. Measured as before (read-only,
without administrator rights, sampled every 15 seconds), on mains power,
Windows' power plan Performance, the engine at 8 threads.

| | Before the service (2026-10-12) | After the service (2026-10-15) |
|---|---|---|
| Idle | not measured | **53.9 °C** a minute after the computer settled (74.9 °C at 10:00:26, 53.9 °C at 10:01:21) |
| Writing, the model | Ministral 3 8B, an hour into the run | Qwen3.5 4B, five minutes answering (10:02–10:07) |
| Temperature under load | 92.9 °C | **92.9 °C** from 45 seconds in, held to the end; mean 91.2 °C over the 19 samples |
| Effective clock | 4.04 to 4.06 GHz (126 to 127 % of base) | **3.34 to 4.05 GHz** (104 to 127 %), mostly 3.5 to 3.7 GHz once hot |
| Throttling by the run's rule | no | **no**: never 95 °C, never under the 3.2 GHz base clock while busy |
| Armoury Crate's registry on mains | Silent (`ThrottleModeOnAC = 2`) | **Silent (`ThrottleModeOnAC = 2`)**, `OnDC = 1` |

- **The service did not lower the temperature under load**: the processor
  reaches the same 92.9 °C, and now holds a lower clock there, which is what
  the processor does to stay under its limit. Idle it cools faster than it
  did before (not measured then, so not compared).
- **Every speed figure measured before today is Silent-mode history**, as the
  owner ruled the same day (document 36, section 2, step 2): the Qwen3.5 4B's
  and Gemma 4 E4B's of 2026-10-11 and the Ministral 3 8B's of 2026-10-12. They
  are kept as records and count for nothing.
- **The owner says Armoury Crate is on Performance; its registry still reads
  Silent.** The run's speed parts wait until the founder confirms the mode in
  Armoury Crate's own window; the mode is never changed from here. Whatever
  he confirms is recorded with the run.

## 2026-10-12, the 4 GB run paused for the laptop's service; its temperatures before it

The founder's laptop (ASUS, AMD Ryzen 7 6800H, 8 cores and 16 threads,
3.2 GHz base; RTX 3050 Ti with 4 GB; 16 GB of memory) ran at about 93 °C on
the processor for an hour during the run, its fans at about 5,000 RPM. It is
being cleaned and its thermal paste replaced; **the run is paused until the
founder says it is back** (stopped at 20:19, nothing left running).

- **Before the service, as measured** (read-only, without administrator
  rights): at 20:17–20:18 the processor's thermal zone read **92.9 °C**, its
  effective clock **4.04 to 4.06 GHz** (126 to 127 % of its base), its load
  68 to 69 %, with the Ministral 3 8B writing; not under its base clock, so
  not throttling by that measure, but 2 °C from its 95 °C limit. **Armoury
  Crate's mode on mains power: Silent** (`ThrottleModeOnAC = 2` in its
  registry, which ASUS's tools read as Silent). **Windows' power plan:
  Performance.** **The engine's threads: 8**, the physical cores: llama.cpp's
  default, as Core passes no thread count.
- **After the service, the speed part runs again for all four models**, the
  ones already finished included, so that all four are measured on the
  serviced laptop: the Qwen3.5 4B's and Gemma 4 E4B's of 2026-10-11 and the
  Ministral 3 8B's of this evening stand as before-service records and do not
  count. Gently, as the owner ruled the same evening: three minutes'
  cooldown between models and between the speed and accuracy parts; the
  processor's temperature and clock sampled every 15 seconds through each
  speed part, its first and last samples the start and end; a speed part
  that ran while the processor throttled (95 °C or more, or under the base
  clock while busy) is marked and run again after a cooldown, once;
  Armoury Crate's mode left as it is and recorded.
- **The accuracy results stand**, since the answers do not depend on the
  temperature: the 4B's and the E4B's counting and language parts, and the
  Ministral 8B's counting part. **Still to run:** the Ministral 8B's
  language part (stopped partway), and the Qwen3.5 9B's counting and
  language parts.

## 2026-10-12, the kit stops cleanly and keeps the computer awake

The owner's ruling, after a stopped run left an engine behind and its
script went on to the next model.

- **Every engine Core starts ends with Core's process** (Windows): it joins
  a job object that ends everything in it when Core's process ends, however
  it ends. So Ctrl+C on `localspace measure`, or one stop of it, ends its
  engine too. The calls are declared in Core's own isolated module
  (`os`), as `footprint` declares its own: no crate is added. **The test**
  (Windows only, so it runs in the package workflow on the Windows runner)
  holds that the first engine and one started again are both in that job.
  Elsewhere nothing changes: the orphan-engine file of the 0.2 basics still
  covers a power cut and other systems.
- **`localspace measure` keeps the computer awake while it runs**, by
  asking Windows as long as the run lasts (`SetThreadExecutionState`): no
  sleep setting is changed, so there is none to restore, and the request
  ends with the run, however it ends.

## 2026-10-11, other labs in, Qwen2.5 out (document 34): the rulings, the entries, the revised 4 GB run

The owner's rulings on the builder's reply (the reply and the dates are in
the internal log).

- **0.2 takes Gemma 4 and the Ministral 3 8B; Qwen2.5 leaves the catalog.**
  Ministral 3 3B and 14B, Phi-4-mini and gpt-oss-20b (low effort by
  default, the renamed file with its new SHA-256) go to 0.2.1; Ministral's
  Instruct models only; Phi-4-reasoning-vision waits until ggml-org or
  unsloth publishes it; Mistral Small 4 belongs with server mode. **The pin
  stays at `b10869`**: it knows `gemma4`, `mistral3`, `phi3`, `gpt-oss` and
  their image projectors.
- **The Gemma 4 files are ggml-org's 4-bit ones built from Google's own
  4-bit-trained weights** (Q4_0, with the Q8_0 vision file); ggml-org no
  longer publishes Q4_K_M. The 12B is the June 2026 model (there was no
  April 12B). Five entries, `hidden` until each has passed its checks; the
  26B-A4B and the 31B need a borrowed machine. **Gemma's attention-cache
  figure** counts its full-attention layers per token and its
  sliding-window layers at their window (512 or 1,024 tokens), spread over
  an 8,192-token context, and leaves out the layers that share another's
  cache: a plain per-token figure would overstate it several times.
- **The Ministral 3 8B Instruct** from unsloth (Q4_K_M and its F16 vision
  file, kept under a name of its own), `hidden` until checked; its template
  refuses a late system message, so the task notes go in the person's
  message (shape 2), found at load as for every model.
- **Temperature: 0.3 for every answer with thinking off, on every model;
  with thinking on, the card's values.** A model whose short pass loops or
  mixes languages at 0.3 takes its card's value instead, and its entry says
  so.
- **Every entry names its maker**, shown as a small label on its row. **The
  list of models that can be added is always open and grouped by verdict**:
  what runs well here first, then what works more slowly, what is too slow,
  what is not yet sized; what will not fit is folded behind one button
  (1.10 of the basics with document 34 §5).
- **Every catalog file's download address is checked in CI**: one request
  per file at the entry's pinned revision, comparing the size and SHA-256
  Hugging Face reports with the entry's (`scripts/check-catalog-files.mjs`);
  nothing is downloaded.
- **The revised 4 GB run**, rules fixed in advance: four contestants, the
  Qwen3.5 4B, Gemma 4 E4B, the Qwen3.5 9B and the Ministral 3 8B (the 9B
  and the 8B partly off the card, with their vision files). **A model
  qualifies** when, on this laptop (the founder's: RTX 3050 Ti with 4 GB, 16
  GB of memory), the median writing speed over the whole message script is
  at least 8 tokens a second and the first word of "Hi there!" comes within
  5 seconds. **Among the qualifiers with no loop and no language mixing, the
  fewest wrong answers on the 35 counting cases wins** (three independent
  readers); a model that reads images beats one that does not on a tie.
  **Before the winner becomes the default**, the founder uses a package with
  it for ten minutes and may veto it on how it feels; then the next
  qualifier wins.

## 2026-10-10, after the founder's test of `aeb31fa`: the whiteboard and the planner leave 0.2; loops, sampling, empty chats

The founder used the package of `aeb31fa` with the Qwen3.5 4B. What he
met, and the owner's rulings on the builder's plan, the same day.

- **What he met.** In a chat where the board had been used, "no need with
  white board show how to solve a basic integral" was refused: "My tools
  are designed only for managing the visual elements on your whiteboard."
  In another, an Uzbek answer about the Sun repeated "Uning yuzasi
  taxminan…" without end, and "in russian" after it was refused the same
  way, in Russian. The trigger: a long chat with the board used earlier,
  and in the second a looped answer in the history. The builder's own
  thirty tries of shorter sequences on the same package (the whiteboard
  installed, the board focused, the requests logged at the engine) met no
  refusal; what they showed is that the 23 board tools reach the model
  twice (in the instructions and through the engine's own tool format) and
  that the board's state is joined to the end of the person's newest
  message.
- **The whiteboard and the planning board leave 0.2.** Neither is in the
  package (`"ship": false` in `scripts/harnesses.json`; the types package
  alone stays, which offers no tool). **Where an older version installed
  them, Core sets them aside** (`localspace_core::SET_ASIDE`, which the app
  and `localspace serve` use unless the settings' `[harnesses] set_aside`
  names another list): not loaded, not offered by a catalog, not
  installable, their boards and the files exported from them not listed
  among the documents. **Nothing is deleted.** What stays on a computer that
  used them, under `%LOCALAPPDATA%\localSpace`: the packages in
  `installed\io.localspace.whiteboard\` and `installed\io.localspace.planner\`
  (and `installed\io.localspace.types\`, their dependency); the boards and
  their PNG and SVG exports in `db\localspace.redb` (the `documents` table
  for their records, `blobs` and `meta` for their contents). No board is
  written anywhere else. **The Store's place in the sidebar is shown only
  once it has a tool to offer**, or one is installed. The tool machinery
  (the agent loop, the tool format, the harness host, install and
  uninstall) stays with every test; CI walks it with the whiteboard
  (`set_aside = []` in its settings).
- **With nothing installed the model is told of no tool** (ruled
  2026-09-24); a new test holds a request to it: no tool list, no state,
  no ledger, the plain instructions alone. A second test holds a whiteboard
  left installed by an older version to "kept on disk, shown nowhere".
- **Empty chats no longer pile up.** A chat is written and listed once its
  first message is sent; "New chat" in a chat with nothing in it stays
  there; an empty chat an older version saved stays in the database and is
  not listed. (Item 1.8 of the basics, "New chat reuses the empty one", is
  this.)
- **Sampling per model, from its maker's card.** Core had sent a
  temperature of 0.2 and nothing else. Each catalog entry now carries
  `sampling: {answer, thinking}` as the card publishes it, read on
  2026-10-10, and every request of that model takes the one for its
  thinking setting:

  | Models | Answer (thinking off) | Thinking on |
  |---|---|---|
  | Qwen3.5 0.8B, 2B | temperature 1.0, top_p 1.0, top_k 20, presence 2.0 | 1.0, 0.95, 20, presence 1.5 |
  | Qwen3.5 4B, 9B, 27B, 35B-A3B | 0.7, 0.8, 20, presence 1.5 | 1.0, 0.95, 20, presence 1.5 |
  | Qwen3.8-27B | 0.7, 0.8, 20, presence 1.5 | 1.0, 0.95, 20, presence 0 |
  | Qwen3-30B-A3B | 0.7, 0.8, 20 | 0.6, 0.95, 20 |
  | Qwen2.5 0.5B, 1.5B | 0.7, 0.8, 20, repetition 1.1 | — |
  | Qwen2.5 3B, 7B, 14B | 0.7, 0.8, 20, repetition 1.05 | — |

  min_p 0 throughout. **Qwen warns that a high presence penalty can mix
  languages:** the cards' values stand only once the kit's Russian and
  Uzbek runs show no mixing; where they do, the penalty is lowered for that
  model and its entry says so. Every 4B measurement before this was at 0.2,
  so the 4 GB counting runs (4B against 7B) run again under these settings.
- **The loop guard.** An answer whose end is one stretch of at least 60
  characters said three times in a row is stopped at once (the engine's
  connection shut): what came before the loop and one saying of it are
  kept, the answer reads to the model as finished, and the chat says "The
  answer started repeating itself, so it was stopped." Text inside a code
  block is not looked at, since code repeats lines on purpose (a matrix of
  zeros was the case); a long table of like rows is not cut. Both are tests.
- **The longest answer: 2,048 tokens**, plus the thinking budget when
  thinking is on (it was 1,024), about a minute of writing on a 4 GB card.
  One that reaches it is kept as stopped and ends with "The answer reached
  its length limit." and the Continue button.
- **The 4 GB counting runs again, under the shipped sampling** (the package
  of `773ca868`, this laptop, thinking off, the same messages five times
  each as on 2026-10-09; results in the internal repository under
  `docs/measure/2026-10-10/count/`; three independent readers, each
  checking doubtful places on the web). **The 7B: 7 wrong of 35**, all
  three readers alike: the Lisbon weather stated as fact five times of
  five, and two places in the Berlin chat (a "Teufelsloch gorge" in
  Brandenburg that does not exist; Bernauer Straße put in Friedrichshain).
  **The 4B: 12 wrong of 35** on what all three readers count, 13 to 14
  with the borderline ones: the egg problem answered 26 four times of five
  (three on 2026-10-09 at temperature 0.2), the exchange rate answered in
  roubles once and sent to a Tajik bank that does not exist once, and six
  Berlin answers with false facts about real places (an "open-air museum
  of German life" in the Tiergarten, a zoo at Sanssouci, hills and
  waterfalls in the Spreewald, a 14th-century church at Wannsee, Dresden
  "the White City", Neukölln in the south-west). On 2026-10-09 at 0.2 the
  two tied at five each. **Under the rule of 2026-10-09 the 4B now errs
  clearly more often**; the default is the owner's to confirm, asked.
  The same package's language runs: **no loop in 45 answers of the 4B**
  (long answers, facts and the integral, five times each), and **no
  language mixing** (the Russian in Cyrillic, the Uzbek in Latin), so the
  presence penalty of its card stands. The facts: the Sun's surface
  temperature and both masses right every time, wrapped in invented
  comparisons; "how far is the Moon" asked in Uzbek answered with the
  Earth–Sun distance or the Earth's radius four times of five. The Uzbek
  long answers are fluent-looking and full of invented facts; the founder
  reads them.
- **The kit gains seven messages, last in the script** so that the others
  keep their order and names: a long answer in Russian and one in Uzbek
  (Latin), about the Sun, where a loop shows; four factual questions, two
  in each language (the Sun's surface temperature, the Earth's mass, the
  Sun's mass, the Moon's distance), each with what a right answer says
  printed under the table for the reader to count against; and "Show how
  to solve a basic integral, step by step.", the case maths display is
  read against. Every table says how many answers Core stopped for
  repeating themselves, and such an answer is marked in the table.
- **The prompt work moves to Learn wave 1, as its first part and a gate:
  no tool ships until it passes.** The assistant-first instructions; the
  board's state kept apart from the person's words and the tools listed
  once; a CI job with the real 4B, run when prompt, agent or model code
  changes and before every release check; and a test built from the
  founder's two sequences, with the whiteboard as the test tool.

## 2026-10-10, the thinking indicator: the thinking kept as its own field, a state shown, never words

The owner's three rulings of 2026-10-09 on the indicator, and what is built
on them (branch `thinking-indicator`), under the ruling of 2026-10-06 on
how it looks.

- **The thinking is persisted as its own field of the message**,
  `ChatMessage.thinking` (a string, empty where the model thought nothing,
  `#[serde(default)]` so every stored row and the legacy file still read),
  stored and deleted exactly as the message is: it travels inside the
  conversation's one row and goes with it. **It is never sent back to the
  model**: `prompt::turns_of` reads a message's words, its stopped mark and
  its calls and nothing else, and the test
  `thinking_is_kept_with_its_answer_and_never_read_back` holds a second
  turn's request to that, as `an_answers_thinking_is_never_a_turn` holds
  the rendered prompt and the engine test the bytes (the fake engine says
  of every request whether earlier thinking came with it). **Never in an
  admin screen or a log**: no request reads another person's transcript,
  the log lines of an answer stay figures, and the end-to-end test looks
  through every trace line and notice for a phrase of the thinking.
- **A stopped or cut answer keeps the thinking that came** before the
  stop, as it keeps the words: the thread that reads the model gathers
  the thinking apart from the words (`StepEnd.thought`), since the reply of
  a step cut short no longer exists. **Continue** appends the thinking of
  the carried-on step to the earlier thinking, a blank line between, as it
  appends the words. (The builder's reading of "stored exactly like its
  message"; for the owner to move.)
- **One event, `ThinkingStarted { conversation }`**, to the chat's own
  person when the first piece of thinking arrives, after `TurnChanged
  Writing` and before the first word; nothing of the thinking is in it, and
  nothing comes once a second from Core. The shell counts the seconds from
  the event itself, in one line that is written anew once a second, and
  nowhere else, so the model keeps the processor (CSS only for the motion,
  2026-10-06). A chat opened again after a reconnect shows "Starting…"
  until its first word, since the turn list carries no beginning: known,
  not fixed here.
- **What the chat shows** in the answer's place: "Starting…" from the send
  until the thinking or the first word is heard of, for every model, the
  ones that never think included (it replaces the "Thinking…" that stood
  there since Test A for every wait before a first word, which was not
  literally true); then "Thinking… 12s", the seconds in the shell's own
  style (`12s`, as "12s ago"); the mark's three nodes pulse in turn, in
  CSS, still under the system's reduced-motion setting; the rail's spinner
  stays. The words give way at the first visible word.
- **The engine's pieces come typed:** `model::Delta::{Words, Thinking}`
  through every streaming callback, so that the thinking never passes the
  Continue de-duplication nor counts as a first word for the silence rule;
  a worker that cannot stream hands its thinking first, then its words.
- **Not in this step:** clicking the indicator to open the thinking text
  (2026-10-06): the field is stored for it, the disclosure is its own
  sub-task, asked below. The budget's words ("Time is up; I give my answer
  now.") are kept in the stored thinking as the engine wrote them; whether
  they are shown is that sub-task's question.
- **The gate:** the indicator is checked on a real package on the laptop
  with the Qwen3.5 4B before it is called done (`docs/BUILD.md`, the
  real-machine checks).
- **Checked on the package of `5ebf768a`** (the `package` run of the
  branch, its portable zip checked against its SHA-256 and served by its
  own `localspace serve` on this laptop, the RTX 3050 Ti with 4 GB, from a
  folder of its own, with a catalog that differs from the built-in one
  only in the 4B's thinking on by default). The egg problem: "Starting…"
  for half a second, then "Thinking… 0s" to "Thinking… 11s", the text
  changing once a second, the mark's nodes pulsing, the rail's spinner on
  throughout; the answer at 12.0 s ("31 whole eggs", right), the
  indicator gone at its first word, the spinner gone at its end. The
  thinking, 1,333 characters, is stored with the answer. A second
  message: the engine read a prompt of about 96 tokens, the first
  answer's words without its thinking (the thinking alone would be
  about 350). Stop after three seconds of its thinking: the turn ended at
  once, the answer kept as stopped with no words and its 310 characters
  of thinking. None of the 23 lines of the stored thinking is in Core's
  log, the engine's log or the audit log. **Not checked here:** the
  system's reduced-motion setting (the browser pane cannot set it), and
  the desktop window itself (the same client, served by the app).
- **Found while checking, and fixed (the owner's ruling, the same day):**
  the log's line for an answer with thinking counted the thinking's tokens
  as written in the time after the first word ("450 tokens in 12.5 s
  (712.1 tokens a second while writing)", where the words took half a
  second). A step that thought now says its two speeds apart: "thought
  from 0.4 s to 11.8 s, 430 tokens (37.7 a second), first word after
  11.9 s, 20 tokens of answer (33.3 a second), 450 tokens in 12.5 s in
  all". The answer's tokens are its streamed pieces, one token each (the
  packaged engine with the 4B, thinking on: 602 pieces of thinking and 76
  of words for 681 tokens, the other 3 the thinking's end marks); the
  thinking's are the rest of the engine's count. A step without thinking
  keeps the line it had. **No recorded result used the wrong figure:** the
  kit reads none of the log (its figures are the engine's own); the two
  scripts that read the line (the prompt item's, 2026-10-07) took only the
  first piece's time and the prompt's size, on Qwen2.5 models, which do
  not think. A Core stopped from outside left its engine running: the
  orphan-engine file of the 0.2 basics, already planned.
- **The owner's rulings on this report (2026-10-10):** "Starting…" stays,
  for every model: it is true, and the old "Thinking…" was not. A stopped
  answer keeps the thinking it had at the stop; Continue appends new
  thinking after a blank line; the thinking is deleted with its message
  and never goes back to the model. **Reduced motion joins the release
  check** on the founder's laptop: with "Animation effects" off in
  Windows' settings, the dots stand still and the seconds still count.
- **Maths display (KaTeX) is in 0.2** (the owner, the same day; ruled
  earlier, the ruling had not reached the builder): KaTeX 0.19.0, MIT,
  offline by design (fonts by relative address), rendered with
  `katex.render()` into an element so that the strict CSP needs no inline
  styles; the browser's woff2 fonts alone are shipped, about 0.34 MB
  compressed.

## 2026-10-09, the refresh's first step: the thinking budget, the exercise record, the counting runs

The owner's answers of the evening of 2026-10-08 on the refresh's plan, and
what is built on them (branch `refresh-thinking`).

- **The 4 GB default is decided by counts, not by one answer.** The
  factual and arithmetic messages (the egg problem, the sum, the Berlin
  ideas, the weather) run five times each on the 4B and on the 7B, with
  thinking off, on the development laptop, and the wrong answers are
  counted per model and recorded with the run. Together with the founder's
  Russian and Uzbek read the counts decide: fewer errors wins; on a tie the
  4B, since it writes at twice the speed; if the 4B errs clearly more
  often, the 7B stays the 4 GB default for 0.2 and the 4B is offered as the
  fast option with its own honest row.
- **The counts, run the same morning** (the package of `b503ce5`, this
  laptop, thinking off; the results in the internal repository under
  `docs/measure/2026-10-09/`; read by four independent readers per model,
  every finding put to two refuters): **a tie, five wrong answers each of
  forty.** The 4B: the egg problem wrong three times of five (it reaches
  31 and ends on "26 whole eggs"), and twice among the five sets of Berlin
  ideas a place that does not exist ("castle ruins" at the Tegeler See; a
  "historic Jewish Quarter in Neukölln"); the sum right five times, the
  weather and the Russian exchange rate declined honestly five times each
  (the rate's advice names the Russian or Belarusian central bank for the
  Uzbek sum, allowed by the rule, noted). The 7B: the weather invented as
  fact five times of five ("mostly sunny with high temperatures around
  25°C"), everything else right every time, the exchange rate declined with
  the right bank. Under the rule a tie goes to the 4B, for its speed; the
  founder's read of its Russian and Uzbek answers completes the decision.
  Noted for the reading: the 7B's five errors are one message that it
  never declines; the 4B's are spread over two.
- **The 4 GB tier's default is the Qwen3.5 4B** (the owner, the same
  morning): the founder read its Russian and Uzbek answers and found them
  all good; the counts tie, and the kind of error confirms it, the 7B
  inventing current facts every time where the 4B declines honestly. Its
  `exercised` record for the 4 GB tier with thinking off is written (the
  full script of 2026-10-08 on this laptop, build `efce1611`), the entry
  is no longer hidden, the 4 GB default moves to it, and the 7B keeps its
  honest row. **Its known limit**, multi-step arithmetic (three of five on
  the egg problem with thinking off, right with thinking on), stands in
  its entry's notes: the case for a "think harder" option in 0.3.
- **How a decided default is kept:** the catalog file carries `defaults`,
  the default per tier where measurement decided one (`"4 GB":
  "qwen3.5-4b-q4_k_m"`), and the recommendation takes it first where the
  entry is exercised for the tier and runs well or works; the rule of
  2026-09-19 decides every other tier as before. **The tiers, by the card
  localSpace would use:** under 7 GB "4 GB", under 14 GB "8 GB", under
  22 GB "16 GB", from 22 GB "24 GB", the processor's graphics or none
  "no card": under-promising at the edges, so a 6 GB card is of the 4 GB
  tier and a 12 GB card of the 8 GB tier. An entry with exercise records
  is offered first only on the tiers it was exercised on; an older entry
  with the plain day alone counts on every tier, as before. (The
  builder's boundaries, for the owner to move.)
- **The exercise record** is a list, `exercised`, of `{tier, thinking,
  day, machine, build}` per entry; `exercised_on` stays, derived as the
  latest of those days, so the default rule reads one thing as before.
- **The thinking budget**: 1,024 thinking tokens plus 1,024 for the answer
  on every Qwen3.5 size, as the catalog's `thinking: {default, budget}` on
  the seven hidden entries, `default` off on all of them until a tier's
  measurements say otherwise. Core starts a thinking model's engine with
  `--reasoning-budget` and `--reasoning-budget-message` ("Time is up; I
  give my answer now."), the words the engine writes into the thinking
  before it ends it, which Core reads back at the thinking's end as the
  step's `budget_spent`; a thinking-on pass of the kit says how often the
  budget ran out, which is what moves the budget per model, from the
  borrowed machines' runs. The request's limit grows by the budget when
  thinking is on. The app takes the entry's default at the load; the kit
  sets its own per pass after. Tests: the flags at the engine's start; the
  limit growing by the budget and only then; the spent words read back.
  The spent path end to end is the kit's: a thinking-on pass must answer
  every message, which `--check` holds it to, and the 4B's weather message
  is the case.
- **Thinking per model only in 0.2**, no switch for a person; a "think
  harder" option can come later, from feedback.
- **The client's generated types**: CI generates them (the proto tests),
  fails when the committed ones differ from the generator's output, and
  keeps that output as the artifact `client-types` for whoever changes
  `proto` without a build of their own; no waiting for WSL. The indicator
  itself is checked on a real package on the laptop before it is called
  done.
- **The kit's counting flags**: `--only <names>` (the messages whose names
  begin so, in the script's order; a counting run, without Continue, the
  reading and the picture), `--repeat <times>` (each in a fresh chat), and
  `--thinking off|on|both`.

*This day's business entries are in the internal log.*

## 2026-10-08, the measurement kit's rules, answered

The owner's answers to the kit's plan (the plan and its estimate are in the
internal log).

- **The read-time measurement stays off the wire:** the command hosts Core
  in-process, so it is a plain Rust call on Core, not a request in the
  protocol. Were it ever a request: personal mode only, refused in
  organisation mode with a test, and figures only, never the engine's port
  or key.
- **Thinking in the kit's runs:** the full script with thinking off, and a
  short thinking-on pass of **the five messages where thinking could
  matter**: the sum, the weather, one reasoning question, one in Russian and
  one in Uzbek; on 16 and 24 GB cards thinking may be the default, and five
  easy messages would not tell. A flag runs the full script with thinking
  on, for a machine available longer.
- **The Russian and Uzbek messages go into the script before the kit goes
  to anyone** (five and five, Uzbek in Latin and in Cyrillic letters): the
  half day moves forward from the refresh. A borrowed evening that does not
  cover the languages means borrowing the machine twice.
- **One image message in the kit's run, for models with a vision file**, run
  with the projector on the card and again on the processor, recording
  which works and how long each takes: the 4 GB card cannot hold the
  projector, and the rule for 8, 16 and 24 GB cards must come from those
  machines, which only the kit reaches.
- **The results file records, per answer, whether thinking was on, and for
  the image message where the projector ran.**
- **The catalog entries come first, as data, with one condition: until an
  entry has passed its checks it is hidden from the app's model list and
  reachable only by `measure`.** Nothing is visible unless it works, and
  `main` stays releasable.
- **The Qwen3.8 load-test file is deleted** (done the same day; 103.6 GB
  free). **For the refresh's first-word measurements:** every new default
  takes shape 2 and so re-reads one exchange per turn; that cost is measured
  with the short-message reading.

**The entries as written, the same day** (the builder's choices under the
rules above; to be confirmed with the kit's report):

- Seven entries, `hidden: true`: `qwen3.5-0.8b-q4_k_m`, `-2b-`, `-4b-`,
  `-9b-`, `-27b-`, `qwen3.5-35b-a3b-q4_k_m` from unsloth, and
  `qwen3.8-27b-q4_k_m` from ggml-org, the one size where ggml-org has a
  Q4_K_M (its Qwen3.5 repositories hold Q4_0, Q8_0 and BF16 alone). Each
  with its vision file from the same publisher: unsloth's `mmproj-F16.gguf`,
  ggml-org's `mmproj-Qwen3.8-27B-BF16.gguf` (its full-precision one, as
  F16 is unsloth's; a Q8_0 of 629 MB is there too, should BF16 fail on a
  card). Every file pinned to the repository's commit, by size and SHA-256,
  checked against Hugging Face by `scripts/check-catalog.mjs` the same day.
- **Three catalog fields:** `hidden` (above); `vision_file`, the file among
  `files` that reads images, downloaded with the model and checked like it;
  `published_as`, a file's name in the repository where it differs from the
  name it is kept under, since unsloth names every size's vision file
  `mmproj-F16.gguf` and the models folder holds every model's files side by
  side (kept as `Qwen3.5-4B-mmproj-F16.gguf` and so on). The app's Core
  drops hidden entries as it starts (`Config::show_hidden_models`, false
  everywhere but in `measure`), so to the app they are not in the catalog:
  not listed, not loadable, not downloadable.
- **The planner's sizes from the real files** (`scripts/gguf-header.mjs`,
  which reads a GGUF header from the disk or from Hugging Face by a range
  request): the tensor bytes as `core_bytes`, the routed experts'
  (`ffn_*_exps`) as `routed_expert_bytes`, the layer count, and **the KV
  bytes per token counted on the layers that keep a cache alone**: a Qwen3.5
  is a hybrid whose every fourth layer has attention (`full_attention_interval`
  4) and whose other layers hold a state of fixed size (about 2 MB a layer
  a sequence, not counted). So the 4B and the 9B keep 32 KB a token (8
  attention layers, 4 KV heads, 256 wide), the 27B and Qwen3.8-27B 64 KB,
  the 0.8B and 2B 12 KB, the 35B-A3B 20 KB; the base model's `config.json`
  would have said four times as much. The 35B-A3B: 256 experts a layer, 8
  routed and one shared, 19.46 GB of routed experts over 2.54 GB of
  everything else; its active parameters, first taken from its name (3B),
  are counted from the header as the owner ruled the same evening: 34.66
  billion in all, of which 32.21 billion are routed experts and a token
  visits 8 of 256 of them, so **3.45 billion active**. Parameter counts
  are the files' own (0.75, 1.88, 4.21, 8.95, 26.9 and 34.66 billion),
  the same as Hugging Face's totals.
- **Context:** the files say 262,144; the entries say so, and the app
  starts a model with its fitted context as before.
- **`exercised_on` stays empty** on every hidden entry, and a test holds
  the built-in catalog to that, to a vision file and every renamed file
  being among the entry's files, and to the digests adding up to the size.

**The kit as built, the same day** (`localspace measure`; the builder's
choices under the answers above, to be confirmed with the report):

- **Core hosted in the process on its own thread**, through the same
  transport as the desktop app (`InProcess`), asked by the same requests;
  what no request carries, the figures, comes by a plain call on Core run
  on its thread in its turn (`InProcess::with`). Nothing of the kit is on
  the wire, and the protocol is unchanged.
- **What Core measures of a step** (`turns::AnswerMeasure`, per chat, the
  last step): whether thinking was asked, the time to the first visible
  word, the whole step, the thinking's words and when it began and ended,
  the engine's own figures (prompt tokens read anew and in how long,
  tokens written and in how long), and how a step ended early. The engine
  hands the thinking apart from the words (`reasoning_content`), and Core
  keeps it apart: nothing of it reaches a person as words.
- **Thinking is asked per request** (`chat_template_kwargs`
  `enable_thinking`) only when Core was told to (`set_thinking`); otherwise
  the model's own default holds, as before. **Before the first word, bytes
  that keep coming hold off the 120-second limit:** it ends an answer only
  once both limits have passed with nothing arriving, so a model that
  thinks for longer than that is at work, not silent (the ruling of
  2026-10-07; built now because the kit's thinking-on pass must see the
  real time to the first word on a 24 GB card). A test holds it.
- **A picture with a message** goes to the engine as the two parts of one
  message (the words, then the PNG as a data URL), kept in Core's memory
  under the message's place in the chat while Core runs; it is not stored
  with the conversation yet, which is the images item's. The kit's picture
  is a blue circle on a red square, 64 by 64, carried in the binary, so a
  reader knows what was shown.
- **A model with a vision file starts with it** (`--mmproj`), **on the
  processor** (`--no-mmproj-offload`) **unless asked for the card**: the
  one card measured (4 GB) cannot hold it beside the model, and the rule
  for 8, 16 and 24 GB cards comes from the kit's runs. The flag holds at a
  re-start with fewer layers. The kit runs the picture where the model
  started, then unloads, asks for the card, loads again and runs it
  there, recording whether the model came up and how long each took.
- **The run:** the whole script with thinking off, then the five marked
  messages with it on, `--thinking-full` for the whole script with it on;
  Continue in each whole pass; then the engine's reading; then the
  picture. The results file (`localSpace-measure-<model>-<day>.md`, beside
  `measure.cmd` when that runs it) names the build, the day, the
  computer as the app sees it, the engine's version, the files with the
  digests Core found, and the run's folder with the engine's log.
- **Downloads:** a model that is not on the computer is named with its
  files, their size and the room left on the drive, and fetched only with
  `--download`, which `measure.cmd` passes, since the tester's one command
  is to download and run (the ruling of 2026-10-07); the app's own room
  check refuses first when the drive has none.
- **In CI** (`scripts/measure-fake.sh`, in the `message-script` job): two
  runs against the test engine, one that streams slowly and one, hidden
  as a new entry is, that thinks first and has a vision file; the results
  files must hold the passes, the picture both ways and the reading. The
  test engine now gives timings and thinks first when its stub says so.
  `scripts/message-script.mjs` stays until its table and the kit's agree
  on the 7B, and is then retired.

**The owner's answers on the step report, the same evening:**

- **`measure-kit` is merged into `main`, squashed**, and the package is
  built from the merge commit: it is for the people who will run the kit
  on borrowed machines, not a release to the app's testers. 0.2 is still
  the release.
- **The 4B's run counts for the 4 GB tier**, since this laptop's card is
  that tier, the one the 4B is proposed for. What is missing is the read:
  the founder judges the Russian and Uzbek answers (sent as plain text
  with their questions, kept in the internal repository), the builder
  reads the English ones against the script's checks. If both pass,
  `exercised_on` is recorded for the 4 GB tier with thinking off; the entry
  stays hidden until the refresh makes it a default. Every other tier
  waits for the kit on borrowed machines.
- **The builder's read of the 4B's English answers** (four independent
  readers, every finding put to two refuters): the four counts of the
  script are clean on every English message with thinking off and on: no
  layout echo, no tool talk, nothing passed off as a picture, no raw call;
  Continue joined as one sentence, did not begin again, ended whole; both
  picture answers describe the blue circle on red. Facts: 408, Ada Pellow
  in 1912, 384,400 km, "Wo ist der nächste Bahnhof?", a two-sentence
  summary, the Berlin turns following on; **wrong once**: the egg problem
  with thinking off ends on "26 whole eggs" after saying 31 in the sentence
  before (with thinking on it ends on 31). Besides: an invented spa town
  ("Bad Pyrmun") among the Berlin ideas, a packing list that names a hat
  and a beanie and no tent, and the weather with thinking on that produced
  no answer (the limit, the refresh's rule). Whether one wrong sum among
  the English answers passes the 4 GB tier is the owner's call with the
  founder's read of the other languages. Found by the read: the kit's run
  folder held no log of Core's, though its results file pointed at one;
  the kit now writes Core's lines to `logs/kit.log` in the run's folder.
- **The 0.8B and the 2B on the same laptop, the same evening** (results in
  the internal repository): both all on the card, ready after 5 s, the
  first word in 0.1 s, 156 and 96 tokens a second; every message answered
  with thinking off, 408 in both, the picture right both ways (the card
  holds their vision files at 143 and 98 tokens a second against 164 and
  103 on the processor). **With thinking on, the limit bites hardest on the
  small ones:** the 0.8B thought 356 to 757 words on all five messages and
  answered none; the 2B answered two of five (the sum after 219 words of
  thinking, the Uzbek price after 412) and none of the other three. The
  refresh's budget must be sized so that these answer. **The 9B**, half on
  the card: ready after 15 s, the first word in 0.8 s, 8.9 tokens a second,
  every message answered with thinking off (the eggs right), the picture
  right both ways (10.8 tokens a second on the processor, 8.8 on the card,
  which came up after 18 s); with thinking on three of five answered, at 35
  to 97 s to the first word, the Uzbek price and the weather cut by the
  limit after about two minutes of thinking each.
- **The thinking limit goes into the refresh**, which is next, under one
  rule: **an answer always arrives.** The engine's thinking budget is
  used, so that when thinking runs out the model is made to answer; the
  weather message with thinking on is the test, and must produce an
  answer; the budget is sized per model from the kit's measurements, not
  guessed.
- **The vision-file finding is a measurement for the projector rule**,
  recorded in `docs/measure/projector-rule.md` with the borrowed machines'
  results to come: the 4 GB card holds the 4B's vision file, at 22.7
  tokens a second against 38 with the file on the processor.

**The owner's answers on the kit as built, the same evening:**

- **The kit can never hang silently.** It has its own limits: a time
  limit per answer (`--answer-seconds`, 600 by default; an answer that
  overruns is stopped, so the next message is not queued behind it), a
  time limit for the whole run (`--max-minutes`, 240 by default, set in
  CI well under the job's thirty minutes), and a progress line before
  every step. When a limit fires, the kit writes what it has and ends
  non-zero, naming the step it was in. The first hang: the kit asked the
  test engine its version for the results file, and the test engine, which
  knew no `--version`, bound its port and served instead; it answers now,
  and the kit gives the version question ten seconds in any case.
- **Found by the first real run:** Core looks at the files already in a
  models folder one model at a time, in the catalog's order, and an entry
  whose turn has not come shows no state; the kit took that for "not on
  this computer". It now waits for Core's look while the entry's files are
  all in the folder (saying so once), and gives up only when Core's
  `checked` event has come and the entry still does not count: then the
  files are not the published ones.
- **The Node script is retired.** On the same package (`efce161`), the
  same laptop and the same afternoon, the kit's thinking-off table and the
  Node script's table on the Qwen2.5 7B agree: thirty messages each, every
  one answered in both, times within a second or two of each other where
  the model's answer ran to the same length (0.8 s for the greeting in
  both; the packing list 22.8 s against 27.9 s; the Russian capital
  question 3.6 s against 9.6 s, the model writing twice as much the second
  time), Continue stopped after thirty words and carried on whole in both
  without beginning again. `scripts/message-script.mjs` and its CI wrapper
  are deleted; CI's job is the kit's. The kit's 7B run: ready after 11 s,
  the first word within 0.2 to 0.7 s of every message, 14 to 19 tokens a
  second; the Uzbek Cyrillic answers slip into Chinese characters and call
  Tashkent "Ташкент шаҳар", for the refresh's reading.
- **The kit's first run on a hidden entry, the Qwen3.5 4B, on the same
  laptop and package** (the results are in the internal repository,
  `docs/measure/2026-10-08/`): ready after 8 s with 22 of its layers on the
  4 GB card; the first word within 0.2 to 0.5 s of every message; 38 to 40
  tokens a second against the 7B's 14 to 19; every message answered with
  thinking off, in Russian and in Uzbek too, and the weather question
  answered honestly. **With thinking on, the five messages thought for 160
  to 674 words before answering**, 10 to 26 s to the first visible word,
  and on the weather question the thinking ran to the request's limit of
  1,024 tokens and no answer came at all: the refresh's thinking item must
  size the token limit, or the thinking budget, to leave room for the
  answer. The engine's probe at load found the family takes shape 2, as
  the engine check had. **The picture:** with the vision file on the
  processor, "a solid red background with a centered blue circle" in 1.0 s;
  on the card the model came up after 13 s, answered the same in 1.8 s, and
  wrote at 22.7 tokens a second instead of 38: the 4 GB card holds the
  file, at a price. The engine reads 29, 74 and 246 new tokens in 0.21,
  0.22 and 0.35 s on this model: the small-batch slowness of the 14B does
  not show on it. Continue joined mid-word and ended whole.
- **The BF16 vision file for Qwen3.8-27B stays:** the model file and its
  vision file come from the same publisher, and on small cards the file
  runs on the processor, where BF16 is fine. Whether it works on a card is
  what the kit's image test on borrowed machines shows; should it fail
  there, the entry switches to unsloth's Qwen3.8 set, both files together.
- **The 35B-A3B's active parameters come from the file's own header**, not
  from its name: the whole count less the routed experts a token is not
  sent to (expert count, experts used per token, the experts' tensors;
  the shared expert and the attention stay), since "A3B" is rounded and
  the planner's speed estimate depends on the real figure.
- **`measure.cmd` downloads, but asks first:** the file names, the total
  size and the free disk are printed, and the tool waits for Enter. It is
  the tester's bandwidth and disk. `--yes` answers for an unattended run.
- **The results file beside `measure.cmd`:** yes, and its full path is the
  last line printed, so the tester knows what to send.
- **The first-word limit change** (thinking counts as activity) **stays
  built now**: the kit's thinking pass needs it.
- **Local building:** an Ubuntu under WSL is being set up on the
  development laptop, where Smart App Control does not apply; builds and
  tests run there before a push once it is in. Windows stays as it is,
  Smart App Control on, for the release check.

*This day's business entries are in the internal log.*

## 2026-10-07, the two prompt changes built and measured: what is open after the conversation, the profile gone

The refresh's first commit, as ruled (answers 1 and 2 of the same day),
merged to `main` after CI on the branch (`5d2772a`; the measurement is in
`docs/prompt/README.md`).

- **The rule, now in plugin spec §16.1:** anything that changes during a
  conversation goes after the conversation; everything before it is
  identical from turn to turn. The system message is the instructions and
  the tools; the conversation follows as turns; what is open and the task
  ledger come after the newest message, as one trailing system message
  where the template keeps one and at the end of the newest message where
  it does not (shape 2, as Qwen3.5 and Qwen3.8 take). The model profile is
  not written for the model at all.
- **The conditions, met:** the test that two consecutive turns with a board
  edit between them match up to the new message (`prompt.rs`); the new
  tokens read on the first message after a board edit, **14B 4,044 → 2,003
  (28.3 s → 9.5 s), 7B 2,356 → 1,023 (4.4 s → 2.0 s)**; the whiteboard
  pass's answers read, the models using the board as before (the cat in 14
  and 21 board calls).
- **The price, measured and accepted:** every other turn now re-reads the
  state block, a few hundred tokens instead of a dozen, in the same seconds
  on this engine. On an engine that reads small batches properly the
  re-read of a few hundred tokens costs under a second on the 14B.
- **For the kit and the refresh:** the warm-up at load reads the system
  message alone (3,369 tokens with the whiteboard installed), so a board
  edit no longer invalidates the engine's cache of the tool descriptions.

## 2026-10-07, the engine check of document 28 (§2): the pin stays

Run on the development laptop (16 GB, a 4 GB card) against the pinned
engine, b10869, with the files approved the same day (unsloth's
`Qwen3.5-4B-Q4_K_M.gguf` and `Qwen3.8-27B-UD-IQ2_XXS.gguf`, each with its
`mmproj-F16.gguf`, checked by SHA-256).

- **Both load on the pinned engine**: the 4B with every layer on the card in
  9 s; the 27B at its smallest quantisation with 8 layers on the card and
  the rest in system memory in 26 s. No architecture error: Qwen3.8-27B is
  `qwen35` to the engine. **The pin does not move for the refresh.**
- **Both templates refuse a system message after the person's** ("System
  message must be at the beginning"), so both take shape 2, which the
  load-time probe already handles: what is open and the ledger go to the
  end of the person's newest message for them. Qwen2.5 took shape 1.
- **Thinking is on by default and can be switched off per request**
  (`chat_template_kwargs: {"enable_thinking": false}`); the engine hands the
  thinking back in a field of its own (`reasoning_content`, `--reasoning-format`
  default `auto`), with `--reasoning-budget` available. On the 4B, "What is
  17 times 24?" thought for 150 words: first visible word after 9.8 s with
  thinking on, 0.1 s with it off, 45 tokens a second either way; so thinking
  off by default is the likely setting on a 4 GB card, measured properly in
  the refresh. On the 27B here (2.5 tokens a second, most of it in system
  memory) 48 s against 3.7 s: a figure for this laptop only, not a speed.
- **Image input works on the pinned engine** (`--mmproj`, an image as a data
  URL in the message): both models described the test picture correctly
  ("a red background with a blue circle in the center"), the 4B in 1 to 2 s,
  the 27B in 16 s. **On the 4 GB card the projector must stay on the
  processor** (`--no-mmproj-offload`): offloaded, the engine ran out of
  device memory while encoding the image and the connection reset. Core
  starts the engine with the projector off the card on small cards; what
  rule decides it is part of the images item. The engine's own note:
  Qwen-VL models want at least 1,024 image tokens for grounding tasks
  (`--image-min-tokens`), to be read when images are built.
- **Found on the way:** the file host reset the 7.3 GB transfer twice; a
  resume loop finished it. A doubled download corrupted two files, since
  two writers shared them; they were discarded and fetched again. The
  download path of the product (one file at a time, verified by SHA-256)
  is unaffected.

## 2026-10-07, the rulings on the document 29 block: the Qwen set alone in 0.2, the list grouped by verdict, the measurement kit's rules

The owner's rulings on the builder's block for document 29; what goes in
which release is in the internal log.

- **The other makers' families are not in 0.2.** Their entries, file
  choices and estimates stay on record for later: Gemma 4 from ggml-org's
  Q4_0 files (Google's own quantisation-aware release; no Q4_K_M exists
  there), gpt-oss-20b from ggml-org's MXFP4 (its native format), Phi-4 and
  Ministral 3 from unsloth's plain Q4_K_M files with their vision files
  (ggml-org has only Q8_0 for Ministral). Taking Gemma's Q4_0 and gpt-oss's
  MXFP4 as the makers' own formats is agreed. None of their files is
  downloaded now.
- **Qwen3.6 is not listed** (official: 27B and 35B-A3B, Apache-2.0, April
  2026): it adds nothing the 3.5 and 3.8 pairs do not cover.
  **Phi-4-reasoning-vision-15B stays out** until a publisher already used
  has a file: only individuals publish GGUF files of it today.
- **The model list is grouped by the verdict the app computes**: "Runs well
  on this computer" first, with the recommended model on top; then "Works",
  "Slower than you read", and "Will not fit", collapsed with a count; each
  row names its maker. With the Qwen set alone the list is about a dozen
  entries, and the grouping must still read well at that size.
- **The measurement kit**: the portable zip and one command that reads the
  machine, downloads and checks the named model, runs the message script
  and the speed measurements, and writes one results file. **The message
  script ported to the command line becomes the one script: CI and the
  builder's own runs use it too, and the Node script is retired once the two
  match on the same model.** Conditions: nothing is sent anywhere; the
  results file is plain text the tester can read before attaching it; its
  first lines say which build, which model file (with its SHA-256) and which
  machine produced it. Built right after the two prompt changes, before the
  refresh's runs, so borrowed machines can measure meanwhile.
- **Downloads**: the remaining Qwen3.5 files (0.8B, 2B and 9B, each with its
  vision file, about 9.3 GB) are approved; the Qwen3.5 small set is kept;
  the Qwen3.8 load-test file is deleted only when asked.

## 2026-10-07, the catalog's files and the wider catalog (document 29): the rules behind the 0.2 plan

The owner's answers to the builder's reply on document 28 and the 0.2
plan, and the founder's `29-localspace-wider-catalog.md`. The plan itself,
its estimates and its marks are in the internal log; here is what the code
follows.

- **The catalog's files.** Qwen publishes no GGUF files for Qwen3.5 or
  Qwen3.8 (its own GGUF repositories stop at Qwen3 and Qwen3-VL), so the
  files come from **ggml-org where it has the quantisation needed, otherwise
  unsloth's plain Q4_K_M files with their vision files** for the Qwen3.5
  sizes. Rules: a model file and its vision file come from the same
  publisher; no "UD" or other publisher-specific variant is a default unless
  measured against the plain file; every file pinned by SHA-256, as now.
  **Noted for later, not for 0.2:** files hosted by third parties can be
  removed, so the registry should eventually mirror them.
- **The official line-up, read on 2026-10-07.** Qwen3.5 (Apache-2.0, all
  reading images, context 262,144): 0.8B, 2B, 4B, 9B and 27B dense; 35B-A3B,
  122B-A10B and 397B-A17B mixture-of-experts. Qwen3.6 exists officially too:
  27B and 35B-A3B, Apache-2.0. Qwen3.8: 27B dense (reads images and video),
  Flash-Next 180B, 2.4T-A95B; **no official Qwen3.8-9B or 35B-A3B**, so none
  goes in the catalog. Qwen3.8-27B's architecture is `qwen3_5` (`qwen35` to
  the engine), the family the engine has run since spring; the pin `b10869`
  is of 9 September 2026, after the Qwen3.8 release week. The engine's newest
  releases are versioned (`v0.6.0`, 5 October 2026).
- **The 16 GB tier is measured in this order:** Qwen3.5-35B-A3B first, then
  Qwen3.5-27B, then Qwen3.8-27B at Q3. The 35B-A3B at Q4 does not fit a
  16 GB card whole, so its question is speed with most of it in system
  memory and which machines have memory enough; **the planner's estimate
  uses its active size.** Ministral 3 14B is an ordinary non-default entry.
- **The wider catalog (document 29):** current small models from several
  makers up to about 27B, plus small mixture-of-experts models, as
  **non-default entries**; Qwen stays the default family. Candidates: Gemma 4
  (E2B, E4B, 12B, 26B-A4B, 31B; Apache-2.0; images at every size), gpt-oss-20b
  (Apache-2.0, text only, its own tool-call format), Phi-4 (MIT; mini 3.8B,
  14B, reasoning 14B, reasoning-vision 15B), Ministral 3 (3B, 8B, 14B;
  Apache-2.0; its template refuses a late system message, so it exercises
  shape 2). Not added: Mistral Small 4 (119B, a server model, noted for the
  server catalog); Llama 4, Gemma 3, Kimi K3, GLM-5.3 and the Qwen "Max"
  weights, whose licences carry user caps, revenue tiers or custom terms;
  anything third-party (fine-tunes, distillations, "uncensored" variants):
  only the maker's own weights, in a quantisation from a publisher already
  used.
- **What a non-default entry needs** (document 29 §3; nothing is visible
  unless it works): it loads on the pinned engine, on the card and partly
  off it; its template is tested at load and the shape logged, and where a
  family's tool calls do not work with the rule-3 handling the entry is
  offered as **chat only** and the app says so; one short pass is read (ten
  messages, two in Russian and two in Uzbek, the weather message and the
  cat), with the thinking indicator and Continue tested where it thinks; an
  SHA-256, a licence field, its address checked in CI and the planner's
  sizes from the real file; and a row that says honestly what it is: a
  measured speed where there is one, otherwise *"not measured on a computer
  like yours yet"*, never an unchecked estimate. A family that fails to load
  or whose template fails on the pinned engine, and would need a pin move,
  stays out of 0.2 and is reported.
- **A measurement kit for other people's machines**: a portable zip and one
  command a tester with a stronger card runs unattended; it reads the
  machine, downloads the named model, runs the message script and the speed
  measurements, and writes one results file for them to send back. No
  account, and nothing sent by itself: the tester attaches the file, as with
  `app.log`.
- **Downloads for the engine check approved** (the four files of the reply:
  unsloth's Qwen3.5-4B Q4_K_M with its vision file, Qwen3.8-27B UD-IQ2_XXS
  with its vision file), into a folder of their own outside the app's model
  folder; before each further download the file names, sizes and the free
  disk left afterwards; nothing deleted without asking. **The Bonsai spike
  waits: no second engine in this release.** **Order: the engine check,
  then the two prompt changes.**

## 2026-10-07, the answers after the slowdown: no padding, what is open moves after the conversation, the hold-back releases early

The owner's four answers to the report on the slowdown and the limit on
rule 3.

- **1. Short messages read slowly: no padding.** It is measured again on
  whatever engine the refresh lands on, with a bounded look at the engine's
  settings during the pin check, and the engine's issue tracker read for the
  behaviour, reading only. **The measurement joins the pin-move list in
  `docs/BUILD.md`**: the time to read about 10, 30 and 100 new tokens on top
  of a long cached prompt. If the refresh's engine still does it, options
  come back to the owner. **A short "ok, thanks" must not wait longer than a
  paragraph.**
- **2. "What is open" moves to after the conversation, beside the ledger**,
  and plugin spec §16.1 is updated in the same commit. **The rule behind it:
  anything that changes during a conversation goes after the conversation;
  everything before it is identical from turn to turn.** It goes in the
  refresh's first commit, with the profile's name leaving the prompt.
  Conditions: a test that two consecutive turns with a board edit between
  them match up to the new message; the new tokens read on the first message
  after a board edit measured before and after (it was 4,044 on the 14B); a
  template that refuses a late system message gets it at the end of the
  newest message, like the ledger (shape 2); and the whiteboard pass's
  answers read, since the model must still use the board's state correctly.
- **3. The builder's two choices on rule 3.** A block counting wherever it
  starts a line: kept. The hold-back while an answer is written: kept, **but
  a held line is released as soon as it can no longer be a call**, not at the
  answer's end: `[` followed by anything but `{` or whitespace is released
  at once, and so is a block that closes without a tool name. A person asking
  for a JSON example or a list of links must still see it stream. Tests for a
  markdown link line and a plain JSON example.
- **4. The model checks and this laptop** (16 GB of memory, a 4 GB card): it
  can show whether the engine loads Qwen3.8-27B, with the smallest
  quantisation, but it cannot give honest speed figures or `exercised_on` for
  the 16 GB and 24 GB tiers. The reply on document 28 (§6) says which entries
  can be fully measured here and which need another machine. Before each
  download: the file names, their sizes, and the free disk space left
  afterwards. No existing model file is deleted without asking.

## 2026-10-07, the whiteboard slowdown looked into: the engine reads a few new tokens slowly

The piece of work ruled with the merge (answer 2, below), done the same day
in under half a day; the measurements are in `docs/prompt/README.md`.

- **The cause:** with the whiteboard installed the first word came about
  0.3 s (7B) and 0.8 s (14B) later after the change because the pinned engine
  (b10869, Vulkan, on this laptop) reads up to about thirty new tokens one at
  a time, at about 120 ms each on the 14B with 3,400 tokens cached, and reads
  more than that together, in under a second (10 new tokens 1.3 s, 29 3.5 s,
  44 0.9 s, 96 1.0 s). The old layout added 35 to 50 new tokens to every
  message and stayed above that line; the new one sends the message alone,
  8 to 25 tokens, below it. Both layouts fall on the same curve.
- **Ruled out:** Core (replayed straight to the engine, the gap is the same);
  the request's setup outside the engine's reading, the template and the tool
  grammar (the time outside the engine's own figures is unchanged, and
  switching the grammar off changes nothing); and the previous answer read
  again because the ledger sat before the conversation (the old layout did
  read it again on every later turn of a conversation, the new one does not).
- **Found with it:** after the board changes, the new layout reads the tool
  descriptions again, since the model's own template puts them after the
  system message, which ends with the board's state: 4,044 new tokens and
  28 s on the 14B for the first message after the cat was drawn, where the
  old layout read 1,592 and 10 s.
- **Not changed:** nothing in the product; what to do about either is put
  to the owner.

## 2026-10-07, the prompt item merged, and the answers to its five questions

The owner saw the counts (`docs/prompt/README.md`) and approved the merge:
the branch `prompt-as-turns` goes onto `main` as one commit, as measured;
the entry below carries its history.

- **1. The profile's name leaves what the model reads**, but not in this
  merge: what was measured is what merges. It is the first commit of the
  model refresh (document 28), whose full runs on the new models measure it.
- **2. The whiteboard slowdown is looked into straight away**, as its own
  piece of work of at most half a day, before the refresh: people will
  install several tools, and the refresh's speed figures are measured on top
  of it. First: whether the extra time is inside the engine's own reading of
  the prompt or outside it, in request setup such as the template and the
  tool grammar rebuilt on every request; and whether the previous answer is
  read again each turn because the ledger sat between it and the
  conversation in the cache. The cause is reported, or what was ruled out.
- **3. The builder's addition to rule 3 stays, with one limit.** A call to a
  tool on offer is carried out only when the reply is nothing but the call:
  bare, in a list, or in a code fence. With words around a call-shaped
  block it is not carried out (the model may only be showing an example):
  the model is told once "to use a tool, reply with only the call; otherwise
  answer in words" and tries again; if it does the same again, its words are
  shown with the block removed. Approvals apply to every call exactly as
  before. Tested with words around a call. Built after the merge, in a
  commit of its own, with **the builder's choices, to be corrected if
  wrong**: a block counts wherever it begins a line of the answer, not only
  at its start, so the words around it can be before it too; while an answer
  is written, a line that begins, or may yet begin, such a block (a line
  starting with `{` or `[`, or a code fence that is bare or tagged json) is
  held back with all that follows it until the answer ends, while a fence for
  code in another language streams as before; and a call to a tool not on
  offer that comes twice ends in "I couldn't do that." with or without words
  around it, as rule 3 has it.
- **4. Squashed into one commit on `main`**: the decision record carries the
  history. **5. The branch `prompt-picture-sentence` is deleted.**
- **For the refresh, the invented weather:** the 7B now states made-up
  weather as fact; the old prompt told it it had no tools, and the new one
  says nothing. The weather message is read on every new default; if any of
  them invents current facts, this sentence is tested, added only when
  nothing installed gives the model live information: "You have no internet
  access and no current information. Say so when asked about current events,
  weather, prices or dates.", reported with and without. A model that invents
  facts cannot be a default.
- **Next:** the slowdown, then the builder's reply on document 28 (§6) with
  the estimate for images.

## 2026-10-07, the prompt item built and measured: turns, plain chat, and a raw tool call never shown

The item of documents 24, 25 and 27 (the entries of 2026-09-24 and
2026-10-06), with the owner's six rulings of 2026-10-07 ahead of the report.
Built on the branch `prompt-as-turns` and judged in CI there (this laptop
could not compile: Smart App Control refused macro libraries it had allowed
before), and measured on packages the `package` workflow built: before on
`4009fc5`, after on the final commit `4f4669d`. The counts, the prompt's sizes
and the time to the first word are in `docs/prompt/README.md`.

- **What the engine is sent.** One system message (the instructions, the
  profile by its name, the tools, the state, in the layout of plugin spec
  §16.1), then the conversation as turns, then the task ledger. A tool call
  in the conversation is the model's own call and the tool's reply, two
  messages in the shape its template gives them, never a line of text such
  as `tool: -> …`. Where the ledger goes is asked of the model's template
  through the engine's `/apply-template` once the model has loaded, and
  logged: a system message after the newest message where the template keeps
  one (Qwen2.5's does), otherwise the end of the person's newest message.
- **With nothing installed from the Store** no tool is on offer, Core's own
  included, and the system message is three sentences and the profile's
  name: 47 tokens where it was 774. A call the model makes anyway to one of
  Core's tools is refused as not on offer; a refusal for an unknown tool
  points at `find_capability` only where it is on offer.
- **The ledger** no longer repeats the person's newest message as its goal
  (the goal stays in the task, where Settings shows it), and is not written
  when that is all it would hold.
- **The model reads its profile by name only**, never its budgets in tokens:
  the first after run had the 7B tell a person "my working set is 8000
  tokens". A test holds the system message to carrying no word a member may
  not be shown.
- **The picture sentence** (rulings 1 and 2): with nothing installed the 0.5B
  pretended to draw the cat in both runs after the change, so the plain
  prompt ends "You reply in text only. You cannot create images.", which
  names no tool, since nothing is promised until it works; with it the 0.5B
  declined in both of two runs, so it stays. It changes when a tool that makes
  pictures ships. The 0.5B leaves the defaults at the model refresh (document
  28), so no more goes into it.
- **A person never sees a raw tool call** (ruling 3). An answer that is, or
  begins with, a block in the shape of a tool call (`name` and `arguments`, or
  `tool` and `params`; bare, in a list or in a code fence; read narrowly: two
  members, a name that is an identifier, arguments that are an object)
  naming a tool not on offer is an attempted call: refused to the model in
  one line, "no such tool; answer in words", and tried once more; a second
  one ends the answer with "I couldn't do that.". Never a third try, never the
  call's text shown or kept, and the log names the tool, never its arguments.
  A reply that begins with `{`, `[` or a code fence is held back whole. Tested
  with the 14B's `translate_text`, which the 14B wrote again in the final run
  and which was refused and answered in words. **The builder's addition, to be
  corrected if wrong:** such a block naming a tool that *is* on offer, in a
  list, a fence or with words after it, is read as the call it is (the 14B
  answered 17 × 24 with nothing but `[{"name": "task.note", …}]`), where the
  rule of 2026-09-19 read only a reply that is exactly one call.
- **The 7B guessing the weather** while having no live data is answer
  quality, not tool talk (ruling 4): no prompt line for it; the same message
  is read on the Qwen3.5 models in the refresh.
- **Every count comes from one build each** (ruling 5): before and after for
  the four defaults with nothing installed and for the 7B and the 14B with the
  whiteboard, the stable part's size and the time to the first word. **Merged
  to `main` only once CI is green on the final commit and the owner has seen
  the counts** (ruling 6).
- **Found by the measurement, not settled:** with the whiteboard installed
  the first word comes later after the change, by about 0.3 s on the 7B and
  0.8 s on the 14B (medians), in the engine's reading of each prompt, though
  it reads fewer new tokens; the before build run again on the same morning
  rules out the machine. With nothing installed it is unchanged. And the
  profile's name still in the system message ("model profile: small model")
  was said back by the 0.5B ("As a small model, I can do many things!").
- **The builder's other choices inside the rulings, to be corrected if
  wrong:** with something installed, the rule about the ledger reads "The
  task ledger is yours to read. Never repeat its headings or its format in a
  reply."; the ledger inside a message goes into the person's newest one,
  never into a tool's reply; a model Core did not start (one connected under
  Advanced) is not asked, and takes the ledger inside the person's message,
  which any template accepts; the warm-up reads the system message and an
  empty message of the person's; the conversation is cut to the working set
  by whole turns from the oldest, beginning with something the person said,
  and the line saying turns were folded away is gone.

## 2026-10-06, the organisation conflicts resolved, and the prompt item's last rules (document 27)

The founder's `27-localspace-answers-org-conflicts.md`, the answer to the
builder's report on document 26, received 2026-10-06. **It amends document
26's §3** (the organisation layer's plan, in the internal log): wherever the
two differed, the existing design stays, except where said. The layer is
still not started before Test B and the founder's go.

- **1. The first administrator stays a one-time link.** No `localspace
  install`, and no password in a command or a file: a password typed into a
  command ends up in shell history and scripts. For a scripted install the
  server writes the link to a file readable only by the account that runs
  it, and prints where that file is.
- **2. A password reset stays a link** an administrator makes and gives to
  the person; the password never passes through the administrator. O1.1's
  item is *make a reset link*, not *set a password*.
- **3. Three roles stay:** administrator, member, can view only. Document
  26's "staff" are members. The view-only refusal test stays.
- **4. Tools follow deployment spec §4.3:** only an administrator installs,
  and what is installed is everyone's. O2.1: the administrator installs and
  members see what is installed; nothing in it lets a member install.
- **5. The audit stays hash-chained files checked by `localspace audit
  verify`.** O2.2 is a viewer and a CSV export that read those files.
- **6. Browsers:** the server already gives any browser the web client, the
  Admin page included. Document 26's §1 now reads: *a browser works wherever
  the server's certificate is one the machine trusts.* Nothing to build.
- **7. Conversations on the server's disk.** The true statement: the product
  gives administrators no way to read conversations, but anyone with full
  control of the server's disk can, as with every self-hosted system, and it
  is said plainly. **The deployment guide requires full-disk encryption on
  the server** (BitLocker on Windows, LUKS on Linux). **`localspace doctor`
  and the server's first start check it:** where the data folder's disk is
  not encrypted, or the check cannot tell, one warning line in the
  administrator's Organisation section, *"This server's disk is not
  encrypted. Anyone with the disk can read conversations."*, a warning and
  never a refusal. Encryption inside the database (SQLCipher or similar) only
  if a customer's compliance team requires it: a new dependency, asked
  first, and its key would still be on the same server.
- **8. The advice line** is *"People waited for the server N times today."*:
  administrators get plain words too.
- **9. Models in O1.2:** members use the one running chat model, and the
  administrator chooses which. Several models in memory at once waits until
  a customer asks and their hardware can hold it.
- **O1.1 also closes the two gaps the report found:** turning an account off
  stops its running answer as Stop does and closes its event stream; the last
  administrator cannot be turned off or demoted. **O2.3, the silent install,
  includes a per-machine install mode**, since IT tools install as SYSTEM; an
  MSI only if a pilot deploys through Group Policy.
- **The prompt item**, with documents 24 and 25. The picture sentence is not
  added: every model declined the cat, the outcome document 25 said needs
  nothing. **The 14B's `find_capability` call written into its text must be
  gone after the change.** **A second pass of the script with the whiteboard
  installed, before and after, on the 7B and the 14B only**: that path has
  the biggest prompt, keeps the ledger, and is where the layout echo was
  seen, so without it the item cannot show the echo fixed. **The ledger's
  goal no longer repeats the person's latest message**, which every prompt
  carried twice. **`/apply-template` chooses the ledger's place, and the
  shape chosen is logged at load.**
- **The export notice says "Documents", not "Data page"**, from this commit:
  "Exported …: in the task ledger and in Documents."
- **The stale credential, found:** GitHub CLI's active slot in Windows'
  credential store held a token of another account than the one its settings
  named, so `gh auth status` read right while every call ran as the other
  account, which cannot see the private repository ("Repository not found").
  The owner's web sign-in of 2026-10-06 replaced it; Git's own credential
  manager holds no GitHub login. Who a call really runs as is read from
  `gh api user`, never from `gh auth status`.
- **Pushing without `scripts/check.sh` until WSL exists: accepted.** The
  document has the founder installing WSL and `check.sh` run on the head
  first once it is in; WSL was dropped on 2026-09-25 and that stands until
  the owner says otherwise, so nothing waits on it.

*This day's business entries are in the internal log.*

## 2026-10-06, the catalog moves to Qwen3.5 and Qwen3.8 (document 28), with a thinking indicator, and images estimated

The founder's `28-localspace-latest-models.md` and two additions to it.
**Not started:** the prompt item's "after" runs are measured on today's
defaults and reported first.

- **The catalog offers the latest generation: Qwen3.5 and Qwen3.8.** Qwen2.5
  is no longer offered to new installs. A person who downloaded one keeps it
  and it keeps working; the app may suggest the newer model once, in plain
  words, and never removes anything by itself. A new entry is a default only
  under the standing rules (commercial use allowed, the message script run
  and its answers read, `exercised_on`), is checked by SHA-256, and its
  address is checked in CI. The planner's figures for each new model (speed,
  verdict lines, per-layer sizes) are measured on real hardware, never
  copied.
- **First: whether the pinned engine (b10869) loads Qwen3.8-27B and Qwen3.5**,
  image input included, which needs the model's vision file beside the main
  one. If it does not, the pin moves and the whole pin-move list of
  `docs/BUILD.md` comes with it; that is estimated and put to the founder
  before it moves.
- **Proposed defaults by graphics memory:** Qwen3.5-2B with no usable card or
  under 4 GB, Qwen3.5-4B at 4 GB, Qwen3.5-9B at 8 GB, Qwen3.8-27B at Q4_K_M at
  24 GB and on servers. Qwen3.5-0.8B is offered for the weakest machines and
  is a default only if its answers pass the read. **16 GB is measured, not
  reasoned:** Qwen3.8-27B at Q3_K_M; an official Qwen3.8 mixture-of-experts
  model, if one exists; Qwen3.8-27B at Q4_K_M with some layers off the card;
  Ministral 3 14B only if all three fail, and said so. The Bonsai 2 27B spike
  (PrismML's own Windows Vulkan binary, pinned by SHA-256, two days at most)
  runs after the engine check.
- **Thinking.** On or off by default per model, decided by the time to the
  first visible word measured both ways. The 120-second limit to the first
  word counts thinking as activity. Continue is tested on every new default;
  where a prefilled reply fails with thinking on, thinking is off for the
  Continue step on that model, written in its catalog entry. **While the
  model thinks and no answer text has come, the answer's place shows the
  three nodes of the localSpace mark pulsing in sequence, "Thinking…" and the
  seconds elapsed;** before any thinking has come, "Starting…". It gives way
  to the answer at the first visible word; clicking it opens the thinking
  text, closed by default; Stop works during thinking as during an answer;
  it is per chat, so a chat thinking in the background shows it on return.
  **CSS only, no script-driven animation loop**, since the processor is busy
  running the model; under the system's reduced-motion setting, a still mark
  and the text.
- **The message script gains five Russian and five Uzbek messages**, the
  Uzbek in Latin and in Cyrillic letters, read for every new default. A model
  that answers well in English and badly in Uzbek is not a default; the
  worst answer per model is reported.
- **Images are not built in this item; they are estimated with it:** pasting
  with Ctrl+V, dragging in, and an attach button for image files; the picture
  shown in the chat and kept with the conversation; the planner counting the
  memory of the model's vision file, and the verdict changing when the model
  no longer fits; on a model that cannot read images, the attach control
  saying so in plain words, never a failure after sending; in organisation
  mode, the image going to the company's server and nowhere else.
- **The builder's reply before building:** what the engine check found; which
  official Qwen3.5 and Qwen3.8 sizes exist, with their licences, corrected
  against the document's table; an estimate split into the pin move if
  needed, the new entries with their measurements, thinking with its
  indicator, and the language messages; and the estimate for images with it.

*This day's business entries are in the internal log.*

## 2026-10-06, installers built on GitHub again; the word rule and the message-script record stay public

The owner's answers after the switch to public.

- **The installers testers get are built on GitHub again**: the `package`
  workflow, started by hand or by a `v*` tag, never on a timer or per push,
  keeping what it builds one day (`docs/BUILD.md`). The repository is
  public, so its Windows runner costs nothing. The rule of 2026-09-24 that
  installers are built on this laptop until October is withdrawn. It stays
  true that **the file the release check ran on is the file testers get**.
- **No test in the public repository may depend on a file in the private
  one.** `docs/test-a/MESSAGE-SCRIPT.md` did not move: the test that holds
  every catalog model said to have been through the message script to a
  section of that record reads it where it is.
- **The rule on which words members may see is product behaviour and stays
  public.** It lives in this log: Directive 5 of 2026-09-12 (the words never
  shown and what to say instead, models by their human names with the id
  under Advanced, an error as one plain sentence and what to do, every empty
  state written) and that day's answers on the shell ("Offline", not
  "Air-gapped", with `airgapped` kept in the configuration and the
  administrator's popover). One part was written only in the Pilot 1 plan,
  now private, and is recorded here: **the administrator's pages and
  Advanced are exempt.** Directive 5 has a check in CI keep the rule; no
  such check exists yet, so the rule is kept by hand.

## 2026-09-25, the repository is public: `ci` on every push again, WSL dropped, business decisions kept privately

The owner's instructions once the repository was switched to public.

- **`ci` runs on every push and pull request again**, as before 2026-09-24:
  GitHub's runners cost nothing for a public repository. It runs once by
  hand on the head as well, reported.
- **WSL is dropped**: the founder is not installing Ubuntu. The rule of
  2026-09-24 that `scripts/check.sh` runs in WSL before every push, and that
  nothing is pushed that it fails on, is withdrawn. `scripts/check.sh` and
  `scripts/wsl-setup.sh` stay in the repository for later; no push waits on
  them.
- **The rule is again: nothing reaches a tester until `ci` is green on that
  exact commit and the release check has passed.**
- **Business decisions go to a private log from now on**,
  `LinusIp/localSpace-internal` (the split proposed before the switch,
  approved as proposed): plans and phases, estimates and dates, pilots and
  customers, the market, and the tests with people and their results. This
  log's business entries moved there: the organisation layer's plan of
  2026-09-24 whole, and the business points of fourteen other days, each of
  which now ends with a line saying so. With them went
  `docs/AFTER-TEST-A.md`, `docs/TEST-A-TESTER-SHEET.md`, the runbook, shot
  list and first-run picture of `docs/test-a/`, and `docs/PILOT-1.md`.
  `docs/prompt/` and `CLAUDE.md` stay public, and the deployment guide no
  longer says when Part A's laptops were tested. **`docs/test-a/MESSAGE-SCRIPT.md`
  stays until the owner decides**: a test reads it, the one that holds every
  catalog model said to have been through the message script to a section
  of that record.

## 2026-09-25, before the repository is made public: the answers to the scan

The owner's answers to the scan before the switch. The scan found no secret
in any commit, commit message or Actions log. The founder makes the
repository public only once items 1 to 4 and 8 below are done.

- **The crates point at `LICENSE`** ("Copyright 2026 localSpace. All rights
  reserved."): `license-file = "LICENSE"` and `publish = false` for the
  workspace, instead of `MIT OR Apache-2.0`, and the landing page no longer
  says "MIT or Apache-2.0". Such a line in a public repository could be read
  as granting an open-source licence.
- **No account name in the files**: a generic path in `docs/BUILD.md`, and
  the app screenshots cropped. What is already in the history stays, which
  is accepted because it is only an account name: no force-push. The v1
  test database, with a temporary path in it, stays as it is; so does the
  author name on the commits.
- **The example.test people** (Anna Karimova, Bek Yusupov, Vera Sultanova)
  were invented for the tests and stay; a real person's name would be
  replaced.
- **No installers are left on GitHub.** The 18 Windows packages among the
  Actions artifacts are deleted before the switch: they are unsigned, and
  the older ones carry the language fault. The newest, `cecb5b74`, is kept
  on the build machine first, for comparisons. The package workflow now
  keeps its upload one day, to be downloaded here, and anything public goes
  out only as a GitHub Release when the founder decides. The benchmark files
  and the server logs stay.
- **Internal documents.** What is in the history becomes public, and that is
  accepted. From now on internal strategy goes to a new private repository,
  `LinusIp/localSpace-internal`: the decision log's business entries, pilot
  and customer plans, the target market, and the results of Tests A and B.
  Technical documents (`BUILD.md`, the specs, the technical decisions) stay
  public. The split is proposed in one short list before anything moves.
- **After the switch `ci` runs on every push again**, free for a public
  repository, and runs once by hand on the head, reported.

## 2026-09-24, until October every check runs on this machine; the answers on the prompt item (document 25)

The instructions that came with document 25, and the founder's answers on the
prompt item (`25-localspace-answers-prompt-item.md`).

**Until the runner allowance resets in October, nothing runs on GitHub's
computers.**
- `ci` runs only when started by hand; `package` only by hand or on a version
  tag. Every commit is still pushed, as a backup.
- **WSL with Ubuntu on this laptop** (approved; the owner does the install and
  the restart). `scripts/check.sh` runs exactly what `ci` runs (formatting,
  clippy, every test, the web checks, the browser walks, the message script)
  inside it **before every push, and nothing is pushed that it fails on**;
  every report carries the commit and its result line. Its first run catches
  up on the commits no CI has seen, and what clippy finds is fixed.
- **Installers are built on this laptop, only when a release is being
  prepared.** The file the release check runs on is the file testers get, and
  its SHA-256 is in the report.
- **In October** `ci` runs once by hand on the head, as a check from a clean
  computer, and whether automatic runs come back is decided with the owner.
- Standing from document 24: nothing reaches testers until CI is green on the
  head and the release check passes.

**The prompt item:**
- **The message script runs against the test engine**, as a `ci` job on the
  Linux runner and in `scripts/check.sh`: it judges nothing, but every message
  must be answered and Continue must carry through, which would have caught
  1.6 and 1.7 breaking the script with no test failing.
- **How the engine splits `-c` between its slots joins the pin-move list**,
  beside Continue and the catalog rerun: it is engine behaviour too.
- **With nothing installed from the Store, Core's own tools go too**: no
  tools, no `find_capability`, no ledger. Someone who installs nothing gets
  plain chat, which pays for no tools, and it is the most direct fix for the
  tool talk. Before the ledger goes, whatever reads it besides tool use is
  found and reported.
- **What is lost is measured, not guessed**: the message script asks "Draw
  me a picture of a cat." With nothing installed, a model that says it cannot
  make pictures is fine: the Store is where people find tools, not the chat.
  One that pretends (a fake image link, a description passed off as a
  picture) or writes tool calls into its text gets one sentence in the
  system prompt, *"You reply in text only. Tools such as Pictures can be
  added from the Store."*, and runs again, both results reported. Anything
  else is reported for the founder to decide. The sentence is not added in
  advance: the last line added about tools is what caused the talk.
- **The ledger's place is chosen per model by testing its template.** Shape
  1: a system message after the newest message. Shape 2: added to the end of
  the newest message, for templates that refuse shape 1 (Ministral 3 14B, a
  Mistral template, is one of document 18's new defaults, so shape 2 is part
  of this item); there doc 22's test becomes "the prompts match up to the
  start of the previous message", one exchange reprocessed a turn. When a
  model loads, a three-message conversation with shape 1 is rendered through
  its own template; if that fails, or the system message is missing from the
  output, shape 2; the choice is logged. A test beats a list of names:
  families change their templates between versions. With nothing installed
  there is no ledger, and no choice to make.
- **The answer that carries on across a workspace switch goes with 1.12.**
  Until then the *Stay* / *Switch* warning is the rule.
- **N is `--parallel N -c N×C`, and `--parallel 1` is passed on a person's
  own computer.** C is the per-chat context the planner promises and its
  offload counts N×C of cache: one calculation, so the words on screen and
  the engine's flags cannot disagree. `--kv-unified-per-slot` is measured on
  the DGX during Test B, the same load with and without, the time to the
  first word reported: when tools are installed every prompt starts with the
  same long tool section. No switch without a number.
- **Next**: the "before" runs with the picture message, and one line of
  counts (layout echo, tool talk, pretending to make a picture) for each of
  the four defaults before anything changes.

*This day's business entries are in the internal log.*

## 2026-09-24, the answers after 1.6 and 1.7: CI while billing is down, two rules for Continue, and the eight questions

The founder's answers to the report on 1.6 and 1.7
(`24-localspace-answers-after-concurrency.md`).

- **While GitHub starts no jobs** (the account's billing, the founder's to
  fix): work goes on and is pushed, but **nothing reaches a tester until CI
  is green on that exact commit and a package built from it has passed the
  release check**; no build after `354c26f` does today. When billing is
  back, first: CI again on the head, whatever clippy then finds in the server
  and client crates fixed, a package of the head, and the release check again
  with Continue in it, reported one line per item.
- **The Windows package is built by hand, for a check or a release being
  prepared, and on a version tag**; never on a timer or per push. It is the
  most expensive job there is, and what tipped the account over.
- **Moving the engine's pin reruns Continue** on every catalog default,
  beside the rerun of the catalog: that the engine says the handed words
  again at the start of its stream is its behaviour today, not a promise.
- **The repeated words are dropped only when the stream begins with exactly
  them.** Otherwise nothing is dropped, and one line in the log says the
  engine did not repeat them. Never by length alone: a wrong guess would cut
  real words out of someone's answer with nobody noticing.
- **N on a server is the chat worker's `max_batch`**, on three conditions.
  The planner counts all N chats: the 1.3 layer offload includes N times the
  per-chat cache, or a server told to write 4 answers at once spills or
  fails on a machine the planner said fits. How b10869 divides `-c` between
  its slots (evenly, or one shared pool) is found out, and `-c` is set so
  that each chat gets at least the context the planner promises; the
  per-chat context is logged at start, and a start that would leave a chat
  less than a real prompt needs is refused, saying why (the whiteboard took
  a prompt to 3,546 tokens in Test A). Defaults: 1 on a person's computer;
  on a server what the administrator sets, and 1 if nothing is set. **Test
  B's server is set to 2**, so that the test covers two answers at once.
- **When every slot is busy** the chat says *"The server is answering other
  people right now. This one will start by itself."* ("server", not "model":
  the person has no choice to make about the model here, and it happens only
  in organisation mode). The spec's position in the queue comes later, with
  the queue.
- **The prompt as a system message and turns is done with the ledger after
  the conversation, as one item**, reversing document 22 (turns as turns in
  Phase 4): the layout echoed back and "no tool is installed…" opening most
  answers are the most visible faults, and Continue already hands the engine
  a reply turn on top of a conversation that is still one block. Conditions:
  with no tools installed the prompt says nothing about tools, no "no tool is
  installed" line at all (document 20: a tool costs something only while in
  use); two consecutive turns' prompts match up to the new message (document
  22's test); the message script runs on every catalog default before and
  after, the answers are read, and the answers that echo the layout or open
  with tool talk are counted before and after. It is estimated first; over
  three days, the founder decides whether to split it.
- **An engine left running by a Core that died is stopped at the next
  start, precisely**, in the 1.3 batch. Never "any engine from our folder":
  the desktop app and a `localspace serve` may share one install. When Core
  starts an engine it writes, in its data folder, its own process ID, the
  engine's, and the engine's start time; at the next start, if that file
  names a Core no longer running, and the engine is still running from our
  engine's path with that start time, it is stopped and one line logged;
  anything else is left alone. **A Windows job object follows**, approved
  because `windows-sys` is already in `Cargo.lock` (0.61.2, through tokio,
  Tauri and wasmtime) and that version is used: it covers a crash when it
  happens; the file stays, for a power cut.
- **A second message while an answer is written stays refused, without
  feeling like a refusal**: the send button is Stop meanwhile, the text box
  stays editable, and what is typed there is never lost.
- **Declining an approval still ends the answer**, because small models
  propose the same change again when told to carry on. The chat says *"You
  declined this change. The answer stopped here."*, and the declined
  proposal is part of what the model reads next, marked as declined.
- **A workspace switch still stops a running answer, for now, never
  silently**: before the switch, *"An answer is being written in [chat
  name]. Switching will stop it."* with *Switch* and *Stay*. Test A asked to
  use other chats while an answer runs and to let it run in the background,
  so this is temporary: what it takes for the answer to carry on and land in
  its own chat is estimated, and a day or less goes into the 1.3 batch.
- **Continue keeps handing over the stopped words, with no tool offered.**
  When the catalog refresh lands, Continue is run on each new default: a
  model that thinks first (Qwen3.5) may have a template that refuses a
  prefilled reply, or start a reasoning block mid-sentence; for such a model
  thinking is off for the Continue step, written in its catalog entry. Never
  back to the line asking the model to continue.

**Built** (the same day; GitHub still started no jobs, so none of it has
been through CI):
- **The package workflow runs by hand and on a version tag only.** It also
  ran every day at 01:30 UTC, a schedule added to keep its cache warm until
  the laptop test: twenty runs since 2026-09-18, six of them by the
  schedule (those of 21, 22 and 23 September rebuilt the same commit,
  `d9bcb17`), fourteen by hand, 15 to 17 minutes each on a runner that bills
  double. The cost of removing it: a first run after a week without one
  starts cold (81 minutes on 2026-09-18). `ci` runs on Linux only, on every
  push and pull request.
- **The handed words are dropped only on an exact match** (`model::SaidAgain`):
  what was held back while it could still be them is passed on however the
  stream ends, and when the engine does not repeat them `app.log` says
  "continue: the engine did not begin its reply with the N characters it was
  handed; nothing was dropped", with the count and never the words.
- **The message script works again, and ends with Continue.** Since 1.6 and
  1.7 a message returns before its answer, so the script would have written
  "no reply" for every message; it now waits for each answer to end, and for
  its model to be found and checked on a fresh data folder before starting
  it. Its last step stops an answer after thirty words, read from the event
  stream, carries it on and writes the join down; `docs/BUILD.md` lists what
  runs when the pin moves. With Qwen2.5 1.5B: "…or in a dangerous area" +
  "of the sea, where it can be seen from a distance", and no line in the log.
- **A declined change** is a tool outcome of its own, `Declined`, carrying
  the question answered ("denied" stays a refusal by permissions). Core marks
  the proposal's waiting record declined in place and ends the answer as
  stopped; the model reads "<- declined by the person, so not made: …"; the
  chat shows that record, alone of the tool records, as the ruled sentence.
- **The words when every slot is busy** are in the chat.
- **What is typed stays with its chat**, kept in the session: the box was
  editable and Enter left the words in it, but they were lost on going to
  Settings and back and followed the person into another chat. Checked in
  the app with a real model: typed while an answer was written, Enter sent
  nothing, the words survived Settings, another chat and the answer's end,
  and were sent after it.
- **A workspace switch asks first, then stops the answers at once.** Found:
  the switch did not stop an answer, as the report of 1.6 and 1.7 said.
  Core looked for it only between steps, so the step being written went on
  to its end in its own chat, and only an answer with another step was
  stopped. Core now stops the person's answers in the workspace they leave
  when they leave it, at Stop's safe points, and says so once.
- **How b10869 divides `-c`** (measured on this laptop, `/props` and
  `/slots`): with no `--parallel`, which is what Core passes today, the
  engine runs 4 slots sharing one pool of `-c` (8,192): one answer may use
  all of it, but answers at once share it. `--parallel 2` splits `-c`
  evenly (4,096 each); `--parallel 2 --kv-unified` shares one pool of 8,192
  again; `--kv-unified-per-slot C` sizes a shared pool to N×C with each
  slot capped at C. So N chats promised C tokens each are `--parallel N`
  with `-c` = N×C (or `--kv-unified-per-slot C`), and the planner counts N×C.
- **`windows-sys` 0.61.2** is in `Cargo.lock`, through tokio (mio), Tauri
  (dirs-sys), wasmtime-wasi and eframe: the job object is approved.

*This day's business entries are in the internal log.*

## 2026-09-23, the plan for 1.6 and 1.7 approved, with additions; a rule for release builds

The founder's answer to the plan for Stop and chats that stay usable
(`23-localspace-concurrency-plan-approval.md`). The plan stands as proposed:
a turn becomes work beside Core's queue instead of one request that holds it;
each answer carries the person and the chat it belongs to; at most one answer
per person, up to N people at once where the engine has N slots (N is 1 on a
person's own computer), at half a day more than hard-coding one; Stop and the
silence limits through a stream reader of our own instead of `ureq` for the
model's answer.

- **Every release build runs at least once on a Windows that speaks another
  language before it ships**: the development laptop, which answers in
  Russian. It stands in `docs/BUILD.md`'s list beside the dry run and the
  Smart App Control run. Earned by the language fault of 2026-09-23: the
  build Test A shipped told such a Windows it had 1 GB of system memory and
  offered only the 1.5B, "the worst failure this product can have".
- **Stop takes effect at a safe point, never in the middle of a change.**
  During streaming, the connection is closed at once and what arrived is
  kept. While a tool call runs in Core's queue, the call finishes completely
  and the turn stops before the next model step. While a turn waits on an
  approval, the pending approval is cancelled, the call it proposed is not
  made, and the person is told in plain words.
- **A stopped answer is kept**: it stays in the chat, marked as stopped, and
  it is part of what the model reads on the next turn, so that "go on from
  there" works.
- **The stream reader is tested for what `ureq` handled quietly**: a
  multi-byte character whose bytes arrive in separate reads (bytes are
  buffered and only complete characters decoded: Russian and Uzbek text
  splits them constantly), an event split across chunks and several events
  in one chunk, the engine going away mid-answer (treated as a stop: what
  came is kept, "The answer stopped here." with *Continue*), and the silence
  limits firing before the first word and between words, each logged with
  the prompt's length. The rest of the test list stands.
- **Answers to the three questions.** The busy first start goes with 1.3
  (document 22 said both; both depend on the same memory figure). A person's
  second chat waits even with a free slot: the ledger is one person's, and on
  a shared server a slot is someone else's turn. The words for a chat that
  waits: **"Waiting for the answer in your other chat to finish. This one
  will start by itself."** (without the second sentence people send the
  message again). Waiting because other people fill every slot of a server
  is a different case, the spec's "server busy" queue, which comes later and
  does not reuse this sentence.
- **For the 1.3 batch**: when a newly chosen model fails to load, the
  message offers **"Go back to [the previous model]"** in one click; and when
  the only graphics listed are integrated, the look asks the engine once
  more after about two seconds before planning, so that a discrete card that
  was asleep is not missed, keeping the log line of what was listed.
- **The deployment spec's `[scheduler] per_user_concurrent`**, whose example
  is 2, is 1 by this ruling until the scheduler's settings are built.

**Built** (the same day):
- **1.6 and 1.7: an answer is written beside Core's queue.** A message
  starts its answer and returns at once (`crate::turns`, `crate::agent`). The
  model is read on a thread of its own; its words, its tool calls and where it
  stands go out as events that name their chat (`TurnChanged`: waits for
  another chat, waits for the model, writing, awaits approval, done, stopped,
  cut); what it asks of tools comes back through Core's queue one call at a
  time. One answer at a time for a person; as many people at once as
  `Config::slots`, which is 1, until the server's setting is decided. Stop
  takes effect at the safe points ruled: the model's step ends within a fifth
  of a second and keeps what came; a tool call at work finishes and the next
  is not begun; an approval waited on is withdrawn (`ApprovalWithdrawn`), its
  call not made, and the person reads "Stopped. The change that waited for
  your approval was not made." A stopped answer stays in its chat, marked
  (`ChatMessage::stopped`), and the model reads it with "[the answer stopped
  here]"; *Continue* hands the stopped words to the engine as the start of its
  reply, which the model carries on (llama-server's prefill; no tool is
  offered for that step), and what it adds joins the answer. The window keeps
  each chat's answer apart: Stop for the chat on screen, "Waiting for the
  answer in your other chat to finish. This one will start by itself.", "The
  answer stopped here." with *Continue* under the chat's last answer when
  nothing is in progress, a spinner beside each chat in the list whose answer
  is being written or waits its turn, and the answers in progress asked for
  again whenever the event stream comes back. A server whose every slot is
  taken shows the spinner alone until the words for that case are decided.
- **1.2, inside it: silences instead of totals.** 120 s with no first word,
  60 s between words, on a stream reader of our own (`crate::stream`:
  HTTP/1.1, chunked or not, server-sent events read from buffered bytes),
  tested for a letter split between two reads, an event split across chunks
  and several in one, the engine going away, both silences and Stop. An
  answer cut short keeps what came and ends as "cut", "The answer stopped
  here." and *Continue* in the chat, and `app.log` says why, with the
  prompt's length ("answer: step 0, cut: the model wrote nothing for 60 s
  after its last word, …, prompt of about 4235 tokens"). The server's
  300-second limit on a request no longer applies to answers. A model
  reached over TLS (connected under Advanced) is still read by `ureq`,
  without the total limit.
- **Found while building, and handled.** On Windows a socket shut from
  another thread does not wake a read already waiting on it: the reader
  waits in steps of a fifth of a second and looks at Stop between them. A
  turn must not hold the model router while it writes: stopping or changing
  the engine takes the router for writing and would wait for the answer to
  end, so a turn takes its worker out first. A way back into Core's own
  queue held strongly would keep Core's thread, and so the engine, alive
  after the app closed: it is held weakly, and a test drops the transport
  and sees the engine go. *Continue* was first built as a line asking the
  model to go on from exactly where it stopped: on this laptop Qwen2.5 3B
  began its answer again instead, and the joined text read "…provide the
  information in aIt seems that…". Handed the words as the start of its
  reply, it carries on mid-sentence. The engine says those words again at
  the start of its stream; Core drops them, so they are neither shown nor
  kept twice.
- **The builder's choices, to be corrected if wrong**: a second message in
  a chat whose answer is being written is refused ("An answer is still
  being written in this chat. Stop it, or wait for it to finish."; the
  window does not send one); a declined approval ends the answer; a person
  who moves to another workspace while an answer is written sees it stop,
  "The answer stopped when you moved to another workspace."; evals take
  their turns to the end inside the request, as before.

*This day's business entries are in the internal log.*

## 2026-09-23, after Test A: the plan of documents 16 to 22, and the answers to the estimates

The founder's reading of the laptop test and the plan that follows from it:
`16-localspace-test-a-feedback-plan.md` (the testers' seventeen items,
triaged), `17-localspace-tester-log-findings.md` (the one full tester log,
read line by line), `18-localspace-catalog-refresh.md`,
`19-localspace-image-generation-plan.md`, `20-localspace-tool-ecosystem.md`,
**`21-localspace-agent-prompt-after-test-a.md`, the plan, which replaces
document 3 and the daily answers after it**, and
`22-localspace-answers-to-estimates.md`, the answers to the builder's reply
of the same day (estimates, what needs other machines, where the documents
and the code disagreed, why the app locks during an answer). The
specifications stay the source of truth for the architecture; document 21
says what is built next, in what order, under which rules.

**Standing rules added** (document 21 §2); every earlier rule stands:
- **Installing a tool costs something only while that tool is in use.**
- **A slow model is shown as information, not as a warning**: the tester
  chose the slow 14B on purpose, for better answers.
- **The app decides what is on the graphics card, never a tool.**
- Test the release build; read what the model says, not whether a script
  ended; what is sent is what was dry-run.
- The default recommendation offers only models whose licence allows
  commercial use (for an image model: of the model and of its pictures); a
  model is the default only after the message script with its answers read;
  moving the engine's pin is a regression event for every catalog model.

**Phase 1, in this order** (answer 10).
1. **1.1, the restart storm**: starting a model is idempotent while a load is
   in flight; nothing is measured or planned while an engine of ours is
   alive; our own engine is left out of "held by other programs".
2. **1.6 Stop and 1.7 other chats usable, with 1.2 inside them**, one change:
   the Stop button exists and has never stopped anything, because Core does
   one request at a time and a turn is one request (the builder's reading).
   **The worker is built for N answers at once where the engine has N
   slots** (document 22 §2): on a person's computer N is 1 and a message in a
   second chat waits its turn, says so and has its own Stop (answer 1); on a
   server N is the engine's `--parallel`; the spec's queue ("server busy,
   position N") comes later and the worker must not block it. If N costs
   meaningfully more than one, the builder says so before starting. **1.2**:
   idle limits replace the total ones, 120 s with no first word and 60 s
   between words, and every firing is logged with the prompt's length
   (answer 2); the answer that came stays, with "The answer stopped here." and
   a *Continue* link that carries on in the same answer (answer 3).
3. **The task ledger after the conversation** (answer 8), with a test that two
   consecutive turns' prompts match up to the new message. It was bug 3's
   cause: rebuilt every turn, it sat between the part the engine keeps and
   the conversation. Sending turns as turns (4.2) stays in Phase 4, for the
   tool talk that leaks into answers.
4. **1.3 with 1.4**: the estimate for a model that sits mostly in system
   memory, and the card table (the RTX 2050, the RTX 3050 6 GB, the MX series
   and the rest of that end of the range).
5. **1.5**: one look at the computer when the app starts.
6. **1.8 to 1.11**: New chat reuses the empty one; rename and delete from a
   menu on a chat's row, delete asking once; the model list always open (the
   "Hide the models that can be added" toggle is deleted); the line under the
   composer becomes **"localSpace can make mistakes."** (answer 7). With 1.10
   the verdict *Too slow for everyday use* becomes **"Slower than you read"**
   (answer 6), and the verdict is made to stand out in its row if that is a
   change of styling under half a day; otherwise it stays on the design list
   (document 22 §5).
7. The first run and the list say how long a download will take at the speed
   it is getting.
8. **1.12**, last: closing the window quits when nothing is running; while an
   answer or a download runs, localSpace stays in the notification area with
   *Open* and *Quit* (answer 4) and says when it is done with a Windows
   notification: **`tauri-plugin-notification` is approved** (answer 5).

**The busy first start is in Phase 1** (document 22 §5 and answer 9). The log
shows the memory reading going both ways: 11.9 GB/s at an ordinary start on a
laptop that reads about 20, and 32.8 GB/s at a start after which the list
said "at least 10 words a second" and the model gave about 7. A noisy reading
can over-promise, which breaks the honesty rule. It is fixed together with
1.3, since both depend on the same figure.

**Phase 2, the catalog** (document 18), beside Phase 1 where the machine
allows. The defaults proposed: Qwen3.5-4B for 4 GB cards, Qwen3.5-9B for
8 GB, Ministral 3 14B for 16 GB; beside them Ministral 3 3B and 8B,
SmolLM3-3B, Granite 4.2 3B and 8B, Gemma 4 E2B, E4B and 12B and gpt-oss-20b,
and for large machines Qwen3.6-35B-A3B, Gemma 4 26B-A4B, GLM-4.7-Flash,
Qwen3.8-27B and Mistral Small 3.2 24B. Each passes the gate: downloaded,
checked, **loaded on the pinned engine with its answers correct** ("loads"
and "answers correctly" are separate facts), through the message script, its
answers read, `exercised_on` recorded. Whatever needs a newer engine moves the
pin once, and the whole catalog is run again. Order: Qwen3.5-4B and 9B,
Ministral 3, Gemma 4 and Granite 4.2, the large models last. Qwen2.5 0.5B,
1.5B and 3B leave the recommendation once their replacements have passed and
stay in the list. **The CI check of every catalog address comes before the new
entries** (document 22 §5). **The Bonsai spike** (Ternary Bonsai 2 27B), after
Phase 1, two days at most: PrismML's own Windows Vulkan release binary, pinned
by SHA-256; if there is none, the spike stops and says so. Building the fork
from source is not part of it (document 22 §4).

**Phase 3, Test B** (the earlier items 5 to 7): "Connect to your
organisation" at the first run, where the app looks at no hardware and
downloads no model; TLS on a network without a company authority; a latency
check across a real network; the Linux tarball; the server starting its model
again after a crash or a reboot; two accounts on two machines. **For Test B,
TLS ends at a reverse proxy** in front of `serve` (`tls = "behind-proxy"`,
Core unchanged), and the desktop app's Rust side pins that proxy's
certificate while the window talks to it over loopback, as ruled on
18 September (that day's answers 5 and 6). Browser access is not part of
Test B. TLS inside Core, with the approved `rcgen` and `tokio-rustls`, comes
after it (document 22 §3).

**Built, and found while building:**
- **The look lost its whole answer on a Windows that names itself in another
  language** (found on 2026-09-23 by the first test of 1.1, and fixed first).
  PowerShell answers in the console's code page unless told otherwise; on the
  development laptop, asked with its Russian settings, Windows called itself
  "Майкрософт Windows 11 Домашняя", which is not UTF-8, and the answer was
  thrown away with it: no memory, no free disk, no card in use by others.
  The question now asks for UTF-8, and the answer is read leniently all the
  same, so that one character in another encoding costs that character and
  nothing else. The app's own logs on that laptop show the English name, so
  it did not strike there; it would have on any Windows that names itself
  in letters beyond plain ASCII, a Russian one among them.
- **1.1, the restart storm.** What the log shows: the 7B, just downloaded,
  took half a minute to start on that laptop (its warm-up alone was still
  running after 25 s), and six starts followed in 45 seconds, each ending
  the one before; the fourth was planned with the engine before it still on
  the card, its 2,999 MiB counted as another program's, and got 1 layer of
  29. The row of a model in Settings → Assistant is the one control that
  could ask for it again while it started (the first run's button waits,
  and the window restarts the last model once, as it opens). Now: **asking
  for the model that starts or runs starts nothing new**, and the log says
  it was asked for again; **the engine is stopped before the card is looked
  at**, and Core waits until Windows no longer shows its graphics memory,
  five seconds at most, the log saying how long it took; **an engine of ours
  that still holds some after that is left out of what other programs
  hold**, by its process, under the card's own LUID; and **the row waits
  while its model starts**. One consequence, accepted: choosing another
  model stops the one in use before the card is looked at, so if the new one
  then does not fit the memory as it is found, nothing runs, where the old
  one used to go on. Tests: the same model asked for three times, twice
  while it starts and once while it runs, is started once; another model is
  a new start, planned only after the engine before it has stopped, whose
  port then answers nothing; what our engine holds is never another
  program's, with the tester's figures; the look still answers everything
  when processes of ours are named.

*This day's business entries are in the internal log.*

## 2026-09-20, Sunday's answers: the squeezed mark is fixed before the dry run, two pictures on one page agree, and two findings are written down

The user's answers to the report on the landing pictures
(`15-localspace-answers-sunday.md`).

- **1. The assistant's round mark was squeezed to an oval beside every long
  answer, and it is fixed before the dry run** (architecture v2 §6; the UI
  reference of 2026-09-13). Seen on the landing pictures taken from the
  release build `05b4d6de`: 18 × 33 px where 33 × 33 is meant, because
  `.avatar` was allowed to shrink and an answer's long paragraph shrank it.
  The builder asked when: now, between the dry run and Monday's build, or
  after the test. Ruled: **now, if the dry run has not begun** (the user
  confirmed on the day that it had not), "through the full gate — commit,
  CI, package build — and the dry run uses that artefact. Not a hand-patched
  folder." Had it begun: after the test, and the report would have said the
  mark is known. Between the dry run and Monday's build nothing changes, not
  even one declaration: **what is sent must be what was dry-run.** And it is
  not called cosmetic: it "appears beside every long answer, in every
  conversation, on every tester's machine", in the main view, and "a visibly
  broken element there costs more trust than any missing feature". No walk
  in CI draws an answer (CI has no model), so the fix is verified by
  measuring the mark in the packaged release build; the gap is on
  `docs/AFTER-TEST-A.md`.
- **2. Two pictures on one page must agree.** The first run and the model
  list stand side by side on the landing page, so they are the pair taken
  in one look, "about 6 to 8 words a second" in both, although that list
  carries the amber "No model — choose one in Settings": "it is the app
  telling a new person what to do next". The speed is measured again at
  every start, which is honest, and on a landing page it "reads as
  sloppiness rather than as honest re-measurement". **Three of four verdicts
  is fine: no machine and no framing is contrived to show all four.**
  Picture 3 is taken again after the fix with a fresh question: a
  mislabelled point in an answer is the model being imperfect, not a
  defect, and still not what goes on the front page.
- **3. The verdict does not stand out in its row**: mid-paragraph, in the
  same grey as the licence and the size of the download, and it is "the
  whole honesty claim". After the test, **first on the design list**
  (`docs/AFTER-TEST-A.md`, due first).
- **4. A busy first start recommends a model one size too small: after the
  test, high priority, and watched on Monday.** Measured on 2026-09-20,
  moments after 4.4 GB had been written to the disk: the memory copied at
  8.2 GB/s, all three samples slow, on a laptop that reads 18 to 21 when
  quiet; the 7B fell under the line and the 1.5B was recommended, with the
  7B already on the disk; a restart on the quiet machine read 20.2 and
  recommended the 7B. The measurement is not touched on the day of a dry
  run. It is not to be called erring on the safe side: **"safe on speed is
  unsafe on answer quality"**. The 1.5B gets arithmetic wrong and invents
  facts by our own message script, and that is what moved the line from 15
  to 10 tokens a second. Nor is it exotic: a tester's first run comes
  moments after an installer wrote its files, on a gaming laptop with a
  launcher and a browser open. The runbook's table for reading `app.log`
  says what a low "copied at" means, so that whoever reads ten logs on
  Monday recognises it. **Nothing in the tester sheet asks a person to
  close other programs first**: that "makes the software feel fragile, and
  it is our problem to solve, not theirs".

*This day's business entries are in the internal log.*

## 2026-09-20, the rulings on the eight dead ends, Smart App Control stays on, and who takes the pictures

The user's rulings on the walk of the shell (`14-localspace-deadend-rulings.md`),
under the rule of 2026-09-19: nothing is visible unless it works.

- **1. "Sign out" is removed from a personal install**, from Settings →
  General and from the account menu: there are no accounts there, and it led
  to a page asking for a token a person cannot obtain. "Not a dead end, a
  locked door." An organisation's members keep it.
- **2. "Attach a file" is removed.** Disabled with a tooltip is the shape the
  rule forbids: the person sees a capability, reaches for it, and is told to
  come back later.
- **3. "Documents" is not in the rail until a first document exists**, and
  the sentence promising that uploading your own files comes with document
  search is deleted: hiding the entry while leaving the promise inside it
  moves the dead end somewhere harder to find.
- **4. The network has two true choices, and the pill follows** (plugin spec
  §8.1): *Offline — nothing is downloaded either* and *Online — only to
  download a model you ask for*. The three-mode pill was specified for a
  product with web search, and no web tool exists (2026-09-19). **A
  deferral, not a reversal: the third mode returns when web tools do**, and
  the spec says so. Whatever else: "Your administrator has not allowed
  this" never appears on a personal computer, where there is no
  administrator, and nothing says that the assistant asks before a request
  to the internet, which it cannot make.
- **5. The "Too large" button is dropped**: the row already says it will not
  fit. **6. With the computer set to stay offline, the sentence stands alone**
  where the Download buttons were. **7. The empty state of Settings → Tools
  stays**: honest, explains itself, and its link works. **8. The link to
  `/api/v1/openapi.json` is removed from the app**: a raw API surface is
  what a member is never supposed to see.
- **Smart App Control on the development machine stays on**, reversing the
  answer of the night before, on that night's evidence: it did not refuse
  our unsigned installer, and it released `rustc` by itself after two hours.
  What is left is a setting that now and then costs a rebuild, against an
  irreversible change to the founder's machine; and it is the only machine
  that tells us anything about how it behaves. CI is used when it
  interferes.

*This day's business entries are in the internal log.*

## 2026-09-19, two gaps found in the pictures: a download could not be stopped, and the shell offers what does not exist

The user's note on the first pictures (`13-localspace-ui-gaps.md`).

- **A download can be stopped, and a model can be deleted** (architecture v2
  §4.4). *Stop* on a download under way keeps what came: the entry reads
  "N% is already here" with *Continue*, as after a lost connection. *Delete*
  on a model that is here and on a part a download stopped in asks once, in
  plain words, naming the model and the space it frees, and the entry is back
  to *Download*; a model in use is stopped first, and the question says so; a
  file the person brought in themselves is never deleted. The buttons stand
  where *Download* stands, not in a menu. In the API: `stop_download` and
  `delete_model`, an administrator's in an organisation like `download_model`
  (the builder's names).
- **One download at a time.** Two at once on a home connection halve both and
  each looks broken. A second one waits its turn, says so ("Waiting for the
  download before it to finish"), can be stopped while it waits, and begins
  when the one before it has ended, however it ended. The stage is `queued`.
- **The drive's room is said in the list, before a download can begin**, as it
  already was at the first run: an entry the models' drive has no room for
  (what is still to fetch, and the gigabyte that is always kept free) offers
  no *Download* and says "It needs 8.4 GB and drive C: has 3.1 GB free: make
  room there first." The refusal at the click stays behind it. The drive is
  looked at no more than once in five seconds: the look costs most of a
  second on Windows, and the list is asked for often.
- **The rule: nothing is visible unless it works.** Not disabled with a
  tooltip, not greyed and mysterious: either it does the thing, or it is not
  on the screen. The one exception is a control whose absence would confuse
  more than its presence, and that one carries a plain sentence saying when
  it will work, never a code and never "coming soon" on its own. **The whole
  shell was walked on a fresh install of the release build and every control
  that leads nowhere was listed for the user; nothing of it is changed until
  the list is agreed.**

## 2026-09-19, the answers of Saturday night, and what the first pictures showed

The user's answers to the night's report (`12-localspace-answers-saturday.md`),
with what was found while taking the pictures they asked for.

- **Every build tested by hand had been a debug build; what ten strangers run
  is a release build, and the two are proven to behave differently here.** So
  the dry run uses the release artefact, and the message script runs against
  the installed release build. **Asked for and done: where else could this
  class of bug hide?** Nowhere: the memory's copy rate was the only place in
  the product where a measured result depends on work an optimiser could
  remove or reorder. Everything else that is timed waits on real input and
  output or on another process (the engine's start, a download, an answer);
  `localspace bench` times calls that have side effects, so they cannot be
  removed, and nothing is decided from it.
- **The pictures for the landing page come from the release build**: the
  portable copy of the package, serving a fresh data folder under a neutral
  name, at the app's own 1440 by 900, in the light theme.
- **AMD machines are the least-validated path in the product**, and their
  logs are read first on Monday (`docs/test-a/RUNBOOK.md`). AMD publishes only
  "up to" figures; they stay as entered, under the estimate's efficiency.
- **Smart App Control on the development machine: the user turns it off**,
  once the founder confirms (it cannot be turned on again without
  reinstalling Windows). The builder never changes it. **What was seen that
  night with it on (state 1):** it refused `rustc.exe` for about two hours
  and then let it run again; and it blocked **neither the unsigned installer
  nor the unsigned app**: the installer opened, was clicked through by the
  person at the machine, and the app ran. Its verdicts are not uniform, so the
  rule for testers stands as ruled, and the block's dialog could not be
  recorded from our installer on this machine.
- **The verdicts' effective lines are accepted as built**: about 9 tokens a
  second for "runs well" and 5.3 for "works", read off the numbers shown.
  Monday's calibration starts from these, not from the 10 and 5 first ruled.
  An unknown card promised the processor's floor errs on the right side:
  faster than promised, and the reliable model.
- **Found while taking the pictures, and fixed:** "connected to localSpace"
  never went away on a page that stood still: a notice's age was looked at
  only when something else redrew the window. It now goes after its eight
  seconds by itself.

*This day's business entries are in the internal log.*

## 2026-09-19, the answers after day 4 and their addendum: the laptop test is on Monday 21 September, and nobody stands beside the testers

The user's answers to the second evening report of 2026-09-19
(`8-localspace-answers-day4.md`) and the addendum that came with them
(`9-localspace-testA-remote-addendum.md`).

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
- **Ready for the certificate:** `scripts/package.mjs` signs every
  executable and library of the package, the engine's and the command
  line's too, when `LOCALSPACE_SIGN_COMMAND` says how one file is signed
  (JSON, in the form tauri's `signCommand` takes), and hands tauri the same
  command for the app, the installer and the uninstaller; the `package`
  workflow rehearses it with a throwaway certificate made on the runner
  (`-f sign=rehearsal`) and refuses a build in which one installed program
  file is not signed by it. Which provider the certificate comes from
  decides the command and what the runner needs; that is the user's to say.

*This day's business entries are in the internal log.*

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
- **A. The verdicts say nothing about quality, and one sentence will**: the
  tiny band carries "Small models answer quickly but get things wrong more
  often.", wherever such a model is recommended or listed. Not a second
  scale: one sentence, attached to the band.

*This day's business entries are in the internal log.*

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

*This day's business entries are in the internal log.*

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

*This day's business entries are in the internal log.*

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
9. **Dates:** in the internal log.

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

**The deployment guide** is `docs/DEPLOYMENT-GUIDE.md`, the user's document,
and it is the **target state**, not a description of today: the list of
mismatches at its head is the definition of the gap. The guide follows the
build: no command is implemented merely because the guide names it, and when
an item lands the guide changes in the same commit. It supersedes the draft
in `docs/PILOT-1.md` §9. One correction to the instructions: no
`[[models.worker]]` entry exists; what exists, and what Test B uses, is an
administrator pointing Core at an external endpoint at runtime.

*This day's business entries are in the internal log.*

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

*This day's business entries are in the internal log.*

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

*This day's business entries are in the internal log.*

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
