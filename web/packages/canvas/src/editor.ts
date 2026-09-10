// The editor: a canvas element, a scene, a camera, a tool, a selection, and
// the pointer and keyboard turned into changes. Undo is not here: it is the
// environment's, through Core's history; Ctrl+Z is an event the host acts on.

import { fit as fitCamera, pan, toBoard, toScreen, visible, zoomAt, zoomTo, type Camera } from "./camera.ts";
import { centre, containsPoint, distance, fromPoints, newId, unionAll, type Box, type Point } from "./geometry.ts";
import { hitHandle, hitTest, nodesWithin, type Handle } from "./hit.ts";
import { DEFAULTS, cloneNode, type Fill, type Kind, type Node } from "./model.ts";
import { LIGHT, Renderer, type Theme } from "./render.ts";
import { Scene } from "./scene.ts";
import { GRID, snapMove, snapReach, snapResize, snapTargets, type Guide, type SnapTargets, type Snapped } from "./snap.ts";
import { CanvasMeasurer, FixedMeasurer, type TextMeasurer } from "./text.ts";

export type Tool = "select" | "hand" | "sticky" | "rect" | "ellipse" | "text" | "arrow" | "ink" | "frame";

export const TOOLS: readonly Tool[] = ["select", "hand", "sticky", "rect", "ellipse", "text", "arrow", "ink", "frame"];

export interface Change {
  set: Node[];
  deleted: string[];
}

export type Cause = "create" | "move" | "resize" | "text" | "delete" | "style" | "order" | "draw" | "duplicate";

export interface EditorEvents {
  change: (change: Change, cause: Cause) => void;
  selection: (ids: string[]) => void;
  camera: (camera: Camera) => void;
  tool: (tool: Tool) => void;
  undo: () => void;
  redo: () => void;
  editing: (id: string | null) => void;
}

export interface EditorOptions {
  canvas: HTMLCanvasElement;
  /** The element the canvas fills; receives the keyboard and the text editor. */
  host: HTMLElement;
  theme?: Theme;
  measurer?: TextMeasurer;
}

// A move keeps the box of what it moves and what that box can rest on, both
// taken at its first step; a resize takes what it can rest on at its first.
type Drag =
  | { kind: "pan"; last: Point }
  | { kind: "move"; start: Point; origin: Map<string, Node>; moved: boolean; box: Box | null; targets: SnapTargets | null }
  | { kind: "resize"; handle: Handle; start: Point; origin: Node; box: Box; targets: SnapTargets | null }
  | { kind: "marquee"; start: Point; additive: boolean }
  | { kind: "create"; tool: Kind; start: Point }
  | { kind: "arrow"; start: Point; from: string | null }
  | { kind: "ink"; points: Point[] };

const HIT_PX = 6;
const HANDLE_PX = 7;
const MIN_SIZE = 20;

/** What a move or a resize comes to rest on. Alt held skips both. */
export interface Snapping {
  /** The edges and centres of the shapes and frames on screen. On by default. */
  shapes: boolean;
  /** The grid's points. Off by default. */
  grid: boolean;
}

export class Editor {
  readonly scene: Scene;
  camera: Camera = { x: -40, y: -40, z: 1 };
  tool: Tool = "select";
  readonly selection = new Set<string>();
  /** The node whose text is being edited, if any. */
  editing: string | null = null;

  private readonly canvas: HTMLCanvasElement;
  private readonly host: HTMLElement;
  private readonly renderer: Renderer;
  private listeners = new Map<keyof EditorEvents, Set<(...args: never[]) => void>>();
  private drag: Drag | null = null;
  private overlay: { marquee?: Box | null; ink?: Point[] | null; arrow?: [Point, Point] | null; guides?: Guide[] | null } = {};
  /** Where the pointer was last seen in a move or resize, in screen pixels: Alt pressed or let go places the shapes again from there. */
  private pointer: Point | null = null;
  private snap: Snapping = { shapes: true, grid: false };
  private frame: number | null = null;
  private viewport = { w: 0, h: 0, dpr: 1 };
  private space = false;
  private observer: ResizeObserver | null = null;
  private textarea: HTMLTextAreaElement | null = null;
  private keepTool = false;
  private disposed = false;

  constructor(options: EditorOptions) {
    this.canvas = options.canvas;
    this.host = options.host;
    const ctx = this.canvas.getContext("2d");
    if (!ctx) throw new Error("the canvas has no 2D context");
    const measurer = options.measurer ?? (typeof document === "undefined" ? new FixedMeasurer() : new CanvasMeasurer(ctx));
    this.scene = new Scene(measurer);
    this.renderer = new Renderer(ctx, options.theme ?? LIGHT);
    this.host.tabIndex = this.host.tabIndex < 0 ? 0 : this.host.tabIndex;
    this.host.style.touchAction = "none";
    this.host.style.outline = "none";
    this.attach();
    this.resize();
  }

