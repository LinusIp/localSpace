// Drawing the scene on a 2D canvas: only what the camera shows, less at a
// distance, batched where the eye cannot tell the order, with the
// selection on top in screen space. The look is the app screens' board
// (the UI reference of 2026-09-13): a dotted page, pastel notes without a
// border whose first line is their title, grey connectors that curve
// between the notes they join, a green selection with corner handles.

import { visible, type Camera } from "./camera.ts";
import { cubicPoint, expand, type Box, type Point } from "./geometry.ts";
import { handlePoint } from "./hit.ts";
import type { Fill, Node } from "./model.ts";
import { STICKY_PADDING, STICKY_SIZE, TEXT_PADDING, type Scene } from "./scene.ts";
import { GRID, type Guide } from "./snap.ts";
import { fontFor } from "./text.ts";

export interface Palette {
  fill: string;
  stroke: string;
}

export interface Theme {
  background: string;
  grid: string;
  ink: string;
  muted: string;
  /** Connectors between shapes, when their colour is the default grey. */
  connector: string;
  selection: string;
  /** Snapping guides. */
  guide: string;
  frame: string;
  /** The soft shadow under a note. */
  shadow: string;
  fontFamily: string;
  palette: Record<Fill, Palette>;
}

export const LIGHT: Theme = {
  background: "#faf9f8",
  grid: "#dedbd6",
  ink: "#1c1e20",
  muted: "#6b6e72",
  connector: "#9a9da1",
  selection: "#1d7a55",
  guide: "#d6336c",
  frame: "#dcd9d5",
  shadow: "rgba(28, 30, 32, 0.10)",
  fontFamily: '"Figtree", system-ui, -apple-system, "Segoe UI", sans-serif',
  palette: {
    red: { fill: "#f6d2cf", stroke: "#b8574d" },
    amber: { fill: "#f8ddb5", stroke: "#a8712a" },
    green: { fill: "#d9efdf", stroke: "#1d7a55" },
    blue: { fill: "#cde3f5", stroke: "#3e6fa8" },
    yellow: { fill: "#fbe8a6", stroke: "#a88a17" },
    grey: { fill: "#e9e7e3", stroke: "#6b6e72" },
    none: { fill: "transparent", stroke: "#1c1e20" },
  },
};

export interface Viewport {
  /** CSS pixels. */
  w: number;
  h: number;
  dpr: number;
}

export interface Overlay {
  /** A marquee being dragged, in board coordinates. */
  marquee?: Box | null;
  /** An ink stroke being drawn, in board coordinates. */
  ink?: Point[] | null;
  /** An arrow being drawn. */
  arrow?: [Point, Point] | null;
  /** The node whose text is in the text editor, so its own text is not drawn under it. */
  editing?: string | null;
  /** Snapping guides for the gesture in progress, in board coordinates. */
  guides?: Guide[] | null;
}
/** Below this many screen pixels per line of text, text is not drawn. */
const TEXT_MIN_PX = 3;
/** Below this many screen pixels of height, a shape is a flat fill in a batch. */
const DETAIL_MIN_PX = 14;
/** Below this zoom, arrows are lines without heads and strokes are batched. */
const HEAD_MIN_DETAIL = 0.35;
/** Below this many screen pixels of height, a note is drawn without its shadow. */
const SHADOW_MIN_PX = 40;
/** Connectors: their width in board units at full detail, and the head's length. */
const CONNECTOR_WIDTH = 2.2;
const HEAD = 9;
/** The gap between a note's title and its body, in board units. */
const TITLE_GAP = 5;
/** The selection sits this far outside the shape, in screen pixels. */
const SELECTION_INSET = 5;
const HANDLE = 9;

export class Renderer {
  private readonly ctx: CanvasRenderingContext2D;
  private theme: Theme;
  /** What was drawn last, so an unchanged frame costs nothing. */
  private drawn = { version: -1, x: NaN, y: NaN, z: NaN, w: 0, h: 0, dpr: 0, selection: "", overlay: "" };
  /** Shapes drawn in the last frame, for the benchmark and the tests. */
  lastDrawn = 0;

