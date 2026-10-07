# The prompt as turns: before and after

The item of documents 24, 25 and 27 and the rulings of 2026-10-07
(docs/DECISIONS.md, 2026-09-24, 2026-10-06 and 2026-10-07): the prompt as a
system message and turns, with the ledger after the newest message; no tools,
no `find_capability` and no ledger when nothing is installed from the Store;
and a raw tool call never shown to a person. It is judged by
`scripts/message-script.mjs` on every catalog default with nothing installed,
and on the 7B and the 14B with the whiteboard installed from the catalog as
the Store installs it (and so in focus): that path has the biggest prompt and
keeps the ledger (document 27). Each run is on a fresh data folder, against
the pinned engine (b10869, Vulkan) on the development laptop.

**Every count comes from one build each** (ruling 5 of 2026-10-07): before,
the package the `package` workflow built from `4009fc5`, whose prompt is the
one of 2026-09-24 (`before/`, `before/whiteboard/`); after, the package of the
final commit `4f4669d` (`after/`, `after/whiteboard/`). The answers are read by
a person; the counts below are that reading, with the same rules for both.
The prompt's sizes and the time to the first word are read from the server's
own log of every answer.

**What is counted, in each of a model's 21 answers** (twenty messages and the
answer carried on by Continue):
- **Layout echo**: the answer says the prompt's own layout or its contents
  back: `[conversation]`, `user:`, `assistant:`, `tool: ->`, `[task …]`,
  `goal:`, `[tools]`, `[state]`, `[environment]`, or what stands under them.
- **Tool talk**: the answer's words are about tools, capabilities, installing,
  or the assistant's own plan or notes ("there is no tool installed for…",
  "let's use the available tools", "planning a task"). Saying plainly that it
  cannot know today's weather is not tool talk, nor is offering to describe a
  picture or find one.
- **Pretending to make a picture**, for "Draw me a picture of a cat.": a fake
  image link, or a description passed off as a picture. Saying it cannot make
  pictures is fine; with the whiteboard installed, drawing the cat on the
  board is the tool doing its work.
- **A raw call shown**: the answer the person sees is, or holds, a tool call
  written as text (`{"name": …, "arguments": …}`).

## The counts

| | Layout echo | Tool talk | Pretends a picture | Raw call shown | Answers that reached for a tool |
|---|---|---|---|---|---|
| Qwen2.5 0.5B | 0 → 1 | 2 → 0 | 0 → 0 | 0 → 0 | 2 → 0 |
| Qwen2.5 1.5B | 0 → 0 | 1 → 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| Qwen2.5 7B | 0 → 0 | 1 → 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| Qwen2.5 14B | 0 → 0 | 4 → 0 | 0 → 0 | 2 → 0 | 4 → 0 |
| Qwen2.5 7B, whiteboard | 0 → 0 | 3 → 1 | 0 → 0 | 0 → 0 | 1 → 2 |
| Qwen2.5 14B, whiteboard | 0 → 0 | 3 → 1 | 0 → 0 | 1 → 0 | 0 → 1 |

Continue carried on mid-sentence in every run, before and after.

**The one layout echo after** is the 0.5B's answer to "What can you do?":
"As a small model, I can do many things! I can be a simple toy, a small
sculpture, or even a small robot." The system message still names the model
profile ("model profile: small model") in its place in the layout, and the
0.5B said the name back.

**The tool talk, word for word.** Before, with nothing installed: 0.5B "I can
help you find out what tools are installed on your system", "What tool do you
need assistance with?"; 1.5B "I don't have any specific information or tools
available to assist you"; 7B "We can use a tool to find a weather service, but
it seems we don't have such a tool available right now. Let's see if we can
find one."; 14B "I can help you plan out steps for a task, keep notes, and find
capabilities that match your needs", "Let's find out how far the Moon is from
the Earth using the available tools" followed by a `find_capability` call
written as text, the whole answer to "What is 17 times 24?" being
`[{"name": "task.note", "arguments": {"text": "Calculate 17 times 24."}}]`, and
"I don't have a tool that can directly provide the current weather data … find
a suitable tool". Before, with the whiteboard: 7B "creating notes, planning
steps, adding shapes and frames to the board", "Would you like to implement
this function in the canvas", "I will need to find a suitable tool for you";
14B "How would you like to start using these tools?", the translation written
as `{"name": "translate_text", "arguments": {…}}`, and "Let's start by adding a
frame for the cat … Would you like to proceed with this plan?". **After**, with
the whiteboard only: the 7B and the 14B each list the board's actions, notes
and plans when asked "What can you do?".

