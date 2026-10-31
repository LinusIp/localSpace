# 2026-10-31: a picture counts as 1,150 tokens on engine b11960

The engine test (run 41190869498, on the package of `main`'s engine move) asked the engine itself what one picture costs. The picture was `docs/screenshot.png` (1,280×705), and the cost is the prompt tokens with it less the same short question without it.

| Model | Tokens |
|---|---|
| Qwen3.5 4B | 1,127 |
| Qwen3.5 9B | 1,127 |
| LFM3-VL 2B | 1,127 |
| Gemma 4 E2B | 482 |
| Gemma 4 E4B | 482 |

- **On b10869 the same picture cost Qwen3.5 882.** b11960 resizes pictures as transformers and torchvision do (llama.cpp v0.8.0).
- **`prompt::PICTURE_TOKENS` goes from 900 to 1,150:** the largest measured cost, rounded up, as 882 became 900. Counting a Gemma picture high only trims a chat a little earlier; counting a Qwen picture low would let a chat overrun the engine's context.
- **`release/0.2` keeps 900 on b10869.**
