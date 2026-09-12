# Pilot 1 — the plan

Written 2026-09-12, after 6.0 landed, against the scope set that day
(`docs/DECISIONS.md`). Pilot 1 is architecture §13 steps 6 and 7 together,
trimmed to the least that makes a pilot real: one organisation installs the
server on its own Linux machine; employees sign in with a company account or
one an admin made for them; they open the app in a browser, share a
Miro-like canvas, and ask questions over the company's own documents. This
document is the plan for that build: what is in and out, where the code
stands, the work in order with estimates, the install guide a sysadmin
follows alone, and the acceptance list that decides "installed".

## 1. In scope

- **Retrieval end to end:** upload, extract, chunk, embed, hybrid index, the
  ACL pre-filter, citations, `docs.search` — the approved 6.1–6.7, with the
  retrieval eval that marks someone else's homework.
- **Accounts and sessions:** OIDC (Authorization Code + PKCE), and a local
  account provider an admin creates users in, allowed for pilots behind a
  persistent warning banner (deployment §4.1, amended in §11 below).
- **Workspaces** with per-document ACLs and the roles (§4 below).
- **The canvas shared between browsers:** Automerge sync, presence (who is
  here, cursors), and agent edits as proposals to apply or discard
  (deployment §6.2, §6.3).
- **The audit log**, hash-chained, written locally; SIEM export optional.
- **Network modes and the gateway**, with the zero-connection proof: packet
  capture with a mandatory positive control.
- **A Linux install path:** the binary, a systemd unit, TLS or behind-proxy,
  `localspace admin bootstrap`, backup and restore, `doctor`.
- **A real model** rather than the 0.5B: whatever the partner's hardware
  holds by the planner's verdict, the reference class when it fits.

## 2. Out of scope, so nobody expects them

- High availability and a standby (deployment §11.4).
- SCIM and SAML (§4.1, §4.2): just-in-time users from OIDC, groups from the
  claim on every login; local accounts otherwise.
- Ingestion connectors (§6.4): upload only.
- Per-document encryption at rest (§9.1 as amended): the host's volume
  encryption plus a `doctor` check, documented to the partner in those words.
- Tier B native harnesses, and so physics and CAD.
- Marketplace commerce and entitlements: harnesses install from the
  organisation's registry folder.
- The `stream` and `native` surface kinds.
- The admin console as a whole (§13): Pilot 1 has the admin pages it needs
  (users, workspaces, network, audit) and no more.
- Multiple documents per harness per workspace: one board per workspace per
  harness, as today; several boards is a step of its own (question 12).

## 3. Where the code stands

What exists (steps 1–5, 6.0): the generated API and the React shell; the
`llama-server` sidecar with the catalog and the planner; the chat harness
with streaming and conversations; the harness runtime with iframe surfaces;
the whiteboard on the own canvas with Automerge replicas synced through
Core, one sync state per replica; export as typed artifacts with the first
types package; the ACL model checked on read, write, sync and tool calls;
the version DAG with undo, redo and drop-run; a hash-chained audit log; the
gateway with three modes, allow and block lists, quotas and a per-domain
approval for `web.fetch`; proposals as a Core type; a CI workflow that has
not run, because the repository has no remote.

What Pilot 1 needs that is not there, in the order it hurts:

1. **One Core, many identities.** In organisation mode the server keys a
   Core by user, but `user_for` names every caller `operator`, so two
   employees today are one person: one transcript, one ledger, one board,
   one line in the audit log for both. `Core::handle` derives the identity
   from its config; no request carries a user; the session cookie is the
   server's one token with no expiry. Everything multi-user sits on top of
   one shared Core with the identity carried per request, per-user state
   (transcript, conversations, ledger, approvals, focus, pins, the agent
   run, replica sync states) separated from shared state (registry,
   documents, DAG, ACLs, audit, gateway, the one engine), and events fanned
   out per user and per replica — today every socket on a session receives
   every event and the frame drops what is not its own.
2. **Identity.** One token, one user; no accounts, no sessions store, no
   roles, no OIDC.
3. **Persisted ACLs and workspaces.** Both live in memory and reset at start;
   there is one workspace per Core, and no way to edit an ACL.
