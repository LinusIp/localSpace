# localSpace — Organisation Deployment

Companion to the Harness Plugin System spec. Covers everything needed to run one localSpace server for one company and its employees: installation, identity, tenancy, shared workspaces, model serving at scale, governance, security, audit, operations, capacity, and the acceptance bar for "deployment ready". Plugin-core concepts (Core, Client, harness, tiers, manifest, DAG, gateway) are as defined there and not repeated.

---

## 1. Scope and non-goals

- **One server, one organisation.** Single-tenant. A hosting provider running many companies runs many servers. No cross-org data path exists in the binary.
- **Employees use a browser.** The desktop binary can also connect to a server (`localspace --server https://ai.corp`) for users who want a native window; it is the same Client code and the same protocol.
- **Everything runs on the organisation's hardware.** No component calls out except the gateway (§7) and provisioning (§8), both admin-controlled and both fully off in air-gapped mode.
- **Not horizontally scaled in v1.** One Core process per server. Scale is vertical (GPUs, RAM) plus active/standby failover (§11.4). This is stated up front so nobody sizes for a cluster that does not exist.
- **Strong hardware, two shapes.** The server floor (profile **S**, §12.1) is a multi-GPU box that runs a 70B-class or larger model, a VLM and GPU simulations at once. A single-GPU workstation (profile **W32**: 32 GB VRAM, 64 GB RAM — plugin spec §1.1) can also run `serve` for a **small team (≤ 10 users)** using hybrid MoE inference for 100B+ models; the console labels it "team mode" and shows the planner's tok/s estimate so nobody expects server throughput from it. Below W32 is unsupported for now; `localspace doctor` says so rather than letting an undersized install limp.

---

## 2. Topology

```
                     ┌────────────────────────── company network ──────────────────────────┐
  browsers ──TLS──▶  reverse proxy (optional)  ──▶  localspace serve                        │
  desktop Clients                                    ├─ HTTP API   /api/v1                  │
                                                     ├─ WebSocket  /ws                      │
                                                     ├─ static     /   (wasm Client)        │
                                                     ├─ admin      /admin                   │
                                                     ├─ Core: docs, DAG, index, scheduler   │
                                                     ├─ inference workers  (GPU 0..n)       │
                                                     ├─ harness runtime  (wasmtime, Tier B) │
                                                     └─ gateway  ──▶ corp proxy ──▶ internet│
                                                     storage: /var/lib/localspace           │
  IdP (OIDC/SAML)  ◀──────────────────────────────── auth                                   │
  SIEM / syslog    ◀──────────────────────────────── audit export                           │
  file shares, SharePoint, Confluence ─────────────▶ ingestion connectors (read-only)       │
```

Processes: one `localspace serve` process. Inference workers and Tier B harnesses are child processes it supervises. Nothing else is required: no external database, no message broker, no object store. That is deliberate; the buyer's ops team must be able to run this with one systemd unit.

---

## 3. Installation

### 3.1 Artefacts

| Artefact | Contents |
|---|---|
| `localspace` binary | Core + server + native Client, per platform (linux-x86_64, linux-aarch64, windows-x86_64, macos-aarch64) |
| `localspace-web.tar` | wasm Client bundle, embedded in the binary by default; separate file only for CDN-style hosting behind a proxy |
| container image | `localspace/server:<version>`, distroless, runs as non-root, CUDA and ROCm variants |
| `.lsbundle` | offline installer: binary + web bundle + chosen models + chosen harnesses + licence + checksums. Built by `localspace bundle create` on a connected machine; imported by `localspace bundle import` on the air-gapped server |

Every artefact is signed; `localspace verify <file>` checks it against the embedded publisher key.

### 3.2 Install paths

**Bare metal / VM (recommended for GPU servers):**

```
sudo localspace install --config /etc/localspace/localspace.toml   # creates user, dirs, systemd unit
sudo systemctl enable --now localspace
localspace admin bootstrap --admin-email ops@corp.example            # first admin, prints one-time login link
```

**Container:** the same binary with `/var/lib/localspace` and `/etc/localspace` as volumes and the GPU passed through. A Helm chart is provided for single-replica deployment only (see §1).

