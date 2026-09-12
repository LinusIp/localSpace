// Exports: what a board looks like outside the app (6.0; plugin spec §18.3,
// the kinds image.v1 and svg.v1). An SVG is written from the nodes
// themselves — the same shapes, fills, wrapped text, arrows and strokes the
// renderer draws, in the same places — so it needs no browser and reads the
// same wherever it is opened. A PNG is the renderer drawing into an
// offscreen canvas at a fixed scale. Neither carries the grid, the selection
// or anything else that is the editor's rather than the board's.

import type { Box } from "./geometry.ts";
import { unionAll } from "./geometry.ts";
import type { Node } from "./model.ts";
import { LIGHT, Renderer, type Theme } from "./render.ts";
import { Scene, TEXT_PADDING } from "./scene.ts";
import { fontFor, type Layout } from "./text.ts";

/** A real fallback stack, written into every SVG, so the text is set the same way elsewhere. */
export const SVG_FONT = "system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif";
/** Board units of room around the content. */
export const EXPORT_PADDING = 24;
/** Device pixels per board unit in a PNG. */
export const EXPORT_SCALE = 2;
/** The longer side of a PNG, in pixels; the scale comes down to fit. */
export const EXPORT_MAX_SIDE = 8192;
/** Where the alphabetic baseline sits below the top of a line, as a share of the font size. */
const BASELINE = 0.8;

export interface ExportOptions {
  padding?: number;
  scale?: number;
  maxSide?: number;
  theme?: Theme;
}

export interface Raster {
  blob: Blob;
  width: number;
  height: number;
  /** Device pixels per board unit, after the cap. */
  scale: number;
}

/**
 * The nodes an export covers, bottom to top: the selection when there is
 * one — a selected frame brings the shapes in it — else the whole board.
 */
export function exportNodes(scene: Scene, selection: ReadonlySet<string>): Node[] {
  if (selection.size === 0) return [...scene.all()];
  const ids = new Set(selection);
  for (const n of scene.all()) if (n.frame && ids.has(n.frame)) ids.add(n.id);
  return scene.all().filter((n) => ids.has(n.id));
}

/** The box around `nodes` — an arrow by its ends — plus the padding; empty for none. */
export function exportBounds(scene: Scene, nodes: readonly Node[], padding = EXPORT_PADDING): Box {
  const boxes = nodes.map((n) => scene.bounds(n.id)).filter((b): b is Box => Boolean(b));
  if (boxes.length === 0) return { x: 0, y: 0, w: 0, h: 0 };
  const b = unionAll(boxes);
  return { x: b.x - padding, y: b.y - padding, w: b.w + padding * 2, h: b.h + padding * 2 };
}

const fmt = (n: number): string => (Number.isInteger(n) ? String(n) : n.toFixed(2).replace(/\.?0+$/, ""));
const esc = (s: string): string => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/**
 * One `<svg>` of `nodes`: every shape as its own element, text as `<text>`
 * with one positioned `<tspan>` per line the canvas would have wrapped, the
 * font size and line height written out, no scripts, no references outside
 * the file.
 */
