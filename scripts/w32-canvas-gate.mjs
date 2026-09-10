// The canvas gate on the W32 reference machine, from its recorded
// measurement (docs/DECISIONS.md: 60 fps with 5,000 shapes on screen,
// measured on the W32 machine with the scripted benchmark; CI records the
// number from docs/gates/). Prints a Markdown summary for the CI job's
// page; the exit code is 1 when the file is missing, from another profile,
// or below the gate.
//
//   node scripts/w32-canvas-gate.mjs [docs/gates/w32-canvas.json]
//
// The measurement is taken on the W32 machine, in web/:
//   node e2e/bench-canvas.mjs --profile w32 --shapes 5000 --out ../docs/gates/w32-canvas.json

import { existsSync, readFileSync } from "node:fs";

const file = process.argv[2] ?? "docs/gates/w32-canvas.json";
const GATE = { shapes: 5000, fps: 60 };

if (!existsSync(file)) {
  console.log("## Canvas gate on W32: no measurement yet\n");
  console.log(`Nothing at \`${file}\`. Take it on the W32 machine, in \`web/\`:\n`);
  console.log(`    node e2e/bench-canvas.mjs --profile w32 --shapes ${GATE.shapes} --out ../${file}`);
  process.exit(1);
}

const m = JSON.parse(readFileSync(file, "utf8"));
const onScreen = m.overview?.drawnPerFrame ?? 0;
const met = m.profile === "w32" && m.shapes >= GATE.shapes && onScreen >= GATE.shapes && m.fpsAtP95 >= GATE.fps;
const ms = (v) => (typeof v === "number" ? v.toFixed(2) : "?");

console.log(`## Canvas gate on W32: ${met ? "met" : "NOT met"}\n`);
console.log("| | measured | gate |");
console.log("|---|---|---|");
console.log(`| shapes on screen | ${onScreen} of ${m.shapes} | at least ${GATE.shapes} |`);
console.log(`| frame rate at the worst p95 | ${ms(m.fpsAtP95)} fps | at least ${GATE.fps} fps |`);
console.log(`| pan, drag p95 at overview | ${ms(m.overview?.panP95)} ms, ${ms(m.overview?.dragP95)} ms | |`);
console.log(`| pan, drag p95 at reading zoom | ${ms(m.reading?.panP95)} ms, ${ms(m.reading?.dragP95)} ms | |`);
const machine = m.machine
  ? `${m.machine.cpu}, ${m.machine.threads} threads, ${m.machine.memoryGb} GB, ${m.machine.os}, ${m.machine.browser}`
  : "a machine the file does not describe";
console.log(`\nRecorded ${m.recorded} on ${machine}, profile \`${m.profile}\`.`);
if (m.profile !== "w32") console.log(`\nThe profile is \`${m.profile}\`: this is not a W32 measurement.`);
process.exitCode = met ? 0 : 1;
