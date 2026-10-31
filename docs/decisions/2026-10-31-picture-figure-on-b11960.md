# 2026-10-31: a picture still counts as 900 tokens on engine b11960

**Corrected the same day.** An earlier version of this file raised `prompt::PICTURE_TOKENS` to 1,150 from a wrong measurement. That figure is in 0.3.0-preview.1. `main` is back at 900.

- **What went wrong:** the engine test (run 41190869498) sent `docs/screenshot.png` to the engine at its full size, 1,434×790. The app never sends that: Core shrinks every picture to a longer side of 1,280 pixels first (`picture::LONGEST_SIDE`), here 1,280×705. The larger picture cost Qwen3.5 1,127 tokens and Gemma 4 482.
- **What the app really sends:** the release check of 0.3.0-preview.1 (run 41201659482) attached the same screenshot through the app on b11960. The engine read 947 tokens for it with a short question, the same as on b10869. That's 882 for the picture, so the figure stays **900**.
- **In 0.3.0-preview.1:** each picture is counted about 270 tokens high. A chat with pictures drops its oldest messages a little sooner than it needs to. Nothing overflows the engine's context.
- **The engine test now shrinks the picture as the app does** (longest side 1,280) before it asks the engine, so its figure is the app's.