4. **Retrieval**, all of it (surveyed for the step-6 plan).
5. **Presence** and **proposals**: nothing carries cursors or who is here.
   A proposal is a type Core creates at the end of an agent run in a
   workspace set to `proposal` — and no runtime workspace is, since the
   only one is personal; there is no request to apply or discard one, and
   no page shows one.
6. **The install path:** no `localspace` binary with subcommands, no config
   file, no TLS or forwarded headers, no systemd unit, no backup, no
   `doctor` beyond the desktop crate's, no `admin bootstrap`.
7. **The audit's shape:** written synchronously on Core's thread with every
   record also kept in memory for the life of the process; no IP, the role
   always `member`, the conversation always `c_1`; no rotation; a verify
   function with no command; no export. Conversations are one file per
   data directory with no user in them.
8. **The gateway's gaps** (6.7): the mode is not saved, no search backend
   can be set, `web.search` skips the approval, failures are not audited,
   and nothing is cached as a cited document.

## 4. Roles for Pilot 1

Deployment §4.3 names six roles. Pilot 1 carries three org-wide roles and
the document levels of §6.1:

| Role | Can, in Pilot 1 |
|---|---|
| **Admin** | everything below; create local users and workspaces; set roles; set network policy and the model; read and export the audit log; break-glass into a workspace with a reason, audited |
| **Member** | use the environments of the workspaces they are in; create documents in them; own their personal workspace |
| **Viewer** | read-only in granted workspaces; can chat with documents; the agent has no write tools for them |

Workspace roles are the document levels, inherited from the workspace and
tightened per document: `owner`, `edit`, `comment`, `view`. Catalog admin
and security auditor fold into Admin for the pilot; they return with the
console. With OIDC, roles come from groups named in the configuration; with
local accounts, the admin sets them.

## 5. The work, in phases

Four phases, each ending in something that runs and is tested, each with a
gate. One engineer, sequential; estimates are working weeks of building,
review latency included, hardware excluded.

### Phase A — one Core, many users (3 weeks)

- Core takes the identity per request: `Core::handle_as(identity, request)`,
  per-user state in a `UserState` map, shared state as today; events carry
  the user they are for, replicas the peer; the server holds one Core and
  routes events to the right connections. The personal desktop is the same
  code with one user.
- Identity: users, sessions and roles in redb; the local provider with
  argon2 password hashes (question 3), a one-time link to set the first
  password, `localspace admin bootstrap` printing the first admin's link;
  sessions as httpOnly SameSite=Strict cookies with `session_ttl`, a
  bearer token for the API; the persistent banner while local is on.
- Workspaces and ACLs persisted in redb: a personal workspace per user at
  first login, shared workspaces made by admins, members with levels,
  per-document tightening, the `view` member's sync rejected as today.
  Minimal pages in the shell: Users, Workspaces, a workspace switcher.
- The `localspace` binary (answer 27): `serve`, `doctor`, `bench`, `evals`,
  `call`, `admin`; `localspace.toml` with the keys the pilot uses (answer
  22), `--config` pointing at it.
- Audit off the hot path: a bounded channel and a writer task, nothing
  retained in memory, the actor's role, IP (from the socket, or from
  `X-Forwarded-For` behind a trusted proxy) and session on each record, the
  real conversation in the scope, daily files, `localspace audit verify`.
  Conversations keyed by user in the shared store.
- **Gate:** two users in two browsers on one server see different personal
  workspaces and the same shared board; a viewer's edit is rejected
  server-side; every request of the above is in the audit log with the
  right actor; the CI's Linux job runs the whole suite (this is where the
  first CI run lands, so budget a few days of friction for the runner:
  Tauri's system libraries, the wasm target, the e2e's browser).

### Phase B — retrieval end to end (4 weeks)

The approved 6.1–6.6 in their order, on the shared Core, with identities:

- 6.1 citations in the proto and the chat, and the CI check on the generated
  TypeScript.
- 6.2 documents as files: `blobs/<blake3>`, records and ACLs in redb,
  integer ids; the 6.0 export documents migrate.
- 6.3 upload: the raw-body route, `max_upload_mb`, type sniffing, the
  optional ClamAV socket, the Data page and the chat's Attach.
- 6.4 extraction in a child process with a deadline: text, Markdown, CSV,
  JSON, HTML, PDF, DOCX, PPTX, XLSX; chunking with `/tokenize`.