  constructor(ctx: CanvasRenderingContext2D, theme: Theme = LIGHT) {
    this.ctx = ctx;
    this.theme = theme;
  }

  setTheme(theme: Theme): void {
    this.theme = theme;
    this.drawn.version = -1;
  }

  /** Forget the last frame, so the next `draw` paints whatever changed outside the scene: a font that arrived. */
  invalidate(): void {
    this.drawn.version = -1;
  }

  /** Draw if anything changed since the last call; returns whether it did. */
  draw(scene: Scene, camera: Camera, viewport: Viewport, selection: ReadonlySet<string>, overlay: Overlay = {}): boolean {
    const selectionKey = [...selection].join(",");
    const overlayKey = JSON.stringify(overlay);
    const d = this.drawn;
    if (
      d.version === scene.version &&
      d.x === camera.x &&
      d.y === camera.y &&
      d.z === camera.z &&
      d.w === viewport.w &&
      d.h === viewport.h &&
      d.dpr === viewport.dpr &&
      d.selection === selectionKey &&
      d.overlay === overlayKey
    ) {
      return false;
    }
    this.drawn = { version: scene.version, x: camera.x, y: camera.y, z: camera.z, w: viewport.w, h: viewport.h, dpr: viewport.dpr, selection: selectionKey, overlay: overlayKey };
    this.paint(scene, camera, viewport, selection, overlay);
    return true;
  }

  /**
   * Draw `nodes` alone into `box` of the board at `scale` device pixels per
   * unit, on the background, with no grid and no selection: an export. The
   * next `draw` repaints the screen in full.
   */
  paintExport(scene: Scene, nodes: readonly Node[], box: Box, scale: number): void {
    const { ctx, theme } = this;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.fillStyle = theme.background;
    ctx.fillRect(0, 0, Math.ceil(box.w * scale), Math.ceil(box.h * scale));
    ctx.setTransform(scale, 0, 0, scale, -box.x * scale, -box.y * scale);
    this.lastDrawn = this.nodes(scene, nodes, scale, null);
    this.drawn.version = -1;
  }

  /** Draw unconditionally. */
  paint(scene: Scene, camera: Camera, viewport: Viewport, selection: ReadonlySet<string>, overlay: Overlay = {}): void {
    const { ctx, theme } = this;
    const { w, h, dpr } = viewport;
    const z = camera.z;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.fillStyle = theme.background;
    ctx.fillRect(0, 0, w, h);

    const view = visible(camera, w, h);
    this.grid(view, camera, w, h);

    // Board space: shapes in their own units, the camera in the transform.
    ctx.setTransform(dpr * z, 0, 0, dpr * z, -camera.x * z * dpr, -camera.y * z * dpr);
    const nodes = scene.query(expand(view, 40));
    this.lastDrawn = this.nodes(scene, nodes, z * dpr, overlay.editing ?? null);

    if (overlay.ink && overlay.ink.length > 1) this.polyline(overlay.ink, theme.palette.blue.stroke, 2 / z);
    if (overlay.arrow) this.arrowLine(overlay.arrow[0], overlay.arrow[1], theme.connector, CONNECTOR_WIDTH / z, true);

    // Screen space: the selection, one pixel wide whatever the zoom.
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    this.selection(scene, camera, selection);
    if (overlay.guides?.length) this.guides(overlay.guides, camera);
    if (overlay.marquee) {
      const m = overlay.marquee;
      ctx.strokeStyle = theme.selection;
      ctx.fillStyle = "rgba(29, 122, 85, 0.08)";
      ctx.lineWidth = 1;
      const sx = (m.x - camera.x) * z;
      const sy = (m.y - camera.y) * z;
      ctx.fillRect(sx, sy, m.w * z, m.h * z);
      ctx.strokeRect(sx, sy, m.w * z, m.h * z);
    }
  }

