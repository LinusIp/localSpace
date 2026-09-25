#!/usr/bin/env bash
# What CI runs, on a Linux machine: the jobs of .github/workflows/ci.yml,
# step for step, on a clean copy of a commit. Kept for later: CI runs on every
# push, and no push waits on this (docs/DECISIONS.md, 2026-09-25).
#
#   scripts/check.sh [<commit>] [--w32-gate]
#
# On Windows, run it inside WSL (Ubuntu 24.04, prepared by
# scripts/wsl-setup.sh). The repository checked is the one this script sits
# in, usually the Windows copy under /mnt/c; the commit (HEAD unless named)
# is cloned into ~/localspace-check and built there, on WSL's own disk, with
# the build kept between runs. The working tree is not checked: what is
# pushed is commits. The last line says how it went, for the report.
#
# Differences from the runner, none of which changes a result: nothing is
# cached or uploaded, the runner's disk is not cleared, the build is
# incremental between runs, and the frame times are this machine's.
set -euo pipefail

w32_gate=false
ref=HEAD
for arg in "$@"; do
  case "$arg" in
    --w32-gate) w32_gate=true ;;
    -*) echo "unknown option $arg" >&2; exit 2 ;;
    *) ref="$arg" ;;
  esac
done

source_repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
commit="$(git -C "$source_repo" rev-parse "$ref")"
short="${commit:0:7}"
subject="$(git -C "$source_repo" log -1 --format=%s "$commit")"
home="${LOCALSPACE_CHECK_HOME:-$HOME/localspace-check}"
repo="$home/repo"
temp="$home/tmp"
log="$home/logs/$short.log"
mkdir -p "$home/logs"
: > "$log"
started=$(date +%s)

export CARGO_TERM_COLOR=never RUST_BACKTRACE=1 CARGO_PROFILE_DEV_DEBUG=line-tables-only

finish() {
  local minutes=$(( ($(date +%s) - started + 59) / 60 ))
  echo "$1 (${minutes} min; log $log)"
}

# Each step as CI names it; its output goes to the log. A failure ends the
# check with the step's name and the end of the log.
step() {
  local name="$1"
  shift
  printf '== %s\n' "$name" | tee -a "$log"
  if ! "$@" >> "$log" 2>&1; then
    tail -n 60 "$log" >&2
    finish "check $short \"$subject\": FAILED at \"$name\""
    exit 1
  fi
}

# The commit, cloned clean; the build and the node modules' cache are kept.
if [ ! -d "$repo/.git" ]; then
  git clone --quiet --no-checkout "$source_repo" "$repo"
fi
git -C "$repo" fetch --quiet --force "$source_repo" '+refs/heads/*:refs/remotes/source/*'
git -C "$repo" checkout --quiet --force --detach "$commit"
git -C "$repo" clean --quiet -ffdx -e /target -e '/harnesses/*/target'
cd "$repo"
if [ -n "$(git -C "$source_repo" status --porcelain --untracked-files=no)" ]; then
  echo "note: the working tree has changes that are not committed; they are not checked" | tee -a "$log"
fi

