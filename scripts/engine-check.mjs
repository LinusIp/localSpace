// What a new engine pin does with one model, read from the engine itself
// (docs/BUILD.md, "When the pin moves"; docs/decisions/2026-10-31-engine-test.md):
//   - a picture's real cost: the prompt tokens of one message with the
//     picture, less the same message without it. Give it the picture as the
//     app sends it, already shrunk to a longer side of 1,280 pixels
//     (picture::LONGEST_SIDE); the workflow does;
//   - how `-c` is divided between the slots under `--parallel 1`,
//     `--parallel 2` and no `--parallel`, from `/props` and `/slots`.
//
//   node scripts/engine-check.mjs <llama-server> <model.gguf> [--mmproj <file>] [--picture <png>] [--out <file.json>]
//
// Starts the engine on loopback, asks, stops it. Prints one JSON object; the
// figures are this machine's, with the engine on the processor.

import { spawn } from "node:child_process";
import fs from "node:fs";

const args = process.argv.slice(2);
const [server, model] = args;
const option = (name) => {
  const i = args.indexOf(name);
  return i > 0 ? args[i + 1] : undefined;
};
if (!server || !model) {
  console.error("usage: node scripts/engine-check.mjs <llama-server> <model.gguf> [--mmproj <file>] [--picture <png>] [--out <file.json>]");
  process.exit(2);
}
const mmproj = option("--mmproj");
const picture = option("--picture");
const out = option("--out");
const CONTEXT = 8192;
let port = 18930;

async function withEngine(extra, use) {
  port += 1;
  const origin = `http://127.0.0.1:${port}`;
  const flags = ["-m", model, "--host", "127.0.0.1", "--port", String(port), "-c", String(CONTEXT), ...extra];
  const engine = spawn(server, flags, { stdio: ["ignore", "ignore", "pipe"] });
  let log = "";
  engine.stderr.on("data", (d) => {
    log = (log + d).slice(-4000);
  });
  try {
    const deadline = Date.now() + 10 * 60 * 1000;
    for (;;) {
      if (engine.exitCode !== null) throw new Error(`the engine ended (${engine.exitCode}):\n${log}`);
      if (Date.now() > deadline) throw new Error(`the engine was not ready in ten minutes:\n${log}`);
      const ok = await fetch(`${origin}/health`).then((r) => r.ok, () => false);
      if (ok) break;
      await new Promise((r) => setTimeout(r, 1000));
    }
    return await use(origin);
  } finally {
    engine.kill();
  }
}

async function json(url, body) {
  const res = await fetch(url, body === undefined ? {} : {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  const text = await res.text();
  if (!res.ok) throw new Error(`${url} answered ${res.status}: ${text.slice(0, 400)}`);
  return JSON.parse(text);
}

async function promptTokens(origin, content) {
  const answer = await json(`${origin}/v1/chat/completions`, {
    messages: [{ role: "user", content }],
    max_tokens: 1,
    temperature: 0,
    cache_prompt: false,
  });
  return answer.usage?.prompt_tokens;
}

const result = { engine: server, model, context: CONTEXT };

if (mmproj && picture) {
  const question = "What is in this picture? One sentence.";
  const url = `data:image/png;base64,${fs.readFileSync(picture).toString("base64")}`;
  result.picture = await withEngine(["--mmproj", mmproj, "-np", "1"], async (origin) => {
    const without = await promptTokens(origin, question);
    const withIt = await promptTokens(origin, [
      { type: "image_url", image_url: { url } },
      { type: "text", text: question },
    ]);
    return { file: picture, prompt_tokens_without: without, prompt_tokens_with: withIt, cost: withIt - without };
  });
}

result.slots = {};
for (const [name, extra] of [["--parallel 1", ["--parallel", "1"]], ["--parallel 2", ["--parallel", "2"]], ["no --parallel", []]]) {
  result.slots[name] = await withEngine(extra, async (origin) => {
    const props = await json(`${origin}/props`);
    const slots = await json(`${origin}/slots`).catch((e) => ({ error: String(e.message) }));
    return {
      total_slots: props.total_slots,
      n_ctx_props: props.default_generation_settings?.n_ctx,
      n_ctx_per_slot: Array.isArray(slots) ? slots.map((s) => s.n_ctx) : slots,
    };
  });
}

const text = JSON.stringify(result, null, 2);
if (out) fs.writeFileSync(out, text + "\n");
console.log(text);
