// Snapping (architecture v2.1 §6.4; docs/DECISIONS.md, 2026-09-10): a box
// that is moved or resized comes to rest on the edges and centres of the
// shapes around it when one is within reach, and the lines it rests on are
// drawn as guides while the gesture lasts. Reach is in screen pixels, so it
// feels the same at every zoom. The grid, when it is on, takes what no
// shape does. Pure functions over boxes: the editor chooses the targets and
// skips all of it while Alt is held.

import type { Box } from "./geometry.ts";
import type { Handle } from "./hit.ts";

/** How near, in screen pixels, a line has to come to another to rest on it. */
export const SNAP_PX = 8;

/** The board's grid in board units: the dots the renderer draws. */
export const GRID = 24;

/** A line something rests on: `at` on `axis`, drawn from `from` to `to` along the other axis. */
export interface Guide {
  axis: "x" | "y";
  at: number;
  from: number;
  to: number;
}

/** What a gesture can rest on: boxes, and their three lines on each axis. */
export interface SnapTargets {
  readonly boxes: readonly Box[];
  /** Left, centre and right of each box, in the boxes' order. */
  readonly xs: Float64Array;
  /** Top, middle and bottom of each box, in the boxes' order. */
  readonly ys: Float64Array;
}

export interface SnapOptions {
  /** How near is near enough, in board units: `snapReach(zoom)`. */
  reach: number;
  /** Rest on the targets' edges and centres. */
  shapes: boolean;
  /** The grid step to fall back on; 0 for none. */
  grid: number;
}

/** Where a gesture comes to rest: the offset to apply, and the guides to draw. */
export interface Snapped {
  dx: number;
  dy: number;
  guides: Guide[];
}

/** Lines nearer than this are one line. */
const SAME = 1e-3;

/** Reach in board units at a zoom: the same screen distance at every zoom. */
export function snapReach(zoom: number): number {
  return SNAP_PX / zoom;
}

export function snapTargets(boxes: readonly Box[]): SnapTargets {
  const xs = new Float64Array(boxes.length * 3);
  const ys = new Float64Array(boxes.length * 3);
  for (let i = 0; i < boxes.length; i++) {
    const b = boxes[i];
    xs[i * 3] = b.x;
    xs[i * 3 + 1] = b.x + b.w / 2;
    xs[i * 3 + 2] = b.x + b.w;
    ys[i * 3] = b.y;
    ys[i * 3 + 1] = b.y + b.h / 2;
    ys[i * 3 + 2] = b.y + b.h;
  }
  return { boxes, xs, ys };
}

const xLines = (b: Box): number[] => [b.x, b.x + b.w / 2, b.x + b.w];
const yLines = (b: Box): number[] => [b.y, b.y + b.h / 2, b.y + b.h];

/** The shortest shift, within reach, that puts one of `lines` on a target line; null when none is near. */
function nearest(lines: readonly number[], targets: Float64Array, reach: number): number | null {
  let best: number | null = null;
  let bestDistance = reach;
  for (const line of lines) {
    for (let i = 0; i < targets.length; i++) {
      const d = targets[i] - line;
      const distance = Math.abs(d);
      if (best === null ? distance <= bestDistance : distance < bestDistance) {
        best = d;
        bestDistance = distance;
      }
    }
  }
  return best;
}

/** The shift that puts `value` on the grid. */
function toGrid(value: number, grid: number): number {
  return Math.round(value / grid) * grid - value;
}

/**
 * The guides for the lines of `box` on one axis that sit on target lines:
 * one per line, spanning the box and every target that shares the line.
 */