- 6.5 embeddings: an engine per role, bge-m3 in the catalog, each chunk
  once per hash, `model.embed`, `find_capability` on embeddings.
- 6.6 the index: tantivy and usearch per workspace, the reciprocal-rank
  merge, the per-user bitmap before ranking, `docs.search` as a Core tool
  and for harnesses, citations validated by Core, `/readyz` says the index
  is loaded.
- The retrieval eval: 30 questions a user would ask and 5 the corpus does
  not answer; 27 of 30 in the top 5 with bge-m3.
- **Gate:** the ACL suite (a user cannot retrieve, cite, open, sync or
  tool-call a document they lack), the browser upload-and-cite test, the
  eval, `docs.search` p95 on 100k chunks recorded as the CI baseline, Core
  idle under 50 MB with that index open.

### Phase C — collaboration and the network (3 weeks)

- Presence over the event stream: who has a board open, their cursor and
  selection, ephemeral, never in the DAG (question 9).
- Proposals: the shared workspace's `agent_writes = "proposal"` default; the
  badge, the review with the diff summary, apply and discard, both a single
  commit; personal workspaces stay `direct`.
- Sync under real conditions: reconnect and catch-up, compaction of a
  long-lived board's history, the six-panel cap holding.
- Admission to the one engine: `per_user_concurrent` and `queue_max` from
  the configuration, a turn past the limit queued with its position shown
  rather than failed (deployment §7.2, the team-mode defaults of §12.1);
  today two Cores would each start a sidecar, and one Core has no queue.
- OIDC: discovery, Authorization Code with PKCE, `group_claim` to roles and
  groups, back-channel logout, refresh at the IdP (question 4).
- 6.7, the gateway: the saved mode and ceiling, SearXNG at a set URL and
  nothing by default, approval for search too, fetched pages cached as
  cited documents, every request audited, the always-visible mode
  indicator, and the zero-connection job in CI with its positive control.
- **Gate:** the co-editing and proposal items of deployment §16; all three
  modes as specified; zero packets in `airgapped` while the control passes;
  a `view` member cannot write; the OIDC login against a test IdP in CI.

### Phase D — the install path and the partner's hardware (2 weeks, plus the partner's week)

- The Linux artefact: `localspace-<version>-linux-x86_64.tar.gz` with the
  binary, the web bundle, the registry folder holding the whiteboard, the
  planner and the types package, a unit file and the guide (question 5).
- `localspace install` writing the unit, the user and the directories;
  `localspace doctor` extended to the pilot's checks (§9 below); TLS or
  `behind-proxy` (question 2); `backup create` and `backup restore`
  (question 6); the forward-only migration at start with the automatic
  pre-migration backup.
- The install guide (§9) written from a clean machine, then followed by
  someone who did not write it.
- The real model: the catalog entry the partner's hardware fits, loaded
  through the same one-click path, the planner's verdict shown; `bench` on
  that machine.
- **Gate:** the acceptance list (§10) run on the partner's server by the
  partner's sysadmin from the guide, with the engineer watching and not
  typing.

### The total

12 weeks of building — about three months — for a pilot-ready build, if
nothing is added and the hardware is there by the start of Phase D. Then
the partner's infrastructure week (deployment §15) and their acceptance run,
which is calendar time on their side.

## 6. Where the rough estimate is wrong

The shape offered was retrieval three to four weeks, multi-user four to six,
install one to two, a pilot-ready build in roughly two to three months. Where
I would move it:

- **Two months is not available.** The three tracks are sequential for one
  engineer, and the shared-Core work in Phase A is the widest change in the
  plan: it touches every request path, every event, the audit and the
  tests. It is the item most likely to slip, and it is first because
  building retrieval, presence and proposals on a one-user Core and
  retrofitting identity would be doing them twice. Multi-user is five to
  six weeks in total here (Phase A plus the collaboration half of Phase C),
  not four.
- **Retrieval at four, not three.** The code is three; the extraction child
  process, the embedding sidecar with `/tokenize`, the 100k-chunk
  measurement and the eval written to its rules are the fourth week. The
  usearch C++ build on the Linux runner is a risk I will report rather than
  absorb (answer 3).
