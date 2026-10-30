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
  {"id": "tiny", "title": "The test engine", "params_b": 0.1, "bytes": $tiny, "context_len": 8192,
   "repo": "example/tiny", "files": ["tiny.gguf"],
   "tensor": {"core_bytes": 16, "routed_expert_bytes": 0, "layers": 2, "moe": null, "kv_bytes_per_token_fp16": 256}},
  {"id": "seer", "title": "The test engine that thinks and sees", "params_b": 0.1, "bytes": $((seer + proj)), "context_len": 8192,
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
# A counting run: the named messages, twice each, thinking off, and
# nothing else (docs/DECISIONS.md, 2026-10-09).
LOCALSPACE_LLAMA_SERVER="$engine" "$localspace" measure --model tiny --catalog "$work/catalog" \
  --models "$work/models" --data "$work/data-count" --out "$work/out-count" --answer-seconds 60 --max-minutes 8 \
  --only "a greeting,arithmetic" --repeat 2 --thinking off --check > "$work/measure-count.log" 2>&1 || status=$?
tail -n 4 "$work/measure-count.log"

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
  must "$seer_md" "The thinking budget ran out on 0 of"
fi
count_md="$(ls "$work"/out-count/localSpace-measure-tiny-*.md 2> /dev/null | head -n 1 || true)"
if [ -z "$count_md" ]; then
  echo "the counting run's results file is missing in $work/out-count" >&2
  status=1
else
  must "$count_md" "#### Thinking off, the chosen messages"
  if grep -q "#### Thinking on" "$count_md" || grep -q '^\*\*Continue' "$count_md"; then
    echo "the counting run ran more than the chosen messages" >&2
    status=1
  fi
  if [ "$(grep -c '^| arithmetic: What is 17 times 24?' "$count_md")" != "2" ]; then
    echo "the counting run did not ask arithmetic twice" >&2
    status=1
  fi
fi
# The person's own pictures (--pictures): a PNG with two questions beside
# it, one picture with none (the default question), and a WebP, which the
# engine cannot read, written down as not asked (docs/DECISIONS.md, 2026-10-18).
mkdir -p "$work/pictures"
cp crates/localspace-cli/assets/measure-picture.png "$work/pictures/a.png"
printf "What is in this picture?
Что на картинке?
" > "$work/pictures/a.png.txt"
cp crates/localspace-cli/assets/measure-picture.png "$work/pictures/b.png"
printf "not a real webp
" > "$work/pictures/c.webp"
LOCALSPACE_LLAMA_SERVER="$engine" "$localspace" measure --model seer --catalog "$work/catalog"   --models "$work/models" --data "$work/data-pictures" --out "$work/out-pictures" --answer-seconds 60 --max-minutes 8   --pictures "$work/pictures" > "$work/measure-pictures.log" 2>&1 || status=$?
tail -n 4 "$work/measure-pictures.log"
pictures_md="$(ls "$work"/out-pictures/localSpace-measure-seer-*.md 2> /dev/null | head -n 1 || true)"
if [ -z "$pictures_md" ]; then
  echo "the pictures run's results file is missing in $work/out-pictures" >&2
  status=1
else
  must "$pictures_md" "#### The pictures asked about, thinking off"
  must "$pictures_md" "| a.png | What is in this picture? |"
  must "$pictures_md" "| a.png | Что на картинке? |"
  must "$pictures_md" "| b.png | What is in this picture? |"
  must "$pictures_md" "| c.webp | What is in this picture? | - | - | - | **not asked: not a PNG or JPEG"
  if grep -q "#### Thinking off" "$pictures_md"; then
    echo "the pictures run ran the script too" >&2
    status=1
  fi
fi
# One chat (--chat): three messages, each in turn in the same chat
# (docs/DECISIONS.md, 2026-10-18).
printf "First message.
Second message.

Third message.
" > "$work/chat.txt"
LOCALSPACE_LLAMA_SERVER="$engine" "$localspace" measure --model tiny --catalog "$work/catalog"   --models "$work/models" --data "$work/data-chat" --out "$work/out-chat" --answer-seconds 60 --max-minutes 8   --chat "$work/chat.txt" > "$work/measure-chat.log" 2>&1 || status=$?
tail -n 4 "$work/measure-chat.log"
chat_md="$(ls "$work"/out-chat/localSpace-measure-tiny-*.md 2> /dev/null | head -n 1 || true)"
if [ -z "$chat_md" ]; then
  echo "the chat run's results file is missing in $work/out-chat" >&2
  status=1
else
  must "$chat_md" "#### One chat, thinking off, message after message"
  must "$chat_md" "| 1 | First message. |"
  must "$chat_md" "| 3 | Third message. |"
  if grep -q "#### Thinking off" "$chat_md"; then
    echo "the chat run ran the script too" >&2
    status=1
  fi
fi
if [ "$status" -ne 0 ]; then
  echo "the kit's logs:" >&2
  tail -n 40 "$work"/measure-*.log >&2
fi
exit "$status"
