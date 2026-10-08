# Where a model's vision file runs: the measurements

The rule for where a model's vision file (its image reader, llama.cpp's
`--mmproj`) runs comes from measurements, not from a guess
(docs/DECISIONS.md, 2026-10-08): the kit (`localspace measure`) runs the
picture with the file on the processor and again on the card, on every
machine it reaches, and this table collects what came back. The app starts
every model with the file on the processor until the rule says otherwise.

The picture is the kit's: a blue circle on a red square, 64 by 64. "Came
up" is whether the model started with the file where asked; the speeds are
the engine's own figures for the answer to the picture question.

| Card | Model | On the processor | On the card | Run |
|---|---|---|---|---|
| RTX 3050 Ti Laptop, 4 GB (16 GB system) | Qwen3.5 4B Q4_K_M, 22 of 37 layers on the card | came up; answered right in 1.0 s, first word 0.5 s, 38.2 tokens a second | came up after 13 s; answered right in 1.8 s, first word 1.1 s, 22.7 tokens a second | 2026-10-08, the development laptop, package `efce161` |

What the 4 GB card says so far: it holds the 4B's vision file, and the
model writes at six tenths of its speed with it there. The rule for 8, 16
and 24 GB cards waits for the kit on those machines.
