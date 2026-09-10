// The step-5 gate, measured (architecture v2.1 §13 step 5: "60 fps with
// 5,000 shapes on screen"). Fills the board with 5,000 mixed shapes, then
// times frames in two views, the overview with everything in sight and a
// reading zoom where shapes are drawn in full, while the camera pans and
// while a shape is dragged through the editor's own pointer handling,
// which is what editing live costs. Results go to the page and to
// `window.__bench` for the script that runs this headless.
//
//   ?shapes=5000  how many shapes (default 5000)
//   ?frames=240   how many frames to time per measurement (default 240)

import { Editor, toScreen, type Node } from "../src/index.ts";

declare global {
  interface Window {
    __bench?: Result;
  }
}

interface Stats {
  mean: number;
  p95: number;
  max: number;
}

interface Scenario {
  zoom: number;
  drawnPerFrame: number;
  panMs: Stats;
  dragMs: Stats;
  /** The paint alone, without the wait for the next frame slot. */
  paintMs: Stats;
}

interface Result {
  shapes: number;
  frames: number;
  dpr: number;
  viewport: { w: number; h: number };
  overview: Scenario;
  reading: Scenario;
  /** The frame rate the worst p95 of the four measurements allows. */
  fpsAtP95: number;
}

const params = new URLSearchParams(location.search);
const count = Number(params.get("shapes") ?? 5000);
const frames = Number(params.get("frames") ?? 240);

const host = document.getElementById("host") as HTMLElement;
const canvas = document.getElementById("board") as HTMLCanvasElement;
const report = document.getElementById("report") as HTMLElement;
const editor = new Editor({ canvas, host });

// A board that looks like work: stickies, a few boxes and labels, arrows, ink.
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
const { w, h } = editor.size;
const extent = editor.scene.extent();

function stats(samples: number[]): Stats {
  const sorted = [...samples].sort((a, b) => a - b);
  const mean = sorted.reduce((a, b) => a + b, 0) / sorted.length;
  return { mean, p95: sorted[Math.floor(sorted.length * 0.95)], max: sorted[sorted.length - 1] };
}

function frame(): Promise<number> {
  return new Promise((resolve) => requestAnimationFrame((t) => resolve(t)));
}

const paints: number[] = [];

async function measure(step: (i: number) => void): Promise<number[]> {
  const times: number[] = [];
  let last = await frame();
  for (let i = 0; i < frames; i++) {
    step(i);
    const start = performance.now();
    editor.paint();
    const painted = performance.now() - start;
    paints.push(painted);
    const now = await frame();
    // The longer of the paint and the frame interval: a frame that missed
    // its slot counts as missed even if the paint itself was quick.
    times.push(Math.max(painted, now - last));
    last = now;
  }
  return times;
}

function pointer(type: string, at: { x: number; y: number }, buttons = 1): void {
  const r = canvas.getBoundingClientRect();
  host.dispatchEvent(
    new PointerEvent(type, {
      bubbles: true,
      cancelable: true,
      pointerId: 1,
      pointerType: "mouse",
      isPrimary: true,
      button: 0,
      buttons,
      clientX: r.left + at.x,
      clientY: r.top + at.y,
    }),
  );
}

/** Pan, then drag a shape with the pointer, as a user would. */
async function scenario(zoom: number): Promise<Scenario> {
  editor.select([]);
  editor.setCamera({ x: extent.x + extent.w / 2 - w / (2 * zoom), y: extent.y + extent.h / 2 - h / (2 * zoom), z: zoom });
  await frame();
  editor.paint();
  const drawnPerFrame = editor.drawnLastFrame;
  paints.length = 0;
  const pans = await measure((i) => editor.setCamera({ ...editor.camera, x: editor.camera.x + (i % 2 ? 3 : -3) / editor.camera.z }));

  // The shape nearest the middle of the view, dragged in a small circle.
  const middle = editor.toBoard({ x: w / 2, y: h / 2 });
  const target = editor.scene.query({ x: middle.x - 200, y: middle.y - 200, w: 400, h: 400 }).find((n) => n.kind === "sticky");
  if (!target) throw new Error("no sticky near the middle of the view");
  const grab = toScreen(editor.camera, { x: target.x + target.w / 2, y: target.y + target.h / 2 });
  pointer("pointerdown", grab);
  const drags = await measure((i) => {
    const a = (i / frames) * Math.PI * 2;
    pointer("pointermove", { x: grab.x + Math.cos(a) * 60 + 8, y: grab.y + Math.sin(a) * 60 });
  });
  pointer("pointerup", grab, 0);
  return { zoom, drawnPerFrame, panMs: stats(pans), dragMs: stats(drags), paintMs: stats(paints) };
}

async function run(): Promise<void> {
  await frame();
  await frame();
  const overviewZoom = Math.min((w - 80) / extent.w, (h - 80) / extent.h);
  const overview = await scenario(overviewZoom);
  const reading = await scenario(0.5);
  const worst = Math.max(overview.panMs.p95, overview.dragMs.p95, reading.panMs.p95, reading.dragMs.p95);
  const result: Result = {
    shapes: nodes.length,
    frames,
    dpr: window.devicePixelRatio || 1,
    viewport: { w, h },
    overview,
    reading,
    fpsAtP95: 1000 / worst,
  };
  window.__bench = result;
  const line = (label: string, s: Stats) => `${label}  mean ${s.mean.toFixed(2)} ms  p95 ${s.p95.toFixed(2)} ms  max ${s.max.toFixed(2)} ms`;
  report.textContent =
    `${result.shapes} shapes, ${w}×${h} @${result.dpr}x, ${frames} frames per measurement\n` +
    `overview (${(overview.zoom * 100).toFixed(0)}%, ${overview.drawnPerFrame} drawn)\n` +
    `  ${line("pan  ", overview.panMs)}\n  ${line("drag ", overview.dragMs)}\n  ${line("paint", overview.paintMs)}\n` +
    `reading (${(reading.zoom * 100).toFixed(0)}%, ${reading.drawnPerFrame} drawn)\n` +
    `  ${line("pan  ", reading.panMs)}\n  ${line("drag ", reading.dragMs)}\n  ${line("paint", reading.paintMs)}\n` +
    `frame rate at the worst p95: ${result.fpsAtP95.toFixed(1)} fps (gate: 60)`;
}

void run().catch((err: unknown) => {
  report.textContent = `benchmark failed: ${String(err)}`;
});