- **Install at two, plus a week that is not mine.** A guide is not done when
  it is written; it is done when someone else has followed it. The first
  install on the partner's machine finds things — drivers, a proxy, a CA,
  clock skew, a firewall — and that week is on the calendar whoever owns
  it.
- **CI has never run.** The first Linux run is in Phase A's gate and will
  cost days: the Tauri shell's system libraries, the wasm target, the e2e's
  browser, the Windows split if the shell will not build on Linux (the
  answer of 2026-09-11). It is in the estimate as friction, not as a task,
  because it cannot be planned closer than that.
- **The model is a dependency, not a task.** A real model cannot be
  exercised on this laptop. If the server arrives after Phase D starts,
  Phase D stretches by exactly that gap; the W32 trip's evals may also
  reopen step 5 for the whiteboard's tool descriptions.
- **Three months is still ahead of Q1 2027.** Twelve weeks from now is
  early December for the build, mid-December with the partner's week; the
  quarter the roadmap promised starts in January. The margin is about a
  month, and the items above are what would eat it.

## 7. Why this order

Identity and the shared Core come first because everything else in the
pilot is defined relative to a user: which documents the index may search,
who is at the cursor, whose proposal it is, who the audit names. Retrieval
comes second because it is the pilot's second sentence ("ask questions over
the company's own documents") and its ACL pre-filter needs real identities
to be tested at all. Collaboration and the gateway come third because they
polish what Phase A made possible and their gates are the security person's
questions. The install path comes last because it packages what exists,
and a guide written earlier would be rewritten.

## 8. Risks and dependencies

| Risk | What it does to the plan | What is done about it |
|---|---|---|
| The shared-Core change slips | Everything after it moves | It is first, alone, with its own gate; the personal desktop stays the same code with one user, so nothing forks |
| No server with a GPU in time | Phase D's model item and the acceptance run wait | The user is finding it in parallel; the build runs against the 3B and 7B here until then |
| The partner's hardware is a W32-class box, not the §12.1 floor | Team mode: one interactive stream, `per_user_concurrent = 1`, queuing shown; 10 users, not 30 | Stated in the guide and the acceptance list (question 7) |
| usearch's C++ build fails on the runner | Reported, not worked around; a pure-Rust HNSW is the user's call | Tried in Phase B's first days |
| The W32 evals fail on the reference model | Step 5 reopens for tool descriptions and front doors | A week, inside Phase B or C, from the whiteboard's own tests |
| CI's first run | Days of runner friction | In Phase A's gate; the Rust job splits if the shell will not build on Linux |
| Smart App Control on the build laptop | Hours lost to refused binaries | Known and written down; nothing in the product depends on it |
| Local accounts stay on after the pilot | Weak identity in production | The banner never goes away while `provider = "local"`; the guide says to move to OIDC |

## 9. The install guide (draft)

Written for a sysadmin who has installed a service from a tarball before
and has never seen localSpace. Every command below exists by the end of
Phase D; the guide ships in the tarball as `INSTALL.md` and is tested by
following it on a clean Ubuntu 24.04 machine.

**You need:** a Linux x86_64 server with an NVIDIA GPU and its driver
installed (the exact floor is in the sizing note the vendor sent with the
model choice), a DNS name for the server, a TLS certificate for that name or
a reverse proxy that terminates TLS, 2 TB of NVMe under `/var/lib`, and
either an OIDC application registered at your identity provider or the
decision to start with local accounts.

1. **Unpack and install.**
   ```
   tar -xzf localspace-<version>-linux-x86_64.tar.gz
   cd localspace-<version>
   sudo ./localspace install --config /etc/localspace/localspace.toml
   ```
   This creates the `localspace` system user, `/var/lib/localspace`,
   `/etc/localspace/localspace.toml` from the template beside the binary, and
   a systemd unit. It does not start anything.
2. **Edit `/etc/localspace/localspace.toml`.** The minimal file is:
   ```toml
   [server]
   bind = "0.0.0.0:8443"
   public_url = "https://ai.corp.example"
   tls = { cert = "/etc/localspace/tls/fullchain.pem", key = "/etc/localspace/tls/privkey.pem" }
   # or, behind nginx or Caddy that terminates TLS:
   # tls = "behind-proxy"
   # trusted_proxies = ["10.0.0.0/8"]

   [auth]
   provider = "local"          # "oidc" once the IdP application exists; see step 6
   session_ttl = "12h"

   [models]
   default = "<the model id from the sizing note>"
   embedding = "bge-m3"

   [network]
   mode_ceiling = "ask"        # "airgapped" for no egress at all
   ```
