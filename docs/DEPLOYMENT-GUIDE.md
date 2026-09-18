# localSpace — deployment guide

Two parts, because there are two ways to run it. **Part A** is one person on one machine — the ten laptops next week. **Part B** is a company: one server, many employees on their own laptops, no inference on those laptops. The DGX Spark appears in Part B as one possible server, not as a requirement.

Naming used throughout: **Core** is what employees connect to and where all data lives. A **worker** is a process that runs the model and stores nothing. In Part A both run on the same laptop and nobody sees them.

> **This guide is the target state, not today's build.** As of 18 September 2026 the following do not exist yet and are being built in the order given in document 3. The guide follows the build: when an item lands, the matching section here stops carrying a warning.
>
> - The `localspace install`, `localspace models download` / `import`, `localspace backup` and `doctor --running` commands. The CLI today has `serve`, `admin`, `doctor`, `bench`, `evals`, `call` and `audit`.
> - `tls = { cert, key }` — refused at start today; only `"behind-proxy"` works. `--self-signed` does not exist.
> - `[models]` accepts only `dir`. `default`, `embedding` and `[[models.worker]]` are refused; an administrator points Core at an external endpoint at runtime instead.
> - GPU detection is `nvidia-smi` only, so an AMD or Intel machine reports no GPU.
> - `-ngl 999` is hardcoded; the partial-offload path uses described rather than measured free VRAM.
> - Downloads restart from zero and nothing is checksummed.
> - No inference engine is included in any package — it must be placed by hand.
> - There is no installer. Personal mode exists (the desktop app, and `serve --personal`), but only from a checkout; there is no Windows package yet.
> - `serve` refuses to start below the hardware floor without `--allow-below-floor`. In personal mode that becomes a plain-words statement rather than a refusal; in server mode the gate stays.
>
> Found when this guide was read against the build on 18 September 2026, and as much *not yet* as the list above:
>
> - §B4's `localspace admin model download` does not exist either. An administrator downloads a model from the Models page of the app.
> - §B5's separate worker unit is not how it runs today: Core starts `llama-server` itself, per model, on a loopback port. The unit file and `worker.env` have nothing to attach to until a worker entry exists.
> - §B6: `[models] tiers`, `default_tier` and `utility`, and `[audit] retention_days`, are refused at start as keys this release does not honour. The `[organisation]`, `[server]`, `[storage]`, `[auth]` and `[network]` keys shown are honoured, `tls = { cert, key }` excepted.
> - §B8.1 and §B9: Admin has People and Workspaces; there is no Overview page yet. There is no `systemctl reload`: a changed settings file takes a restart.
> - §B3 is the honest state for Linux until the tarball arrives with the server test: build from source.

---

# Part A — one person, one machine

## A1. What the machine needs

Windows 10/11 or Linux, x86-64. 16 GB of system RAM. 20 GB of free disk for the app and one small model — more if you want a larger one. A GPU is not required; without one the app runs on the CPU and says so.

Any GPU is usable: NVIDIA, AMD or Intel. The default backend is Vulkan, so there is no driver stack to install beyond the ordinary graphics driver the machine already has.

## A2. Install

Run the installer. There is no configuration step, no terminal, no port to choose.

At first run the app checks the machine and shows what it found in plain words — "NVIDIA RTX 4060, 8 GB of graphics memory, 32 GB of system memory" — followed by the model it recommends and roughly how fast it will be. Accept it and it downloads; the download resumes if the connection drops and is checksum-verified when it finishes.

Then it is a chat window. Nothing else is installed by default — the whiteboard and every other tool comes from the Store when you want it.

## A3. Choosing a different model

Settings → Model shows the whole catalog with a verdict for this machine against each entry: *runs well*, *runs slowly*, *will not fit*, and an estimated speed. To use something not in the catalog, paste its Hugging Face repo id and the same verdict appears before anything downloads.

The rule the app is applying, if you want to check its work: a model at Q4 needs roughly 0.6 GB per billion parameters, plus a gigabyte or two for the context. What fits in VRAM runs fast; what spills into system RAM runs at a fraction of that; what does not fit in RAM either will not run at all.

## A4. When something is wrong

| Symptom | Cause |
|---|---|
| "No usable GPU found" on a machine with a GPU | Graphics driver older than the Vulkan version required — update the driver |
| Much slower than the estimate | Another application is holding VRAM; close it and restart the app |
| Download stalls | It resumes — leave it, or restart it from Settings → Model |
| Model will not load after a driver update | Delete the app's cache folder from Settings → Advanced → Diagnostics |

