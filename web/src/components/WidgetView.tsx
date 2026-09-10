// The `widgets` view kind, rendered by the shell (plugin spec §5.1). A
// harness's logic describes a tree of plain controls; every interaction is a
// `widget_event` to Core, which runs the logic and answers with the tree
// again. Nothing here is the harness's code.

import { useEffect, useState } from "react";
import { Button, Field, Input, Pill, Select, Textarea } from "@localspace/ui";
import { ApiError, call, pick } from "../api/client";
import type { Widget, WidgetValue } from "../api/generated";
import { bus } from "../surfaces/bus";
import { useSession } from "../store";

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

  if (error) return <p className="ls-pad-6 ls-danger">{error}</p>;
  if (!root) return <p className="ls-pad-6 ls-muted">Loading…</p>;
  return (
    <div className="ls-fill ls-scroll ls-pad-4" style={{ boxSizing: "border-box" }}>
      <Node widget={root} fire={fire} />
    </div>
  );
}

function Node({ widget, fire }: { widget: Widget; fire: (id: string, value: WidgetValue) => void }) {
  if (widget === "separator") return <hr className="ls-hr" />;
  if ("column" in widget)
    return (
      <div className="ls-col ls-gap-2">
        {widget.column.children.map((c, i) => (
          <Node key={i} widget={c} fire={fire} />
        ))}
      </div>
    );
  if ("row" in widget)
    return (
      <div className="ls-row ls-wrap ls-gap-2">
        {widget.row.children.map((c, i) => (
          <Node key={i} widget={c} fire={fire} />
        ))}
      </div>
    );
  if ("text" in widget)
    return <p style={{ margin: 0 }} className={`${widget.text.strong ? "ls-strong" : ""} ${widget.text.muted ? "ls-muted" : ""}`}>{widget.text.text}</p>;
  if ("heading" in widget) return <h3 className="ls-section-title">{widget.heading.text}</h3>;
  if ("space" in widget) return <div style={{ height: widget.space.size }} />;
  if ("button" in widget)
    return (
      <Button disabled={!widget.button.enabled} onClick={() => fire(widget.button.id, "clicked")}>
        {widget.button.label}
      </Button>
    );
  if ("input" in widget) {
    const { id, label, value, multiline } = widget.input;
    return (
      <Field label={label}>
        {multiline ? (
          <Textarea rows={3} defaultValue={value} onBlur={(e) => e.target.value !== value && fire(id, { text: e.target.value })} />
        ) : (
          <Input
            defaultValue={value}
            onBlur={(e) => e.target.value !== value && fire(id, { text: e.target.value })}
            onKeyDown={(e) => e.key === "Enter" && fire(id, { text: (e.target as HTMLInputElement).value })}
          />
        )}
      </Field>
    );
  }
  if ("checkbox" in widget)
    return (
      <label className="ls-row ls-gap-2">
        <input type="checkbox" checked={widget.checkbox.value} onChange={(e) => fire(widget.checkbox.id, { bool: e.target.checked })} />
        {widget.checkbox.label}
      </label>
    );
  if ("select" in widget)
    return (
      <Field label={widget.select.label}>
        <Select value={widget.select.value} onChange={(e) => fire(widget.select.id, { text: e.target.value })} style={{ width: "auto" }}>
          {widget.select.options.map((o) => (
            <option key={o} value={o}>
              {o}
            </option>
          ))}
        </Select>
      </Field>
    );
  if ("slider" in widget) {
    const { id, label, value, min, max } = widget.slider;
    return (
      <Field label={`${label}: ${value}`}>
        <input type="range" style={{ width: "100%" }} min={min} max={max} step={(max - min) / 100} defaultValue={value} onChange={(e) => fire(id, { number: Number(e.target.value) })} />
      </Field>
    );
  }
  if ("list" in widget)
    return (
      <ul style={{ margin: 0, paddingLeft: 20 }}>
        {widget.list.items.map((it, i) => (
          <li key={i}>{it}</li>
        ))}
      </ul>
    );
  if ("table" in widget)
    return (
      <table className="ls-table">
        <thead>
          <tr>
            {widget.table.headers.map((h) => (
              <th key={h}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {widget.table.rows.map((row, i) => (
            <tr key={i}>
              {row.map((cell, j) => (
                <td key={j}>{cell}</td>
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