  /**
   * Draw a set of nodes in board space (the transform is already set).
   * Small shapes are batched by colour into one path each; large ones are
   * drawn one by one in z order with their text. Returns how many.
   */
  nodes(scene: Scene, nodes: readonly Node[], detail: number, editing: string | null): number {
    const { ctx, theme } = this;
    const frames = new Path2D();
    let anyFrame = false;
    const flatFills = new Map<string, Path2D>();
    const flatStrokes = new Map<string, Path2D>();
    const lines = new Map<string, Path2D>();
    const strokes = new Map<string, Path2D>();
    const detailed: Node[] = [];
    const heads = detail >= HEAD_MIN_DETAIL;
    const into = (map: Map<string, Path2D>, colour: string): Path2D => {
      let p = map.get(colour);
      if (!p) {
        p = new Path2D();
        map.set(colour, p);
      }
      return p;
    };

    for (const n of nodes) {
      switch (n.kind) {
        case "frame":
          frames.rect(n.x, n.y, n.w, n.h);
          anyFrame = true;
          break;
        case "sticky":
        case "rect":
        case "ellipse": {
          if (n.h * detail >= DETAIL_MIN_PX) {
            detailed.push(n);
            break;
          }
          const p = theme.palette[n.fill] ?? theme.palette.grey;
          if (n.kind === "sticky") {
            into(flatFills, p.fill).rect(n.x, n.y, n.w, n.h);
          } else {
            into(flatFills, "#ffffff").rect(n.x, n.y, n.w, n.h);
            const s = into(flatStrokes, p.stroke);
            if (n.kind === "ellipse") s.ellipse(n.x + n.w / 2, n.y + n.h / 2, n.w / 2, n.h / 2, 0, 0, Math.PI * 2);
            else s.rect(n.x, n.y, n.w, n.h);
          }
          break;
        }
        case "text":
          if (n.id !== editing) detailed.push(n);
          break;
        case "arrow": {
          if (heads) {
            detailed.push(n);
            break;
          }
          const [a, b] = scene.endpoints(n);
          const path = into(lines, this.connectorColour(n));
          path.moveTo(a.x, a.y);
          path.lineTo(b.x, b.y);
          break;
        }
        case "ink": {
          const pts = n.points ?? [];
          if (pts.length < 2) break;
          const p = theme.palette[n.fill] ?? theme.palette.blue;
          const path = into(strokes, p.stroke);
          path.moveTo(pts[0][0], pts[0][1]);
          for (let i = 1; i < pts.length; i++) path.lineTo(pts[i][0], pts[i][1]);
          break;
        }
      }
    }

    if (anyFrame) {
      ctx.strokeStyle = theme.frame;
      ctx.lineWidth = 1 / detail;
      ctx.stroke(frames);
      if (12 * detail >= TEXT_MIN_PX) {
        ctx.fillStyle = theme.muted;
        ctx.font = fontFor(12, theme.fontFamily, 500);
        ctx.textBaseline = "alphabetic";
        for (const n of nodes) if (n.kind === "frame") ctx.fillText(n.name ?? "Frame", n.x + 8, n.y - 6);
      }
    }
    for (const [colour, path] of flatFills) {
      ctx.fillStyle = colour;
      ctx.fill(path);
    }
    if (flatStrokes.size) {
      ctx.lineWidth = 1 / detail;
      for (const [colour, path] of flatStrokes) {
        ctx.strokeStyle = colour;
        ctx.stroke(path);
      }
    }
    for (const n of detailed) {
      switch (n.kind) {
        case "sticky":
          this.sticky(scene, n, detail, n.id === editing);
          break;
        case "rect":
        case "ellipse":
          this.shape(scene, n, detail, n.id === editing);
          break;
        case "text":
          this.label(scene, n, detail);
          break;
        case "arrow":
          this.arrow(scene, n, detail);
          break;
        default:
          break;
      }
    }
    if (lines.size) {
      ctx.lineWidth = Math.max(1 / detail, 1.6);
      ctx.lineCap = "round";
      for (const [colour, path] of lines) {
        ctx.strokeStyle = colour;
        ctx.stroke(path);
      }
    }
    if (strokes.size) {
      ctx.lineWidth = detail < 0.3 ? 2 / detail : 2.2;
      ctx.lineJoin = "round";
      ctx.lineCap = "round";
      for (const [colour, path] of strokes) {
        ctx.strokeStyle = colour;
        ctx.stroke(path);
      }
    }
    return nodes.length;
  }

