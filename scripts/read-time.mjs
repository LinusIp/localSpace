// How long the engine takes to read a few new tokens on top of a long cached
// prompt: one item of the pin-move list (docs/BUILD.md, "When the pin
// moves"; docs/DECISIONS.md, 2026-10-07). b10869 on the development laptop
// read up to about thirty new tokens one at a time and more than that
// together, so a short message waited longer than a long one.
//
//   node scripts/read-time.mjs <engine origin> <serve origin> <token>
//
// The engine is llama-server started by hand with the flags app.log shows
// for the model (-c, -ngl, --device), and no key. The server is a
// `localspace serve --personal` with the whiteboard installed, from which the
// system message and the tools of a turn are taken, as Core sends them. The
// engine reads that once, then messages of about 10, 30 and 100 new tokens,
// three times each; its own timings are printed, median first.

const [engine, serve, token] = process.argv.slice(2);
if (!engine || !serve || !token) {
  console.error("usage: node scripts/read-time.mjs <engine origin> <serve origin> <token>");
  process.exit(2);
}

const headers = { Authorization: `Bearer ${token}`, "content-type": "application/json" };
const request = async (body) =>
  (await fetch(`${serve}/api/v1/request`, { method: "POST", headers, body: JSON.stringify(body) })).json();
const system = (await request({ preview_context: { budget: 0 } })).context?.prompt_preview;
const active = (await request("get_active_set")).active;
if (!system || !active) throw new Error("the server gave no prompt or no active set");
const tools = active.tools.map((t) => ({
  type: "function",
  function: { name: t.name.replaceAll(".", "__"), description: t.summary, parameters: t.params },
}));

async function timings(message) {
  const res = await fetch(`${engine}/v1/chat/completions`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      messages: [
        { role: "system", content: system },
        { role: "user", content: message },
      ],
      tools,
      tool_choice: "auto",
      max_tokens: 1,
      temperature: 0,
      stream: false,
      cache_prompt: true,
    }),
  });
  const json = await res.json();
  if (!res.ok) throw new Error(JSON.stringify(json).slice(0, 300));
  return json.timings;
}

const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  const m = Math.floor(s.length / 2);
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};

const warm = await timings("");
console.log(`the system message and the tools: ${warm.prompt_n} tokens read in ${(warm.prompt_ms / 1000).toFixed(1)} s`);
// Short, medium and long messages, each made of words the cache has not
// seen, so that every token is new.
const words = ["river", "stone", "cloud", "maple", "harbor", "lantern", "meadow", "copper", "violet", "thunder"];
for (const about of [10, 30, 100]) {
  const reads = [];
  for (let round = 0; round < 3; round++) {
    const message = Array.from({ length: Math.round(about * 0.6) }, (_, i) => `${words[(i + round) % words.length]}${round}${i}`).join(" ");
    const t = await timings(message);
    reads.push({ n: t.prompt_n, ms: t.prompt_ms });
  }
  console.log(
    `about ${about} new tokens: read ${median(reads.map((r) => r.n))} tokens in ${(median(reads.map((r) => r.ms)) / 1000).toFixed(2)} s (median of ${reads.length}; each ${reads.map((r) => `${r.n}:${(r.ms / 1000).toFixed(2)} s`).join(", ")})`,
  );
}