**Air-gapped:** `localspace bundle import corp-2026-09.lsbundle` then the two commands above. No step requires network.

### 3.3 Configuration

`/etc/localspace/localspace.toml`. Every key has a default; a minimal file is the four lines under `[server]` and `[auth]`.

```toml
[server]
bind = "0.0.0.0:8443"
public_url = "https://ai.corp.example"
tls = { cert = "/etc/localspace/tls/fullchain.pem", key = "/etc/localspace/tls/privkey.pem" }
# tls = "behind-proxy"   # accept plain HTTP from a trusted reverse proxy, honour X-Forwarded-*
trusted_proxies = ["10.0.0.0/8"]
max_upload_mb = 200

[storage]
root = "/var/lib/localspace"
encryption = "at-rest"                    # off | at-rest (per-document keys, master key in [secrets])

[secrets]
master_key = "file:/etc/localspace/master.key"   # or "env:LOCALSPACE_MASTER_KEY" or "kms:<provider url>"

[auth]
provider = "oidc"                         # oidc | saml | local (dev only)
issuer = "https://login.corp.example/realms/corp"
client_id = "localspace"
client_secret = "file:/etc/localspace/oidc.secret"
group_claim = "groups"
admin_group = "localspace-admins"
session_ttl = "12h"
scim = { enabled = true, token = "file:/etc/localspace/scim.token" }

[models]
dir = "/var/lib/localspace/models"
default = "llama-3.3-70b-instruct-fp8"
embedding = "bge-m3"
vision = "qwen2.5-vl-72b-fp8"
[[models.worker]]
model = "llama-3.3-70b-instruct-fp8"
backend = "mistralrs"                     # mistralrs | llamacpp | vllm | sglang | trtllm
gpus = [0, 1, 2, 3]                       # tensor-parallel across these (NVLinked)
max_batch = 64
ctx = 65536
draft = "llama-3.2-3b-instruct"           # speculative decoding
kv_cache = "fp8"
prefix_cache_gb = 40
[[models.worker]]
model = "qwen2.5-vl-72b-fp8"
gpus = [4, 5]
max_batch = 16
[[models.worker]]
model = "qwen2.5-7b-instruct-fp8"
role = "utility"                          # titles, summaries, reranking, compaction, non-reasoning harness calls
gpus = [5]
max_batch = 128
[[models.worker]]
model = "bge-m3"
gpus = [5]                                # small; shares with the utility model

[scheduler]
priority_classes = { interactive = 1, background = 3, ingestion = 5 }
per_user_concurrent = 2
per_user_tokens_per_hour = 400000
queue_max = 500
queue_timeout = "60s"

[network]
mode_ceiling = "ask"                      # airgapped | ask | online — users can only go stricter
allowlist = ["*.wikipedia.org", "arxiv.org", "docs.rs"]
blocklist = []
proxy = "http://proxy.corp.example:3128"
ca_bundle = "/etc/localspace/corp-ca.pem"
search = { backend = "searxng", url = "http://searxng.corp.example:8080" }

[harnesses]
tier_b = "admin-approved"                 # disabled | admin-approved
registry = "https://registry.localspace.io"   # or "offline"
gpus = [6, 7]                             # pool reserved for Tier B simulations; never a model worker's GPU
per_process = { cpu = 8, memory_mb = 65536, vram_gb = 40 }
exclusive_queue_max = 8                   # exclusive-GPU jobs waiting before new ones are refused

[audit]
sink = ["local", "syslog://siem.corp.example:6514?format=cef"]
retention_days = 730

[limits]
per_user_storage_gb = 20
per_workspace_storage_gb = 200
```

Config changes are picked up with `systemctl reload localspace` for everything except `[server]`, `[storage]`, `[secrets]` and `[[models.worker]]`, which need a restart; the admin console shows which pending changes need which.

### 3.4 Storage layout

```
/var/lib/localspace/
  db/           redb: users, groups, workspaces, ACLs, environments, DAG commit graph, sessions
  blobs/        blake3 content-addressed, per-document encrypted when at-rest is on
  docs/         automerge document heads and incremental saves
  index/        embedding index (usearch) + BM25, per workspace
  models/       GGUF / safetensors, one directory per model id, with manifest and checksum
  harnesses/    installed .hpack contents, per id/version
  audit/        append-only, hash-chained, daily files
  tmp/
```

