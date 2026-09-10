// The whiteboard's web surface (v2 §6.3): a plain ES module in the harness's
// iframe, rendering the board document the shell hands it and writing it
// back on every edit. Step 5 replaces this with tldraw; what this proves is
// the loop: iframe → bridge → Core → DAG → doc back to the iframe.

import { connect } from "@localspace/harness-sdk";

const COLOURS = {
  red: ["#fbe9e9", "#c73e3e"],
  amber: ["#fbf3e4", "#b7791f"],
  green: ["#e8f6ee", "#1f9d5b"],
  blue: ["#e8f0fb", "#2f6fcb"],
  yellow: ["#fdf6d8", "#a88a17"],
  grey: ["#eef0ef", "#5f6763"],
};

const root = document.getElementById("root");
root.style.cssText = "position:relative;width:100%;height:100%;overflow:hidden;background:#f5f7f6;font:13px system-ui,sans-serif;color:#1c1f1d";

let zoom = 1;
let pan = { x: 40, y: 40 };
let harness;

const toolbar = document.createElement("div");
toolbar.style.cssText = "position:absolute;left:12px;top:12px;z-index:2;display:flex;gap:6px";
const addButton = document.createElement("button");
addButton.textContent = "Add sticky";
addButton.style.cssText = "border:1px solid #e3e7e4;background:#fff;border-radius:8px;padding:6px 10px;cursor:pointer";
const status = document.createElement("span");
status.style.cssText = "align-self:center;color:#9aa39e;font-size:12px";
toolbar.append(addButton, status);
root.append(toolbar);

const stage = document.createElement("div");
stage.style.cssText = "position:absolute;left:0;top:0;transform-origin:0 0";
root.append(stage);

function render(doc) {
  stage.innerHTML = "";
  stage.style.transform = `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`;
  const frames = doc?.frames ?? [];
  const shapes = doc?.shapes ?? [];
  const selection = new Set(doc?.selection ?? []);
  for (const f of frames) {
    const el = document.createElement("div");
    el.style.cssText = `position:absolute;left:${f.x}px;top:${f.y}px;width:${f.w}px;height:${f.h}px;border:1px solid #cfd5d1;border-radius:10px`;
    const label = document.createElement("div");
    label.textContent = f.name ?? "frame";
    label.style.cssText = "position:absolute;left:10px;top:-18px;color:#9aa39e;font-size:11px";
    el.append(label);
    stage.append(el);
  }
  for (const s of shapes) {
    if (s.kind === "arrow" || s.kind === "ink") continue;
    const [tint, accent] = COLOURS[s.fill] ?? COLOURS.grey;
    const el = document.createElement("div");
    el.dataset.id = s.id;
    const selected = selection.has(s.id);
    el.style.cssText = `position:absolute;left:${s.x}px;top:${s.y}px;width:${s.w}px;height:${s.h}px;background:${s.kind === "text" ? "transparent" : "#fff"};border:${s.kind === "text" ? "none" : `1.4px solid ${accent}`};border-radius:${s.kind === "ellipse" ? "50%" : "10px"};box-shadow:${selected ? "0 0 0 2px #1f9d5b" : "none"};padding:8px;box-sizing:border-box;cursor:pointer;overflow:hidden`;
    if (s.kind !== "text") {
      const chip = document.createElement("div");
      chip.style.cssText = `width:14px;height:14px;border-radius:4px;background:${tint};border:3px solid ${tint};box-shadow:inset 0 0 0 4px ${accent}`;
      el.append(chip);
    }
    const text = document.createElement("div");
    text.textContent = s.text ?? "";
    text.style.cssText = `margin-top:6px;font-size:${s.kind === "text" ? Math.max(12, s.size ?? 14) : 12.5}px;line-height:1.3;word-break:break-word`;
    el.append(text);
    el.addEventListener("click", (e) => {
      e.stopPropagation();
      const next = structuredClone(harness.doc());
      next.selection = e.shiftKey ? [...new Set([...(next.selection ?? []), s.id])] : [s.id];
      harness.write(next);
      render(next);
    });
    stage.append(el);
  }
  status.textContent = `${shapes.length} shape${shapes.length === 1 ? "" : "s"} · ${Math.round(zoom * 100)}%`;
}

addButton.addEventListener("click", () => {
  const next = structuredClone(harness.doc() ?? { shapes: [], frames: [], selection: [], title: "Board" });
  next.shapes = next.shapes ?? [];
  const n = next.shapes.length;
  const id = `w${Date.now().toString(36)}_${n}`;
  next.shapes.push({ id, kind: "sticky", x: 40 + (n % 5) * 150, y: 40 + Math.floor(n / 5) * 130, w: 130, h: 110, fill: "yellow", text: "New note", locked: false, z: n + 1 });
  next.selection = [id];
  harness.write(next);
  render(next);
});

root.addEventListener("click", () => {
  const next = structuredClone(harness.doc());
  if (!next || !(next.selection ?? []).length) return;
  next.selection = [];
  harness.write(next);
  render(next);
});

root.addEventListener("wheel", (e) => {
  e.preventDefault();
  zoom = Math.min(4, Math.max(0.25, zoom * (1 - e.deltaY * 0.0015)));
  render(harness.doc());
}, { passive: false });

connect().then((h) => {
  harness = h;
  render(h.doc());
  h.on("doc", (doc) => render(doc));
  h.on("command", ({ name, args }) => {
    if (name === "zoom" && typeof args?.value === "number") {
      zoom = Math.min(4, Math.max(0.25, args.value));
      render(h.doc());
    }
    if (name === "fit") {
      zoom = 1;
      pan = { x: 40, y: 40 };
      render(h.doc());
    }
  });
}).catch((err) => {
  status.textContent = String(err);
});