  // -- events ---------------------------------------------------------------

  on<K extends keyof EditorEvents>(event: K, fn: EditorEvents[K]): () => void {
    let set = this.listeners.get(event);
    if (!set) {
      set = new Set();
      this.listeners.set(event, set);
    }
    set.add(fn as (...args: never[]) => void);
    return () => set?.delete(fn as (...args: never[]) => void);
  }

  private emit<K extends keyof EditorEvents>(event: K, ...args: Parameters<EditorEvents[K]>): void {
    for (const fn of this.listeners.get(event) ?? []) (fn as (...a: Parameters<EditorEvents[K]>) => void)(...args);
  }

  // -- the document ----------------------------------------------------------

  /** Replace everything: the document as it arrived. */
  load(nodes: readonly Node[]): void {
    const held = this.heldIds();
    const keep = new Map<string, Node>();
    for (const id of held) {
      const n = this.scene.get(id);
      if (n) keep.set(id, n);
    }
    this.scene.clear();
    for (const n of nodes) this.scene.set(keep.get(n.id) ?? cloneNode(n));
    for (const [id, n] of keep) if (!this.scene.has(id)) this.scene.set(n);
    this.pruneSelection();
    this.render();
  }

  /** Changes from elsewhere: no event back, the user's own gesture untouched. */
  applyRemote(change: Change): void {
    const held = this.heldIds();
    for (const n of change.set) if (!held.has(n.id)) this.scene.set(cloneNode(n));
    for (const id of change.deleted) if (!held.has(id)) this.scene.delete(id);
    this.pruneSelection();
    this.render();
  }

  /** A local change: into the scene, out as an event. */
  private commit(change: Change, cause: Cause): void {
    for (const n of change.set) this.scene.set(n);
    for (const id of change.deleted) this.scene.delete(id);
    this.pruneSelection();
    this.render();
    if (change.set.length || change.deleted.length) this.emit("change", change, cause);
  }

  /** Ids the user is in the middle of moving, resizing or editing. */
  private heldIds(): Set<string> {
    const held = new Set<string>();
    if (this.editing) held.add(this.editing);
    if (this.drag?.kind === "move") for (const id of this.drag.origin.keys()) held.add(id);
    if (this.drag?.kind === "resize") held.add(this.drag.origin.id);
    return held;
  }

  // -- selection -------------------------------------------------------------

  select(ids: readonly string[]): void {
    const next = ids.filter((id) => this.scene.has(id));
    const same = next.length === this.selection.size && next.every((id) => this.selection.has(id));
    if (same) return;
    this.selection.clear();
    for (const id of next) this.selection.add(id);
    this.render();
    this.emit("selection", [...this.selection]);
  }

  selectAll(): void {
    this.select(this.scene.all().map((n) => n.id));
  }

  private pruneSelection(): void {
    let changed = false;
    for (const id of [...this.selection]) {
      if (!this.scene.has(id)) {
        this.selection.delete(id);
        changed = true;
      }
    }
    if (this.editing && !this.scene.has(this.editing)) this.stopEditing(false);
    if (changed) this.emit("selection", [...this.selection]);
  }

  // -- tools and camera ------------------------------------------------------

  setTool(tool: Tool, keep = false): void {
    this.keepTool = keep;
    if (this.tool === tool) return;
    this.tool = tool;
    this.cursor();
    this.emit("tool", tool);
  }

  /** What moves and resizes come to rest on. */
  get snapping(): Snapping {
    return { ...this.snap };
  }

  setSnapping(next: Partial<Snapping>): void {
    this.snap = { ...this.snap, ...next };
  }

  setCamera(camera: Camera): void {
    if (camera.x === this.camera.x && camera.y === this.camera.y && camera.z === this.camera.z) return;
    this.camera = camera;
    this.positionTextarea();
    this.render();
    this.emit("camera", camera);
  }

  zoomTo(z: number): void {
    this.setCamera(zoomTo(this.camera, z, this.viewport.w, this.viewport.h));
  }

  zoomBy(factor: number, at?: Point): void {
    this.setCamera(zoomAt(this.camera, at ?? { x: this.viewport.w / 2, y: this.viewport.h / 2 }, factor));
  }

  fit(): void {
    this.setCamera(fitCamera(this.scene.extent(), this.viewport.w, this.viewport.h));
  }

  /** Board coordinates of a screen point relative to the canvas. */
  toBoard(p: Point): Point {
    return toBoard(this.camera, p);
  }

  // -- editing operations ---------------------------------------------------