Everything under `root` is the complete state. Backup = snapshot this directory (§11.3).

---

## 4. Identity and access

### 4.1 Authentication

- **OIDC** (Authorization Code + PKCE) or **SAML 2.0** against the company IdP. `local` provider exists for development and the initial bootstrap only; the console warns permanently if it is enabled in production.
- **Sessions**: httpOnly, SameSite=Strict cookie for the browser; bearer token for desktop Clients and API use. Refresh via IdP; hard expiry at `session_ttl`; IdP-initiated logout honoured (back-channel logout).
- **MFA** is the IdP's job. localSpace can require an `amr` claim containing `mfa` for admin roles.
- **API tokens**: per-user, scoped, expiring, created in the console, shown once. Service accounts are users with no interactive login, created by admins, for ingestion connectors and automation.

### 4.2 Provisioning

- **SCIM 2.0** endpoint for users and groups. Deprovisioned user → sessions killed within 60 s, personal workspace frozen (readable by admins, transferable), API tokens revoked.
- Without SCIM: just-in-time creation at first login, groups from `group_claim` on every login.

### 4.3 Roles

Roles are bound to IdP groups; membership is never edited inside localSpace, so the IdP stays the source of truth.

| Role | Can |
|---|---|
| **Org admin** | everything below, plus config, licence, backups, break-glass access to any workspace (audited, with reason) |
| **Catalog admin** | approve harnesses and versions, set capability policy, manage models, set network policy |
| **Security auditor** | read audit log, export, read policy; no data access |
| **Workspace owner** | manage members and ACLs of their workspace, install approved harnesses into it |
| **Member** | use environments, create personal documents, join workspaces they are granted |
| **Viewer** | read-only in granted workspaces; can chat with documents but the agent has no write tools |

Users hold roles per scope: org-wide roles from groups, workspace roles from workspace ACLs.

---

## 5. Tenancy model

```
Organisation
 └─ Users, Groups (from IdP)
 └─ Workspaces
     ├─ Personal (one per user, created at first login)
     └─ Shared    (owned by a group or by named users)
         └─ Documents        (harness docs, uploaded files, ingested sources, web cache)
         └─ Environments     (harness set + model + network mode, one default + optional named ones)
         └─ Conversations    (per user, private by default, shareable into the workspace)
```

- A **workspace** is the unit of access control, storage quota, retrieval scope and audit scope.
- An **environment** belongs to a workspace, not to a user: a shared workspace has one agreed harness set so the agent behaves the same for everyone in it. Users may create named environments in their personal workspace freely.
- A **conversation** always runs inside one workspace; its retrieval scope is that workspace's documents (filtered by document ACL) plus any other workspaces the user explicitly attaches for that conversation.

---

## 6. Shared workspaces and collaboration

### 6.1 Document ACL

Every document has an ACL: `{principal (user|group), level (owner|edit|comment|view)}`. Defaults are inherited from the workspace; a document can be tightened, never loosened beyond its workspace. Core checks the ACL on every read, every tool call, every retrieval hit and every Automerge sync message. There is no path that bypasses it, including admin break-glass, which is a normal access with an audit reason attached.

Ingested external documents (§6.4) carry the ACL of their source, mapped to IdP groups, and are re-synced on the connector's schedule; a permission revoked in SharePoint is revoked here within one sync interval.

### 6.2 Real-time co-editing

Harness documents are Automerge docs (per the plugin spec). Any number of members with `edit` can have the same board open; the server relays sync messages per document, presence (who is here, their cursor/selection) is a separate ephemeral channel. Concurrent edits merge; there is no locking and no "someone else has this open". `view` members receive sync messages but their outgoing changes are rejected server-side.

Blob-backed documents (physics scenes, meshes) do not co-edit. They use single-writer with an explicit lease: opening for edit takes a 10-minute renewable lease; others open read-only and see who holds it.

### 6.3 Agents in shared documents

An agent run against a shared document never writes to the shared head directly. It writes to a **proposal branch** in the DAG: the requesting user sees it live, others see a "1 proposal from Anna's agent" badge. The requesting user, or any `owner`, reviews the diff summary and **applies** or **discards**. Applying merges the branch (Automerge merge for CRDT docs, head replacement for blobs, both a single commit). A workspace owner can set `agent_writes = "direct"` for low-stakes workspaces, in which case an agent commit is like any member commit and is undoable in the same way.

