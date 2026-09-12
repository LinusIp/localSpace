// The board: the canvas engine's editor in a host, a toolbar of the
// environment's own controls above it, and the replica between the editor
// and Core.

import { useEffect, useRef, useState } from "react";
import type { Harness } from "@localspace/harness-sdk";
import { Editor, FILLS, TOOLS, exportNodes, rasterize, toSvg, type Fill, type Tool } from "@localspace/canvas";
import {
  ArrowIcon,
  CursorIcon,
  DownloadIcon,
  EllipseIcon,
  FitIcon,
  FrameIcon,
  GridIcon,
  HandIcon,
  IconButton,
  LayersIcon,
  LockIcon,
  Menu,
  PenIcon,
  RectIcon,
  RedoIcon,
  StickyIcon,
  TextIcon,
  UndoIcon,
  UnlockIcon,
} from "@localspace/ui";
import { Replica } from "./replica.ts";

const TOOL_LABELS: Record<Tool, { label: string; key: string; icon: typeof CursorIcon }> = {
  select: { label: "Select", key: "V", icon: CursorIcon },
  hand: { label: "Pan", key: "H", icon: HandIcon },
  sticky: { label: "Sticky note", key: "N", icon: StickyIcon },
  rect: { label: "Rectangle", key: "R", icon: RectIcon },
  ellipse: { label: "Ellipse", key: "O", icon: EllipseIcon },
  text: { label: "Text", key: "T", icon: TextIcon },
  arrow: { label: "Arrow", key: "A", icon: ArrowIcon },
  ink: { label: "Pen", key: "P", icon: PenIcon },
  frame: { label: "Frame", key: "F", icon: FrameIcon },
};

/** The board's title as a file stem: lower-case, dashes, nothing a path minds. */
function slug(title: string | undefined): string {
  return (title ?? "")
    .toLowerCase()
    .replace(/[^a-z0-9а-яёЀ-ӿ]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 40);
}