  /** Create a node of `kind` at a board point with the default size. */
  create(kind: Kind, at: Point, size?: { w: number; h: number }): Node {
    const d = DEFAULTS[kind];
    const w = size?.w ?? d.w;
    const h = size?.h ?? d.h;
    const node: Node = {
      id: newId(kind === "frame" ? "f" : kind === "arrow" ? "a" : kind === "text" ? "t" : "s"),
      kind,
      x: at.x,
      y: at.y,
      w,
      h,
      fill: d.fill,
      text: "",
      frame: kind === "frame" ? null : this.frameAt(at),
      z: this.nextZ(kind),
      locked: false,
    };
    if (kind === "text") node.size = 16;
    if (kind === "frame") node.name = `Frame ${this.scene.all().filter((n) => n.kind === "frame").length + 1}`;
    if (kind === "ink") node.points = [];
    this.commit({ set: [node], deleted: [] }, "create");
    return node;
  }

  deleteSelection(): void {
    const ids = [...this.selection].filter((id) => !this.scene.get(id)?.locked);
    if (!ids.length) return;
    this.commit({ set: [], deleted: ids }, "delete");
  }

  setFill(fill: Fill): void {
    const set = [...this.selection]
      .map((id) => this.scene.get(id))
      .filter((n): n is Node => !!n && !n.locked && n.kind !== "frame" && n.kind !== "text")
      .map((n) => ({ ...cloneNode(n), fill }));
    if (set.length) this.commit({ set, deleted: [] }, "style");
  }

  setLocked(locked: boolean): void {
    const set = [...this.selection]
      .map((id) => this.scene.get(id))
      .filter((n): n is Node => !!n)
      .map((n) => ({ ...cloneNode(n), locked }));
    if (set.length) this.commit({ set, deleted: [] }, "style");
  }

  order(where: "front" | "back" | "forward" | "backward"): void {
    const all = this.scene.all().filter((n) => n.kind !== "frame");
    const ids = new Set(this.selection);
    if (!ids.size) return;
    const top = (all[all.length - 1]?.z ?? 0) + 1;
    const bottom = (all[0]?.z ?? 0) - 1;
    const set: Node[] = [];
    for (const n of all) {
      if (!ids.has(n.id) || n.locked) continue;
      const c = cloneNode(n);
      c.z = where === "front" ? top : where === "back" ? bottom : where === "forward" ? n.z + 1.5 : n.z - 1.5;
      set.push(c);
    }
    if (set.length) this.commit({ set, deleted: [] }, "order");
  }

  duplicate(): void {
    const set: Node[] = [];
    for (const id of this.selection) {
      const n = this.scene.get(id);
      if (!n || n.kind === "arrow") continue;
      const c = cloneNode(n);
      c.id = newId(n.kind === "frame" ? "f" : n.kind === "text" ? "t" : "s");
      c.x += 24;
      c.y += 24;
      c.z = this.nextZ(n.kind);
      if (c.points) c.points = c.points.map(([x, y]) => [x + 24, y + 24]);
      set.push(c);
    }
    if (!set.length) return;
    this.commit({ set, deleted: [] }, "duplicate");
    this.select(set.map((n) => n.id));
  }

  nudge(dx: number, dy: number): void {
    const set = [...this.selection]
      .map((id) => this.scene.get(id))
      .filter((n): n is Node => !!n && !n.locked)
      .map((n) => moved(n, dx, dy));
    if (set.length) this.commit({ set, deleted: [] }, "move");
  }

  /** Change a node's text; the text editor calls this when it closes. */
  setText(id: string, text: string): void {
    const n = this.scene.get(id);
    if (!n || n.text === text) return;
    this.commit({ set: [{ ...cloneNode(n), text }], deleted: [] }, "text");
  }

  private nextZ(kind: Kind): number {
    if (kind === "frame") return -1;
    let top = 0;
    for (const n of this.scene.all()) if (n.kind !== "frame" && n.z > top) top = n.z;
    return top + 1;
  }

  /** The topmost frame containing a board point, if any. */
  private frameAt(p: Point): string | null {
    const frames = this.scene.query({ x: p.x, y: p.y, w: 1, h: 1 }).filter((n) => n.kind === "frame");
    const hit = frames[frames.length - 1];
    return hit && containsPoint({ x: hit.x, y: hit.y, w: hit.w, h: hit.h }, p) ? hit.id : null;
  }

  // -- text editing ----------------------------------------------------------

