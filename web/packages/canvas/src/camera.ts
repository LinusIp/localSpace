// The camera: where the board is on the screen. `x, y` is the board point
// at the screen's top-left; `z` is pixels per board unit.

import { clamp, type Box, type Point } from "./geometry.ts";

export interface Camera {
  x: number;
  y: number;
  z: number;
}

export const MIN_ZOOM = 0.05;
export const MAX_ZOOM = 8;

export function toScreen(c: Camera, p: Point): Point {
  return { x: (p.x - c.x) * c.z, y: (p.y - c.y) * c.z };
}

export function toBoard(c: Camera, p: Point): Point {
  return { x: p.x / c.z + c.x, y: p.y / c.z + c.y };
}

/** The board rectangle a viewport of `w × h` screen pixels shows. */
export function visible(c: Camera, w: number, h: number): Box {
  return { x: c.x, y: c.y, w: w / c.z, h: h / c.z };
}

export function pan(c: Camera, dxScreen: number, dyScreen: number): Camera {
  return { x: c.x - dxScreen / c.z, y: c.y - dyScreen / c.z, z: c.z };
}

/** Zoom by `factor` keeping the board point under `at` (screen) in place. */
export function zoomAt(c: Camera, at: Point, factor: number): Camera {
  const z = clamp(c.z * factor, MIN_ZOOM, MAX_ZOOM);
  if (z === c.z) return c;
  const before = toBoard(c, at);
  return { x: before.x - at.x / z, y: before.y - at.y / z, z };
}

/** Zoom to exactly `z`, keeping the centre of a `w × h` viewport in place. */
export function zoomTo(c: Camera, z: number, w: number, h: number): Camera {
  const target = clamp(z, MIN_ZOOM, MAX_ZOOM);
  return zoomAt(c, { x: w / 2, y: h / 2 }, target / c.z);
}

/** Frame `b` inside a `w × h` viewport with a margin, never above 1:1. */
export function fit(b: Box, w: number, h: number, margin = 40): Camera {
  if (b.w <= 0 || b.h <= 0) return { x: -w / 2, y: -h / 2, z: 1 };
  const z = clamp(Math.min((w - margin * 2) / b.w, (h - margin * 2) / b.h, 1), MIN_ZOOM, MAX_ZOOM);
  return { x: b.x + b.w / 2 - w / (2 * z), y: b.y + b.h / 2 - h / (2 * z), z };
}
