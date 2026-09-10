// The top bar: workspace, model, canvas zoom, readiness, settings, the user.

import { ChevronDown, Minus, Plus, Settings, Sparkles } from "lucide-react";
import { useSession } from "../store";
import { Dot } from "./ui";

export function TopBar() {
  const { environment, models, live, me, go, selectModel, panels, activePanel, setZoom } = useSession();
  const model = environment?.model ?? null;
  // Zoom acts on the active panel when it is a web view; those are the
  // surfaces that take the shell's commands.
  const canvas = panels.find((p) => p.key === activePanel && p.kind === "web") ?? null;

  const readiness = !live
    ? { tone: "neutral" as const, label: "Reconnecting" }
    : model
      ? { tone: "ok" as const, label: "Ready" }
      : { tone: "warn" as const, label: "No model" };

  const initials = (me?.user ?? "?")
    .split(/[\s._-]+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0]?.toUpperCase() ?? "")
    .join("");

  return (
    <header className="flex items-center gap-3 px-6 py-3">
      <label className="flex items-center gap-2 rounded-lg border border-line bg-white px-3 py-1.5 text-sm">
        <select
          className="appearance-none bg-white pr-1 outline-none"
          value={environment?.workspace ?? ""}
          onChange={() => undefined}
          aria-label="Workspace"
        >
          <option value={environment?.workspace ?? ""}>{environment?.workspace ?? "Workspace"}</option>
        </select>
        <ChevronDown size={14} className="text-muted" />
      </label>

      <div className="ml-auto flex items-center gap-3">
        <label className="flex items-center gap-2 rounded-lg border border-line bg-white px-3 py-1.5 text-sm">
          <Sparkles size={15} className="text-muted" />
          <select
            className="appearance-none bg-white pr-1 outline-none"
            value={model?.id ?? ""}
            onChange={(e) => {
              if (e.target.value) void selectModel(e.target.value);
              else go("models");
            }}
            aria-label="Model"
          >
            {!model && <option value="">No model</option>}
            {models.length === 0 && model && <option value={model.id}>{model.id}</option>}
            {models.map((m) => (
              <option key={m.id} value={m.id}>
                {m.id}
              </option>
            ))}
            {models.length === 0 && <option value="">Choose in Models…</option>}
          </select>
          <ChevronDown size={14} className="text-muted" />
        </label>

        <div
          className={`flex items-center rounded-lg border border-line bg-white text-sm ${canvas ? "text-ink" : "text-faint"}`}
          title={canvas ? `Zoom ${canvas.title}` : "Zoom acts on an open canvas; no canvas panel is open"}
        >
          <button
            className="px-2 py-1.5 disabled:cursor-not-allowed"
            disabled={!canvas}
            onClick={() => canvas && setZoom(canvas.key, canvas.zoom - 0.1)}
            aria-label="Zoom out"
          >
            <Minus size={14} />
          </button>
          <button
            className="px-1 tabular-nums disabled:cursor-not-allowed"
            disabled={!canvas}
            onClick={() => canvas && setZoom(canvas.key, 1)}
            title={canvas ? "Back to 100%" : undefined}
          >
            {Math.round((canvas?.zoom ?? 1) * 100)}%
          </button>
          <button
            className="px-2 py-1.5 disabled:cursor-not-allowed"
            disabled={!canvas}
            onClick={() => canvas && setZoom(canvas.key, canvas.zoom + 0.1)}
            aria-label="Zoom in"
          >
            <Plus size={14} />
          </button>
        </div>

        <span
          className={`flex items-center gap-2 rounded-lg border px-3 py-1.5 text-sm font-medium ${
            readiness.tone === "ok"
              ? "border-accent/20 bg-accent-soft text-accent"
              : readiness.tone === "warn"
                ? "border-warn/20 bg-warn-soft text-warn"
                : "border-line bg-white text-muted"
          }`}
        >
          <Dot tone={readiness.tone} />
          {readiness.label}
        </span>

        <button
          className="rounded-lg border border-line bg-white p-2 text-muted hover:text-ink"
          onClick={() => go("settings")}
          aria-label="Settings"
        >
          <Settings size={16} />
        </button>
        <button
          className="flex h-9 w-9 items-center justify-center rounded-full bg-accent-soft text-xs font-semibold text-accent"
          onClick={() => go("settings")}
          aria-label="Account"
          title={me?.user}
        >
          {initials || "?"}
        </button>
      </div>
    </header>
  );
}
