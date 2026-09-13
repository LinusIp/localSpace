// The retained scene: every node by id, an R-tree over their boxes for the
// viewport and the pointer, the z order cached until something changes, and
// the two things that depend on other nodes, an arrow's ends and a text
// label's height.

import { boxEdgeTowards, cubicPoints, ellipseEdgeTowards, centre, sideTowards, type Box, type Point } from "./geometry.ts";
import { boundsOf, type Node } from "./model.ts";
import { RTree } from "./rtree.ts";
import { fontFor, layout, type Layout, type TextMeasurer } from "./text.ts";

export const TEXT_PADDING = 8;
/** A note's text sits this far inside its edges, at this size and line height (the app screens' notes). */
export const STICKY_PADDING = 14;
export const STICKY_SIZE = 13.5;
export const STICKY_LINE_HEIGHT = 1.45;

/** A note's text laid out: the first line is its title, the rest its body. */
export interface StickyLayout {
  title: Layout;
  body: Layout | null;
}

/** A connector's curve between two shapes: a cubic from `a` to `b` through `c1` and `c2`. */
export interface Curve {
  a: Point;
  b: Point;
  c1: Point;
  c2: Point;
}

export class Scene {
  private nodes = new Map<string, Node>();
  private index = new RTree();
  private order: Node[] | null = null;
  /** Arrows attached to each shape, by shape id. */
  private attached = new Map<string, Set<string>>();
  private layouts = new Map<string, { key: string; layout: Layout }>();
  private stickyLayouts = new Map<string, { key: string; layout: StickyLayout }>();
  /** Bumped on every change; the renderer uses it to know when to redraw. */
  version = 0;
  readonly measurer: TextMeasurer;
  /** The face the text is measured in, the renderer's. */
  private fontFamily: string;

  constructor(measurer: TextMeasurer, fontFamily = "system-ui, sans-serif") {
    this.measurer = measurer;
    this.fontFamily = fontFamily;
  }

  /** Measure in another face from now on: every layout is done again. */
  setFontFamily(fontFamily: string): void {
    if (fontFamily === this.fontFamily) return;
    this.fontFamily = fontFamily;
    this.layouts.clear();
    this.stickyLayouts.clear();
    this.version += 1;
  }

  get size(): number {
    return this.nodes.size;
  }

  get(id: string): Node | undefined {
    return this.nodes.get(id);
  }

  has(id: string): boolean {
    return this.nodes.has(id);
  }

  /** Every node, bottom to top. */
  all(): readonly Node[] {
    if (!this.order) {
      this.order = [...this.nodes.values()].sort((a, b) => a.z - b.z || (a.id < b.id ? -1 : 1));
    }
    return this.order;
  }

  /** The nodes whose boxes touch `b`, bottom to top. */
  query(b: Box): Node[] {
    const keys = this.index.search(b);
    const out: Node[] = [];
    for (const k of keys) {
      const n = this.nodes.get(k);
      if (n) out.push(n);
    }
    out.sort((a, b) => a.z - b.z || (a.id < b.id ? -1 : 1));
    return out;
  }

  bounds(id: string): Box | undefined {
    return this.index.boxOf(id);
  }

  /** The box around every node, or an empty box for an empty scene. */
  extent(): Box {
    let out: Box | null = null;
    for (const n of this.nodes.values()) {
      const b = this.index.boxOf(n.id);
      if (!b) continue;
      if (!out) out = { ...b };
      else {
        const x = Math.min(out.x, b.x);
        const y = Math.min(out.y, b.y);
        const r = Math.max(out.x + out.w, b.x + b.w);
        const d = Math.max(out.y + out.h, b.y + b.h);
        out = { x, y, w: r - x, h: d - y };
      }
    }
    return out ?? { x: 0, y: 0, w: 0, h: 0 };
  }

  /** Add or replace a node. Text labels get their height from their text. */
  set(node: Node): void {
    const previous = this.nodes.get(node.id);
    if (node.kind === "text") node.h = this.textLayout(node).height + TEXT_PADDING * 2;
    if (previous?.kind === "arrow" || node.kind === "arrow") this.detach(node.id, previous);
    this.nodes.set(node.id, node);
    this.index.insert(node.id, boundsOf(node, (a) => this.endpoints(a)));
    if (node.kind === "arrow") this.attach(node);
    this.order = null;
    this.version += 1;
    // Arrows bound to this shape follow it.
    if (node.kind !== "arrow") {
      const arrows = this.attached.get(node.id);
      if (arrows) for (const id of arrows) this.reindex(id);
    }
  }

  delete(id: string): boolean {
    const node = this.nodes.get(id);
    if (!node) return false;
    // An arrow whose end is going keeps pointing where the shape was: its
    // ends are read while the shape is still here.
    const loosened: Node[] = [];
    for (const arrowId of this.attached.get(id) ?? []) {
      const arrow = this.nodes.get(arrowId);
      if (!arrow) continue;
      const [start, end] = this.endpoints(arrow);
      const next: Node = { ...arrow };
      if (arrow.from === id) {
        next.from = null;
        next.start = [start.x, start.y];
      }
      if (arrow.to === id) {
        next.to = null;
        next.end = [end.x, end.y];
      }
      loosened.push(next);
    }
    this.attached.delete(id);
    this.nodes.delete(id);
    this.index.remove(id);
    this.layouts.delete(id);
    this.stickyLayouts.delete(id);
    if (node.kind === "arrow") this.detach(id, node);
    this.order = null;
    this.version += 1;
    for (const arrow of loosened) this.set(arrow);
    return true;
  }