  /** A connector is grey unless it was given a colour. */
  private connectorColour(n: Node): string {
    return n.fill === "grey" ? this.theme.connector : (this.theme.palette[n.fill] ?? this.theme.palette.grey).stroke;
  }

  /** The selection outlines and corner handles, in screen space (transform already set). */
  private selection(scene: Scene, camera: Camera, selection: ReadonlySet<string>): void {
    const { ctx, theme } = this;
    const z = camera.z;
    for (const id of selection) {
      const b = scene.bounds(id);
      const n = scene.get(id);
      if (!b || !n) continue;
      const box = {
        x: (b.x - camera.x) * z - SELECTION_INSET,
        y: (b.y - camera.y) * z - SELECTION_INSET,
        w: b.w * z + SELECTION_INSET * 2,
        h: b.h * z + SELECTION_INSET * 2,
      };
      ctx.strokeStyle = theme.selection;
      ctx.lineWidth = 1.6;
      ctx.setLineDash(n.locked ? [4, 3] : []);
      this.roundRect(box.x, box.y, box.w, box.h, 6);
      ctx.stroke();
      ctx.setLineDash([]);
      if (selection.size === 1 && !n.locked && n.kind !== "arrow" && n.kind !== "ink") {
        for (const hnd of ["nw", "ne", "se", "sw"] as const) {
          const p = handlePoint(box, hnd);
          ctx.fillStyle = "#ffffff";
          this.roundRect(p.x - HANDLE / 2, p.y - HANDLE / 2, HANDLE, HANDLE, 2);
          ctx.fill();
          ctx.stroke();
        }
      }
    }
  }

  /** Snapping guides, one pixel wide in screen space (transform already set). */
  private guides(guides: readonly Guide[], camera: Camera): void {
    const { ctx, theme } = this;
    const z = camera.z;
    const path = new Path2D();
    for (const g of guides) {
      if (g.axis === "x") {
        const sx = Math.round((g.at - camera.x) * z) + 0.5;
        path.moveTo(sx, (g.from - camera.y) * z);
        path.lineTo(sx, (g.to - camera.y) * z);
      } else {
        const sy = Math.round((g.at - camera.y) * z) + 0.5;
        path.moveTo((g.from - camera.x) * z, sy);
        path.lineTo((g.to - camera.x) * z, sy);
      }
    }
    ctx.strokeStyle = theme.guide;
    ctx.lineWidth = 1;
    ctx.setLineDash([]);
    ctx.stroke(path);
  }

  /** The page's dots, one at every grid point the camera shows. */
  private grid(view: Box, camera: Camera, w: number, h: number): void {
    const z = camera.z;
    const step = GRID * z;
    if (step < 8) return;
    const { ctx, theme } = this;
    const x0 = Math.floor(view.x / GRID) * GRID;
    const y0 = Math.floor(view.y / GRID) * GRID;
    const cols = Math.ceil(w / step) + 1;
    const rows = Math.ceil(h / step) + 1;
    if (cols * rows > 6000) return;
    const r = 1.2;
    const dots = new Path2D();
    for (let i = 0; i < cols; i++) {
      const sx = (x0 + i * GRID - camera.x) * z;
      for (let j = 0; j < rows; j++) {
        const sy = (y0 + j * GRID - camera.y) * z;
        dots.moveTo(sx + r, sy);
        dots.arc(sx, sy, r, 0, Math.PI * 2);
      }
    }
    ctx.fillStyle = theme.grid;
    ctx.fill(dots);
  }

