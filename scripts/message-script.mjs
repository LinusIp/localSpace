// The message script of the laptop test (docs/DECISIONS.md, 2026-09-19, the
// answers after day 2): the things ten ordinary people type on a Friday
// afternoon, put to a model through Core's own API exactly as the app puts
// them, the same script for every model so that the results compare. Not
// eval cases: nothing is scored here. What came back, how long it took and
// which tools were reached for is written down for a person to judge.
//
//   node scripts/message-script.mjs <origin> <token> <model id> [--out <file.md>]
//       [--stop-after <words>] [--check]
//
// `--check` ends with an error when a message gets no answer or Continue does
// not carry through: that is what CI and scripts/check.sh run against the
// test engine (docs/DECISIONS.md, 2026-09-24), which has nothing to judge but
// would have caught the script reading every chat before its answer came.
//
// Needs a running `localspace serve --personal` with the model on disk; use a
// fresh data folder, which is what a tester has. One model at a time.
//
// A model may only be the default if it has been through this script on some
// machine and a person has read what came back: that day is the entry's
// `exercised_on` in models/catalog.json, and the answers belong in
// docs/test-a/MESSAGE-SCRIPT.md under a heading with the model's title.
//
// A message is answered beside Core's queue (docs/DECISIONS.md, 2026-09-23):
// sending it returns at once, and the script waits for the answer to end
// before reading it. The last step is Continue, which runs on every catalog
// default whenever the engine's pin moves (docs/DECISIONS.md, 2026-09-24): an
// answer is stopped after thirty words and carried on, and the join is written
// down for a person to read.

import { writeFileSync } from "node:fs";

const args = process.argv.slice(2);
/** The value after a flag, and the positions it and the flag take. */
const option = (flag) => {
  const at = args.indexOf(flag);
  return at >= 0 ? { value: args[at + 1], at } : { value: undefined, at: -1 };
};
const out = option("--out");
const stopAfter = option("--stop-after");
const check = args.includes("--check");
const taken = new Set([out.at + 1, stopAfter.at + 1].filter((i) => i > 0));
const [origin, token, model] = args.filter((a, i) => !a.startsWith("--") && !taken.has(i));
if (!origin || !token || !model) {
  console.error("usage: node scripts/message-script.mjs <origin> <token> <model id> [--out <file.md>] [--stop-after <words>] [--check]");
  process.exit(2);
}

const LONG_TEXT = `The town of Harrowfield sits where two rivers meet, and for most of its history it lived from the water. In 1847 a wooden footbridge was the only crossing, and the ferryman, a man called Tobias Wren, charged a penny a head. The railway arrived in 1869 and with it the first brick warehouses along the east bank. By 1890 the town had three mills, a brewery and a population of eleven thousand. The great flood of March 1912 carried away the footbridge, two of the mills and forty-one houses; nobody died, because the miller's daughter, Ada Pellow, saw the water rising at four in the morning and rang the chapel bell until the street was awake. The stone bridge that replaced the footbridge was opened in 1915 and still carries the main road. After the second war the mills closed one by one, the last in 1971, and the warehouses stood empty until the 1990s, when they were turned into flats and workshops. Today the town has about nineteen thousand people, a weekly market on Thursdays, and a small museum in the old brewery whose most visited exhibit is the chapel bell.`;

const SCRIPT = [
  { name: "a greeting", turns: ["Hi there!"] },
  { name: "what can you do", turns: ["What can you do?"] },
  { name: "a factual question", turns: ["How far is the Moon from the Earth?"] },
  { name: "a short email", turns: ["Write a short, polite email to my landlord asking him to fix the leaking tap in the kitchen."] },
  {
    name: "something to summarise",
    turns: [`Summarise this in two sentences:\n\n${LONG_TEXT}`],
  },
  { name: "a question about a detail of a longer text", turns: [`Read this and then answer: who rang the bell, and in which year?\n\n${LONG_TEXT}`] },
  { name: "arithmetic", turns: ["What is 17 times 24?"] },
  { name: "a word problem", turns: ["I have 3 boxes with 12 eggs in each, and I break 5 eggs. How many whole eggs do I have left?"] },
  { name: "a translation", turns: ["Translate into German: Where is the nearest train station?"] },
  {
    name: "make this shorter",
    turns: ["Make this shorter: I am writing to let you know that, due to circumstances which are unfortunately outside of my control, I will regrettably not be able to attend the meeting that has been scheduled for Thursday afternoon."],
  },
  { name: "a question in Spanish", turns: ["¿Cuál es la capital de Argentina y por qué es conocida?"] },
  { name: "a question in Russian", turns: ["Какая столица Франции и чем она знаменита?"] },
  { name: "something deliberately vague", turns: ["Can you help me with my thing?"] },
  { name: "a packing list", turns: ["Give me a packing list for a three-day hiking trip in autumn."] },
  { name: "a little code", turns: ["Write a Python function that checks whether a number is prime."] },
  { name: "something it cannot know", turns: ["What is the weather like in Lisbon today?"] },
  // Only a tool from the Store could do this: what does a model say when
  // nothing is installed? (docs/DECISIONS.md, 2026-09-24, answers on the
  // prompt item.)
  { name: "something only a Store tool can do", turns: ["Draw me a picture of a cat."] },
  {
    name: "three turns, each depending on the one before",
    turns: ["Give me three ideas for a weekend trip from Berlin, one line each.", "Tell me more about the second one.", "Roughly what would that cost for two people?"],
  },
];

