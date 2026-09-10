// The top bar: workspace, model, canvas zoom, readiness, settings, the user.

import { Dot, IconButton, MinusIcon, PlusIcon, Select, SettingsIcon, SparkIcon } from "@localspace/ui";
import { useSession } from "../store";

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
    <header className="ls-topbar">
      <Select aria-label="Workspace" value={environment?.workspace ?? ""} onChange={() => undefined} style={{ width: "auto" }}>
        <option value={environment?.workspace ?? ""}>{environment?.workspace ?? "Workspace"}</option>
      </Select>

      <div className="ls-ml-auto ls-row ls-gap-3">
        <span className="ls-row ls-gap-2">
          <SparkIcon size={15} className="ls-muted" />
          <Select
            aria-label="Model"
            value={model?.id ?? ""}
            style={{ width: "auto" }}
            onChange={(e) => {
              if (e.target.value) void selectModel(e.target.value);
              else go("models");
            }}
          >
            {!model && <option value="">No model</option>}
            {models.length === 0 && model && <option value={model.id}>{model.id}</option>}
            {models.map((m) => (
              <option key={m.id} value={m.id}>
                {m.id}
              </option>
            ))}
            {models.length === 0 && <option value="">Choose in Models…</option>}
          </Select>
        </span>

        <span
          className={`ls-row ls-card ${canvas ? "" : "ls-faint"}`}
          style={{ borderRadius: "var(--ls-radius)" }}
          title={canvas ? `Zoom ${canvas.title}` : "Zoom acts on an open canvas; no canvas panel is open"}
        >
          <IconButton label="Zoom out" quiet disabled={!canvas} onClick={() => canvas && setZoom(canvas.key, canvas.zoom - 0.1)}>
            <MinusIcon size={14} />
          </IconButton>
          <button
            type="button"
            className="ls-link-button ls-tabular"
            style={{ fontSize: 14, color: "inherit", padding: "0 4px" }}
            disabled={!canvas}
            onClick={() => canvas && setZoom(canvas.key, 1)}
            title={canvas ? "Back to 100%" : undefined}
          >
            {Math.round((canvas?.zoom ?? 1) * 100)}%
          </button>
          <IconButton label="Zoom in" quiet disabled={!canvas} onClick={() => canvas && setZoom(canvas.key, canvas.zoom + 0.1)}>
            <PlusIcon size={14} />
          </IconButton>
        </span>

        <span className={`ls-pill ${readiness.tone === "ok" ? "ls-ok" : readiness.tone === "warn" ? "ls-warn-pill" : ""}`} style={{ fontSize: 14, padding: "6px 12px" }}>
          <Dot tone={readiness.tone} />
          {readiness.label}
        </span>

        <IconButton label="Settings" onClick={() => go("settings")}>
          <SettingsIcon size={16} />
        </IconButton>
        <button type="button" className="ls-avatar" onClick={() => go("settings")} aria-label="Account" title={me?.user}>
          {initials || "?"}
        </button>
      </div>
    </header>
  );
}