  startEditing(id: string): void {
    const n = this.scene.get(id);
    if (!n || n.kind === "arrow" || n.kind === "ink" || n.kind === "frame" || n.locked) return;
    if (this.editing === id) return;
    this.stopEditing(true);
    this.editing = id;
    const ta = document.createElement("textarea");
    ta.value = n.text;
    ta.setAttribute("aria-label", "Shape text");
    ta.style.cssText =
      "position:absolute;margin:0;border:0;padding:0;resize:none;overflow:hidden;background:transparent;color:inherit;outline:none;font-family:system-ui,sans-serif;line-height:1.3;box-sizing:border-box;z-index:2";
    ta.addEventListener("keydown", (e) => {
      e.stopPropagation();
      if (e.key === "Escape" || (e.key === "Enter" && (e.ctrlKey || e.metaKey))) {
        e.preventDefault();
        this.stopEditing(true);
        this.host.focus();
      }
    });
    ta.addEventListener("blur", () => this.stopEditing(true));
    ta.addEventListener("input", () => {
      // Grow a label with its text; the height follows the wrap.
      const live = this.scene.get(id);
      if (live && live.kind === "text") {
        this.scene.set({ ...cloneNode(live), text: ta.value });
        this.render();
        this.positionTextarea();
      }
    });
    this.host.appendChild(ta);
    this.textarea = ta;
    this.positionTextarea();
    ta.focus();
    ta.select();
    this.render();
    this.emit("editing", id);
  }

  stopEditing(commit: boolean): void {
    const id = this.editing;
    const ta = this.textarea;
    if (!id || !ta) return;
    this.editing = null;
    this.textarea = null;
    const text = ta.value;
    ta.remove();
    const n = this.scene.get(id);
    if (n && commit) {
      if (n.kind === "text" && text.trim() === "") {
        this.commit({ set: [], deleted: [id] }, "delete");
      } else {
        // A live label already holds the text in the scene; the event is what is missing.
        const before = n.kind === "text" ? { ...cloneNode(n), text: "" } : n;
        if (before.text !== text || n.kind === "text") this.commit({ set: [{ ...cloneNode(n), text }], deleted: [] }, "text");
      }
    }
    this.render();
    this.emit("editing", null);
  }

  private positionTextarea(): void {
    const ta = this.textarea;
    const id = this.editing;
    if (!ta || !id) return;
    const n = this.scene.get(id);
    if (!n) return;
    const z = this.camera.z;
    const p = toScreen(this.camera, { x: n.x, y: n.y });
    const size = (n.kind === "text" ? (n.size ?? 16) : 13) * z;
    const pad = 8 * z;
    const top = n.kind === "text" ? pad : 30 * z;
    ta.style.left = `${p.x + pad}px`;
    ta.style.top = `${p.y + top}px`;
    ta.style.width = `${Math.max(8, n.w * z - pad * 2)}px`;
    ta.style.height = `${Math.max(size * 1.3, n.h * z - top - pad)}px`;
    ta.style.fontSize = `${size}px`;
  }

  // -- input -----------------------------------------------------------------

  private attach(): void {
    const h = this.host;
    h.addEventListener("pointerdown", this.onPointerDown);
    h.addEventListener("pointermove", this.onPointerMove);
    h.addEventListener("pointerup", this.onPointerUp);
    h.addEventListener("pointercancel", this.onPointerUp);
    h.addEventListener("dblclick", this.onDoubleClick);
    h.addEventListener("wheel", this.onWheel, { passive: false });
    h.addEventListener("keydown", this.onKeyDown);
    h.addEventListener("keyup", this.onKeyUp);
    h.addEventListener("contextmenu", (e) => e.preventDefault());
    if (typeof ResizeObserver !== "undefined") {
      this.observer = new ResizeObserver(() => this.resize());
      this.observer.observe(h);
    }
  }

  destroy(): void {
    this.disposed = true;
    this.stopEditing(false);
    this.observer?.disconnect();
    const h = this.host;
    h.removeEventListener("pointerdown", this.onPointerDown);
    h.removeEventListener("pointermove", this.onPointerMove);
    h.removeEventListener("pointerup", this.onPointerUp);
    h.removeEventListener("pointercancel", this.onPointerUp);
    h.removeEventListener("dblclick", this.onDoubleClick);
    h.removeEventListener("wheel", this.onWheel);
    h.removeEventListener("keydown", this.onKeyDown);
    h.removeEventListener("keyup", this.onKeyUp);
    if (this.frame !== null) cancelAnimationFrame(this.frame);
  }

  private local(e: MouseEvent): Point {
    const r = this.canvas.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  }