Personal-workspace documents default to `direct`.

### 6.4 Bringing company knowledge in

Ingestion connectors are read-only Tier A harnesses run under a service account, scheduled by Core:

- Filesystem / SMB share, SharePoint / OneDrive, Confluence, Google Drive, Git repositories, email archives (mbox/PST).
- Each connector yields `{document, source_url, acl, last_modified, hash}`; Core chunks, embeds on the embedding worker under the `ingestion` priority class, and indexes per workspace.
- Connectors run inside the network mode of their workspace; most company sources are on the intranet and work under `airgapped` because the intranet is not the internet — the allowlist distinguishes them (`intranet = ["*.corp.example", "10.0.0.0/8"]` is always permitted).
- Re-index is incremental by hash; a full re-index is an admin action.

Retrieval always returns citations (source URL, title, chunk locator) and the ACL filter is applied **before** ranking so the top-k is never diluted by hits the user cannot see.

---

## 7. Model serving at organisation scale

### 7.1 Workers

Each `[[models.worker]]` is a supervised child process pinned to its GPUs, loading one model with continuous batching and tensor parallelism across its GPUs. Backends, all behind the same internal worker trait: `mistralrs` (default, Rust), `llamacpp` (Apple Metal and odd hardware), and as optional external processes `vllm`, `sglang`, `trtllm` for organisations that want the last 30 % of throughput on NVIDIA and accept a Python or TensorRT runtime on the box. Core has no code dependency on the external ones; they are launched, health-checked and metered exactly like the Rust ones. Workers are restarted on crash with backoff; a model that crashes three times in ten minutes is marked unhealthy and the console alerts.

Reference layout on one 8× H100/H200 server: chat worker 70B FP8 on GPUs 0–3 (64k ctx, batch 64), VLM 72B FP8 on GPUs 4–5, embedding model sharing GPU 5, GPUs 6–7 reserved for Tier B simulations. A second server for a large MoE (Qwen3-235B-A22B or DeepSeek-V3-class at FP8 needs 8 GPUs by itself) is a separate deployment or a second worker box attached to the same Core over the worker protocol (`worker.remote = "grpc://gpu-2.corp:7001"`) — the one place a second machine is supported in v1, because it holds no state.

### 7.2 Scheduler

Sits between every caller (conversations, harness `model.*` calls, ingestion) and the workers.

- **Priority classes**: `interactive` (a user is waiting) > `background` (agent sub-steps, harness calls, summarisation) > `ingestion`. Weighted fair queuing between classes; strict FIFO within a class per user; round-robin across users so one user's 40-step agent run cannot starve the others.
- **Per-user limits**: `per_user_concurrent` requests in flight, `per_user_tokens_per_hour` budget (generated + prompt), configurable per group (a research group may get 5× the default). Over budget → request queues at `background` priority with a visible message, never hard-fails.
- **Queue**: bounded (`queue_max`); when full, new `interactive` requests get an immediate "server busy, position N" and the Client shows it. `queue_timeout` returns a clean error rather than a hanging spinner.
- **Prompt cache**: prefix/radix KV reuse across all users on the same worker; the prompt layout is fixed (plugin spec §16.1) precisely so this hits. Hit rate is on `/metrics` and alerts under 80 %.
- **Routing by request class**: conversation turns and agent steps go to the chat worker; titling, summarisation, compaction, `find_capability` reranking, ingestion cleanup and non-reasoning harness calls go to the `utility` worker. The console shows the split; a deployment where the big model takes more than 60 % of calls is misconfigured.
- **Speculative decoding** is on by default per worker (`draft`), monitored by acceptance rate, disabled automatically below 60 %.
- **Admission for harness calls**: a harness `model.complete` call is billed to the user whose action triggered it and runs at `background`.

### 7.3 Model catalog

