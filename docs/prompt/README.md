# The prompt as turns: before and after

The item of documents 24 and 25 (docs/DECISIONS.md, 2026-09-24): the prompt
as a system message and turns, with the ledger after the newest message, and
with no tools, no `find_capability` and no ledger when nothing is installed
from the Store. It is judged by `scripts/message-script.mjs` on every catalog
default, each on a fresh data folder with nothing installed, against the
pinned engine (b10869, Vulkan) on the development laptop, before the change
(`before/`) and after it (`after/`). The answers are read by a person; the
counts below are that reading, with the same rules for both.

**What is counted, in each of a model's 21 answers** (twenty messages and the
answer carried on by Continue):
- **Layout echo**: the answer says the prompt's own layout back:
  `[conversation]`, `user:`, `assistant:`, `tool: ->`, `[task …]`, `goal:`,
  `[tools]`, `[state]`, `[environment]`.
- **Tool talk**: the answer's words are about tools, capabilities, installing,
  or the assistant's own plan or notes ("there is no tool installed for…",
  "let's use the available tools", "planning a task"), or it writes a tool
  call into its text. Saying plainly that it cannot know today's weather is
  not tool talk.
- **Pretending to make a picture**, for "Draw me a picture of a cat.": a fake
  image link, a description passed off as a picture, or a tool call written
  into the text. Saying it cannot make pictures is fine.

## Before (2026-09-24, head 81811d4)

| Model | Layout echo | Tool talk | Pretends a picture | The cat |
|---|---|---|---|---|
| Qwen2.5 0.5B | 0 | 2 | 0 | "I'm sorry, but I can't assist with that." |
| Qwen2.5 1.5B | 0 | 1 | 0 | "I'm sorry, but I can't assist with that." |
| Qwen2.5 7B | 0 | 2 | 0 | it can't draw; offers steps to draw one |
| Qwen2.5 14B | 0 | 5 | 0 | it can't draw; offers to find a picture or guide |

The tool talk, word for word: 0.5B "…help me find the right tools to address
the issue", "What tool would you like to use to help you with your thing?";
1.5B "I don't have any specific information or tools to assist you";
7B "…like writing a note, planning a task, or finding a suitable tool for a
job", "Since there's no tool installed for direct translation…"; 14B "…helping
plan tasks and using specific tools to get things done", "let's use the
available tools to find this information" followed by a `find_capability` call
written as text, "we don't need a special tool for that", "I don't have a tool
that can provide the current weather data", "we don't have a specific tool to
estimate the transportation costs".

Tools reached for, in answers: 0.5B 1, 1.5B 0, 7B 1, 14B 4. No layout echo in
this script; the one seen on 2026-09-23 came from Qwen2.5 3B after two tool
calls in one answer ("Write a story…"), which the script does not ask.
Continue carried on mid-sentence for all four.
