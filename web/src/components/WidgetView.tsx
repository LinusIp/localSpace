// The `widgets` view kind, rendered by the shell (plugin spec §5.1). A
// harness's logic describes a tree of plain controls; every interaction is a
// `widget_event` to Core, which runs the logic and answers with the tree
// again. Nothing here is the harness's code.

import { useEffect, useState } from "react";
import { ApiError, call, pick } from "../api/client";
import type { Widget, WidgetValue } from "../api/generated";
import { bus } from "../surfaces/bus";
import { useSession } from "../store";
import { Button, Pill } from "./ui";

export function WidgetView({ harness, view }: { harness: string; view: string }) {
  const [root, setRoot] = useState<Widget | null>(null);
  const [error, setError] = useState<string | null>(null);
  const notify = useSession((s) => s.notify);

  useEffect(() => {
    let cancelled = false;
    call({ get_widget_view: { harness, view } })
      .then((r) => {
        const v = pick(r, "widget_view");
        if (!cancelled && v) setRoot(v.root);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(err instanceof ApiError ? err.message : "the server could not be reached");
      });
    const off = bus.on("widget_view_changed", (m) => {
      if (m.harness === harness && m.view === view) setRoot(m.root);
    });
    return () => {
      cancelled = true;
      off();
    };
  }, [harness, view]);

  const fire = async (id: string, value: WidgetValue) => {
    try {
      const r = await call({ widget_event: { harness, view, event: { id, value } } });
      const v = pick(r, "widget_view");
      if (v) setRoot(v.root);
    } catch (err) {
      notify("error", err instanceof ApiError ? err.message : "the server could not be reached");
    }
  };

  if (error) return <p className="p-6 text-sm text-danger">{error}</p>;
  if (!root) return <p className="p-6 text-sm text-muted">Loading…</p>;
  return (
    <div className="h-full overflow-auto p-5">
      <Node widget={root} fire={fire} />
    </div>
  );
}

function Node({ widget, fire }: { widget: Widget; fire: (id: string, value: WidgetValue) => void }) {
  if (widget === "separator") return <hr className="my-2 border-line" />;
  if ("column" in widget)
    return (
      <div className="flex flex-col gap-2">
        {widget.column.children.map((c, i) => (
          <Node key={i} widget={c} fire={fire} />
        ))}
      </div>
    );
  if ("row" in widget)
    return (
      <div className="flex flex-wrap items-center gap-2">
        {widget.row.children.map((c, i) => (
          <Node key={i} widget={c} fire={fire} />
        ))}
      </div>
    );
  if ("text" in widget)
    return (
      <p className={`text-sm ${widget.text.strong ? "font-semibold" : ""} ${widget.text.muted ? "text-muted" : ""}`}>
        {widget.text.text}
      </p>
    );
  if ("heading" in widget) return <h3 className="text-[15px] font-semibold">{widget.heading.text}</h3>;
  if ("space" in widget) return <div style={{ height: widget.space.size }} />;
  if ("button" in widget)
    return (
      <Button disabled={!widget.button.enabled} onClick={() => fire(widget.button.id, "clicked")}>
        {widget.button.label}
      </Button>
    );
  if ("input" in widget) {
    const { id, label, value, multiline } = widget.input;
    const cls = "w-full rounded-lg border border-line px-3 py-1.5 text-sm outline-none focus:border-accent";
    return (
      <label className="block text-xs text-muted">
        {label}
        {multiline ? (
          <textarea
            className={`mt-1 ${cls}`}
            rows={3}
            defaultValue={value}
            onBlur={(e) => e.target.value !== value && fire(id, { text: e.target.value })}
          />
        ) : (
          <input
            className={`mt-1 ${cls}`}
            defaultValue={value}
            onBlur={(e) => e.target.value !== value && fire(id, { text: e.target.value })}
            onKeyDown={(e) => e.key === "Enter" && fire(id, { text: (e.target as HTMLInputElement).value })}
          />
        )}
      </label>
    );
  }
  if ("checkbox" in widget)
    return (
      <label className="flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          checked={widget.checkbox.value}
          onChange={(e) => fire(widget.checkbox.id, { bool: e.target.checked })}
        />
        {widget.checkbox.label}
      </label>
    );
  if ("select" in widget)
    return (
      <label className="block text-xs text-muted">
        {widget.select.label}
        <select
          className="mt-1 block rounded-lg border border-line bg-white px-2 py-1.5 text-sm"
          value={widget.select.value}
          onChange={(e) => fire(widget.select.id, { text: e.target.value })}
        >
          {widget.select.options.map((o) => (
            <option key={o} value={o}>
              {o}
            </option>
          ))}
        </select>
      </label>
    );
  if ("slider" in widget) {
    const { id, label, value, min, max } = widget.slider;
    return (
      <label className="block text-xs text-muted">
        {label}: {value}
        <input
          type="range"
          className="mt-1 block w-full"
          min={min}
          max={max}
          step={(max - min) / 100}
          defaultValue={value}
          onChange={(e) => fire(id, { number: Number(e.target.value) })}
        />
      </label>
    );
  }
  if ("list" in widget)
    return (
      <ul className="list-disc pl-5 text-sm">
        {widget.list.items.map((it, i) => (
          <li key={i}>{it}</li>
        ))}
      </ul>
    );
  if ("table" in widget)
    return (
      <table className="text-sm">
        <thead className="text-left text-xs text-faint">
          <tr>
            {widget.table.headers.map((h) => (
              <th key={h} className="pb-1 pr-4 font-medium">
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody className="divide-y divide-line">
          {widget.table.rows.map((row, i) => (
            <tr key={i}>
              {row.map((cell, j) => (
                <td key={j} className="py-1 pr-4">
                  {cell}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    );
  if ("badge" in widget) {
    const tone = widget.badge.tone === "good" ? "ok" : widget.badge.tone === "warn" ? "warn" : widget.badge.tone === "bad" ? "danger" : "neutral";
    return <Pill tone={tone}>{widget.badge.text}</Pill>;
  }
  return null;
}
