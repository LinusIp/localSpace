// The board's document as an Automerge replica in the frame (architecture
// v2.1 §6.1, §6.3). Core's snapshot is loaded once; the editor's changes go
// into the replica and out to Core as sync messages, which Core commits;
// Core's own changes, the agent's and an undo's among them, arrive the same
// way and are applied to the editor as remote changes. Whole documents
// never cross the bridge after the first one.
//
// Core writes strings as Automerge's uncollaborative strings, which this
// library surfaces as `ImmutableString`; the replica reads them as plain
// strings and writes the same kind back, so both ends hold one shape.

import * as Automerge from "@automerge/automerge";
import type { Harness } from "@localspace/harness-sdk";
import { fromDoc, sameNode, type BoardDoc, type Change, type Editor, type Node } from "@localspace/canvas";

type Str = string | Automerge.ImmutableString;

interface AmShape {
  id: Str;
  kind: Str;
  x: number;
  y: number;
  w: number;
  h: number;
  fill?: Str;
  text?: Str;
  frame?: Str | null;
  z?: number;
  locked?: boolean;
  size?: number;
  from?: Str | null;
  to?: Str | null;
  start?: number[];
  end?: number[];
  points?: number[][];
}

interface AmFrame {
  id: Str;
  name?: Str;
  x: number;
  y: number;
  w: number;
  h: number;
}

interface AmBoard {
  title?: Str;
  shapes?: AmShape[];
  frames?: AmFrame[];
  selection?: Str[];
}

const text = (v: unknown): string | undefined => (Automerge.isImmutableString(v) ? v.val : typeof v === "string" ? v : undefined);
const raw = (s: string): Automerge.ImmutableString => new Automerge.ImmutableString(s);
const same = (a: unknown, b: string): boolean => text(a) === b;

/** The replica's document with plain strings, as the canvas model reads it. */
function plain(doc: AmBoard): BoardDoc {
  return {
    title: text(doc.title),
    frames: (doc.frames ?? []).map((f) => ({ id: text(f.id) ?? "", name: text(f.name), x: f.x, y: f.y, w: f.w, h: f.h })),
    shapes: (doc.shapes ?? []).map((s) => ({
      id: text(s.id) ?? "",
      kind: text(s.kind) ?? "",
      x: s.x,
      y: s.y,
      w: s.w,
      h: s.h,
      fill: text(s.fill),
      text: text(s.text),
      frame: text(s.frame) ?? null,
      z: s.z ?? 0,
      locked: s.locked === true,
      size: s.size,
      from: text(s.from) ?? null,
      to: text(s.to) ?? null,
      start: s.start && s.start.length === 2 ? [s.start[0], s.start[1]] : undefined,
      end: s.end && s.end.length === 2 ? [s.end[0], s.end[1]] : undefined,
      points: s.points?.map((p) => [p[0], p[1]] as [number, number]),
    })),
    selection: (doc.selection ?? []).map((s) => text(s) ?? ""),
  };
}

export class Replica {
  private doc: Automerge.Doc<AmBoard>;
  private state = Automerge.initSyncState();
  private shown = new Map<string, Node>();
  private stopFns: Array<() => void> = [];
  private readonly harness: Harness;
  private readonly editor: Editor;

  constructor(snapshot: Uint8Array, harness: Harness, editor: Editor) {
    this.harness = harness;
    this.editor = editor;
    this.doc = Automerge.load<AmBoard>(snapshot);
  }

  /** How many shapes and frames the replica holds, for the status line and the tests. */
  get size(): number {
    return (this.doc.shapes?.length ?? 0) + (this.doc.frames?.length ?? 0);
  }

  start(): void {
    this.show(true);
    this.stopFns.push(
      this.harness.on("sync", (message) => this.receive(message)),
      this.editor.on("change", (change) => this.local(change)),
    );
    this.flush();
  }

  stop(): void {
    for (const fn of this.stopFns) fn();
    this.stopFns = [];
  }

  /** The document as the editor's nodes; what changed becomes the editor's. */
  private show(load: boolean): void {
    const nodes = fromDoc(plain(this.doc));
    const next = new Map(nodes.map((n) => [n.id, n]));
    if (load) {
      this.editor.load(nodes);
    } else {
      const set = nodes.filter((n) => {
        const before = this.shown.get(n.id);
        return !before || !sameNode(before, n);
      });
      const deleted = [...this.shown.keys()].filter((id) => !next.has(id));
      if (set.length || deleted.length) this.editor.applyRemote({ set, deleted });
    }
    this.shown = next;
  }