**The cat.** Before, with nothing installed: the 0.5B "I can draw a picture of
a cat." (a claim, nothing passed off as a picture), the 1.5B "I'm sorry, but I
can't assist with that.", the 7B and the 14B decline and offer to find a
picture. After: all four decline in plain words, the 7B and the 14B offering to
describe one. With the whiteboard, before: the 7B drew a frame and a grey
ellipse, the 14B described a plan and asked whether to go on; after: the 7B
drew three shapes and a label, the 14B drew the cat's face and body in 23 board
calls.

**The raw calls.** The 14B wrote the `translate_text` call again in the final
run with the whiteboard; Core read it as an attempted call to a tool that is
not on offer, refused it in one line and let the model try once more, and the
log says so ("a call to `translate_text`, which is not on offer, was written
into the answer; the model was asked to answer in words"). The second try
answered in words, correcting itself on the way to "Wo ist der nächste
Bahnhof?". Nothing of the call was shown.

## The prompt and the first word

| | Stable part, read at load | Prompt of an answer (median) | First word (median) |
|---|---|---|---|
| Nothing installed | 774 → **47** tokens | 827 → **62** tokens | 0.5B and 1.5B about 0.1 s both; 7B 0.4 → 0.4 s; 14B 1.4 → 1.4 s |
| Whiteboard installed | 3,451 → 3,411 tokens | 3,504 → 3,425 tokens | 7B 0.5 → **0.8 s**; 14B 1.5 → **2.3 s** |

With nothing installed the system message is 47 tokens where it was 774, and
the first word comes as soon as before: the warm-up already kept the long one
in the engine's cache. **With the whiteboard installed the first word comes
later** after the change, by about 0.3 s on the 7B and 0.8 s on the 14B
(medians). It is not the machine: the before build run again on the same
morning, minutes before the after runs, gave 0.5 s and 1.5 s (the evening run
of 2026-10-06 gave 0.6 s and 1.5 s), and the engine wrote at the same speed in
all three runs (about 100 ms a token on the 7B, 300 on the 14B). It is where
the engine reads the prompt: it reuses about 99 % of it from its cache in both
layouts, and reads fewer new tokens after (8 to 25 for a short message where
it read 27 to 58), yet takes longer over them (median 2.4 s a message on the
14B where it took 1.4 s). Why is not found yet.

## Found during the item, and what came of it

- **"my working set is 8000 tokens"**: on the first after run (package
  `89dd27d`) the 7B told a person so, read off the profile's budgets in the
  system message. The profile is now given by name only, and a test holds the
  system message to no word a member may not be shown.
- **The 0.5B pretended to draw** with nothing installed after the change, in
  both runs ("Sure! Here's a simple drawing of a cat: [Image of a small cat]").
  With the ruled sentence, "You reply in text only. You cannot create images.",
  it declined in both of two runs, so the sentence stays; in one of them it
  still said, asked what it can do, "I can perform various tasks, such as
  drawing, painting, or even creating animations".
- **The 14B's list-wrapped call** (`[{"name": "task.note", …}]`), found by the
  before rerun: a list's first item now counts as the block, and a call to a
  tool on offer written so is read as the call.

## Answer quality noted, not counted

- **The 7B and the weather**: after the change it said, with nothing
  installed, "Today in Lisbon, the weather is expected to be mostly sunny with
  a high around 22°C"; before, it offered to look the weather up. Ruled answer
  quality (ruling 4): no prompt line; the same message is read on the Qwen3.5
  models in the refresh.
- **The 0.5B** gets sums, the bell's year and France's capital wrong before and
  after; it leaves the defaults at the refresh (document 28).
- **The 14B's translation** after the refused call corrects itself three times
  before it is right.

## For reference: the baseline of 2026-09-24

The first before run (head `81811d4`, a debug build; in git history at
`25a3435`) counted tool talk 2, 1, 2 and 5 for the 0.5B, 1.5B, 7B and 14B, no
layout echo and no pretended picture. The rerun above, on the package of
`4009fc5`, is what the after runs are measured against, so that before and
after are each one release build.