# --- check: format, lint, types ---------------------------------------------
step "rustfmt, the workspace and the harness crates" bash -c '
  set -euo pipefail
  rustup toolchain install stable --profile minimal --component rustfmt,clippy
  rustup default stable
  rustup target add wasm32-wasip2
  cargo fmt --all -- --check
  for crate in harnesses/*-logic; do (cd "$crate" && cargo fmt -- --check); done'
step "npm ci" bash -c 'cd web && npm ci'
step "npm run lint" bash -c 'cd web && npm run lint'
step "types, the app and its build config" bash -c 'cd web && npm run typecheck'
step "types, the canvas package" bash -c 'cd web && npm run typecheck:packages'
step "types, the whiteboard surface" bash -c 'cd web && npm run typecheck:whiteboard'

# --- web: tests, build, sizes, frame times ----------------------------------
step "npm test" bash -c 'cd web && npm test'
step "npm run build" bash -c 'cd web && npm run build'
step "npm run build:whiteboard" bash -c 'cd web && npm run build:whiteboard'
step "bundle sizes against their limits" bash -c 'cd web && node scripts/check-sizes.mjs'
step "canvas frame times against the CI baseline" bash -c '
  set -euo pipefail
  cd web
  if [ -f packages/canvas/bench/baseline.ci.json ]; then
    node e2e/bench-canvas.mjs --profile ci --runner ubuntu-24.04 --tolerance 1.0
  else
    node e2e/bench-canvas.mjs --profile ci --runner ubuntu-24.04 --record
    echo "warning: no CI frame-time baseline is committed; as on the runner, this run only recorded one"
  fi'

# --- rust: clippy, tests ----------------------------------------------------
step "the harness packages the tests load" node scripts/hpack.mjs --in-place
step "clippy, with unwrap_used a warning and everything else an error" \
  cargo clippy --workspace --all-targets -- -D warnings --force-warn clippy::unwrap_used
step "tests" cargo test --workspace
suites=$(grep -c '^test result: ok' "$log" || true)
tests=$(grep '^test result: ok' "$log" | sed -E 's/.* ([0-9]+) passed.*/\1/' | awk '{ n += $1 } END { print n + 0 }')

# --- e2e: the step-5 gate and Phase A's gate, end to end in a browser -------
rm -rf "$temp"
mkdir -p "$temp"
server=""
stop() {
  if [ -n "$server" ]; then
    kill "$server" 2> /dev/null || true
    wait "$server" 2> /dev/null || true
    server=""
  fi
}
trap stop EXIT
step "build the shell, the harness packages and the server" cargo build --release -p localspace-cli
cat > "$temp/localspace.toml" << EOF
[server]
bind = "127.0.0.1:8443"
[storage]
root = "$temp/data"
[harnesses]
catalogs = ["harnesses", "registry"]
EOF
./target/release/localspace serve --config "$temp/localspace.toml" \
  --personal --token ci-token --allow-below-floor > "$temp/serve.log" 2>&1 &
server=$!
for _ in $(seq 1 60); do curl -fsS http://127.0.0.1:8443/healthz > /dev/null 2>&1 && break; sleep 1; done
step "serve, then install the whiteboard from the catalog and walk the gate" \
  bash -c 'cd web && node e2e/whiteboard.mjs http://127.0.0.1:8443 ci-token --no-agent'
stop
cat > "$temp/org.toml" << EOF
[organisation]
name = "Meridian Bank"
[server]
bind = "127.0.0.1:8446"
public_url = "http://127.0.0.1:8446"
[storage]
root = "$temp/orgdata"
[harnesses]
catalogs = ["harnesses", "registry"]
EOF
./target/release/localspace serve --config "$temp/org.toml" --allow-below-floor --insecure \
  > "$temp/org-serve.log" 2>&1 &
server=$!
for _ in $(seq 1 60); do curl -fsS http://127.0.0.1:8446/healthz > /dev/null 2>&1 && break; sleep 1; done
step "an organisation server; two people in two browsers; a viewer refused" \
  bash -c "cd web && node e2e/org.mjs http://127.0.0.1:8446 --data '$temp/orgdata'"
# The chain is checked with the server stopped: the database is the service's while it runs.
stop
step "the audit log verified" ./target/release/localspace audit verify --data "$temp/orgdata"

# --- message-script: the message script against the test engine ------------
step "the server and the test engine" \
  cargo build --release -p localspace-cli -p localspace-core --bin localspace --bin fake_llama_server
step "every message answered, Continue carried through" \
  scripts/message-script-fake.sh target/release/localspace target/release/fake_llama_server "$temp/message-script"
answered=$(grep -o 'message script: .*' "$log" | tail -1 | sed 's/^message script: //')

# --- w32-gate, only when asked, as on the runner ----------------------------
if [ "$w32_gate" = true ]; then
  step "the canvas gate on W32, from its recorded measurement" \
    node scripts/w32-canvas-gate.mjs docs/gates/w32-canvas.json
fi

finish "check $short \"$subject\": passed. Format, lint, types; web tests, build, sizes, frame times; clippy; $tests tests in $suites suites; the browser walks and the audit; message script: $answered"