  private onPointerDown = (e: PointerEvent): void => {
    if (e.button !== 0 && e.button !== 1) return;
    if (this.textarea && e.target === this.textarea) return;
    this.host.focus();
    if (this.editing) this.stopEditing(true);
    try {
      this.host.setPointerCapture(e.pointerId);
    } catch {
      // A synthetic event has no pointer to capture; the gesture still works.
    }
    const s = this.local(e);
    const p = toBoard(this.camera, s);
    if (e.button === 1 || this.tool === "hand" || this.space) {
      this.drag = { kind: "pan", last: s };
      this.cursor("grabbing");
      return;
    }
    const tolerance = HIT_PX / this.camera.z;
    switch (this.tool) {
      case "select": {
        if (this.selection.size === 1) {
          const [only] = [...this.selection];
          const n = this.scene.get(only);
          const b = this.scene.bounds(only);
          if (n && b && !n.locked && n.kind !== "arrow" && n.kind !== "ink") {
            const handle = hitHandle(b, p, HANDLE_PX / this.camera.z);
            if (handle) {
              this.drag = { kind: "resize", handle, start: p, origin: cloneNode(n), box: b, targets: null };
              return;
            }
          }
        }
        const hit = hitTest(this.scene, p, tolerance);
        if (hit) {
          if (e.shiftKey) {
            const next = new Set(this.selection);
            if (next.has(hit.id)) next.delete(hit.id);
            else next.add(hit.id);
            this.select([...next]);
          } else if (!this.selection.has(hit.id)) {
            this.select([hit.id]);
          }
          const origin = new Map<string, Node>();
          for (const id of this.selection) {
            const n = this.scene.get(id);
            if (n && !n.locked) {
              origin.set(id, cloneNode(n));
              // A frame carries what is in it.
              if (n.kind === "frame") {
                for (const inner of this.scene.all()) {
                  if (inner.frame === n.id && !inner.locked && !origin.has(inner.id)) origin.set(inner.id, cloneNode(inner));
                }
              }
            }
          }
          this.drag = { kind: "move", start: p, origin, moved: false, box: null, targets: null };
        } else {
          if (!e.shiftKey) this.select([]);
          this.drag = { kind: "marquee", start: p, additive: e.shiftKey };
        }
        return;
      }
      case "sticky":
      case "rect":
      case "ellipse":
      case "frame":
        this.drag = { kind: "create", tool: this.tool, start: p };
        return;
      case "text": {
        const node = this.create("text", p);
        this.select([node.id]);
        this.finishTool();
        this.startEditing(node.id);
        return;
      }
      case "arrow": {
        const hit = hitTest(this.scene, p, tolerance);
        const from = hit && hit.kind !== "arrow" && hit.kind !== "ink" && hit.kind !== "frame" ? hit.id : null;
        this.drag = { kind: "arrow", start: p, from };
        return;
      }
      case "ink":
        this.drag = { kind: "ink", points: [p] };
        return;
    }
  };

  private onPointerMove = (e: PointerEvent): void => {
    const s = this.local(e);
    const p = toBoard(this.camera, s);
    const d = this.drag;
    if (!d) {
      this.hoverCursor(p);
      return;
    }
    switch (d.kind) {
      case "pan":
        this.setCamera(pan(this.camera, s.x - d.last.x, s.y - d.last.y));
        d.last = s;
        return;
      case "move":
      case "resize":
        this.pointer = s;
        this.follow(p, e.altKey);
        return;
      case "marquee":
        this.overlay.marquee = fromPoints(d.start, p);
        this.render();
        return;
      case "create":
        this.overlay.marquee = fromPoints(d.start, p);
        this.render();
        return;
      case "arrow":
        this.overlay.arrow = [d.start, p];
        this.render();
        return;
      case "ink": {
        const last = d.points[d.points.length - 1];
        if (distance(last, p) * this.camera.z >= 1.5) d.points.push(p);
        this.overlay.ink = d.points;
        this.render();
        return;
      }
    }
  };

