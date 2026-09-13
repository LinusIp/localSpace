// The board's shapes, as the whiteboard's logic writes them and as the
// canvas draws them: one model, no translation layer. Frames live in the
// document's own `frames` list and here as nodes of kind `frame`.

import type { Box, Point } from "./geometry.ts";

export type Kind = "sticky" | "rect" | "ellipse" | "text" | "arrow" | "ink" | "frame";

export const KINDS: readonly Kind[] = ["sticky", "rect", "ellipse", "text", "arrow", "ink", "frame"];

/** The palette the logic knows; `none` is a text label's "no fill". */
export type Fill = "red" | "amber" | "green" | "blue" | "yellow" | "grey" | "none";

export const FILLS: readonly Fill[] = ["red", "amber", "green", "blue", "yellow", "grey"];

export interface Node {
  id: string;
  kind: Kind;
  x: number;
  y: number;
  w: number;
  h: number;
  fill: Fill;
  text: string;
  /** The frame this shape sits in, or null. Coordinates stay absolute. */
  frame: string | null;
  z: number;
  locked: boolean;
  /** Text labels: the font size in board units. */
  size?: number;
  /** Arrows: the shapes at each end, or null for a free end. */
  from?: string | null;
  to?: string | null;
  /** Arrows with a free end: where that end is. */
  start?: [number, number];
  end?: [number, number];
  /** Ink: the stroke, absolute. */
  points?: [number, number][];
  /** Frames: the name shown above them. */
  name?: string;
}

/** The whiteboard document as Core holds it. */
export interface BoardDoc {
  title?: string;
  shapes?: DocShape[];
  frames?: DocFrame[];
  selection?: string[];
}

