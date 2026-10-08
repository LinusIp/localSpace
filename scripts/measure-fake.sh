#!/usr/bin/env bash
# The measurement kit against the test engine: every message must get an
# answer back and Continue must carry through, and the results file must
# hold the thinking pass, the picture with the vision file on the processor
# and on the card, and the engine's reading (docs/DECISIONS.md, 2026-10-08).
# The test engine has nothing to judge; what this catches is the kit, or the
# way answers arrive, breaking without a test failing.
#
#   scripts/measure-fake.sh <localspace binary> <test engine binary> <work dir>
#
# Run from the repository root, by CI's `measure` job and by
# scripts/check.sh. The work dir is emptied first.
set -euo pipefail

localspace="$(realpath "$1")"
engine="$(realpath "$2")"
work="$3"

rm -rf "$work"
mkdir -p "$work/models" "$work/catalog" "$work/out"
work="$(realpath "$work")"

# Two models, whose files tell the test engine how to answer: one that
# streams slowly (twenty words, a pause between each, so that an answer can
# be stopped part-way), and one, hidden as a new entry is, that thinks first
# and has a vision file.
printf 'streams slowly\n' > "$work/models/tiny.gguf"
# Slowly too: an answer that comes whole at once cannot be stopped for the
# Continue step.
printf 'thinks first streams slowly\n' > "$work/models/seer.gguf"
printf 'not a real projector\n' > "$work/models/seer-mmproj.gguf"
tiny="$(wc -c < "$work/models/tiny.gguf" | tr -d ' ')"
seer="$(wc -c < "$work/models/seer.gguf" | tr -d ' ')"
proj="$(wc -c < "$work/models/seer-mmproj.gguf" | tr -d ' ')"
cat > "$work/catalog/catalog.json" <<EOF
{"version": 1, "models": [
  {"id": "tiny", "title": "The test engine", "params_b": 0.1, "bytes": $tiny, "context_len": 2048,
   "repo": "example/tiny", "files": ["tiny.gguf"],
   "tensor": {"core_bytes": 16, "routed_expert_bytes": 0, "layers": 2, "moe": null, "kv_bytes_per_token_fp16": 256}},
  {"id": "seer", "title": "The test engine that thinks and sees", "params_b": 0.1, "bytes": $((seer + proj)), "context_len": 2048,
   "repo": "example/seer", "files": ["seer.gguf", "seer-mmproj.gguf"], "vision_file": "seer-mmproj.gguf", "hidden": true,
   "tensor": {"core_bytes": 16, "routed_expert_bytes": 0, "layers": 2, "moe": null, "kv_bytes_per_token_fp16": 256}}
]}
EOF

status=0
run() {
  local model="$1"
  shift
  # Well under the job's thirty minutes: a run that hangs ends with what it
  # has, and its last progress line says where.
  LOCALSPACE_LLAMA_SERVER="$engine" "$localspace" measure --model "$model" --catalog "$work/catalog" \
    --models "$work/models" --data "$work/data-$model" --out "$work/out" --stop-after 5 \
    --answer-seconds 60 --max-minutes 8 --check "$@" \
    > "$work/measure-$model.log" 2>&1 || status=$?
  tail -n 4 "$work/measure-$model.log"
}
run tiny
run seer --thinking-full

# What the results files must hold.
must() {
  local file="$1" text="$2"
  if ! grep -qF -- "$text" "$file"; then
    echo "missing in $(basename "$file"): $text" >&2
    status=1
  fi
}
tiny_md="$(ls "$work"/out/localSpace-measure-tiny-*.md 2> /dev/null | head -n 1 || true)"
seer_md="$(ls "$work"/out/localSpace-measure-seer-*.md 2> /dev/null | head -n 1 || true)"
if [ -z "$tiny_md" ] || [ -z "$seer_md" ]; then
  echo "a results file is missing in $work/out" >&2
  status=1
else
  must "$tiny_md" "#### Thinking off, the whole script"
  must "$tiny_md" "#### Thinking on, the five messages where it could matter"
  # The test engine begins its twenty words again when carried on, by
  # design; what must hold is that the answer ended whole.
  must "$tiny_md" "The answer ended whole."
  must "$tiny_md" "12 tokens read anew in"
  must "$tiny_md" "About 100 new tokens: read 12 tokens"
  must "$seer_md" "#### Thinking on, the whole script"
  must "$seer_md" "on: 5 words"
  must "$seer_md" "read 12 in 0.0 s; wrote 20 at"
  must "$seer_md" "| on the processor | yes |"
  must "$seer_md" "| on the card | yes, after"
  must "$seer_md" "word1 word2 word3"
fi
if [ "$status" -ne 0 ]; then
  echo "the kit's logs:" >&2
  tail -n 40 "$work"/measure-*.log >&2
fi
exit "$status"