  private onPointerUp = (e: PointerEvent): void => {
    const d = this.drag;
    if (!d) return;
    this.drag = null;
    this.pointer = null;
    this.overlay = {};
    const s = this.local(e);
    const p = toBoard(this.camera, s);
    try {
      this.host.releasePointerCapture(e.pointerId);
    } catch {
      // the capture may already be gone
    }
    switch (d.kind) {
      case "pan":
        this.cursor();
        return;
      case "move": {
        if (!d.moved) {
          this.render();
          return;
        }
        const { dx, dy } = this.snapped(d, p.x - d.start.x, p.y - d.start.y, e.altKey);
        const set: Node[] = [];
        for (const n of d.origin.values()) {
          const m = moved(n, dx, dy);
          // A shape dropped into a frame belongs to it; out of one, to none.
          if (m.kind !== "frame" && m.kind !== "arrow" && !d.origin.has(m.frame ?? "")) m.frame = this.frameAt(centre(m));
          set.push(m);
        }
        this.commit({ set, deleted: [] }, "move");
        return;
      }
      case "resize": {
        const { dx, dy } = this.snapped(d, p.x - d.start.x, p.y - d.start.y, e.altKey);
        this.commit({ set: [resized(d.origin, d.handle, dx, dy)], deleted: [] }, "resize");
        return;
      }
      case "marquee": {
        const b = fromPoints(d.start, p);
        if (b.w * this.camera.z < 3 && b.h * this.camera.z < 3) {
          this.render();
          return;
        }
        const ids = nodesWithin(this.scene, b).map((n) => n.id);
        this.select(d.additive ? [...new Set([...this.selection, ...ids])] : ids);
        return;
      }
      case "create": {
        const b = fromPoints(d.start, p);
        const dragged = b.w * this.camera.z >= 4 || b.h * this.camera.z >= 4;
        const defaults = DEFAULTS[d.tool];
        const size = dragged ? { w: Math.max(MIN_SIZE, b.w), h: Math.max(MIN_SIZE, b.h) } : { w: defaults.w, h: defaults.h };
        const at = dragged ? { x: b.x, y: b.y } : { x: d.start.x - size.w / 2, y: d.start.y - size.h / 2 };
        const node = this.create(d.tool, at, size);
        this.select([node.id]);
        this.finishTool();
        if (d.tool === "sticky") this.startEditing(node.id);
        return;
      }
      case "arrow": {
        const hit = hitTest(this.scene, p, HIT_PX / this.camera.z);
        const to = hit && hit.kind !== "arrow" && hit.kind !== "ink" && hit.kind !== "frame" && hit.id !== d.from ? hit.id : null;
        if (!d.from && !to && distance(d.start, p) * this.camera.z < 8) {
          this.render();
          return;
        }
        const node: Node = {
          id: newId("a"),
          kind: "arrow",
          x: d.start.x,
          y: d.start.y,
          w: 0,
          h: 0,
          fill: "grey",
          text: "",
          frame: null,
          z: this.nextZ("arrow"),
          locked: false,
          from: d.from,
          to,
          start: [d.start.x, d.start.y],
          end: [p.x, p.y],
        };
        this.commit({ set: [node], deleted: [] }, "create");
        this.select([node.id]);
        this.finishTool();
        return;
      }
      case "ink": {
        if (d.points.length < 2) {
          this.render();
          return;
        }
        const pts = simplify(d.points, 0.75 / this.camera.z);
        const node: Node = {
          id: newId("s"),
          kind: "ink",
          x: 0,
          y: 0,
          w: 0,
          h: 0,
          fill: "blue",
          text: "",
          frame: null,
          z: this.nextZ("ink"),
          locked: false,
          points: pts.map((q) => [round(q.x), round(q.y)]),
        };
        this.commit({ set: [node], deleted: [] }, "draw");
        // The pen stays a pen; drawing is many strokes.
        return;
      }
    }
  };

  private onDoubleClick = (e: MouseEvent): void => {
    const p = toBoard(this.camera, this.local(e));
    const hit = hitTest(this.scene, p, HIT_PX / this.camera.z);
    if (hit && (hit.kind === "sticky" || hit.kind === "rect" || hit.kind === "ellipse" || hit.kind === "text")) {
      this.select([hit.id]);
      this.startEditing(hit.id);
    } else if (!hit && this.tool === "select") {
      const node = this.create("sticky", { x: p.x - DEFAULTS.sticky.w / 2, y: p.y - DEFAULTS.sticky.h / 2 });
      this.select([node.id]);
      this.startEditing(node.id);
    }
  };

  private onWheel = (e: WheelEvent): void => {
    e.preventDefault();
    const s = this.local(e);
    if (e.ctrlKey || e.metaKey) {
      const factor = Math.exp(-e.deltaY * (e.deltaMode === 1 ? 0.05 : 0.0015));
      this.zoomBy(factor, s);
    } else {
      const dx = e.shiftKey && e.deltaX === 0 ? e.deltaY : e.deltaX;
      const dy = e.shiftKey && e.deltaX === 0 ? 0 : e.deltaY;
      this.setCamera(pan(this.camera, -dx, -dy));
    }
  };

