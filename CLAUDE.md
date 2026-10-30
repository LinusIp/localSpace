# CLAUDE.md: localSpace

You are the coding agent for **localSpace**: private AI that runs on your own computer. This file holds the architecture, the working rules and the engineering practice for this repository. The founder, Abdulloh Shavkatjonov (GitHub: LinusIp), decides product, scope and anything outside this file. He isn't a full-time engineer, so explain things to him in plain, short words.

This repository is visible on GitHub, all rights reserved. No open-source licence has been decided.

---

## 1. Product architecture (current stack; keep it)

- **Core:** Rust. The single authority for permissions, policy, the engine interface, storage, the compacter, tools and plugins.
- **Desktop shell:** Tauri with WebView2. The web client is TypeScript.
- **Engine process:** `llama-server` (Vulkan), as a sidecar.
- **Personal mode:** the app talks to Core over loopback only, exposed to nothing else.
- **Memory and context rules already decided:**
  - load with `--load-mode none --no-host`;
  - desktop `-np 1`, `--cache-ram 256`, `--ctx-checkpoints 4`;
  - a database page cache of 64 MiB;
  - 8,192 tokens of context per chat;
  - a chat's messages ≤ context − answer room − system part (about 6,000 tokens on a laptop for a plain chat; less with thinking on or tools on offer);
  - big-step trimming to half of that, with a divider;
  - a picture counts as 400 tokens.
- **Default-model rule:** model system part + 2.4 GB, + 3 GB for other programs, + 3 GB free. That gives the 4B on 16 GB with a 4 GB card, and the 9B elsewhere.
- **Pictures (P5):**
  - load the vision part on the GPU only if all layers fit, plus 256 MiB;
  - otherwise in normal memory if there's room (1.5 GB margin);
  - otherwise leave it out, with a "Try again" button;
  - never suggest a bigger model.
- **Every new feature is behind a switch, off by default.** Previews turn on their own features, and CI tests with all switches on and all off.

---

## 2. Working rules

**Honesty and wording**
- Real numbers only, measured or clearly marked as estimates. No invented quotes, testimonials, customer names or compliance claims.
- Never write that a feature works if it doesn't yet. "Coming soon" features stay labelled.
- The word "Claude" never appears in user-facing material: the app, the site, release notes or docs for users. Commits and this file are fine.
- Never tell anyone to turn off Smart App Control or other security features.

**Privacy and security**
- No telemetry, ever. Diagnostics are exported by the user by hand, never sent automatically.
- Never put secrets in code, logs, chat or PRs.
- New dependencies, build tools and services need the founder's yes first.

**Git and CI**
- Work on branches. Every change goes through a PR with green CI. The founder merges anything that touches licences, releases or publishing.
- Keep CI lean:
  - the full browser walk and the Windows build run only on PRs to `main` or `release/*` and on releases;
  - superseded runs are cancelled;
  - document-only changes skip CI.
- Record decisions with the date (section 3.5). Keep plans and this file in step.

**Talking to the founder**
- Plain, short words; tables for schedules; one clear list of the decisions he needs to make, each with your recommendation.
- Before big work, send the plan and a week-by-week table, and wait for his yes.
- When something is blocked or a claim is uncertain, say so plainly. Check the code or primary sources before passing it on.

---

## 3. Engineering practice

### 3.1 Documents, in order

1. This file.
2. For technical detail, the specifications in `docs/`, higher overriding lower:
   1. `docs/localspace-architecture-v2.md`;
   2. `docs/localspace-harness-plugin-spec.md`;
   3. `docs/localspace-organisation-deployment-spec.md`;
   4. `docs/localspace-marketplace-spec.md`.
3. `docs/DECISIONS.md` (3.5).

Before asking, search these for the term. If it's settled there, follow it and cite the section.

**Spec issues.** Implementing a spec section can show it won't work as written: a crate doesn't support what it assumes, two sections can't both hold, or a budget is physically out of reach. Don't work around it silently. Write a **Spec issue** with:
- the section;
- what fails;
- the evidence (an error, a measurement, a link);
- the smallest change that would fix it.

Then wait for the answer.

### 3.2 Coding standards

- **Rust:**
  - edition 2024;
  - `#![deny(unsafe_code)]`, except in isolated, documented modules that need it (GPU, shared memory);
  - `thiserror` for library errors, `anyhow` only in binaries;
  - `tracing` for logs, never `println!`;
  - no `unwrap()` outside tests.