## A5. Recording results (for next week)

For each laptop write down: GPU and VRAM, system RAM, what the app recommended, what was actually run, first-token latency, and steady tokens per second. Ten rows of that is the most valuable thing the week produces.

---

# Part B — a company server

## B1. Choosing the server machine

Any 64-bit Linux box with a GPU and enough memory for the model you want. What changes with the machine is which model class makes sense:

| Server | Model to run | Concurrency |
|---|---|---|
| One 24 GB consumer GPU | a 24–32B dense model at Q4 | about 10 at once |
| One 48–80 GB data-centre GPU | a 70B dense at Q4, or a 30B MoE | about 50 |
| DGX Spark (128 GB unified, 273 GB/s) | **mixture-of-experts only** — gpt-oss-120b | about 60 |
| Multi-GPU server | anything, including 70B+ dense | hundreds |

The Spark row carries a trap worth stating plainly: on that machine a dense 70B runs at **2.7 tokens/s**, which nobody will accept, while an MoE with ~5B active parameters runs at about 60. Bandwidth, not capacity, is the limit. The catalog says so before you download 60 GB, but know it in advance.

A Spark is also **aarch64**, so it needs an aarch64 build of Core and of the worker. If an x86 machine with a GPU is available, it is the simpler choice and proves exactly the same thing about the company model.

## B2. Prerequisites

A hostname and a static address the employees' laptops can reach. Working NTP — audit chains and sessions care about clocks. A DNS name for the product, for example `ai.company.internal`, pointing at the server.

**Certificate.** localSpace refuses to serve plaintext on a non-loopback address. That refusal is deliberate; do not override it. Three ways to satisfy it:

1. A certificate from the company CA — what a real deployment does.
2. A reverse proxy such as Caddy in front, terminating TLS.
3. `localspace install --self-signed`, which generates a certificate and prints its fingerprint. The desktop app shows that fingerprint on first connection and asks the person to confirm it, then pins it. This is the right answer for a LAN test and for small deployments without a CA.

## B3. Build and install

```bash
curl https://sh.rustup.rs -sSf | sh -s -- -y && source ~/.cargo/env
sudo apt install -y build-essential cmake pkg-config libssl-dev nodejs npm libvulkan-dev

# the inference worker — Vulkan covers every GPU vendor
git clone https://github.com/ggml-org/llama.cpp && cd llama.cpp
cmake -B build -DGGML_VULKAN=ON
cmake --build build --config Release -j"$(nproc)" --target llama-server
sudo cp build/bin/llama-server /usr/local/bin/

# on an NVIDIA box, optionally build the faster CUDA path instead:
#   cmake -B build -DGGML_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES=<cc>
#   89 for Ada, 90 for Hopper, 120 for consumer Blackwell, 121 for GB10/Spark

# localSpace
git clone <the repository> localspace && cd localspace
cargo build --release -p localspace-cli
(cd web && npm ci && npm run build)
sudo ./target/release/localspace install --config /etc/localspace/localspace.toml
```

## B4. The model

```bash
localspace admin model download <model-id>     # checksum verified
```

Pick from the catalog, which shows the verdict for *this* server before downloading. On a Spark pick an MoE (`gpt-oss-120b` for chat, `gpt-oss-20b` for the utility role). On a 24 GB GPU pick a mid dense model. To use something outside the catalog, paste its Hugging Face repo id in Admin → Models.

An air-gapped site imports the model and the catalog index from a drive instead.

## B5. The worker

`/etc/localspace/worker.env`:

```
MODEL=/var/lib/localspace/models/<model>/<file>.gguf
NGL=999          # layers on GPU; 999 = all. Lower it if the model does not fit VRAM
```

`/etc/systemd/system/localspace-worker.service`:

```ini
[Unit]
Description=localSpace inference worker
After=network-online.target

[Service]
EnvironmentFile=/etc/localspace/worker.env
ExecStart=/usr/local/bin/llama-server \
  -m ${MODEL} \
  --host 127.0.0.1 --port 8080 \
  -ngl ${NGL} --no-mmap \
  -c 32768 --parallel 8 --cont-batching \
  --flash-attn on --cache-type-k q8_0 --cache-type-v q8_0 \
  --jinja --reasoning-format auto \
  --metrics
Restart=always
RestartSec=5
User=localspace

[Install]
WantedBy=multi-user.target
```

`--jinja` is required for gpt-oss's chat template and reasoning-effort handling. `--no-mmap` measurably improves load time on unified-memory machines. `--parallel 8` gives eight concurrent slots; raise it once you have measured. Bind the worker to loopback — employees reach Core, never the worker.