Admins manage models in the console: curated list (repo id, quantization, licence, and — from the placement planner — the verdict for this machine: `resident` / `hybrid` / `streaming` / `does not fit`, with estimated tok/s and the VRAM/RAM plan), download (or bundle import when air-gapped), assign to a worker, set as default, retire. Users pick among **assigned** models per environment; they never download. Model licence text is shown at download time and stored, because legal will ask.

---

## 8. Governance of harnesses and network

### 8.1 Harness lifecycle

```
registry / bundle  →  candidate  →  admin review  →  approved (pinned version)  →  rollout  →  retired
```

- **Review** is the org's change-control step, not a code review — every harness is written and signed by localSpace. It shows: manifest, capability list in plain language, tier and `native_reason`, signature status, eval score on the org's default model, and the diff against the currently approved version.
- **Rollout**: approve for a pilot group first, then all. Pinned version; updates re-enter review if capabilities widen, otherwise auto-approve is a per-harness switch.
- **Capability policy**: org-wide maxima (e.g. `net` never grantable, `fs` at most `workspace`, Tier B disabled). A harness needing more than policy allows can still be approved with the excess capability disabled, and it must degrade gracefully (plugin spec §8.3).
- **Per-workspace installs** are drawn only from the approved set; workspace owners cannot exceed it.

### 8.2 Network policy

- `mode_ceiling` sets the strictest mode users may relax to; per-group overrides allowed (research gets `online`, finance gets `airgapped`).
- Allowlist/blocklist with wildcard domains; intranet ranges always allowed for connectors; corporate proxy and CA bundle honoured by the gateway.
- Every outbound request is audited with user, workspace, conversation, URL, bytes, and result. The gateway is the only socket; a harness or worker attempting egress otherwise is blocked by the sandbox and logged as a security event.

---

## 9. Security

### 9.1 Data protection

- **In transit**: TLS 1.2+ (TLS 1.3 preferred), HSTS; or plain HTTP only from `trusted_proxies`.
- **At rest**: per-document data keys (XChaCha20-Poly1305), wrapped by the master key; master key from file, env or an external KMS. Model files are not encrypted (they are public weights); the index is, because chunks are content.
- **Erasure**: the DAG is immutable, so deletion is **crypto-shredding** — destroying a document's key makes every version unreadable while the commit graph stays consistent. This satisfies GDPR-style erasure without rewriting history. Backups older than the retention window age out on schedule.
- **Uploads**: type-sniffed, size-limited, scanned by an optional ClamAV socket if configured.

### 9.2 Isolation