/** A shape as the document carries it: the kind and fill are strings there, the text may be absent. */
export type DocShape = Omit<Node, "kind" | "fill" | "name" | "text"> & { kind: string; fill?: string; text?: string };
export interface DocFrame {
  id: string;
  name?: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

export const DEFAULTS: Record<Kind, { w: number; h: number; fill: Fill }> = {
  sticky: { w: 196, h: 108, fill: "yellow" },
  rect: { w: 160, h: 90, fill: "grey" },
  ellipse: { w: 160, h: 90, fill: "grey" },
  text: { w: 120, h: 26, fill: "none" },
  arrow: { w: 0, h: 0, fill: "grey" },
  ink: { w: 0, h: 0, fill: "blue" },
  frame: { w: 600, h: 420, fill: "none" },
};

const num = (v: unknown, d: number): number => (typeof v === "number" && Number.isFinite(v) ? v : d);

function fillOf(v: unknown, d: Fill): Fill {
  return v === "red" || v === "amber" || v === "green" || v === "blue" || v === "yellow" || v === "grey" || v === "none"
    ? v
    : d;
}

function pairs(v: unknown): [number, number][] | undefined {
  if (!Array.isArray(v)) return undefined;
  const out: [number, number][] = [];
  for (const p of v) {
    if (Array.isArray(p) && Number.isFinite(p[0]) && Number.isFinite(p[1])) out.push([p[0], p[1]]);
  }
  return out;
}

function pair(v: unknown): [number, number] | undefined {
  return Array.isArray(v) && Number.isFinite(v[0]) && Number.isFinite(v[1]) ? [v[0], v[1]] : undefined;
}

/** The document's shapes and frames as nodes. Unknown kinds are dropped. */
export function fromDoc(doc: BoardDoc | null | undefined): Node[] {
  const out: Node[] = [];
  for (const f of doc?.frames ?? []) {
    if (!f || typeof f.id !== "string") continue;
    out.push({
      id: f.id,
      kind: "frame",
      x: num(f.x, 0),
      y: num(f.y, 0),
      w: Math.max(1, num(f.w, DEFAULTS.frame.w)),
      h: Math.max(1, num(f.h, DEFAULTS.frame.h)),
      fill: "none",
      text: "",
      name: typeof f.name === "string" ? f.name : "Frame",
      frame: null,
      z: -1,
      locked: false,
    });
  }
  for (const s of doc?.shapes ?? []) {
    if (!s || typeof s.id !== "string") continue;
    const kind = s.kind;
    if (kind !== "sticky" && kind !== "rect" && kind !== "ellipse" && kind !== "text" && kind !== "arrow" && kind !== "ink") continue;
    const d = DEFAULTS[kind];
    const node: Node = {
      id: s.id,
      kind,
      x: num(s.x, 0),
      y: num(s.y, 0),
      w: Math.max(kind === "arrow" || kind === "ink" ? 0 : 1, num(s.w, d.w)),
      h: Math.max(kind === "arrow" || kind === "ink" ? 0 : 1, num(s.h, d.h)),
      fill: fillOf(s.fill, d.fill),
      text: typeof s.text === "string" ? s.text : "",
      frame: typeof s.frame === "string" ? s.frame : null,
      z: num(s.z, 0),
      locked: s.locked === true,
    };
    if (kind === "text") node.size = num(s.size, 16);
    if (kind === "arrow") {
      node.from = typeof s.from === "string" ? s.from : null;
      node.to = typeof s.to === "string" ? s.to : null;
      const start = pair(s.start);
      const end = pair(s.end);
      if (start) node.start = start;
      if (end) node.end = end;
    }
    if (kind === "ink") node.points = pairs(s.points) ?? [];
    out.push(node);
  }
  return out;
}

/** The nodes back into a document, keeping what the logic wrote elsewhere. */
export function toDoc(nodes: readonly Node[], previous: BoardDoc | null | undefined): BoardDoc {
  const shapes: DocShape[] = [];
  const frames: DocFrame[] = [];
  const sorted = [...nodes].sort((a, b) => a.z - b.z);
  let z = 0;
  for (const n of sorted) {
    if (n.kind === "frame") {
      frames.push({ id: n.id, name: n.name ?? "Frame", x: n.x, y: n.y, w: n.w, h: n.h });
      continue;
    }
    z += 1;
    const s: DocShape = {
      id: n.id,
      kind: n.kind,
      x: n.x,
      y: n.y,
      w: n.w,
      h: n.h,
      fill: n.fill,
      text: n.text,
      frame: n.frame,
      z,
      locked: n.locked,
    };
    if (n.kind === "text") s.size = n.size ?? 16;
    if (n.kind === "arrow") {
      s.from = n.from ?? null;
      s.to = n.to ?? null;
      if (n.start) s.start = n.start;
      if (n.end) s.end = n.end;
    }
    if (n.kind === "ink") s.points = n.points ?? [];
    shapes.push(s);
  }
  return { ...(previous ?? {}), title: previous?.title ?? "Board", frames, shapes, selection: previous?.selection ?? [] };
}

/** The bounding box of a node; ink and arrows are computed from their points. */
export function boundsOf(n: Node, endpoints?: (n: Node) => [Point, Point]): Box {
  if (n.kind === "ink" && n.points && n.points.length) {
    let x0 = Infinity;
    let y0 = Infinity;
    let x1 = -Infinity;
    let y1 = -Infinity;
    for (const [x, y] of n.points) {
      if (x < x0) x0 = x;
      if (y < y0) y0 = y;
      if (x > x1) x1 = x;
      if (y > y1) y1 = y;
    }
    return { x: x0, y: y0, w: Math.max(1, x1 - x0), h: Math.max(1, y1 - y0) };
  }
  if (n.kind === "arrow" && endpoints) {
    const [a, b] = endpoints(n);
    return { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y), w: Math.max(1, Math.abs(a.x - b.x)), h: Math.max(1, Math.abs(a.y - b.y)) };
  }
  return { x: n.x, y: n.y, w: n.w, h: n.h };
}

export function cloneNode(n: Node): Node {
  const c: Node = { ...n };
  if (n.points) c.points = n.points.map((p) => [p[0], p[1]]);
  if (n.start) c.start = [n.start[0], n.start[1]];
  if (n.end) c.end = [n.end[0], n.end[1]];
  return c;
}

/** Whether two nodes would write the same document entry. */
export function sameNode(a: Node, b: Node): boolean {
  if (a === b) return true;
  if (
    a.id !== b.id ||
    a.kind !== b.kind ||
    a.x !== b.x ||
    a.y !== b.y ||
    a.w !== b.w ||
    a.h !== b.h ||
    a.fill !== b.fill ||
    a.text !== b.text ||
    a.frame !== b.frame ||
    a.z !== b.z ||
    a.locked !== b.locked ||
    a.size !== b.size ||
    (a.from ?? null) !== (b.from ?? null) ||
    (a.to ?? null) !== (b.to ?? null) ||
    a.name !== b.name
  )
    return false;
  const pa = a.points ?? [];
  const pb = b.points ?? [];
  if (pa.length !== pb.length) return false;
  for (let i = 0; i < pa.length; i++) if (pa[i][0] !== pb[i][0] || pa[i][1] !== pb[i][1]) return false;
  const ends = (p?: [number, number], q?: [number, number]) => (p === q) || (!!p && !!q && p[0] === q[0] && p[1] === q[1]);
  return ends(a.start, b.start) && ends(a.end, b.end);
}