function guidesOn(axis: "x" | "y", lines: readonly number[], box: Box, targets: SnapTargets): Guide[] {
  const values = axis === "x" ? targets.xs : targets.ys;
  const out: Guide[] = [];
  for (const at of lines) {
    if (out.some((g) => Math.abs(g.at - at) <= SAME)) continue;
    let from = axis === "x" ? box.y : box.x;
    let to = axis === "x" ? box.y + box.h : box.x + box.w;
    let shared = false;
    for (let i = 0; i < values.length; i++) {
      if (Math.abs(values[i] - at) > SAME) continue;
      const t = targets.boxes[Math.floor(i / 3)];
      shared = true;
      from = Math.min(from, axis === "x" ? t.y : t.x);
      to = Math.max(to, axis === "x" ? t.y + t.h : t.x + t.w);
    }
    if (shared) out.push({ axis, at, from, to });
  }
  return out;
}

/**
 * A box moved by `dx, dy` from where the gesture began. Its edges and its
 * centre come to rest on the nearest target line within reach, axis by
 * axis; on an axis where none is near, its top-left corner comes to rest
 * on the grid when there is one.
 */
export function snapMove(box: Box, dx: number, dy: number, targets: SnapTargets | null, options: SnapOptions): Snapped {
  const moved = { x: box.x + dx, y: box.y + dy, w: box.w, h: box.h };
  let ox = targets && options.shapes ? nearest(xLines(moved), targets.xs, options.reach) : null;
  let oy = targets && options.shapes ? nearest(yLines(moved), targets.ys, options.reach) : null;
  const onX = ox !== null;
  const onY = oy !== null;
  if (ox === null && options.grid > 0) ox = toGrid(moved.x, options.grid);
  if (oy === null && options.grid > 0) oy = toGrid(moved.y, options.grid);
  const rest = { x: moved.x + (ox ?? 0), y: moved.y + (oy ?? 0), w: box.w, h: box.h };
  const guides: Guide[] = [];
  if (targets && onX) guides.push(...guidesOn("x", xLines(rest), rest, targets));
  if (targets && onY) guides.push(...guidesOn("y", yLines(rest), rest, targets));
  return { dx: dx + (ox ?? 0), dy: dy + (oy ?? 0), guides };
}

/**
 * A box resized by `handle`, the pointer `dx, dy` from where the gesture
 * began. Only the edges the handle moves come to rest: on the nearest
 * target line within reach, or else on the grid when there is one.
 */
export function snapResize(box: Box, handle: Handle, dx: number, dy: number, targets: SnapTargets | null, options: SnapOptions): Snapped {
  const east = handle.includes("e");
  const west = handle.includes("w");
  const south = handle.includes("s");
  const north = handle.includes("n");
  let ox: number | null = null;
  let oy: number | null = null;
  let onX = false;
  let onY = false;
  if (east || west) {
    const edge = east ? box.x + box.w + dx : box.x + dx;
    ox = targets && options.shapes ? nearest([edge], targets.xs, options.reach) : null;
    onX = ox !== null;
    if (ox === null && options.grid > 0) ox = toGrid(edge, options.grid);
  }
  if (north || south) {
    const edge = south ? box.y + box.h + dy : box.y + dy;
    oy = targets && options.shapes ? nearest([edge], targets.ys, options.reach) : null;
    onY = oy !== null;
    if (oy === null && options.grid > 0) oy = toGrid(edge, options.grid);
  }
  const fdx = dx + (ox ?? 0);
  const fdy = dy + (oy ?? 0);
  // The box as the handle leaves it, for the guides' extent.
  const left = west ? box.x + fdx : box.x;
  const right = east ? box.x + box.w + fdx : box.x + box.w;
  const top = north ? box.y + fdy : box.y;
  const bottom = south ? box.y + box.h + fdy : box.y + box.h;
  const rest = { x: Math.min(left, right), y: Math.min(top, bottom), w: Math.abs(right - left), h: Math.abs(bottom - top) };
  const guides: Guide[] = [];
  if (targets && onX) guides.push(...guidesOn("x", [east ? right : left], rest, targets));
  if (targets && onY) guides.push(...guidesOn("y", [south ? bottom : top], rest, targets));
  return { dx: fdx, dy: fdy, guides };
}