function sizeLabel(bytes: number): string {
  return bytes < 1024 * 1024 ? `${Math.max(1, Math.round(bytes / 1024))} KB` : `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

const SWATCH: Record<Fill, string> = {
  red: "#c73e3e",
  amber: "#b7791f",
  green: "#1f9d5b",
  blue: "#2f6fcb",
  yellow: "#a88a17",
  grey: "#5f6763",
  none: "transparent",
};

export function Board({ harness }: { harness: Harness }) {
  const host = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const [editor, setEditor] = useState<Editor | null>(null);
  const [tool, setTool] = useState<Tool>("select");
  const [selection, setSelection] = useState<string[]>([]);
  const [zoom, setZoom] = useState(1);
  const [gridSnap, setGridSnap] = useState(false);
  /** What the last export came to, shown in the status for a while. */
  const [note, setNote] = useState<string | null>(null);
  const replicaRef = useRef<Replica | null>(null);

  useEffect(() => {
    if (!host.current || !canvas.current) return;
    const ed = new Editor({ canvas: canvas.current, host: host.current });
    const snapshot = harness.snapshot();
    const replica = snapshot ? new Replica(snapshot, harness, ed) : null;
    replicaRef.current = replica;
    replica?.start();
    const off = [
      harness.on("artifact", (r) => {
        setNote(r.ok ? `Exported ${r.name ?? r.id ?? ""}${typeof r.bytes === "number" ? ` (${sizeLabel(r.bytes)})` : ""}` : `Export failed: ${r.error ?? "unknown error"}`);
      }),
      ed.on("tool", setTool),
      ed.on("selection", setSelection),
      ed.on("camera", (c) => setZoom(c.z)),
      ed.on("undo", () => harness.undo()),
      ed.on("redo", () => harness.redo()),
      harness.on("command", ({ name, args }) => {
        if (name === "zoom") {
          const value = (args as { value?: unknown } | null)?.value;
          if (typeof value === "number") ed.zoomTo(value);
        } else if (name === "fit") ed.fit();
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
    if (ed.scene.size > 0) ed.fit();
    setEditor(ed);
    host.current.focus();
    // For the end-to-end check and for anyone debugging the surface: the
    // editor, the replica and the bridge, reachable from the frame's console.
    (window as unknown as { __localspace?: unknown }).__localspace = { editor: ed, replica, harness };
    return () => {
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
  // the file, pins it to the board's head and names it.
  const exportAs = async (kind: "image.v1" | "svg.v1") => {
    if (!editor) return;
    const nodes = exportNodes(editor.scene, editor.selection);
    if (nodes.length === 0) {
      setNote("Nothing to export");
      return;
    }
    const stem = slug(replicaRef.current?.title) || "board";
    const what = `${nodes.length} ${selected ? "selected " : ""}shape${nodes.length === 1 ? "" : "s"}`;
    setNote("Exporting…");
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

  return (
    <div className="board">
      <div className="board-toolbar" role="toolbar" aria-label="Board tools">
        {TOOLS.map((t) => {
          const { label, key, icon: Icon } = TOOL_LABELS[t];
          return (
            <IconButton key={t} label={`${label} (${key})`} quiet on={tool === t} onClick={() => editor?.setTool(t, t === "ink")}>
              <Icon size={16} />
            </IconButton>
          );
        })}
        <span className="board-sep" />
        {FILLS.map((f) => (
          <button
            key={f}
            type="button"
            className="board-swatch"
            style={{ background: SWATCH[f] }}
            title={`Fill: ${f}`}
            aria-label={`Fill ${f}`}
            disabled={!selected}
            onClick={() => editor?.setFill(f)}
          />
        ))}
        <span className="board-sep" />
        <IconButton label={anyLocked ? "Unlock" : "Lock"} quiet disabled={!selected} onClick={() => editor?.setLocked(!anyLocked)}>
          {anyLocked ? <UnlockIcon size={16} /> : <LockIcon size={16} />}
        </IconButton>
        <IconButton label="Bring to front (])" quiet disabled={!selected} onClick={() => editor?.order("front")}>
          <LayersIcon size={16} />
        </IconButton>
        <span className="board-sep" />
        <IconButton label="Undo (Ctrl+Z)" quiet onClick={() => harness.undo()}>
          <UndoIcon size={16} />
        </IconButton>
        <IconButton label="Redo (Ctrl+Shift+Z)" quiet onClick={() => harness.redo()}>
          <RedoIcon size={16} />
        </IconButton>
        <span className="board-sep" />
        <IconButton label="Fit the board (Ctrl+0)" quiet onClick={() => editor?.fit()}>
          <FitIcon size={16} />
        </IconButton>
        <IconButton
          label="Snap to the grid (Alt held skips snapping)"
          quiet
          on={gridSnap}
          onClick={() => {
            editor?.setSnapping({ grid: !gridSnap });
            setGridSnap(!gridSnap);
          }}
        >
          <GridIcon size={16} />
        </IconButton>
        <span className="board-sep" />
        <Menu
          align="left"
          trigger={(open, isOpen) => (
            <IconButton label="Export" quiet on={isOpen} disabled={!editor || editor.scene.size === 0} onClick={open}>
              <DownloadIcon size={16} />
            </IconButton>
          )}
          items={[
            { id: "png", label: selected ? "PNG of the selection" : "PNG of the board", onSelect: () => void exportAs("image.v1") },
            { id: "svg", label: selected ? "SVG of the selection" : "SVG of the board", onSelect: () => void exportAs("svg.v1") },
          ]}
        />
        <span className="board-zoom ls-tabular ls-small ls-muted">{Math.round(zoom * 100)}%</span>
        <span className="board-status ls-small ls-faint">
          {editor ? `${editor.scene.size} shape${editor.scene.size === 1 ? "" : "s"}` : ""}
          {selected ? ` · ${selection.length} selected` : ""}
          {note ? ` · ${note}` : ""}
        </span>
      </div>
      <div ref={host} className="board-host" tabIndex={0} aria-label="The board">
        <canvas ref={canvas} />
      </div>
    </div>
  );
}