  /** A note: a pastel card without a border, its first line the title in bold, the rest below. */
  private sticky(scene: Scene, n: Node, detail: number, quiet: boolean): void {
    const { ctx, theme } = this;
    const p = theme.palette[n.fill] ?? theme.palette.yellow;
    ctx.save();
    if (n.h * detail >= SHADOW_MIN_PX) {
      ctx.shadowColor = theme.shadow;
      ctx.shadowBlur = 5 * detail;
      ctx.shadowOffsetY = 2 * detail;
    }
    ctx.fillStyle = p.fill;
    this.roundRect(n.x, n.y, n.w, n.h, 4);
    ctx.fill();
    ctx.restore();
    if (!n.text || quiet || STICKY_SIZE * detail < TEXT_MIN_PX) return;
    const l = scene.stickyLayout(n);
    ctx.save();
    ctx.beginPath();
    ctx.rect(n.x, n.y, n.w, n.h);
    ctx.clip();
    ctx.fillStyle = theme.ink;
    ctx.textBaseline = "top";
    const x = n.x + STICKY_PADDING;
    let y = n.y + STICKY_PADDING;
    ctx.font = fontFor(STICKY_SIZE, theme.fontFamily, 600);
    for (const line of l.title.lines) {
      ctx.fillText(line, x, y);
      y += l.title.lineHeight;
    }
    if (l.body) {
      y += TITLE_GAP;
      ctx.font = fontFor(STICKY_SIZE, theme.fontFamily);
      for (const line of l.body.lines) {
        ctx.fillText(line, x, y);
        y += l.body.lineHeight;
      }
    }
    ctx.restore();
  }

  /** A rectangle or an ellipse: white, outlined in its colour, with the colour as a chip. */
  private shape(scene: Scene, n: Node, detail: number, quiet: boolean): void {
    const { ctx, theme } = this;
    const p = theme.palette[n.fill] ?? theme.palette.grey;
    ctx.fillStyle = "#ffffff";
    if (n.kind === "ellipse") {
      ctx.beginPath();
      ctx.ellipse(n.x + n.w / 2, n.y + n.h / 2, n.w / 2, n.h / 2, 0, 0, Math.PI * 2);
    } else {
      this.roundRect(n.x, n.y, n.w, n.h, 10);
    }
    ctx.fill();
    ctx.strokeStyle = p.stroke;
    ctx.lineWidth = 1.4;
    ctx.stroke();
    // The colour chip: what the fill means on a white shape.
    ctx.fillStyle = p.fill;
    ctx.fillRect(n.x + 10, n.y + 10, 14, 14);
    ctx.strokeStyle = p.stroke;
    ctx.lineWidth = 1;
    ctx.strokeRect(n.x + 10, n.y + 10, 14, 14);
    if (n.text && !quiet && 13 * detail >= TEXT_MIN_PX) {
      const l = scene.textLayout(n);
      ctx.save();
      ctx.beginPath();
      ctx.rect(n.x, n.y, n.w, n.h);
      ctx.clip();
      ctx.fillStyle = theme.ink;
      ctx.font = fontFor(13, theme.fontFamily);
      ctx.textBaseline = "top";
      const top = n.y + 30;
      for (let i = 0; i < l.lines.length; i++) {
        ctx.fillText(l.lines[i], n.x + TEXT_PADDING, top + i * l.lineHeight);
      }
      ctx.restore();
    }
  }

  private label(scene: Scene, n: Node, detail: number): void {
    const size = n.size ?? 16;
    if (size * detail < TEXT_MIN_PX) return;
    const { ctx, theme } = this;
    const l = scene.textLayout(n);
    ctx.fillStyle = theme.ink;
    ctx.font = fontFor(size, theme.fontFamily);
    ctx.textBaseline = "top";
    for (let i = 0; i < l.lines.length; i++) {
      ctx.fillText(l.lines[i], n.x + TEXT_PADDING, n.y + TEXT_PADDING + i * l.lineHeight);
    }
  }

