// The board: the canvas engine's editor in a host, a toolbar of the
// environment's own controls above it, and the replica between the editor
// and Core.

import { useEffect, useRef, useState } from "react";
import type { Harness } from "@localspace/harness-sdk";
import { Editor, FILLS, TOOLS, type Fill, type Tool } from "@localspace/canvas";
import {
  ArrowIcon,
  CursorIcon,
  EllipseIcon,
  FitIcon,
  FrameIcon,
  HandIcon,
  IconButton,
  LayersIcon,
  LockIcon,
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

  useEffect(() => {
    if (!host.current || !canvas.current) return;
    const ed = new Editor({ canvas: canvas.current, host: host.current });
    const snapshot = harness.snapshot();
    const replica = snapshot ? new Replica(snapshot, harness, ed) : null;
    replica?.start();
    const off = [
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

  const selected = selection.length > 0;
  const anyLocked = editor ? selection.some((id) => editor.scene.get(id)?.locked) : false;

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
        <span className="board-zoom ls-tabular ls-small ls-muted">{Math.round(zoom * 100)}%</span>
        <span className="board-status ls-small ls-faint">
          {editor ? `${editor.scene.size} shape${editor.scene.size === 1 ? "" : "s"}` : ""}
          {selected ? ` · ${selection.length} selected` : ""}
        </span>
      </div>
      <div ref={host} className="board-host" tabIndex={0} aria-label="The board">
        <canvas ref={canvas} />
      </div>
    </div>
  );
}