  clear(): void {
    this.nodes.clear();
    this.index.clear();
    this.attached.clear();
    this.layouts.clear();
    this.stickyLayouts.clear();
    this.order = null;
    this.version += 1;
  }

  /**
   * Where an arrow starts and ends. Between two shapes it runs from the
   * middle of the side of one that faces the other, so a row of notes is
   * joined side to side and a column top to bottom; towards a free end it
   * leaves the shape's edge in that direction.
   */
  endpoints(arrow: Node): [Point, Point] {
    const from = arrow.from ? this.nodes.get(arrow.from) : undefined;
    const to = arrow.to ? this.nodes.get(arrow.to) : undefined;
    const freeStart: Point = arrow.start ? { x: arrow.start[0], y: arrow.start[1] } : { x: arrow.x, y: arrow.y };
    const freeEnd: Point = arrow.end ? { x: arrow.end[0], y: arrow.end[1] } : { x: arrow.x + arrow.w, y: arrow.y + arrow.h };
    if (from && to) return [sideTowards(from, centre(to)), sideTowards(to, centre(from))];
    const start = from ? edgeTowards(from, freeEnd) : freeStart;
    const end = to ? edgeTowards(to, freeStart) : freeEnd;
    return [start, end];
  }

  /**
   * The curve of an arrow between two shapes, or null for one with a free
   * end, which is straight. It leaves and arrives square to the sides it
   * joins: an S between notes side by side, a straight drop between notes
   * one above the other.
   */
  arrowCurve(arrow: Node): Curve | null {
    if (!arrow.from || !arrow.to || !this.nodes.has(arrow.from) || !this.nodes.has(arrow.to)) return null;
    const [a, b] = this.endpoints(arrow);
    const dx = b.x - a.x;
    const dy = b.y - a.y;
    if (Math.abs(dx) >= Math.abs(dy)) return { a, b, c1: { x: a.x + dx / 2, y: a.y }, c2: { x: b.x - dx / 2, y: b.y } };
    return { a, b, c1: { x: a.x, y: a.y + dy / 2 }, c2: { x: b.x, y: b.y - dy / 2 } };
  }

  /** The arrow as a polyline: its two ends, or its curve sampled. */
  arrowPoints(arrow: Node): Point[] {
    const curve = this.arrowCurve(arrow);
    return curve ? cubicPoints(curve.a, curve.c1, curve.c2, curve.b) : this.endpoints(arrow);
  }

  /** The wrapped lines of a node's text, cached by what they depend on. */
  textLayout(node: Node): Layout {
    const size = node.kind === "text" ? (node.size ?? 16) : 13;
    const width = Math.max(8, node.w - TEXT_PADDING * 2);
    const key = `${node.text}\u0000${size}\u0000${width}\u0000${this.fontFamily}`;
    const hit = this.layouts.get(node.id);
    if (hit && hit.key === key) return hit.layout;
    const font = fontFor(size, this.fontFamily);
    const l = layout(this.measurer, node.text, font, size, width);
    this.layouts.set(node.id, { key, layout: l });
    return l;
  }

  /** A note's title and body, wrapped inside its padding, cached like `textLayout`. */
  stickyLayout(node: Node): StickyLayout {
    const width = Math.max(8, node.w - STICKY_PADDING * 2);
    const key = `${node.text}\u0000${width}\u0000${this.fontFamily}`;
    const hit = this.stickyLayouts.get(node.id);
    if (hit && hit.key === key) return hit.layout;
    const cut = node.text.indexOf("\n");
    const titleText = cut < 0 ? node.text : node.text.slice(0, cut);
    const bodyText = cut < 0 ? "" : node.text.slice(cut + 1);
    const title = layout(this.measurer, titleText, fontFor(STICKY_SIZE, this.fontFamily, 600), STICKY_SIZE, width, STICKY_LINE_HEIGHT);
    const body = bodyText.trim() ? layout(this.measurer, bodyText, fontFor(STICKY_SIZE, this.fontFamily), STICKY_SIZE, width, STICKY_LINE_HEIGHT) : null;
    const result = { title, body };
    this.stickyLayouts.set(node.id, { key, layout: result });
    return result;
  }

  private reindex(arrowId: string): void {
    const arrow = this.nodes.get(arrowId);
    if (arrow) this.index.insert(arrowId, boundsOf(arrow, (a) => this.endpoints(a)));
  }

  private attach(arrow: Node): void {
    for (const target of [arrow.from, arrow.to]) {
      if (!target) continue;
      let set = this.attached.get(target);
      if (!set) {
        set = new Set();
        this.attached.set(target, set);
      }
      set.add(arrow.id);
    }
  }

  private detach(arrowId: string, arrow: Node | undefined): void {
    if (!arrow) return;
    for (const target of [arrow.from, arrow.to]) {
      if (!target) continue;
      this.attached.get(target)?.delete(arrowId);
    }
  }
}

function edgeTowards(node: Node, target: Point): Point {
  const b = { x: node.x, y: node.y, w: node.w, h: node.h };
  return node.kind === "ellipse" ? ellipseEdgeTowards(b, target) : boxEdgeTowards(b, target);
}