```bash
sudo systemctl enable --now localspace-worker
curl -s http://localhost:8080/health
```

## B6. Core

`/etc/localspace/localspace.toml` — the keys that matter:

```toml
[organisation]
name = "Company Name"                   # shown on sign-in, invitations and the tab

[server]
bind = "0.0.0.0:8443"
public_url = "https://ai.company.internal"
tls = { cert = "/etc/localspace/tls/cert.pem", key = "/etc/localspace/tls/key.pem" }
# or tls = "behind-proxy" with trusted_proxies = ["127.0.0.1/32"]

[storage]
root = "/var/lib/localspace"            # everything lives here — this is the backup

[auth]
provider = "local"                      # company email + invitation links
session_ttl = "12h"

[models]
tiers = { fast     = { pool = "chat", reasoning = "low" },
          balanced = { pool = "chat", reasoning = "medium" },
          capable  = { pool = "chat", reasoning = "high" } }
default_tier = "balanced"
utility   = "<a small model>"

[[models.worker]]
pool = "chat"
url  = "http://127.0.0.1:8080"
# more workers, on this box or another, are more entries

[network]
mode_ceiling = "airgapped"              # fully offline
allowlist = []

[audit]
sink = ["local"]
retention_days = 730
```

Tiers are a setting on one model, not three models — Fast, Balanced and Most capable are low, medium and high reasoning effort on the same worker pool. A distinct model per tier is also supported, which is the right answer on a multi-GPU server.

```bash
sudo systemctl enable --now localspace
sudo journalctl -u localspace -f        # prints the one-time first-admin link
```

Open the link, set the administrator password, then Admin → People to invite people, and the Store to install the whiteboard into the shared workspace.

## B7. Employees

**Desktop app:** install it, choose *Connect to your organisation*, enter `ai.company.internal`, confirm the certificate fingerprint if the server is self-signed, sign in with the company email and the password set from the invitation.

Their laptop does no inference. It needs no GPU, no model download and no disk space beyond the app. A five-year-old machine with integrated graphics is a perfectly good client.

**Browser:** `https://ai.company.internal`, same account, same data.

Roles are Administrator, Member and Can-view-only. There is no seat limit; the People page shows how many accounts exist.

## B8. Verify before letting anyone in

1. Admin → Overview shows the worker healthy.
2. From a *different machine on the network*, not the server: open the address in a browser and sign in. This is the step that has never been exercised — do it first.
3. Desktop app on that same other machine: connect, confirm the fingerprint, sign in, send a message. Confirm first token under two seconds.
4. Confirm the client machine's GPU is idle during generation — `nvidia-smi` or Task Manager. If it is not, the client is doing local inference and the mode is wrong.
5. Three people on one shared board from three different machines — edits cross within a second, presence avatars correct.
6. A view-only account cannot write on the board; the refusal appears in the audit log.
7. With `mode_ceiling = "airgapped"`, packet-capture the server's uplink through a full session: zero packets out. This is the demonstration a security team wants.
8. `localspace audit verify` — chain intact.

## B9. Operations

**Backups.** `localspace backup create` nightly to a network share. Everything irreplaceable is under `/var/lib/localspace`; workers hold nothing.

**Updates.** Replace the binary and restart. Workers change only when the model does.

**Monitoring.** `/metrics` on Core; Admin → Overview shows the same picture.

**Adding a second worker later.** Install the unit on the other box, copy the model, add one `[[models.worker]]` entry pointing at it, `systemctl reload localspace`. Core health-checks it and routes to it; Core does not launch it.

## B10. When something is wrong

| Symptom | Cause |
|---|---|
| Refuses to bind | No TLS and no trusted proxy — §B2, do not override |
| Employees cannot connect | DNS or firewall between their machines and the server, not localSpace |
| Desktop app rejects the certificate | Fingerprint changed since first connection — reissue, or clear the pin in Settings → Advanced |
| Model painfully slow | A dense model on a bandwidth-limited box. §B1 |
| Worker never healthy | Wrong model path, or still loading — `curl localhost:8080/health` on that box |
| Server will not start | `journalctl -u localspace -e` gives the reason in one line |

---

## Not in this release — tell the site before they ask

Uploading documents and asking questions over them; single sign-on (accounts are created by the administrator); automatic failover; cross-machine model sharding — one model per box, many boxes, HTTP between them is the design. The whiteboard works but does not yet have everything Miro does.