/** The message whose answer is stopped and carried on, and after how many words. */
const CONTINUE = {
  text: "Explain in about 500 words how a lighthouse works and why lighthouses were built where they were.",
  stopAfter: Number(stopAfter.value ?? 30),
};

/** Longest an answer may take here; Core's own limits are on silence, not on length. */
const ANSWER_MS = 600000;

const headers = { Authorization: `Bearer ${token}`, "content-type": "application/json" };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const words = (s) => s.split(/\s+/).filter(Boolean);
const request = async (body, ms = 60000) => {
  const abort = new AbortController();
  const timer = setTimeout(() => abort.abort(), ms);
  try {
    const res = await fetch(`${origin}/api/v1/request`, { method: "POST", headers, body: JSON.stringify(body), signal: abort.signal });
    return await res.json();
  } catch (e) {
    return { error: e.name === "AbortError" ? `no answer within ${ms / 1000} s` : String(e) };
  } finally {
    clearTimeout(timer);
  }
};
const environment = async () => (await (await fetch(`${origin}/api/v1/environment`, { headers })).json()).environment;
const currentChat = async () => (await request("list_conversations")).conversations?.current;
const transcript = async () => (await request("get_transcript")).transcript?.messages ?? [];

/** Waits until nothing is being written or waits to be; false if that took too long. */
const answerEnds = async () => {
  const began = Date.now();
  while (Date.now() - began < ANSWER_MS) {
    const turns = (await request("list_turns")).turns?.list;
    if (Array.isArray(turns) && turns.length === 0) return true;
    await sleep(250);
  }
  return false;
};

/** What the person is sent while answers are written: the words of each chat, as they come. */
const shown = new Map();
const socket = new WebSocket(`${origin.replace(/^http/, "ws")}/ws/json?token=${encodeURIComponent(token)}`);
socket.onmessage = (message) => {
  const delta = JSON.parse(message.data)?.body?.Event?.assistant_delta;
  if (delta) shown.set(delta.conversation, (shown.get(delta.conversation) ?? "") + delta.text);
};
await new Promise((resolve, reject) => {
  socket.onopen = resolve;
  socket.onerror = () => reject(new Error("the event stream could not be opened"));
});

try {
  await run();
} finally {
  socket.close();
}

