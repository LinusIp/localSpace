// Drawing the scene on a 2D canvas: only what the camera shows, less at a
// distance, with the selection on top in screen space.

import { visible, type Camera } from "./camera.ts";
import { expand, type Box, type Point } from "./geometry.ts";
import { HANDLES, handlePoint } from "./hit.ts";
import type { Fill, Node } from "./model.ts";
import { TEXT_PADDING, type Scene } from "./scene.ts";
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
  selection: string;
  frame: string;
  fontFamily: string;
  palette: Record<Fill, Palette>;
}

export const LIGHT: Theme = {
  background: "#f5f7f6",
  grid: "#d9dedb",
  ink: "#1c1f1d",
  muted: "#5f6763",
  selection: "#1f9d5b",
  frame: "#cfd5d1",
  fontFamily: "system-ui, sans-serif",
  palette: {
    red: { fill: "#fbe9e9", stroke: "#c73e3e" },
    amber: { fill: "#fbf3e4", stroke: "#b7791f" },
    green: { fill: "#e8f6ee", stroke: "#1f9d5b" },
    blue: { fill: "#e8f0fb", stroke: "#2f6fcb" },
    yellow: { fill: "#fdf6d8", stroke: "#a88a17" },
    grey: { fill: "#eef0ef", stroke: "#5f6763" },
    none: { fill: "transparent", stroke: "#1c1f1d" },
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
}

const GRID = 24;
/** Below this many screen pixels per line of text, text is not drawn. */
const TEXT_MIN_PX = 3;
/** Below this many screen pixels of height, a shape is a flat rectangle. */
const DETAIL_MIN_PX = 6;

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
    const detail = z * dpr;
    let drawn = 0;
    for (const n of nodes) {
      drawn += 1;
      const quiet = n.id === overlay.editing;
      switch (n.kind) {
        case "frame":
          this.frame(n, detail);
          break;
        case "sticky":
        case "rect":
        case "ellipse":
          this.shape(scene, n, detail, quiet);
          break;
        case "text":
          if (!quiet) this.label(scene, n, detail);
          break;
        case "arrow":
          this.arrow(scene, n, detail);
          break;
        case "ink":
          this.ink(n, detail);
          break;
      }
    }
    this.lastDrawn = drawn;

    if (overlay.ink && overlay.ink.length > 1) this.polyline(overlay.ink, theme.palette.blue.stroke, 2 / z);
    if (overlay.arrow) this.arrowLine(overlay.arrow[0], overlay.arrow[1], theme.muted, 1.5 / z);

    // Screen space: the selection, one pixel wide whatever the zoom.
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    for (const id of selection) {
      const b = scene.bounds(id);
      const n = scene.get(id);
      if (!b || !n) continue;
      const sx = (b.x - camera.x) * z;
      const sy = (b.y - camera.y) * z;
      const sw = b.w * z;
      const sh = b.h * z;
      ctx.strokeStyle = theme.selection;
      ctx.lineWidth = 1.5;
      ctx.setLineDash(n.locked ? [4, 3] : []);
      ctx.strokeRect(sx - 2, sy - 2, sw + 4, sh + 4);
      ctx.setLineDash([]);
      if (selection.size === 1 && !n.locked && n.kind !== "arrow" && n.kind !== "ink") {
        ctx.fillStyle = "#ffffff";
        for (const hnd of HANDLES) {
          const p = handlePoint({ x: sx, y: sy, w: sw, h: sh }, hnd);
          ctx.fillRect(p.x - 4, p.y - 4, 8, 8);
          ctx.strokeRect(p.x - 4, p.y - 4, 8, 8);
        }
      }
    }
    if (overlay.marquee) {
      const m = overlay.marquee;
      ctx.strokeStyle = theme.selection;
      ctx.fillStyle = "rgba(31, 157, 91, 0.08)";
      ctx.lineWidth = 1;
      const sx = (m.x - camera.x) * z;
      const sy = (m.y - camera.y) * z;
      ctx.fillRect(sx, sy, m.w * z, m.h * z);
      ctx.strokeRect(sx, sy, m.w * z, m.h * z);
    }
  }

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
    ctx.fillStyle = theme.grid;
    for (let i = 0; i < cols; i++) {
      const sx = (x0 + i * GRID - camera.x) * z;
      for (let j = 0; j < rows; j++) {
        const sy = (y0 + j * GRID - camera.y) * z;
        ctx.fillRect(sx - 0.75, sy - 0.75, 1.5, 1.5);
      }
    }
  }

  private frame(n: Node, detail: number): void {
    const { ctx, theme } = this;
    ctx.strokeStyle = theme.frame;
    ctx.lineWidth = 1 / detail;
    this.roundRect(n.x, n.y, n.w, n.h, 10);
    ctx.stroke();
    if (12 * detail >= TEXT_MIN_PX) {
      ctx.fillStyle = theme.muted;
      ctx.font = fontFor(12, theme.fontFamily, 500);
      ctx.textBaseline = "alphabetic";
      ctx.fillText(n.name ?? "Frame", n.x + 8, n.y - 6);
    }
  }

  private shape(scene: Scene, n: Node, detail: number, quiet = false): void {
    const { ctx, theme } = this;
    const p = theme.palette[n.fill] ?? theme.palette.grey;
    const flat = n.h * detail < DETAIL_MIN_PX;
    ctx.fillStyle = n.kind === "sticky" ? p.fill : "#ffffff";
    if (n.kind === "ellipse") {
      ctx.beginPath();
      ctx.ellipse(n.x + n.w / 2, n.y + n.h / 2, n.w / 2, n.h / 2, 0, 0, Math.PI * 2);
    } else {
      this.roundRect(n.x, n.y, n.w, n.h, n.kind === "sticky" ? 6 : 10);
    }
    ctx.fill();
    if (flat) return;
    ctx.strokeStyle = p.stroke;
    ctx.lineWidth = 1.4;
    ctx.stroke();
    if (n.kind !== "sticky") {
      // The colour chip: what the fill means on a white shape.
      ctx.fillStyle = p.fill;
      ctx.fillRect(n.x + 10, n.y + 10, 14, 14);
      ctx.strokeStyle = p.stroke;
      ctx.lineWidth = 1;
      ctx.strokeRect(n.x + 10, n.y + 10, 14, 14);
    }
    if (n.text && !quiet && 13 * detail >= TEXT_MIN_PX) {
      const l = scene.textLayout(n);
      ctx.save();
      ctx.beginPath();
      ctx.rect(n.x, n.y, n.w, n.h);
      ctx.clip();
      ctx.fillStyle = theme.ink;
      ctx.font = fontFor(13, theme.fontFamily);
      ctx.textBaseline = "top";
      const top = n.y + (n.kind === "sticky" ? 30 : 30);
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

  private arrow(scene: Scene, n: Node, detail: number): void {
    const { ctx, theme } = this;
    const [a, b] = scene.endpoints(n);
    const p = theme.palette[n.fill] ?? theme.palette.grey;
    this.arrowLine(a, b, p.stroke, 1.6);
    if (n.text && 12 * detail >= TEXT_MIN_PX) {
      const mx = (a.x + b.x) / 2;
      const my = (a.y + b.y) / 2;
      ctx.font = fontFor(12, theme.fontFamily);
      const w = ctx.measureText(n.text).width + 8;
      ctx.fillStyle = theme.background;
      ctx.fillRect(mx - w / 2, my - 9, w, 18);
      ctx.fillStyle = theme.muted;
      ctx.textBaseline = "middle";
      ctx.textAlign = "center";
      ctx.fillText(n.text, mx, my);
      ctx.textAlign = "start";
    }
  }

  private arrowLine(a: Point, b: Point, colour: string, width: number): void {
    const { ctx } = this;
    ctx.strokeStyle = colour;
    ctx.fillStyle = colour;
    ctx.lineWidth = width;
    ctx.lineCap = "round";
    ctx.beginPath();
    ctx.moveTo(a.x, a.y);
    ctx.lineTo(b.x, b.y);
    ctx.stroke();
    const angle = Math.atan2(b.y - a.y, b.x - a.x);
    const head = 10;
    ctx.beginPath();
    ctx.moveTo(b.x, b.y);
    ctx.lineTo(b.x - head * Math.cos(angle - 0.45), b.y - head * Math.sin(angle - 0.45));
    ctx.lineTo(b.x - head * Math.cos(angle + 0.45), b.y - head * Math.sin(angle + 0.45));
    ctx.closePath();
    ctx.fill();
  }

  private ink(n: Node, detail: number): void {
    const pts = n.points ?? [];
    if (pts.length < 2) return;
    const p = this.theme.palette[n.fill] ?? this.theme.palette.blue;
    const width = detail < 0.3 ? 2 / detail : 2.2;
    this.polyline(
      pts.map(([x, y]) => ({ x, y })),
      p.stroke,
      width,
    );
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