  private onKeyDown = (e: KeyboardEvent): void => {
    if (e.key === "Alt" && this.pointer) {
      // Alt held skips snapping, at once rather than at the next move.
      e.preventDefault();
      this.follow(toBoard(this.camera, this.pointer), true);
      return;
    }
    if (this.editing) return;
    const meta = e.ctrlKey || e.metaKey;
    if (e.key === " " && !this.space) {
      this.space = true;
      this.cursor("grab");
      e.preventDefault();
      return;
    }
    if (meta && (e.key === "z" || e.key === "Z")) {
      e.preventDefault();
      if (e.shiftKey) this.emit("redo");
      else this.emit("undo");
      return;
    }
    if (meta && e.key === "y") {
      e.preventDefault();
      this.emit("redo");
      return;
    }
    if (meta && e.key === "a") {
      e.preventDefault();
      this.selectAll();
      return;
    }
    if (meta && e.key === "d") {
      e.preventDefault();
      this.duplicate();
      return;
    }
    if (meta && (e.key === "0" || e.key === "1")) {
      e.preventDefault();
      if (e.key === "0") this.fit();
      else this.zoomTo(1);
      return;
    }
    if (meta && (e.key === "=" || e.key === "+" || e.key === "-")) {
      e.preventDefault();
      this.zoomBy(e.key === "-" ? 1 / 1.2 : 1.2);
      return;
    }
    if (meta) return;
    switch (e.key) {
      case "Delete":
      case "Backspace":
        this.deleteSelection();
        e.preventDefault();
        return;
      case "Escape":
        if (this.drag) {
          this.drag = null;
          this.pointer = null;
          this.overlay = {};
          this.render();
        } else if (this.selection.size) this.select([]);
        else this.setTool("select");
        return;
      case "Enter":
        if (this.selection.size === 1) this.startEditing([...this.selection][0]);
        return;
      case "ArrowLeft":
      case "ArrowRight":
      case "ArrowUp":
      case "ArrowDown": {
        const step = e.shiftKey ? 10 : 1;
        const dx = e.key === "ArrowLeft" ? -step : e.key === "ArrowRight" ? step : 0;
        const dy = e.key === "ArrowUp" ? -step : e.key === "ArrowDown" ? step : 0;
        this.nudge(dx, dy);
        e.preventDefault();
        return;
      }
      case "v":
        this.setTool("select");
        return;
      case "h":
        this.setTool("hand");
        return;
      case "n":
        this.setTool("sticky");
        return;
      case "r":
        this.setTool("rect");
        return;
      case "o":
        this.setTool("ellipse");
        return;
      case "t":
        this.setTool("text");
        return;
      case "a":
        this.setTool("arrow");
        return;
      case "p":
        this.setTool("ink", true);
        return;
      case "f":
        this.setTool("frame");
        return;
      case "[":
        this.order(e.shiftKey ? "back" : "backward");
        return;
      case "]":
        this.order(e.shiftKey ? "front" : "forward");
        return;
    }
  };

  private onKeyUp = (e: KeyboardEvent): void => {
    if (e.key === "Alt" && this.pointer) {
      e.preventDefault();
      this.follow(toBoard(this.camera, this.pointer), false);
      return;
    }
    if (e.key === " ") {
      this.space = false;
      this.cursor();
    }
  };

  /**
   * A move or a resize following the pointer to board point `p`: where the
   * shapes come to rest, snapped unless `bypass`, with the guides for it.
   */
  private follow(p: Point, bypass: boolean): void {
    const d = this.drag;
    if (d?.kind === "move") {
      const dx = p.x - d.start.x;
      const dy = p.y - d.start.y;
      if (!d.moved && Math.hypot(dx, dy) * this.camera.z < 3) return;
      if (!d.moved) {
        d.moved = true;
        const boxes = [...d.origin.keys()].map((id) => this.scene.bounds(id)).filter((b): b is Box => b !== undefined);
        d.box = boxes.length ? unionAll(boxes) : null;
        d.targets = this.snapTargetsExcept(d.origin);
      }
      const s = this.snapped(d, dx, dy, bypass);
      for (const n of d.origin.values()) this.scene.set(moved(n, s.dx, s.dy));
      this.overlay.guides = s.guides;
      this.render();
    } else if (d?.kind === "resize") {
      const s = this.snapped(d, p.x - d.start.x, p.y - d.start.y, bypass);
      this.scene.set(resized(d.origin, d.handle, s.dx, s.dy));
      this.overlay.guides = s.guides;
      this.render();
    }
  }

  /** Where a move or a resize comes to rest: snapped, or exactly at the pointer when snapping is skipped. */
  private snapped(d: Extract<Drag, { kind: "move" | "resize" }>, dx: number, dy: number, bypass: boolean): Snapped {
    if (bypass || (!this.snap.shapes && !this.snap.grid)) return { dx, dy, guides: [] };
    const options = { reach: snapReach(this.camera.z), shapes: this.snap.shapes, grid: this.snap.grid ? GRID : 0 };
    if (d.kind === "resize") {
      d.targets ??= this.snapTargetsExcept(new Set([d.origin.id]));
      return snapResize(d.box, d.handle, dx, dy, d.targets, options);
    }
    return d.box ? snapMove(d.box, dx, dy, d.targets, options) : { dx, dy, guides: [] };
  }

  /** What a gesture can rest on: the shapes and frames on screen, less what it moves. */
  private snapTargetsExcept(moving: { has(id: string): boolean }): SnapTargets | null {
    if (!this.snap.shapes) return null;
    const boxes: Box[] = [];
    for (const n of this.scene.query(visible(this.camera, this.viewport.w, this.viewport.h))) {
      if (moving.has(n.id) || n.kind === "arrow" || n.kind === "ink") continue;
      const b = this.scene.bounds(n.id);
      if (b) boxes.push(b);
    }
    return snapTargets(boxes);
  }

