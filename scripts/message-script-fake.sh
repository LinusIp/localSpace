#!/usr/bin/env bash
# The message script against the test engine: every message must get an
# answer back and the Continue step must carry through (docs/DECISIONS.md,
# 2026-09-24, answers on the prompt item). The test engine has nothing to
# judge; what this catches is the script, or the way answers arrive, breaking
# without a test failing, as 1.6 and 1.7 did to it.
#
#   scripts/message-script-fake.sh <localspace binary> <test engine binary> <work dir>
#
# Run from the repository root, by CI's `message-script` job and by
# scripts/check.sh. The work dir is emptied first.
set -euo pipefail

localspace="$(realpath "$1")"
engine="$(realpath "$2")"
work="$3"
port="${MESSAGE_SCRIPT_PORT:-8447}"

rm -rf "$work"
mkdir -p "$work/data/models" "$work/catalog"
work="$(realpath "$work")"
# Paths as the server reads them: a Windows build under Git Bash wants C:/…
native="$work"
if command -v cygpath > /dev/null 2>&1; then native="$(cygpath -m "$work")"; fi

# One model, whose file tells the test engine how to answer: twenty words, a
# pause between each, so that an answer can be stopped part-way.
printf 'streams slowly\n' > "$work/data/models/tiny.gguf"
bytes="$(wc -c < "$work/data/models/tiny.gguf" | tr -d ' ')"
cat > "$work/catalog/catalog.json" <<EOF
{"version": 1, "models": [{
  "id": "tiny", "title": "The test engine", "params_b": 0.1, "bytes": $bytes, "context_len": 2048,
  "repo": "example/tiny", "files": ["tiny.gguf"],
  "tensor": {"core_bytes": 16, "routed_expert_bytes": 0, "layers": 2, "moe": null, "kv_bytes_per_token_fp16": 256}
}]}
EOF
cat > "$work/localspace.toml" <<EOF
[server]
bind = "127.0.0.1:$port"
[storage]
root = "$native/data"
[models]
dir = "$native/catalog"
EOF

LOCALSPACE_LLAMA_SERVER="$engine" "$localspace" serve --config "$work/localspace.toml" \
  --personal --token message-script --allow-below-floor > "$work/serve.log" 2>&1 &
server=$!
stop_server() {
  kill "$server" 2> /dev/null || true
  wait "$server" 2> /dev/null || true
  # The test engine this run started, and no other.
  pkill -f "$work/data/models" 2> /dev/null || true
}
trap stop_server EXIT

for _ in $(seq 1 60); do
  curl -fsS "http://127.0.0.1:$port/healthz" > /dev/null 2>&1 && break
  sleep 1
done

status=0
node scripts/message-script.mjs "http://127.0.0.1:$port" message-script tiny \
  --stop-after 5 --check --out "$work/answers.md" || status=$?
if [ "$status" -ne 0 ]; then
  echo "the server's log:" >&2
  tail -n 40 "$work/serve.log" >&2
fi
exit "$status"