3. **Check the machine.** `sudo -u localspace localspace doctor` reports the
   GPU and driver, VRAM and RAM against the chosen model, disk space and
   speed under `/var/lib/localspace`, whether that volume is encrypted (it
   should be; localSpace does not encrypt documents itself in this release),
   the clock, the certificate and its name, and the identity provider's
   reachability when one is configured. Fix anything red before going on.
4. **Bring the models in.** Connected: `sudo -u localspace localspace models
   download <model id>` for the default and for `bge-m3`. Air-gapped: copy
   the model directories prepared on a connected machine into
   `/var/lib/localspace/models/` and run `localspace models import`.
5. **Start it.** `sudo systemctl enable --now localspace`, then `localspace
   doctor --running` to see it answer on `/readyz` with the model loaded and
   the index open.
6. **The first admin.** `sudo -u localspace localspace admin bootstrap
   --email you@corp.example` prints a one-time link. Open it, set the
   password, and you are the admin. The screen carries a banner until the
   provider is OIDC: local accounts are for pilots.
7. **Users and a workspace.** In the app, Admin → Users → Add creates a user
   and prints their one-time link; Admin → Workspaces → New makes the shared
   workspace and adds members with their level. Each user also has a
   personal workspace from their first login.
8. **Documents and the network.** Members upload files from the Data page
   or the chat's Attach; they are indexed in the background and the Data
   page shows when each is searchable. Admin → Network sets the ceiling; in
   `ask`, the first web fetch per site per session asks the user; in
   `airgapped`, the model has no web tools at all.
9. **Back it up.** `sudo -u localspace localspace backup create
   /backups/ls-$(date +%F).tar.zst` runs online. Test the restore once
   before anyone relies on the server: stop the service, `localspace backup
   restore <file>` onto an empty `/var/lib/localspace`, start it, sign in.
10. **Moving to OIDC.** Register an application at your provider with the
    redirect URL `https://ai.corp.example/api/v1/auth/callback`, set
    `provider = "oidc"`, `issuer`, `client_id`, `client_secret = "file:…"`,
    `group_claim` and `admin_group` in the configuration, `systemctl reload
    localspace`, and sign in with the company account. Local users keep
    working until you disable the provider; the banner goes when it is off.

**If it does not work:** `journalctl -u localspace -e` has the structured
log; `localspace doctor --running` names what is wrong in one line per
check; the audit log under `/var/lib/localspace/audit/` shows every request
that was refused and why.

## 10. The pilot acceptance list

The subset of deployment §16 that must pass on the partner's own hardware,
run by the partner's sysadmin from the guide, before the build is called
installed. Each line is a test with a yes or no; the engineer watches.

1. A fresh install from the guide, on the partner's machine, reaches a
   working sign-in in under an hour, without a call to the vendor.
2. `localspace doctor` is green, or every red line is one the partner has
   accepted in writing (the volume-encryption line among them).
3. The first admin was made with `admin bootstrap`; two more users were
   made in the app; each signs in in their own browser and sees their own
   personal workspace.
4. Two users co-edit the shared workspace's board in two browsers; edits
   appear in both within a second; presence shows both; a third user with
   `view` sees the board and cannot change it, and the refusal is in the
   audit log.
5. An agent asked for a plan on the shared board produces a proposal, not a
   change; the requester applies it as one commit; a second proposal is
   discarded and leaves no trace on the board.
6. A member uploads a PDF and a DOCX; both become searchable; a question
   whose answer is in the PDF is answered with a citation that opens the
   right page; a question the documents do not answer is answered without a
   confident citation.
7. The ACL suite, run on the partner's machine: a user without access to a
   document cannot retrieve it, cite it, open it, sync it or tool-call it,
   in each case with a refusal in the audit log.
8. In `airgapped`, the model's tool set has no `web.*` tool (seen in the
   trace) and the partner's own egress monitoring sees no connection from
   the server during a scripted session; in `ask`, the first fetch to a
   site asks, and the approval is in the log; in `online`, a fetch outside
   the allowlist is refused.
