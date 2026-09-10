// Points, boxes and the few measurements a canvas needs. Plain data, no
// classes to allocate in the hot path.

export interface Point {
  x: number;
  y: number;
}

/** An axis-aligned box: `x, y` is the top-left corner. */
export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

export const EMPTY: Box = { x: 0, y: 0, w: 0, h: 0 };

export function box(x: number, y: number, w: number, h: number): Box {
  return { x, y, w, h };
}

export function fromPoints(a: Point, b: Point): Box {
  const x = Math.min(a.x, b.x);
  const y = Math.min(a.y, b.y);
  return { x, y, w: Math.abs(a.x - b.x), h: Math.abs(a.y - b.y) };
}

export function intersects(a: Box, b: Box): boolean {
  return a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y;
}

export function containsPoint(b: Box, p: Point): boolean {
  return p.x >= b.x && p.x <= b.x + b.w && p.y >= b.y && p.y <= b.y + b.h;
}

/** Whether `inner` lies entirely within `outer`. */
export function containsBox(outer: Box, inner: Box): boolean {
  return (
    inner.x >= outer.x &&
    inner.y >= outer.y &&
    inner.x + inner.w <= outer.x + outer.w &&
    inner.y + inner.h <= outer.y + outer.h
  );
}

export function union(a: Box, b: Box): Box {
  const x = Math.min(a.x, b.x);
  const y = Math.min(a.y, b.y);
  const r = Math.max(a.x + a.w, b.x + b.w);
  const d = Math.max(a.y + a.h, b.y + b.h);
  return { x, y, w: r - x, h: d - y };
}

export function unionAll(boxes: readonly Box[]): Box {
  if (boxes.length === 0) return EMPTY;
  let out = boxes[0];
  for (let i = 1; i < boxes.length; i++) out = union(out, boxes[i]);
  return out;
}

export function expand(b: Box, by: number): Box {
  return { x: b.x - by, y: b.y - by, w: b.w + by * 2, h: b.h + by * 2 };
}

export function area(b: Box): number {
  return b.w * b.h;
}

/** How much the union of two boxes grows over the first. */
export function enlargement(a: Box, b: Box): number {
  return area(union(a, b)) - area(a);
}

export function centre(b: Box): Point {
  return { x: b.x + b.w / 2, y: b.y + b.h / 2 };
}

export function distance(a: Point, b: Point): number {
  return Math.hypot(a.x - b.x, a.y - b.y);
}

/** Distance from `p` to the segment `a`–`b`. */
export function distanceToSegment(p: Point, a: Point, b: Point): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const len2 = dx * dx + dy * dy;
  if (len2 === 0) return distance(p, a);
  const t = Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2));
  return distance(p, { x: a.x + t * dx, y: a.y + t * dy });
}

/** Distance from `p` to a polyline. */
export function distanceToPolyline(p: Point, pts: readonly Point[]): number {
  if (pts.length === 0) return Infinity;
  if (pts.length === 1) return distance(p, pts[0]);
  let best = Infinity;
  for (let i = 1; i < pts.length; i++) {
    const d = distanceToSegment(p, pts[i - 1], pts[i]);
    if (d < best) best = d;
  }
  return best;
}

/** Whether a point lies inside the ellipse inscribed in `b`. */
export function ellipseContains(b: Box, p: Point): boolean {
  if (b.w <= 0 || b.h <= 0) return false;
  const c = centre(b);
  const nx = (p.x - c.x) / (b.w / 2);
  const ny = (p.y - c.y) / (b.h / 2);
  return nx * nx + ny * ny <= 1;
}

/** Where a ray from the centre of `b` towards `target` leaves the box. */
export function boxEdgeTowards(b: Box, target: Point): Point {
  const c = centre(b);
  const dx = target.x - c.x;
  const dy = target.y - c.y;
  if (dx === 0 && dy === 0) return c;
  const hw = b.w / 2;
  const hh = b.h / 2;
  const scale = Math.min(hw / Math.abs(dx || 1e-9), hh / Math.abs(dy || 1e-9));
  return { x: c.x + dx * scale, y: c.y + dy * scale };
}

/** Where a ray from the centre of the ellipse in `b` towards `target` leaves it. */
export function ellipseEdgeTowards(b: Box, target: Point): Point {
  const c = centre(b);
  const dx = target.x - c.x;
  const dy = target.y - c.y;
  if (dx === 0 && dy === 0) return c;
  const a = b.w / 2;
  const e = b.h / 2;
  const t = 1 / Math.sqrt((dx * dx) / (a * a) + (dy * dy) / (e * e));
  return { x: c.x + dx * t, y: c.y + dy * t };
}

export function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}

/** A short, unique-enough id for a shape made on this machine. */
export function newId(prefix: string): string {
  const time = Date.now().toString(36);
  const rand = Math.floor(Math.random() * 0xffffff).toString(36);
  return `${prefix}${time}_${rand}`;
}
