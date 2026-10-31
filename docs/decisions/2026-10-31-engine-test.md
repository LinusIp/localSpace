# 2026-10-31: the engine test

`.github/workflows/engine-test.yml` is started by hand with a package run's id. It's how `docs/BUILD.md`'s "When the pin moves" is done on GitHub's Windows machine.

- **One machine per model:**
  - the package's files are checked against their SHA-256s;
  - `localspace measure` runs on a fresh data folder: the message script, thinking, Continue, the read time of short messages, and the picture;
  - `scripts/engine-check.mjs` asks the engine itself:
    - for a picture model, one picture's real cost: `docs/screenshot.png` (1280×705), the prompt tokens with it less the same message without it;
    - how `-c 8192` is divided between slots under `--parallel 1`, `--parallel 2` and no `--parallel`.
- **The engine folder is listed,** so the keep list is read again.
- **The summary** is a table of the picture costs and the slot sizes, and names what failed. The answers are in each model's measure file in the artifact, and they're read before the pin moves.
- **The models by default:**
  - Qwen3.5 4B and 9B;
  - Gemma 4 E2B and E4B;
  - LFM3 1.7B and LFM3-VL 2B.
- **Not tested here:** Qwen3 30B A3B and gpt-oss 120B, which are larger than the machine's 16 GB. They stay hidden on `main` ([2026-10-31-engine-b11960.md](2026-10-31-engine-b11960.md)).
- **The picture figure:** if a Qwen3.5 picture costs more than the 900 tokens Core counts (`PICTURE_TOKENS`), the figure is raised to the measured cost.
- **Limits of this machine:** it has no graphics card. Its figures compare one engine with another here and are never published.
