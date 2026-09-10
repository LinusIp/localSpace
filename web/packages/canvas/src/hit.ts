// Hit-testing: what is under a board point, and which handle of a selection.

import {
  containsPoint,
  distanceToPolyline,
  distanceToSegment,
  ellipseContains,
  expand,
  type Box,
  type Point,
} from "./geometry.ts";
import type { Node } from "./model.ts";
import type { Scene } from "./scene.ts";

export type Handle = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";

export const HANDLES: readonly Handle[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];

export function handlePoint(b: Box, h: Handle): Point {
  const cx = b.x + b.w / 2;
  const cy = b.y + b.h / 2;
  switch (h) {
    case "nw":
      return { x: b.x, y: b.y };
    case "n":
      return { x: cx, y: b.y };
    case "ne":
      return { x: b.x + b.w, y: b.y };
    case "e":
      return { x: b.x + b.w, y: cy };
    case "se":
      return { x: b.x + b.w, y: b.y + b.h };
    case "s":
      return { x: cx, y: b.y + b.h };
    case "sw":
      return { x: b.x, y: b.y + b.h };
    case "w":
      return { x: b.x, y: cy };
  }
}

/** The handle within `reach` board units of `p`, corners before edges. */
export function hitHandle(b: Box, p: Point, reach: number): Handle | null {
  for (const h of ["nw", "ne", "se", "sw", "n", "e", "s", "w"] as const) {
    const q = handlePoint(b, h);
    if (Math.abs(q.x - p.x) <= reach && Math.abs(q.y - p.y) <= reach) return h;
  }
  return null;
}

/** Whether `p` is on the node: inside a filled shape, near a line, on a frame's edge. */
export function nodeHit(scene: Scene, n: Node, p: Point, tolerance: number): boolean {
  const b = { x: n.x, y: n.y, w: n.w, h: n.h };
  switch (n.kind) {
    case "sticky":
    case "rect":
    case "text":
      return containsPoint(expand(b, tolerance), p);
    case "ellipse":
      return ellipseContains(expand(b, tolerance), p);
    case "arrow": {
      const [a, c] = scene.endpoints(n);
      return distanceToSegment(p, a, c) <= tolerance + 3;
    }
    case "ink": {
      const pts = (n.points ?? []).map(([x, y]) => ({ x, y }));
      return distanceToPolyline(p, pts) <= tolerance + 3;
    }
    case "frame": {
      // The edge and the title, not the inside: what is inside is its own.
      const inside = containsPoint(b, p);
      const onEdge = inside && !containsPoint(expand(b, -tolerance * 2), p);
      const onTitle = p.x >= b.x && p.x <= b.x + Math.max(60, Math.min(b.w, 200)) && p.y >= b.y - 22 && p.y < b.y;
      return onEdge || onTitle;
    }
  }
}

/** The topmost node under `p`, or null. */
export function hitTest(scene: Scene, p: Point, tolerance: number): Node | null {
  const probe = { x: p.x - tolerance - 3, y: p.y - tolerance - 3, w: (tolerance + 3) * 2, h: (tolerance + 3) * 2 };
  const candidates = scene.query(probe);
  for (let i = candidates.length - 1; i >= 0; i--) {
    if (nodeHit(scene, candidates[i], p, tolerance)) return candidates[i];
  }
  return null;
}

/** The nodes entirely within `b`, for a marquee; frames only when fully inside. */
export function nodesWithin(scene: Scene, b: Box): Node[] {
  return scene.query(b).filter((n) => {
    const nb = scene.bounds(n.id);
    return !!nb && nb.x >= b.x && nb.y >= b.y && nb.x + nb.w <= b.x + b.w && nb.y + nb.h <= b.y + b.h;
  });
}