export function toSvg(scene: Scene, nodes: readonly Node[], options: ExportOptions = {}): string {
  if (nodes.length === 0) throw new Error("nothing to export");
  const theme = options.theme ?? LIGHT;
  const box = exportBounds(scene, nodes, options.padding ?? EXPORT_PADDING);
  const out: string[] = [];
  const w = fmt(box.w);
  const h = fmt(box.h);
  out.push(
    `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="${fmt(box.x)} ${fmt(box.y)} ${w} ${h}" font-family="${SVG_FONT}">`,
  );
  out.push(`<rect x="${fmt(box.x)}" y="${fmt(box.y)}" width="${w}" height="${h}" fill="${theme.background}"/>`);

  const text = (x: number, top: number, size: number, colour: string, layout: Layout, clip?: string): void => {
    const clipAttr = clip ? ` clip-path="url(#${clip})"` : "";
    out.push(
      `<text font-size="${fmt(size)}" fill="${colour}" style="line-height:${fmt(layout.lineHeight)}px" xml:space="preserve"${clipAttr}>`,
    );
    layout.lines.forEach((line, i) => {
      out.push(`<tspan x="${fmt(x)}" y="${fmt(top + i * layout.lineHeight + size * BASELINE)}">${esc(line)}</tspan>`);
    });
    out.push("</text>");
  };

  let clips = 0;
  for (const n of nodes) {
    const p = theme.palette[n.fill] ?? theme.palette.grey;
    switch (n.kind) {
      case "frame": {
        out.push(`<rect x="${fmt(n.x)}" y="${fmt(n.y)}" width="${fmt(n.w)}" height="${fmt(n.h)}" fill="none" stroke="${theme.frame}" stroke-width="1"/>`);
        out.push(
          `<text x="${fmt(n.x + 8)}" y="${fmt(n.y - 6)}" font-size="12" font-weight="500" fill="${theme.muted}" xml:space="preserve">${esc(n.name ?? "Frame")}</text>`,
        );
        break;
      }
      case "sticky":
      case "rect":
      case "ellipse": {
        const fill = n.kind === "sticky" ? p.fill : "#ffffff";
        if (n.kind === "ellipse") {
          out.push(
            `<ellipse cx="${fmt(n.x + n.w / 2)}" cy="${fmt(n.y + n.h / 2)}" rx="${fmt(n.w / 2)}" ry="${fmt(n.h / 2)}" fill="${fill}" stroke="${p.stroke}" stroke-width="1.4"/>`,
          );
        } else {
          const r = Math.min(n.kind === "sticky" ? 6 : 10, n.w / 2, n.h / 2);
          out.push(
            `<rect x="${fmt(n.x)}" y="${fmt(n.y)}" width="${fmt(n.w)}" height="${fmt(n.h)}" rx="${fmt(r)}" fill="${fill}" stroke="${p.stroke}" stroke-width="1.4"/>`,
          );
        }
        if (n.kind !== "sticky") {
          // The colour chip: what the fill means on a white shape.
          out.push(`<rect x="${fmt(n.x + 10)}" y="${fmt(n.y + 10)}" width="14" height="14" fill="${p.fill}" stroke="${p.stroke}" stroke-width="1"/>`);
        }
        if (n.text) {
          clips += 1;
          const id = `clip${clips}`;
          out.push(`<clipPath id="${id}"><rect x="${fmt(n.x)}" y="${fmt(n.y)}" width="${fmt(n.w)}" height="${fmt(n.h)}"/></clipPath>`);
          text(n.x + TEXT_PADDING, n.y + 30, 13, theme.ink, scene.textLayout(n), id);
        }
        break;
      }
      case "text": {
        const size = n.size ?? 16;
        text(n.x + TEXT_PADDING, n.y + TEXT_PADDING, size, theme.ink, scene.textLayout(n));
        break;
      }
      case "arrow": {
        const [a, b] = scene.endpoints(n);
        out.push(
          `<line x1="${fmt(a.x)}" y1="${fmt(a.y)}" x2="${fmt(b.x)}" y2="${fmt(b.y)}" stroke="${p.stroke}" stroke-width="1.6" stroke-linecap="round"/>`,
        );
        const angle = Math.atan2(b.y - a.y, b.x - a.x);
        const size = 10;
        const p1 = { x: b.x - size * Math.cos(angle - 0.45), y: b.y - size * Math.sin(angle - 0.45) };
        const p2 = { x: b.x - size * Math.cos(angle + 0.45), y: b.y - size * Math.sin(angle + 0.45) };
        out.push(`<polygon points="${fmt(b.x)},${fmt(b.y)} ${fmt(p1.x)},${fmt(p1.y)} ${fmt(p2.x)},${fmt(p2.y)}" fill="${p.stroke}"/>`);
        if (n.text) {
          const mx = (a.x + b.x) / 2;
          const my = (a.y + b.y) / 2;
          const width = scene.measurer.width(n.text, fontFor(12)) + 8;
          out.push(`<rect x="${fmt(mx - width / 2)}" y="${fmt(my - 9)}" width="${fmt(width)}" height="18" fill="${theme.background}"/>`);
          out.push(
            `<text x="${fmt(mx)}" y="${fmt(my + 4)}" font-size="12" fill="${theme.muted}" text-anchor="middle" xml:space="preserve">${esc(n.text)}</text>`,
          );
        }
        break;
      }
      case "ink": {
        const pts = n.points ?? [];
        if (pts.length < 2) break;
        const stroke = (theme.palette[n.fill] ?? theme.palette.blue).stroke;
        out.push(
          `<polyline points="${pts.map(([x, y]) => `${fmt(x)},${fmt(y)}`).join(" ")}" fill="none" stroke="${stroke}" stroke-width="2.2" stroke-linejoin="round" stroke-linecap="round"/>`,
        );
        break;
      }
    }
  }
  out.push("</svg>");
  return out.join("\n");
}

/**
 * A PNG of `nodes` through the renderer: `scale` device pixels per board
 * unit, the longer side capped at `maxSide` by lowering the scale, the
 * board's background and no grid. Browser only.
 */
export async function rasterize(scene: Scene, nodes: readonly Node[], options: ExportOptions = {}): Promise<Raster> {
  if (nodes.length === 0) throw new Error("nothing to export");
  const theme = options.theme ?? LIGHT;
  const box = exportBounds(scene, nodes, options.padding ?? EXPORT_PADDING);
  const maxSide = options.maxSide ?? EXPORT_MAX_SIDE;
  const scale = Math.min(options.scale ?? EXPORT_SCALE, maxSide / Math.max(box.w, box.h, 1));
  const width = Math.max(1, Math.round(box.w * scale));
  const height = Math.max(1, Math.round(box.h * scale));
  if (typeof OffscreenCanvas === "undefined") throw new Error("this browser has no OffscreenCanvas to draw the export into");
  const canvas = new OffscreenCanvas(width, height);
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("no 2D context for the export");
  // The offscreen context draws with the same calls as the screen's.
  new Renderer(ctx as unknown as CanvasRenderingContext2D, theme).paintExport(scene, nodes, box, scale);
  const blob = await canvas.convertToBlob({ type: "image/png" });
  return { blob, width, height, scale };
}
