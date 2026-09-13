// The board (the app screens, 4): the canvas fills the frame; the tools
// float on its left, the zoom on its bottom right, and what applies to the
// selection above it while there is one. Undo and redo are Ctrl+Z and
// Ctrl+Shift+Z, the environment's. The replica sits between the editor
// and Core.

import { useEffect, useRef, useState, type ButtonHTMLAttributes } from "react";
import type { Harness } from "@localspace/harness-sdk";
import { Editor, FILLS, LIGHT, exportNodes, rasterize, toSvg, type Fill, type Tool } from "@localspace/canvas";
import {
  ArrowIcon,
  CursorIcon,
  EllipseIcon,
  FitIcon,
  FrameIcon,
  LayersIcon,
  LockIcon,
  MinusIcon,
  PenIcon,
  PlusIcon,
  RectIcon,
  StickyIcon,
  TextIcon,
  TrashIcon,
  UnlockIcon,
} from "@localspace/ui";
import { Replica } from "./replica.ts";

type ExportKind = "image.v1" | "svg.v1";

/** The palette, top to bottom as on the screens. Panning is the space bar, the middle button, or H. */
const PALETTE: Array<[Tool, string, typeof CursorIcon]> = [
  ["select", "Select (V)", CursorIcon],
  ["sticky", "Sticky note (N)", StickyIcon],
  ["rect", "Rectangle (R)", RectIcon],
  ["ellipse", "Ellipse (O)", EllipseIcon],
  ["arrow", "Connector (A)", ArrowIcon],
  ["text", "Text (T)", TextIcon],
  ["ink", "Pen (P)", PenIcon],
  ["frame", "Frame (F)", FrameIcon],
];

/** The board's title as a file stem: lower-case, dashes, nothing a path minds. */
function slug(title: string | undefined): string {
  return (title ?? "")
    .toLowerCase()
    .replace(/[^a-z0-9а-яёЀ-ӿ]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 40);
}

/** A floating control: its label is its accessible name and its tooltip. */
function Btn({ label, className = "pill-btn", ...rest }: { label: string } & ButtonHTMLAttributes<HTMLButtonElement>) {
  return <button type="button" className={className} aria-label={label} title={label} {...rest} />;
}