9. The audit chain verifies after the above; the records name the users,
   the IPs and the sessions; the SIEM export, if configured, received every
   event class.
10. `backup create` then `backup restore` onto an empty directory yields the
    same documents, users and board, and the chain still verifies.
11. The default model loads with one click from the catalog, the planner's
    verdict is shown, and a single stream decodes at the rate the sizing
    note promised for this machine (`bench` records it); with `per_user_
    concurrent` reached, a second user sees the queue position rather than
    an error.
12. The service survives a restart with every session, document and board
    intact, and comes up within the time the guide states.

## 11. Spec changes this plan asks for

- **Deployment §4.1:** "`local` provider exists for development and the
  initial bootstrap only" becomes: also allowed for pilots, behind a
  persistent warning banner in the app, and named as such in the install
  guide (the user's decision of 2026-09-12; the amendment lands with Phase
  A's identity commit).
- **Deployment §12.1:** the pilot may run on a W32-class machine in team
  mode; the guide states the consequences (one interactive stream, ten
  users) rather than the §12.1 floor (question 7).
- **Deployment §11.3:** a backup without the master key is a backup of
  everything in this release, since nothing is encrypted per document yet;
  the guide says so.

## 12. Questions

1. **Phase order.** Identity and the shared Core before retrieval, as in §7;
   the option is retrieval first, on the one-user Core, then identity.
   **Recommend** identity first.
2. **TLS.** Options: native TLS in the binary with `rustls` through
   `axum-server`, two new dependencies; or `behind-proxy` only for Pilot 1,
   with Caddy or nginx in the guide terminating TLS, native TLS following.
   **Recommend** behind-proxy first, since every organisation has a proxy
   and it removes certificate handling from the first install; native TLS
   as a Phase D stretch if the partner cannot run a proxy.
3. **Password hashing** for local accounts: `argon2` (RustCrypto), a new
   dependency. **Recommend** it.
4. **OIDC.** Options: the `openidconnect` crate (discovery, PKCE, JWKS,
   token validation; pulls `oauth2`), or discovery and PKCE by hand over
   `ureq` with `jsonwebtoken` for the tokens. **Recommend** `openidconnect`:
   the failure modes of OIDC are in the details it covers.
5. **The artefact.** A tarball with the binary, the web bundle as files, the
   registry folder and the unit; or the web bundle embedded in the binary
   as deployment §3.1 says. **Recommend** the tarball for Pilot 1 and the
   embedding when the release pipeline exists (step 10).
6. **Backup.** `localspace backup create` writing a consistent snapshot
   directory with a checksum manifest, compressed by the sysadmin's tools;
   or the single `.tar.zst` of §11.3, which needs `tar` and `zstd` crates.
   **Recommend** the single file as the spec says, with the two crates.
7. **The pilot's hardware profile.** The §12.1 floor (4 × 80 GB), or a
   W32-class box in team mode (one stream, ten users). **Recommend**
   planning for team mode and being glad of more: it is what the user is
   likely to find, and the sizing note in the guide follows from it.
8. **Admin pages or a CLI first.** Users and workspaces managed in the app,
   or by `localspace admin user add` and `workspace create` in Phase A with
   the pages in Phase C. **Recommend** the pages in Phase A: the sysadmin
   bootstraps from the CLI, everything after is in the app.
9. **Presence transport.** Ephemeral events over `/ws/json`, at most ten a
   second per user, never in the DAG; the option is a separate socket.
   **Recommend** the event stream.
10. **The shared workspace's agent default.** `proposal`, as §6.3 says for
    shared workspaces, with the owner able to set `direct`. **Recommend**
    that.
11. **Local users' first password.** A one-time link the admin hands over,
    or an admin-typed initial password. **Recommend** the link, so no
    password ever passes through an admin.
12. **One board per workspace per harness** stays for Pilot 1, or several
    boards per workspace come in. **Recommend** one, named in §2 as out.
13. **The audit's IP behind a proxy.** From `X-Forwarded-For` only when the
    peer is in `trusted_proxies`, else the socket's address. **Recommend**
    that.
14. **`doctor`'s encryption check.** Warn and show a banner when the storage
    root is not on an encrypted volume, or refuse to serve. **Recommend**
    warn: the partner decides, in writing, on the acceptance list.