  /** Send everything Core does not have yet. */
  private flush(): void {
    for (let i = 0; i < 16; i++) {
      const [state, message] = Automerge.generateSyncMessage(this.doc, this.state);
      this.state = state;
      if (!message) return;
      this.harness.sync(message);
    }
  }

  private receive(message: Uint8Array): void {
    const [doc, state] = Automerge.receiveSyncMessage(this.doc, this.state, message);
    const changed = doc !== this.doc;
    this.doc = doc;
    this.state = state;
    if (changed) this.show(false);
    this.flush();
  }

  /** The editor's change into the replica: the same shapes, by id. */
  private local(change: Change): void {
    this.doc = Automerge.change(this.doc, (d) => {
      if (!d.shapes) d.shapes = [];
      if (!d.frames) d.frames = [];
      for (const id of change.deleted) {
        removeById(d.shapes, id);
        removeById(d.frames, id);
      }
      for (const n of change.set) {
        if (n.kind === "frame") {
          const i = d.frames.findIndex((f) => same(f.id, n.id));
          if (i < 0) d.frames.push(frameOf(n));
          else writeFrame(d.frames[i], n);
        } else {
          const i = d.shapes.findIndex((s) => same(s.id, n.id));
          if (i < 0) d.shapes.push(shapeOf(n));
          else writeShape(d.shapes[i], n);
        }
      }
    });
    for (const n of change.set) this.shown.set(n.id, n);
    for (const id of change.deleted) this.shown.delete(id);
    this.flush();
  }
}

function removeById(list: Array<{ id: Str }>, id: string): void {
  const i = list.findIndex((item) => same(item.id, id));
  if (i >= 0) list.splice(i, 1);
}

function frameOf(n: Node): AmFrame {
  return { id: raw(n.id), name: raw(n.name ?? "Frame"), x: n.x, y: n.y, w: n.w, h: n.h };
}

function writeFrame(f: AmFrame, n: Node): void {
  if (!same(f.name, n.name ?? "Frame")) f.name = raw(n.name ?? "Frame");
  if (f.x !== n.x) f.x = n.x;
  if (f.y !== n.y) f.y = n.y;
  if (f.w !== n.w) f.w = n.w;
  if (f.h !== n.h) f.h = n.h;
}

function shapeOf(n: Node): AmShape {
  const s: AmShape = {
    id: raw(n.id),
    kind: raw(n.kind),
    x: n.x,
    y: n.y,
    w: n.w,
    h: n.h,
    fill: raw(n.fill),
    text: raw(n.text),
    frame: n.frame ? raw(n.frame) : null,
    z: n.z,
    locked: n.locked,
  };
  if (n.kind === "text") s.size = n.size ?? 16;
  if (n.kind === "arrow") {
    s.from = n.from ? raw(n.from) : null;
    s.to = n.to ? raw(n.to) : null;
    if (n.start) s.start = [n.start[0], n.start[1]];
    if (n.end) s.end = [n.end[0], n.end[1]];
  }
  if (n.kind === "ink") s.points = (n.points ?? []).map((p) => [p[0], p[1]]);
  return s;
}

/** Only what differs is written, so a move is a move and not a rewrite. */
function writeShape(s: AmShape, n: Node): void {
  if (!same(s.kind, n.kind)) s.kind = raw(n.kind);
  for (const key of ["x", "y", "w", "h", "z"] as const) {
    if (s[key] !== n[key]) s[key] = n[key];
  }
  if (s.locked !== n.locked) s.locked = n.locked;
  if (!same(s.fill, n.fill)) s.fill = raw(n.fill);
  if (!same(s.text, n.text)) s.text = raw(n.text);
  if ((text(s.frame) ?? null) !== n.frame) s.frame = n.frame ? raw(n.frame) : null;
  if (n.kind === "text") {
    if (s.size !== (n.size ?? 16)) s.size = n.size ?? 16;
  }
  if (n.kind === "arrow") {
    if ((text(s.from) ?? null) !== (n.from ?? null)) s.from = n.from ? raw(n.from) : null;
    if ((text(s.to) ?? null) !== (n.to ?? null)) s.to = n.to ? raw(n.to) : null;
    for (const key of ["start", "end"] as const) {
      const value = n[key];
      const before = s[key];
      if (!value) {
        if (before) delete s[key];
      } else if (!before || before[0] !== value[0] || before[1] !== value[1]) {
        s[key] = [value[0], value[1]];
      }
    }
  }
  if (n.kind === "ink") {
    const pts = n.points ?? [];
    const before = s.points ?? [];
    const equal = before.length === pts.length && before.every((p, i) => p[0] === pts[i][0] && p[1] === pts[i][1]);
    if (!equal) s.points = pts.map((p) => [p[0], p[1]]);
  }
}