export function Board({ harness }: { harness: Harness }) {
  const host = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const [editor, setEditor] = useState<Editor | null>(null);
  const [tool, setTool] = useState<Tool>("select");
  const [selection, setSelection] = useState<string[]>([]);
  const [zoom, setZoom] = useState(1);
  /** A word about an export that did not leave the frame, shown for a while; the shell reports the ones that did. */
  const [note, setNote] = useState<string | null>(null);
  const replicaRef = useRef<Replica | null>(null);
  const exportRef = useRef<(kind: ExportKind) => void>(() => undefined);

  useEffect(() => {
    if (!host.current || !canvas.current) return;
    const ed = new Editor({ canvas: canvas.current, host: host.current });
    const snapshot = harness.snapshot();
    const replica = snapshot ? new Replica(snapshot, harness, ed) : null;
    replicaRef.current = replica;
    replica?.start();
    const off = [
      ed.on("tool", setTool),
      ed.on("selection", setSelection),
      ed.on("camera", (c) => setZoom(c.z)),
      ed.on("undo", () => harness.undo()),
      ed.on("redo", () => harness.redo()),
      harness.on("command", ({ name, args }) => {
        const a = (args ?? null) as { value?: unknown; kind?: unknown } | null;
        if (name === "zoom") {
          if (typeof a?.value === "number") ed.zoomTo(a.value);
        } else if (name === "fit") {
          ed.fit();
        } else if (name === "export") {
          exportRef.current(a?.kind === "svg.v1" ? "svg.v1" : "image.v1");
        }
      }),
      harness.on("focus", (focused) => {
        if (focused) host.current?.focus();
      }),
    ];
    let reported = 0;
    const offCamera = ed.on("camera", (c) => {
      if (Math.abs(c.z - reported) > 0.001) {
        reported = c.z;
        harness.report({ zoom: c.z });
      }
    });
    harness.report({ zoom: ed.camera.z });
    // Fit the board once the frame has a size: a frame opened in a hidden
    // tab has none yet, and a fit to nothing is the smallest zoom there is.
    let fitted = false;
    const fitOnce = () => {
      if (fitted || ed.size.w < 40 || ed.size.h < 40) return;
      fitted = true;
      if (ed.scene.size > 0) ed.fit();
    };
    fitOnce();
    const sizes = new ResizeObserver(() => {
      ed.resize();
      fitOnce();
    });
    sizes.observe(host.current);
    // The text is measured and drawn in the page's face; when it arrives
    // after the first frame, the board is drawn again in it.
    const fonts = typeof document !== "undefined" ? document.fonts : undefined;
    if (fonts) {
      void Promise.all([fonts.load('600 13.5px "Figtree"'), fonts.load('400 13.5px "Figtree"')])
        .catch(() => undefined)
        .then(() => ed.refresh());
    }
    setEditor(ed);
    host.current.focus();
    // For the end-to-end check and for anyone debugging the surface: the
    // editor, the replica and the bridge, reachable from the frame's console.
    (window as unknown as { __localspace?: unknown }).__localspace = { editor: ed, replica, harness };
    return () => {
      sizes.disconnect();
      for (const fn of off) fn();
      offCamera();
      replica?.stop();
      ed.destroy();
    };
  }, [harness]);

  useEffect(() => {
    if (!note) return;
    const timer = setTimeout(() => setNote(null), 8000);
    return () => clearTimeout(timer);
  }, [note]);

  const selected = selection.length > 0;
  const anyLocked = editor ? selection.some((id) => editor.scene.get(id)?.locked) : false;

  // Export (6.0): the selection when there is one, else the board, rendered
  // here and handed to Core through the bridge as an artifact; Core keeps
  // the file, pins it to the board's head and names it. The shell's Export
  // menu asks for it by command.
  const exportAs = async (kind: ExportKind) => {
    if (!editor) return;
    const nodes = exportNodes(editor.scene, editor.selection);
    if (nodes.length === 0) {
      setNote("Nothing to export");
      return;
    }
    const stem = slug(replicaRef.current?.title) || "board";
    const what = `${nodes.length} ${selected ? "selected " : ""}shape${nodes.length === 1 ? "" : "s"}`;
    try {
      if (kind === "svg.v1") {
        const svg = toSvg(editor.scene, nodes);
        harness.export({ kind, name: stem, mime: "image/svg+xml", bytes: new TextEncoder().encode(svg), summary: `SVG of ${what}` });
      } else {
        const { blob, width, height } = await rasterize(editor.scene, nodes);
        harness.export({ kind, name: stem, mime: "image/png", bytes: await blob.arrayBuffer(), summary: `PNG of ${what}, ${width}×${height} px` });
      }
    } catch (err) {
      setNote(`Export failed: ${err instanceof Error ? err.message : String(err)}`);
    }
  };
  // The command handler was made once, in the mount effect; it reaches the
  // current editor and selection through this ref, kept current after every
  // render.
  useEffect(() => {
    exportRef.current = (kind) => void exportAs(kind);
  });

  return (
    <div className="board">
      <div ref={host} className="board-host" tabIndex={0} aria-label="The board">
        <canvas ref={canvas} />
      </div>

      <div className="palette" role="toolbar" aria-label="Board tools" aria-orientation="vertical">
        {PALETTE.map(([t, label, Icon]) => (
          <Btn key={t} label={label} className={`palette-btn${tool === t ? " on" : ""}`} aria-pressed={tool === t} onClick={() => editor?.setTool(t, t === "ink")}>
            <Icon size={18} />
          </Btn>
        ))}
      </div>

      {selected && (
        <div className="selbar" role="toolbar" aria-label="The selection">
          {FILLS.map((f: Fill) => (
            <Btn key={f} label={`Colour: ${f}`} className="swatch" style={{ background: LIGHT.palette[f].fill }} onClick={() => editor?.setFill(f)} />
          ))}
          <span className="selbar-sep" />
          <Btn label={anyLocked ? "Unlock" : "Lock"} className="selbar-btn" onClick={() => editor?.setLocked(!anyLocked)}>
            {anyLocked ? <UnlockIcon size={16} /> : <LockIcon size={16} />}
          </Btn>
          <Btn label="Bring to front (])" className="selbar-btn" onClick={() => editor?.order("front")}>
            <LayersIcon size={16} />
          </Btn>
          <Btn label="Delete (Del)" className="selbar-btn" disabled={anyLocked} onClick={() => editor?.deleteSelection()}>
            <TrashIcon size={16} />
          </Btn>
        </div>
      )}

      <div className="zoom" role="toolbar" aria-label="Zoom">
        <Btn label="Zoom out" onClick={() => editor?.zoomBy(1 / 1.2)}>
          <MinusIcon size={16} />
        </Btn>
        <Btn label="Back to 100%" className="zoom-level ls-tabular" onClick={() => editor?.zoomTo(1)}>
          {Math.round(zoom * 100)}%
        </Btn>
        <Btn label="Zoom in" onClick={() => editor?.zoomBy(1.2)}>
          <PlusIcon size={16} />
        </Btn>
        <span className="selbar-sep" />
        <Btn label="Fit the board (Ctrl+0)" onClick={() => editor?.fit()}>
          <FitIcon size={16} />
        </Btn>
      </div>

      {note && (
        <div className="board-note" role="status">
          {note}
        </div>
      )}
    </div>
  );
}