  /** A connector: curved between the two shapes it joins, straight to a free end, with a head at its end. */
  private arrow(scene: Scene, n: Node, detail: number): void {
    const { ctx, theme } = this;
    const [a, b] = scene.endpoints(n);
    const curve = scene.arrowCurve(n);
    const colour = this.connectorColour(n);
    ctx.strokeStyle = colour;
    ctx.fillStyle = colour;
    ctx.lineWidth = CONNECTOR_WIDTH;
    ctx.lineCap = "round";
    ctx.beginPath();
    ctx.moveTo(a.x, a.y);
    if (curve) ctx.bezierCurveTo(curve.c1.x, curve.c1.y, curve.c2.x, curve.c2.y, b.x, b.y);
    else ctx.lineTo(b.x, b.y);
    ctx.stroke();
    this.head(curve ? curve.c2 : a, b);
    if (n.text && 12 * detail >= TEXT_MIN_PX) {
      const mid = curve ? cubicPoint(a, curve.c1, curve.c2, b, 0.5) : { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
      ctx.font = fontFor(12, theme.fontFamily);
      const w = ctx.measureText(n.text).width + 8;
      ctx.fillStyle = theme.background;
      ctx.fillRect(mid.x - w / 2, mid.y - 9, w, 18);
      ctx.fillStyle = theme.muted;
      ctx.textBaseline = "middle";
      ctx.textAlign = "center";
      ctx.fillText(n.text, mid.x, mid.y);
      ctx.textAlign = "start";
    }
  }

  /** A filled head at `b`, pointing away from `from`. */
  private head(from: Point, b: Point): void {
    const { ctx } = this;
    const angle = Math.atan2(b.y - from.y, b.x - from.x);
    ctx.beginPath();
    ctx.moveTo(b.x, b.y);
    ctx.lineTo(b.x - HEAD * Math.cos(angle - 0.5), b.y - HEAD * Math.sin(angle - 0.5));
    ctx.lineTo(b.x - HEAD * Math.cos(angle + 0.5), b.y - HEAD * Math.sin(angle + 0.5));
    ctx.closePath();
    ctx.fill();
  }

  private arrowLine(a: Point, b: Point, colour: string, width: number, head: boolean): void {
    const { ctx } = this;
    ctx.strokeStyle = colour;
    ctx.fillStyle = colour;
    ctx.lineWidth = width;
    ctx.lineCap = "round";
    ctx.beginPath();
    ctx.moveTo(a.x, a.y);
    ctx.lineTo(b.x, b.y);
    ctx.stroke();
    if (head) this.head(a, b);
  }

  private polyline(pts: readonly Point[], colour: string, width: number): void {
    const { ctx } = this;
    ctx.strokeStyle = colour;
    ctx.lineWidth = width;
    ctx.lineJoin = "round";
    ctx.lineCap = "round";
    ctx.beginPath();
    ctx.moveTo(pts[0].x, pts[0].y);
    for (let i = 1; i < pts.length; i++) ctx.lineTo(pts[i].x, pts[i].y);
    ctx.stroke();
  }

  private roundRect(x: number, y: number, w: number, h: number, r: number): void {
    const { ctx } = this;
    const rr = Math.min(r, w / 2, h / 2);
    ctx.beginPath();
    ctx.moveTo(x + rr, y);
    ctx.lineTo(x + w - rr, y);
    ctx.arcTo(x + w, y, x + w, y + rr, rr);
    ctx.lineTo(x + w, y + h - rr);
    ctx.arcTo(x + w, y + h, x + w - rr, y + h, rr);
    ctx.lineTo(x + rr, y + h);
    ctx.arcTo(x, y + h, x, y + h - rr, rr);
    ctx.lineTo(x, y + rr);
    ctx.arcTo(x, y, x + rr, y, rr);
    ctx.closePath();
  }
}
