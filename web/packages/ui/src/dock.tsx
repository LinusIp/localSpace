// The docking layout the shell needs today (architecture v2.1 §6.1): a
// main area with a side column either side, each side resizable by its
// gutter and collapsible, remembered per viewer in the browser.

import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { cx } from "./controls.tsx";

export interface DockProps {
  main: ReactNode;
  left?: ReactNode;
  right?: ReactNode;
  /** Initial widths in pixels; the user's drag is remembered under `id`. */
  leftWidth?: number;
  rightWidth?: number;
  minSide?: number;
  id?: string;
  className?: string;
}

function remembered(id: string | undefined, side: string, fallback: number): number {
  if (!id) return fallback;
  try {
    const v = localStorage.getItem(`ls-dock:${id}:${side}`);
    const n = v ? Number(v) : NaN;
    return Number.isFinite(n) && n > 0 ? n : fallback;
  } catch {
    return fallback;
  }
}

function remember(id: string | undefined, side: string, width: number): void {
  if (!id) return;
  try {
    localStorage.setItem(`ls-dock:${id}:${side}`, String(Math.round(width)));
  } catch {
    // storage may be unavailable; the width still applies for this page
  }
}

export function Dock({ main, left, right, leftWidth = 240, rightWidth = 380, minSide = 200, id, className }: DockProps) {
  const [widths, setWidths] = useState(() => ({ left: remembered(id, "left", leftWidth), right: remembered(id, "right", rightWidth) }));
  const [dragging, setDragging] = useState<"left" | "right" | null>(null);
  const root = useRef<HTMLDivElement>(null);

  const start = useCallback(
    (side: "left" | "right") => (e: React.PointerEvent<HTMLDivElement>) => {
      e.preventDefault();
      setDragging(side);
      const startX = e.clientX;
      const startWidth = side === "left" ? widths.left : widths.right;
      const max = (root.current?.clientWidth ?? 1200) * 0.6;
      const onMove = (ev: PointerEvent) => {
        const delta = side === "left" ? ev.clientX - startX : startX - ev.clientX;
        const width = Math.max(minSide, Math.min(max, startWidth + delta));
        setWidths((w) => ({ ...w, [side]: width }));
      };
      const onUp = (ev: PointerEvent) => {
        const delta = side === "left" ? ev.clientX - startX : startX - ev.clientX;
        remember(id, side, Math.max(minSide, Math.min(max, startWidth + delta)));
        setDragging(null);
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", onUp);
      };
      window.addEventListener("pointermove", onMove);
      window.addEventListener("pointerup", onUp);
    },
    [id, minSide, widths.left, widths.right],
  );

  useEffect(() => {
    if (!dragging) return;
    const previous = document.body.style.cursor;
    document.body.style.cursor = "col-resize";
    return () => {
      document.body.style.cursor = previous;
    };
  }, [dragging]);

  return (
    <div ref={root} className={cx("ls-dock", className)}>
      {left && (
        <>
          <div className="ls-dock-side" style={{ width: widths.left }}>
            {left}
          </div>
          <div className={cx("ls-dock-gutter", dragging === "left" && "ls-on")} onPointerDown={start("left")} role="separator" aria-orientation="vertical" aria-label="Resize the left column" />
        </>
      )}
      <div className="ls-dock-main">{main}</div>
      {right && (
        <>
          <div className={cx("ls-dock-gutter", dragging === "right" && "ls-on")} onPointerDown={start("right")} role="separator" aria-orientation="vertical" aria-label="Resize the right column" />
          <div className="ls-dock-side" style={{ width: widths.right }}>
            {right}
          </div>
        </>
      )}
    </div>
  );
}