  /** After one shape, back to the arrow; a tool set with `keep` stays. */
  private finishTool(): void {
    if (!this.keepTool) this.setTool("select");
  }

  private hoverCursor(p: Point): void {
    if (this.tool !== "select") return this.cursor();
    if (this.selection.size === 1) {
      const [only] = [...this.selection];
      const b = this.scene.bounds(only);
      const n = this.scene.get(only);
      if (b && n && !n.locked && n.kind !== "arrow" && n.kind !== "ink") {
        const h = hitHandle(b, p, HANDLE_PX / this.camera.z);
        if (h) return this.cursor(`${h}-resize`);
      }
    }
    const hit = hitTest(this.scene, p, HIT_PX / this.camera.z);
    this.cursor(hit ? "move" : "default");
  }

  private cursor(explicit?: string): void {
    const c =
      explicit ??
      (this.space || this.tool === "hand" ? "grab" : this.tool === "select" ? "default" : this.tool === "ink" ? "crosshair" : "crosshair");
    if (this.host.style.cursor !== c) this.host.style.cursor = c;
  }

  // -- drawing ---------------------------------------------------------------

  resize(): void {
    const r = this.host.getBoundingClientRect();
    const dpr = typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1;
    const w = Math.max(1, Math.round(r.width));
    const h = Math.max(1, Math.round(r.height));
    if (w === this.viewport.w && h === this.viewport.h && dpr === this.viewport.dpr) return;
    this.viewport = { w, h, dpr };
    this.canvas.width = Math.round(w * dpr);
    this.canvas.height = Math.round(h * dpr);
    this.canvas.style.width = `${w}px`;
    this.canvas.style.height = `${h}px`;
    this.render();
  }

  /** Draw on the next animation frame; many calls, one frame. */
  render(): void {
    if (this.frame !== null || this.disposed) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = null;
      this.paint();
    });
  }

  /** Draw now. Returns whether anything was drawn. */
  paint(): boolean {
    return this.renderer.draw(this.scene, this.camera, this.viewport, this.selection, {
      ...this.overlay,
      editing: this.editing,
    });
  }

  /** How many shapes the last frame drew. */
  get drawnLastFrame(): number {
    return this.renderer.lastDrawn;
  }

  get size(): { w: number; h: number; dpr: number } {
    return this.viewport;
  }
}

function moved(n: Node, dx: number, dy: number): Node {
  const c = cloneNode(n);
  c.x = round(n.x + dx);
  c.y = round(n.y + dy);
  if (c.points) c.points = c.points.map(([x, y]) => [round(x + dx), round(y + dy)]);
  if (c.start) c.start = [round(c.start[0] + dx), round(c.start[1] + dy)];
  if (c.end) c.end = [round(c.end[0] + dx), round(c.end[1] + dy)];
  return c;
}

function resized(origin: Node, handle: Handle, dx: number, dy: number): Node {
  const c = cloneNode(origin);
  let { x, y, w, h } = origin;
  if (handle.includes("e")) w = Math.max(MIN_SIZE, origin.w + dx);
  if (handle.includes("s")) h = Math.max(MIN_SIZE, origin.h + dy);
  if (handle.includes("w")) {
    w = Math.max(MIN_SIZE, origin.w - dx);
    x = origin.x + origin.w - w;
  }
  if (handle.includes("n")) {
    h = Math.max(MIN_SIZE, origin.h - dy);
    y = origin.y + origin.h - h;
  }
  c.x = round(x);
  c.y = round(y);
  c.w = round(w);
  c.h = round(h);
  return c;
}

function round(v: number): number {
  return Math.round(v * 100) / 100;
}

/** Ramer–Douglas–Peucker: the stroke with the points that do not matter removed. */
export function simplify(points: readonly Point[], epsilon: number): Point[] {
  if (points.length <= 2) return [...points];
  const keep = new Array<boolean>(points.length).fill(false);
  keep[0] = true;
  keep[points.length - 1] = true;
  const stack: [number, number][] = [[0, points.length - 1]];
  while (stack.length) {
    const [a, b] = stack.pop() as [number, number];
    let worst = 0;
    let index = -1;
    for (let i = a + 1; i < b; i++) {
      const d = perpendicular(points[i], points[a], points[b]);
      if (d > worst) {
        worst = d;
        index = i;
      }
    }
    if (index !== -1 && worst > epsilon) {
      keep[index] = true;
      stack.push([a, index], [index, b]);
    }
  }
  return points.filter((_, i) => keep[i]);
}

function perpendicular(p: Point, a: Point, b: Point): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const len = Math.hypot(dx, dy);
  if (len === 0) return distance(p, a);
  return Math.abs(dy * p.x - dx * p.y + b.x * a.y - b.y * a.x) / len;
}