async function run() {
  const began = Date.now();
  // As the app does first: the list of models, which is when Core finds the
  // files already on this computer and checks their SHA-256, in the
  // background. The model can be started once its entry says it is installed.
  let entry;
  for (;;) {
    entry = ((await request("list_model_catalog", ANSWER_MS)).model_catalog?.entries ?? []).find((m) => m.id === model);
    if (!entry) throw new Error(`no model ${model} in the catalog`);
    if (entry.installed) break;
    if (Date.now() - began > ANSWER_MS) throw new Error(`${model} is not on this computer: ${JSON.stringify(entry.download)}`);
    await sleep(1000);
  }
  const loaded = await request({ load_model: { id: model } });
  if (loaded && typeof loaded === "object" && "error" in loaded) throw new Error(`the model could not be loaded: ${JSON.stringify(loaded.error)}`);
  while (!(await environment()).engine.running) {
    if (Date.now() - began > 600000) throw new Error("the engine did not come up in ten minutes");
    await sleep(500);
  }
  const readyAfter = (Date.now() - began) / 1000;

  const results = [];
  for (const item of SCRIPT) {
    await request("new_conversation");
    const turns = [];
    for (const text of item.turns) {
      const asked = Date.now();
      const sent = await request({ send_message: { text } });
      const ended = sent.error ? false : await answerEnds();
      const seconds = (Date.now() - asked) / 1000;
      const messages = await transcript();
      // What this turn added: everything after the last message of the person.
      const lastUser = messages.map((m) => m.role).lastIndexOf("user");
      const since = messages.slice(lastUser + 1);
      const reply = since.filter((m) => m.role === "assistant").pop()?.content?.trim() ?? "";
      const tools = since.flatMap((m) => (m.tool_calls ?? []).map((c) => c.tool));
      const error = sent.error ? JSON.stringify(sent.error) : ended ? undefined : `no end within ${ANSWER_MS / 1000} s`;
      turns.push({ text, seconds, reply, tools, error });
      console.error(`${item.name}: ${seconds.toFixed(1)} s${tools.length ? `, tools: ${tools.join(", ")}` : ""}${reply ? "" : "  — NO REPLY"}`);
    }
    results.push({ name: item.name, turns });
  }

  // Continue: an answer stopped after a number of words, then carried on.
  await request("new_conversation");
  const chat = await currentChat();
  const carried = { stoppedAfter: 0, kept: "", added: "", whole: undefined, note: "" };
  {
    await request({ send_message: { text: CONTINUE.text } });
    const asked = Date.now();
    while (Date.now() - asked < ANSWER_MS) {
      if (words(shown.get(chat) ?? "").length >= CONTINUE.stopAfter) break;
      if ((await request("list_turns")).turns?.list?.length === 0) break;
      await sleep(50);
    }
    carried.stoppedAfter = words(shown.get(chat) ?? "").length;
    await request({ cancel_turn: { conversation: chat } });
    await answerEnds();
    const stopped = (await transcript()).at(-1);
    if (!stopped?.stopped) {
      carried.note = "the answer ended before it could be stopped: nothing to carry on";
    } else {
      carried.kept = stopped.content;
      await request({ continue_answer: { conversation: chat } });
      if (!(await answerEnds())) carried.note = `no end within ${ANSWER_MS / 1000} s`;
      const answer = (await transcript()).at(-1);
      carried.whole = answer?.stopped === false;
      carried.added = answer?.content?.startsWith(carried.kept) ? answer.content.slice(carried.kept.length) : (answer?.content ?? "");
    }
    console.error(`continue: stopped after ${carried.stoppedAfter} words; ${carried.note || `${words(carried.added).length} words added`}`);
  }
  await request("unload_model");

  const cell = (s) => s.replace(/\|/g, "\\|").replace(/\s*\n\s*/g, " ⏎ ").trim();
  const lines = [];
  lines.push(`### ${entry?.title ?? model}`);
  lines.push("");
  lines.push(`\`${model}\`, through Core on a fresh data folder. Ready after ${readyAfter.toFixed(0)} s. The app says of it here: ${entry ? `"${entry.verdict_label}${entry.speed ? ` · ${entry.speed}` : ""}"; ${entry.placement}` : "nothing (not in the catalog)"}`);
  lines.push("");
  lines.push("| Message | Time | Tools reached for | What came back |");
  lines.push("|---|---|---|---|");
  for (const item of results) {
    item.turns.forEach((t, i) => {
      const label = item.turns.length > 1 ? `${item.name} (${i + 1}): ${t.text}` : `${item.name}: ${t.text.length > 90 ? `${t.text.slice(0, 90)}…` : t.text}`;
      const back = t.error ? `**${t.error}**` : t.reply ? t.reply : "**no reply**";
      lines.push(`| ${cell(label)} | ${t.seconds.toFixed(1)} s | ${t.tools.length ? t.tools.join(", ") : "none"} | ${cell(back)} |`);
    });
  }
  lines.push("");
  lines.push(`**Continue**: "${CONTINUE.text}" stopped after ${carried.stoppedAfter} words, then carried on.`);
  if (carried.note) {
    lines.push(`**${carried.note}**`);
  } else {
    const last = words(carried.kept).slice(-8).join(" ");
    const next = words(carried.added).slice(0, 16).join(" ");
    // An answer that begins again says its opening words a second time.
    const opening = words(carried.kept).slice(0, 6).join(" ");
    const beganAgain = opening.length > 0 && words(carried.added).slice(0, 60).join(" ").includes(opening);
    lines.push(`The join: "…${cell(last)}" ‖ "${cell(next)}…". ${beganAgain ? "**It began its answer again.**" : "It did not begin again."} ${carried.whole ? "The answer ended whole." : "**The answer did not end whole.**"} Whether the engine said the handed words again is in app.log: a line "continue: the engine did not begin its reply …" means it did not, and nothing was dropped.`);
  }
  lines.push("");
  const report = lines.join("\n");
  if (out.value) writeFileSync(out.value, report);
  else console.log(report);

  // With --check, one line, and an error when anything went unanswered.
  const turnsIn = results.flatMap((item) => item.turns);
  const unanswered = turnsIn.filter((t) => t.error || !t.reply).length;
  const carriedThrough = !carried.note && carried.whole === true;
  if (check) {
    const how = carriedThrough ? "carried through" : `did not carry through (${carried.note || "the answer did not end whole"})`;
    console.error(`message script: ${turnsIn.length - unanswered} of ${turnsIn.length} messages answered; Continue ${how}`);
    if (unanswered > 0 || !carriedThrough) process.exitCode = 1;
  }
}