- Tier A harness logic: wasmtime, capability-scoped, memory-limited, fuel-metered per call.
- Tier B: separate OS user per process, landlock + seccomp profile, cgroup limits from `[harnesses.per_process]`, no network namespace unless `net` is granted (and then only to the gateway's unix socket). Disabled by default org-wide.
- Surfaces run in the employee's browser sandbox; the web Client has a strict CSP (`default-src 'self'; connect-src 'self' wss:; script-src 'self' 'wasm-unsafe-eval'`), no inline scripts, no third-party origins.
- Inference workers run as a separate OS user with read-only access to `models/` and no storage access.

### 9.3 Agent safety boundaries

- Retrieved documents, web content and harness tool results are all wrapped as untrusted content; the agent loop never executes instructions found in them.
- Write tools respect `confirm` levels; `always` and `destructive` confirmations are per-user and cannot be pre-approved by a harness.
- Every agent action is a DAG commit attributed to `{user, agent run id, model, tool}`; shared documents use proposals (§6.3).
- Rate limits per user and per IP on auth and API endpoints.

### 9.4 Supply chain

- All artefacts signed; SBOM shipped with each release; `cargo audit` and `cargo deny` gates in CI; reproducible builds for the binary.
- Harness packages are signed by localSpace; the org can additionally require its own countersignature (`harnesses.require_org_signature = true`) so nothing runs that its security team did not approve.
- Vulnerability disclosure policy and a security contact are printed in `localspace --about`.

---

## 10. Audit and compliance

### 10.1 Audit log

Append-only, hash-chained (each record carries the blake3 of the previous), daily rotated, verifiable with `localspace audit verify`. Exported live to syslog/CEF or JSON-over-HTTPS for the SIEM.

Record schema:

```json
{
  "ts": "2026-09-03T08:12:44.120Z",
  "id": "01J...", "prev": "blake3:...",
  "actor": { "user": "u_123", "session": "s_9ab", "ip": "10.1.4.22", "role": "member" },
  "scope": { "workspace": "ws_finance", "conversation": "c_77", "document": "d_401" },
  "event": "tool.call",
  "detail": { "harness": "io.localspace.whiteboard", "tool": "canvas.add_shape", "confirm": "never", "commit": "blake3:..." },
  "result": "ok"
}
```

Logged events: login/logout/session revoke, role changes, ACL changes, document create/read/write/share/delete/erase, every tool call, every model request (metadata and token counts, **not** prompt content by default; content logging is a per-workspace switch with a banner), every gateway request, harness install/approve/retire, model download, config change, backup, break-glass access with reason, security events (sandbox violations, signature failures).

### 10.2 Retention, residency, holds

- `audit.retention_days` and per-workspace document retention; conversations expire per policy (default: never in shared workspaces, 365 days in personal).
- **Legal hold** on a workspace freezes erasure and retention; exports include DAG history.
- All data is on the org's disk; residency is wherever the server is. There is no telemetry; `localspace --about` states this and `[telemetry]` does not exist as a section.
- **Export**: `localspace export --workspace ws_finance --format zip` produces documents, history, conversations and audit slice for that scope, for eDiscovery or migration.

---

## 11. Operations

### 11.1 Endpoints

| Path | Purpose |
|---|---|
| `/healthz` | process up |
| `/readyz` | db open, default model worker healthy, index loaded |
| `/metrics` | Prometheus: queue depth per class, tokens/s per worker, prompt-cache hit rate, draft acceptance rate, utility-vs-chat call split, GPU memory and utilisation per GPU, request latency p50/p95, active sessions, stream bitrate, storage used, connector lag, harness process count, sandbox violations |
| `/api/v1` | the proto API, OpenAPI document at `/api/v1/openapi.json` |
| `/ws` | Client stream |
| `/admin` | console (same wasm Client, admin routes) |

Alert rules shipped as a Prometheus rule file: worker down, queue p95 wait > 20 s for 5 min, GPU memory > 95 %, disk > 85 %, audit sink failing, connector lag > 2× interval, sandbox violation count > 0.

### 11.2 Logs and tracing

Structured JSON logs to stdout (journald/container-friendly), levels per subsystem, OpenTelemetry traces optional (OTLP endpoint in config), request ids propagated Client → Core → worker → harness.

### 11.3 Backup and restore

- `localspace backup create /backups/ls-$(date +%F).tar.zst` — consistent snapshot (redb checkpoint + blobs + docs + index + audit + config; models optional with `--with-models`). Runs online; typical 200-user deployment without models: single-digit GB.
- `localspace backup restore <file>` onto a stopped server. Restore is tested in CI on every release and must be tested by the org before go-live (§13).
- Master key is **not** in the backup. Losing it loses the data; the install guide says this in bold and the console nags until the admin confirms the key is escrowed.

### 11.4 High availability

Active/standby: two servers, storage replicated (`localspace replicate --to standby.corp` streams DAG commits, docs and blobs continuously; or block-level replication of `root`), a VIP or DNS switch, and `localspace promote` on the standby. Failover is a minute-scale operation with sessions surviving (sessions are in the db). GPUs on the standby load models at promote time; expect 1–3 minutes to full capacity. No automatic split-brain protection beyond a fencing token — automatic failover should be driven by the org's own cluster tooling if they have it.

### 11.5 Upgrades

- Single binary swap + `systemctl restart`. Schema migrations run at startup, forward-only, with a pre-migration backup taken automatically.
- Rollback = restore that backup and run the previous binary; supported across one minor version.
- Release cadence: monthly minor, security patches as needed; each release lists harness-api compatibility.
- Web Client and Core ship together; a browser holding an old Client gets a "reload to update" banner from a version check on the WebSocket handshake.

### 11.6 Runbooks (shipped in docs)

GPU OOM on a worker; queue saturation; disk full; IdP outage (existing sessions keep working until `session_ttl`); audit sink unreachable (buffers to local, alerts); harness process leak; index corruption (rebuild from docs); master key rotation (re-wrap document keys online); certificate rotation; standby promotion.

---

## 12. Capacity planning

### 12.1 Hardware floor

`localspace doctor` checks these and `serve` refuses to start below them without `--allow-below-floor` (which is logged and shown permanently in the console).

| | Floor | Why |
|---|---|---|
| GPUs | 4× 80 GB-class (H100/A100 80 GB/MI300X) — 6× recommended, 8× for a VLM plus simulations | 70B FP8 with 64k-context KV cache at batch 64 needs ~4 GPUs; the VLM and simulations need their own |
| GPU interconnect | NVLink/NVSwitch or equivalent within the tensor-parallel group | PCIe-only tensor parallelism halves throughput |
| RAM | 512 GB | model loading, page cache for weights, Tier B processes |
| Storage | 4 TB NVMe, ≥ 3 GB/s sequential | weights (a 70B FP8 is ~70 GB, a large MoE 300–700 GB), index, blobs, backups staging |
| CPU | 32 cores | 8 per GPU plus simulation processes |
| Network | 25 GbE to the office core | streams of `stream` surfaces to many browsers |

Workstation profiles (W32, W96) are in the plugin spec §1.1. **Team mode** on a W32 machine: the placement planner runs a 100B+-class MoE at Q4 in hybrid mode; expect one interactive stream at ≥ 15 tok/s and graceful queuing beyond that (`per_user_concurrent = 1`, `queue_max = 20` are the team-mode defaults). Simulations share the single GPU through a VRAM reservation, so a running simulation slows the model and the Client says so. No VLM larger than 8B, no HA. Everything else in this document — identity, ACLs, workspaces, audit, backups, network policy — is identical, which is the point: a team that starts on one workstation moves to a server by changing the machine, not the deployment.

### 12.2 Sizing

Plan with these assumptions, then measure with `localspace bench` (which drives synthetic users against the real scheduler and prints the numbers below for the actual hardware).

**Assumptions:** an active user consumes ~25 tok/s while a response streams; 10 % of employees are active at any moment during work hours; an agent step averages 3k prompt tokens and 400 generated with a 70B-class model; VLM calls are 5 % of requests; ingestion is scheduled off-hours; simulations are separate (below).

| Employees | Concurrent users | Aggregate generation | Chat model hardware (70B FP8, TP, continuous batching) | + VLM | + simulations | Storage year 1 |
|---|---|---|---|---|---|---|
| ≤ 100 | ~10 | ~300 tok/s | 2× H100/H200 | 1–2 GPUs | 1–2 GPUs | 500 GB |
| ≤ 500 | ~50 | ~1500 tok/s | 4× H100/H200 | 2 GPUs | 2 GPUs | 2 TB |
| ≤ 2000 | ~200 | ~5000 tok/s | 8× H200/B200, or two 4× TP workers on one Core | 2 GPUs | 2–4 GPUs, second box | 8 TB |

Beyond ~2000 employees, deploy per division (separate Cores, separate data) rather than waiting for horizontal scaling.

**Simulations** are sized separately because they do not batch: one exclusive Tier B job holds one GPU for its duration. Budget `harnesses.gpus` = expected concurrent exclusive simulations + 1 for shared/non-exclusive ones. A CFD or rigid-body harness ships a `benchmark` entry in `evals.json` so the console can show wall-clock per job on the actual GPUs, and the scheduler shows the exclusive queue depth to users before they submit.

Rules of thumb: a large MoE (235B-A22B-class) at FP8 needs 8 GPUs to itself and gives roughly the throughput of a 70B dense at higher quality; 64k context at batch 64 needs KV headroom — leave 25 % of VRAM in the TP group free; the VLM's tokens-per-image (≈ 1–2k) are what make screenshots expensive, hence text-first context providers. RAM: 2× total resident model size plus 32 GB base. CPU: Tier B harnesses are what eat CPU, budget `per_process.cpu × expected concurrent Tier B sessions`.

---

## 13. Admin console

Routes under `/admin`, same Client. Pages: Overview (health, queue, GPU, storage, alerts); Users & groups (from IdP, roles, sessions, tokens, deprovision); Workspaces (list, owners, quota, retention, legal hold, break-glass); Models (catalog, workers, assignment, benchmarks); Harnesses (catalog, review queue, policy, rollout groups, per-harness usage); Network (modes, allowlists, proxy, gateway log); Connectors (sources, schedule, lag, ACL sync status); Audit (search, export, verify chain); Backups & HA; Configuration (pending reload/restart changes); Licence.

Every admin action is itself audited.

---

## 14. Licensing and activation

The platform licence is an entitlement like any other (marketplace spec §5): a signed offline file naming the org, seat count, expiry and enabled features (Tier B, HA, connectors, private registry). Checked at startup and daily; a lapsed licence turns the server read-only after a 30-day grace with console warnings from day one — it never deletes anything and never phones home. Seat = a user who has logged in during the last 30 days.

Harness, template and model-pack purchases for the organisation are entitlements in the same store, bought in the console (card or purchase order), assigned to seats or to everyone, and delivered inside the offline bundle for air-gapped sites. The org's own private listings are entitled with the org's key and never touch the public registry.

---

## 15. Rollout plan

1. **Infrastructure week**: server at or above the floor (§12.1), GPUs and interconnect verified, TLS, IdP app registration, SIEM endpoint, backup target, proxy/CA. Run `localspace doctor` (checks the floor, GPU drivers and NVLink topology, disk speed, IdP reachability, clock, CA trust) and `localspace bench` to record the baseline tok/s the sizing will be judged against.
2. **Pilot (2–4 weeks, 10–30 users)**: one shared workspace, two harnesses (whiteboard + planning board), one connector, `mode_ceiling = "ask"`. Collect the queue and token metrics; adjust worker layout.
3. **Department rollout**: per-group network policy and quotas, more connectors, catalog review process running.
4. **Org-wide**: HA standby, retention policy signed off by legal, runbooks handed to ops, restore drill completed.

---

## 16. Acceptance checklist — "deployment ready"

The release is deployment-ready when an org can tick all of these without the vendor on the call:

- Fresh install from the documented commands (bare metal, container, and air-gapped bundle) reaches a working login in under one hour.
- OIDC and SAML login work; SCIM deprovision kills a session within 60 s.
- Two users co-edit a whiteboard in two browsers with conflict-free merge; a third with `view` cannot write; an agent proposal can be applied and discarded.
- ACL test suite passes: a user cannot retrieve, cite, open, sync or tool-call a document they lack; a revoked SharePoint permission disappears after one sync.
- All three network modes behave as specified; in `airgapped` the `web.*` tools are absent from the model's tool set (verified in the trace) and the gateway makes zero connections under packet capture.
- Scheduler: the sized number of synthetic users on the sized hardware with the 70B-class reference model, p95 first-token wait under 3 s, no starvation of any user during a 40-step agent run by another.
- Simulation under load: the reference physics harness runs an exclusive-GPU job at its benchmarked wall-clock while the chat workers are at the sized load, with no measurable effect on chat p95; a second exclusive job queues with a visible position rather than degrading the first.
- Efficiency budgets (plugin spec §16.5) all pass on the org's hardware as reported by `localspace bench`: prompt-cache hit ≥ 80 %, ≥ 40 % of calls on the utility model, decode ≥ 90 % of baseline, Client idle CPU < 1 %, idle stream bitrate 0.
- Team mode (W32): a Qwen3-family 100B+-class MoE at Q4 loads from the catalog with one click, the planner reports `hybrid`, and a single stream decodes at ≥ 15 tok/s with the W32 budgets met; a simulation started mid-conversation slows the model without unloading it and the Client shows why.
- A Tier B harness attempting egress or file access outside its grant is blocked and produces a security audit event.
- `backup create` → wipe → `backup restore` yields a byte-identical DAG head set and passing `audit verify`.
- Standby promotion completes in under 3 minutes with sessions intact.
- Audit chain verifies after 24 h of load; SIEM receives every event class listed in §10.1.
- Crypto-shred erasure makes all versions of a document unreadable, including in restored backups made after the shred.
- Upgrade and rollback across one minor version with data intact.
- `localspace doctor` reports green; `/metrics` and alert rules load in Prometheus; every runbook in §11.6 executed once by the org's ops team.
- No outbound connection exists from the server other than to the IdP, the SIEM sink, the configured search backend and admin-initiated provisioning — verified by the org's own egress monitoring.
