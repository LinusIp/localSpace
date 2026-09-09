// The right column of the chat page: the active model, what the agent may
// use, what it will see, and what happened last. Every line is Core's own
// state; nothing here is decoration.

import { useEffect } from "react";
import { Boxes, ChevronRight, FileText, Globe, History as HistoryIcon, Network } from "lucide-react";
import { useSession } from "../store";
import { Card, Dot, SectionTitle, timeAgo } from "./ui";
import { Switch } from "./Switch";

export function RightPanel() {
  const {
    environment,
    active,
    task,
    history,
    go,
    setEnabled,
    setNetwork,
    refreshActive,
    refreshTask,
    refreshHistory,
  } = useSession();

  useEffect(() => {
    void refreshActive();
    void refreshTask();
    void refreshHistory();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const model = environment?.model ?? null;
  const online = environment ? environment.network !== "airgapped" : false;
  const mayGoOnline = environment ? environment.network_ceiling !== "airgapped" : false;
  const recent = history.slice(0, 3);

  return (
    <aside className="flex w-[380px] shrink-0 flex-col gap-4 overflow-y-auto">
      <Card className="p-4">
        <SectionTitle>Active Model</SectionTitle>
        {model ? (
          <div className="flex items-start gap-3 rounded-lg border border-line p-3">
            <span className="rounded-lg bg-page p-2 text-muted">
              <Boxes size={18} />
            </span>
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <span className="truncate font-medium">{model.id}</span>
                <span className="flex items-center gap-1 text-xs text-accent">
                  <Dot tone="ok" /> {model.loaded ? "Running" : "Selected"}
                </span>
              </div>
              <div className="text-xs text-muted">
                {model.backend} · {model.context_len.toLocaleString()} context
                {model.supports_tools ? " · tools" : ""}
                {model.supports_vision ? " · vision" : ""}
              </div>
            </div>
            <button className="text-muted" onClick={() => go("models")} aria-label="Models">
              <ChevronRight size={16} />
            </button>
          </div>
        ) : (
          <button
            className="flex w-full items-center gap-3 rounded-lg border border-dashed border-line p-3 text-left hover:bg-page"
            onClick={() => go("models")}
          >
            <span className="rounded-lg bg-page p-2 text-muted">
              <Boxes size={18} />
            </span>
            <div>
              <div className="font-medium">No model loaded</div>
              <div className="text-xs text-muted">
                {environment?.engine.detail ?? "Choose one in Models"}
              </div>
            </div>
          </button>
        )}
      </Card>

      <Card className="p-4">
        <SectionTitle action="Manage" onAction={() => go("tools")}>
          Tools
        </SectionTitle>
        <ul className="divide-y divide-line">
          <li className="flex items-center gap-3 py-2.5">
            <span className="text-muted">
              <Globe size={18} />
            </span>
            <div className="flex-1">
              <div className="text-sm">Web search and fetch</div>
              <div className="text-xs text-faint">
                {environment
                  ? environment.network === "online"
                    ? "online: allowed"
                    : environment.network === "ask"
                      ? "asks before each request"
                      : "air-gapped: nothing leaves this machine"
                  : ""}
              </div>
            </div>
            <Switch
              checked={online}
              disabled={!mayGoOnline}
              label="Web search"
              onChange={(on) => void setNetwork(on ? "ask" : "airgapped")}
            />
          </li>
          {(environment?.harnesses ?? []).map((h) => {
            const exposed = active?.tools.filter((t) => t.harness === h.id).length ?? 0;
            return (
              <li key={h.id} className="flex items-center gap-3 py-2.5">
                <span className="text-muted">
                  <Network size={18} />
                </span>
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm">{h.title}</div>
                  <div className="text-xs text-faint">
                    {h.tool_count} tools, {exposed} in context this turn
                    {h.degraded ? ` · ${h.degraded}` : ""}
                  </div>
                </div>
                <Switch
                  checked={h.enabled}
                  label={`${h.title} enabled`}
                  onChange={(on) => void setEnabled(h.id, on)}
                />
              </li>
            );
          })}
        </ul>
      </Card>

      <Card className="p-4">
        <SectionTitle action="Manage" onAction={() => go("agents")}>
          Context
        </SectionTitle>
        <button
          className="flex w-full items-center gap-3 rounded-lg border border-line p-3 text-left hover:bg-page"
          onClick={() => go("agents")}
        >
          <span className="rounded-lg bg-page p-2 text-muted">
            <FileText size={18} />
          </span>
          <div className="min-w-0 flex-1">
            <div className="font-medium">Current context</div>
            <div className="text-xs text-muted">
              {environment
                ? `Focus: ${environment.focus ?? "none"} · ${active ? `${active.tools.length} tools, ~${active.token_estimate} of ${active.budget} tokens` : "…"}`
                : ""}
            </div>
            <div className="text-xs text-muted">
              {task ? `Task: ${task.goal || "none yet"} · ${task.artifacts.length} artifacts` : "No run yet"}
            </div>
          </div>
          <ChevronRight size={16} className="text-muted" />
        </button>

        <h3 className="mb-2 mt-4 text-sm font-medium">Recent changes</h3>
        {recent.length === 0 ? (
          <p className="text-xs text-faint">Nothing committed yet.</p>
        ) : (
          <ul className="divide-y divide-line">
            {recent.map((c) => (
              <li key={c.id} className="flex items-center gap-3 py-2">
                <HistoryIcon size={16} className="text-muted" />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm">{c.diff_summary || c.tool}</div>
                  <div className="text-xs text-faint">{c.harness}</div>
                </div>
                <span className="text-xs text-faint">{timeAgo(c.at_ms)}</span>
              </li>
            ))}
          </ul>
        )}
      </Card>

      <Card className="p-4">
        <button className="flex w-full items-center gap-3 text-left" onClick={() => go("history")}>
          <span className="rounded-lg bg-page p-2 text-muted">
            <HistoryIcon size={18} />
          </span>
          <div className="flex-1">
            <div className="font-medium">Version history</div>
            <div className="text-xs text-muted">
              {history.length} commit{history.length === 1 ? "" : "s"} in this environment; every
              change is undoable.
            </div>
          </div>
          <ChevronRight size={16} className="text-muted" />
        </button>
      </Card>
    </aside>
  );
}