- **TypeScript:** strict mode; no `any`; React function components only, no class components.
- **API types:** `localspace-proto` is the single source. The client's types are generated, never written by hand on either side. `cargo test -p localspace-proto` regenerates `web/src/api/generated/`, and CI fails if the committed files differ.
- **Core holds the trust:** no logic in the client that Core would have to trust. Every call that changes something is permission-checked in Core.
- **Both:** no TODO without an issue reference; no dead code; no commented-out code.

### 3.3 Tests and gates

- **Tests:** unit tests for logic, integration tests for API endpoints and permission checks, and an end-to-end test for each gate.
- **Every permission check gets a negative test** (the forbidden case) for every positive one.
- **A gate that can be measured** (tokens a second, memory, bundle size, latency) becomes a CI check that fails on regression, and the number goes in the report.
- **Never skip or weaken a gate or budget.** If one can't be met, report the measurement and ask; don't raise the limit.
- **Nothing is done** while there are failing tests, a red CI, an unmet gate or an open question.

**Budgets**, kept until the founder drops them:

| Budget | Limit | Where it's checked today |
|---|---|---|
| Base client bundle | ≤ 2 MB gzipped | CI: `web/scripts/check-sizes.mjs` against `web/size-limits.json` |
| Core at idle | ≤ 50 MB private RSS | `localspace bench` and `/metrics` (`crates/localspace-core/src/footprint.rs`); not yet a CI check |
| Shell at idle | ≤ 80 MB JS heap | not measured yet |

### 3.4 Commits and pull requests

- **Conventional commits:** `feat(core): …`, `fix(client): …`.
- **Small, reviewable commits.** Each compiles, passes tests and lint, and does one thing.
- **One PR per task.** Each links the plan or spec section it implements, and its description says:
  - what changed;
  - why (the section);
  - how it was tested;
  - the gate numbers, if any.

### 3.5 Decision logs

- **Technical decisions** about code in this repository go to `docs/DECISIONS.md`: newest first, dated, naming the section they affect. They land in the same commit as the code that uses them.
- **Business decisions** go to the founder's private log. A day with both keeps a one-line pointer in each.

### 3.6 Build, test and release

**Locally**
- Run only the one or two test suites you're working on (`cargo test -p localspace-core --test <suite>`), and leave `cargo test --workspace` to CI.

**Before every push**
```bash
cargo fmt --all -- --check
for crate in harnesses/*-logic; do (cd "$crate" && cargo fmt -- --check); done
# in web/:
npm run typecheck
npm run lint        # oxlint
npm run test
```

Use the scripts in `web/package.json`; never `npx` a tool.

**What CI runs** (`.github/workflows/ci.yml`; a newer push cancels the older run)
- **`check`:**
  - the catalog's download addresses (`scripts/check-catalog-files.mjs`);
  - one version in `Cargo.toml` and `tauri.conf.json` (`scripts/check-version.mjs`);
  - rustfmt, lint and types;
  - the third-party notices (`scripts/third-party-notices.mjs --check`).
- **`web`:** the tests, the build, the bundle sizes and the canvas frame times.
- **`rust`** (Linux): `cargo clippy --workspace --all-targets -- -D warnings --force-warn clippy::unwrap_used`, `cargo test --workspace`, and a check that the generated client types match.
- **`e2e`** (the browser walks) and **`measure`** (the test kit against the test engine): on pull requests, on pushes to `main`, and by hand.

**The test kit** is `localspace measure` (`docs/BUILD.md`, "The measurement kit").

**The Windows package** (`.github/workflows/package.yml`)
- An NSIS installer and a portable zip around the pinned llama.cpp Vulkan engine (`scripts/engine.json`), laid out by `scripts/package.mjs`.
- It's built by hand or on a `v*` tag. Third-party notices are generated into `licences/THIRD-PARTY-NOTICES.txt`.
- Moving the engine pin follows `docs/BUILD.md`, "When the pin moves", measured with the test kit, and needs the founder's yes.

**Releases**
- 0.2.0 is frozen on `release/0.2`. `main` takes later work behind switches.
- A `v*` tag builds the package, and nothing is published by the tag alone. `node scripts/check-version.mjs <tag>` must pass.
- **Publishing is the founder's:** after the release check passes on that package, he runs `package.yml` by hand with `publish_tag`. The publish job checks the files against `SHA256SUMS.txt` and needs `docs/release-notes/<tag>.md`.
- Nothing reaches a tester until `ci` is green on that exact commit and the release check has passed.
