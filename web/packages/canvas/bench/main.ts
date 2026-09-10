// The step-5 gate, measured (architecture v2.1 §13 step 5: "60 fps with
// 5,000 shapes on screen"). Fills the board with 5,000 mixed shapes, fits
// them all into the viewport, then times frames while the camera pans and a
// shape is dragged, which is what editing live costs. Results go to the page
// and to `window.__bench` for the script that runs this headless.
//
//   ?shapes=5000  how many shapes (default 5000)
//   ?frames=240   how many frames to time (default 240)

import { Editor, type Node } from "../src/index.ts";

declare global {
  interface Window {
    __bench?: Result;
  }
}

interface Result {
  shapes: number;
  drawnPerFrame: number;
  frames: number;
  panMs: { mean: number; p95: number; max: number };
  dragMs: { mean: number; p95: number; max: number };
  fpsAtP95: number;
  dpr: number;
  viewport: { w: number; h: number };
}

const params = new URLSearchParams(location.search);
const count = Number(params.get("shapes") ?? 5000);
const frames = Number(params.get("frames") ?? 240);

const host = document.getElementById("host") as HTMLElement;
const canvas = document.getElementById("board") as HTMLCanvasElement;
const report = document.getElementById("report") as HTMLElement;
const editor = new Editor({ canvas, host });

// A board that looks like work: stickies in frames, a few labels, arrows, ink.
const nodes: Node[] = [];
const cols = Math.ceil(Math.sqrt(count * 1.6));
const fills = ["yellow", "green", "blue", "amber", "red", "grey"] as const;
let z = 1;
for (let i = 0; i < count; i++) {
  const col = i % cols;
  const row = Math.floor(i / cols);
  const kind = i % 23 === 0 ? "rect" : i % 37 === 0 ? "ellipse" : i % 53 === 0 ? "text" : "sticky";
  nodes.push({
    id: `s${i}`,
    kind,
    x: col * 150,
    y: row * 130,
    w: kind === "text" ? 120 : 130,
    h: kind === "text" ? 26 : 110,
    fill: kind === "text" ? "none" : fills[i % fills.length],
    text: kind === "text" ? `Section ${i}` : `Item ${i}: a short note about the work`,
    frame: null,
    z: z++,
    locked: false,
    ...(kind === "text" ? { size: 16 } : {}),
  });
}
for (let i = 0; i < count / 50; i++) {
  const from = `s${(i * 97) % count}`;
  const to = `s${(i * 97 + 1) % count}`;
  nodes.push({ id: `a${i}`, kind: "arrow", x: 0, y: 0, w: 0, h: 0, fill: "grey", text: "", frame: null, z: z++, locked: false, from, to });
}
for (let i = 0; i < 20; i++) {
  const pts: [number, number][] = [];
  for (let k = 0; k < 40; k++) pts.push([i * 300 + k * 7, 200 + Math.sin(k / 3) * 30]);
  nodes.push({ id: `i${i}`, kind: "ink", x: 0, y: 0, w: 0, h: 0, fill: "blue", text: "", frame: null, z: z++, locked: false, points: pts });
}
editor.load(nodes);
editor.fit();
// `fit` never zooms past 1:1; force everything into view whatever the size.
const extent = editor.scene.extent();
const { w, h } = editor.size;
const zoom = Math.min((w - 80) / extent.w, (h - 80) / extent.h);
editor.setCamera({ x: extent.x - 40 / zoom, y: extent.y - 40 / zoom, z: zoom });

function stats(samples: number[]): { mean: number; p95: number; max: number } {
  const sorted = [...samples].sort((a, b) => a - b);
  const mean = sorted.reduce((a, b) => a + b, 0) / sorted.length;
  return { mean, p95: sorted[Math.floor(sorted.length * 0.95)], max: sorted[sorted.length - 1] };
}

function frame(): Promise<number> {
  return new Promise((resolve) => requestAnimationFrame((t) => resolve(t)));
}

async function measure(step: (i: number) => void): Promise<number[]> {
  const times: number[] = [];
  let last = await frame();
  for (let i = 0; i < frames; i++) {
    step(i);
    const start = performance.now();
    editor.paint();
    const painted = performance.now() - start;
    const now = await frame();
    // The longer of the paint and the frame interval: a frame that missed
    // its slot counts as missed even if the paint itself was quick.
    times.push(Math.max(painted, now - last));
    last = now;
  }
  return times;
}

async function run(): Promise<void> {
  await frame();
  await frame();
  const drawn = editor.paint() ? editor.drawnLastFrame : editor.drawnLastFrame;
  const pans = await measure((i) => editor.setCamera({ ...editor.camera, x: editor.camera.x + (i % 2 ? 3 : -3) / editor.camera.z }));
  const target = editor.scene.get("s10") as Node;
  const drags = await measure((i) => {
    editor.applyRemote({ set: [{ ...target, x: target.x + (i % 20), y: target.y + (i % 13) }], deleted: [] });
  });
  const pan = stats(pans);
  const drag = stats(drags);
  const result: Result = {
    shapes: nodes.length,
    drawnPerFrame: drawn,
    frames,
    panMs: pan,
    dragMs: drag,
    fpsAtP95: 1000 / Math.max(pan.p95, drag.p95),
    dpr: window.devicePixelRatio || 1,
    viewport: { w, h },
  };
  window.__bench = result;
  report.textContent =
    `${result.shapes} shapes, ${result.drawnPerFrame} drawn per frame, ${w}×${h} @${result.dpr}x\n` +
    `pan   mean ${pan.mean.toFixed(2)} ms  p95 ${pan.p95.toFixed(2)} ms  max ${pan.max.toFixed(2)} ms\n` +
    `drag  mean ${drag.mean.toFixed(2)} ms  p95 ${drag.p95.toFixed(2)} ms  max ${drag.max.toFixed(2)} ms\n` +
    `frame rate at p95: ${result.fpsAtP95.toFixed(1)} fps (gate: 60)`;
}

void run();
