// The right column of the chat page: the active model, what the agent may
// use, what it will see, and what happened last. Every line is Core's own
// state; nothing here is decoration.

import { useEffect } from "react";
import { BoxesIcon, Card, ChevronRightIcon, Dot, FileIcon, GlobeIcon, HistoryIcon, NetworkIcon, SectionTitle, Switch } from "@localspace/ui";
import { useSession } from "../store";
import { timeAgo } from "../lib/time";

export function RightPanel() {
  const { environment, active, task, history, go, setEnabled, setNetwork, refreshActive, refreshTask, refreshHistory } = useSession();

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
    <aside className="ls-col ls-gap-4 ls-scroll" style={{ width: 380, flexShrink: 0 }}>
      <Card className="ls-pad-4">
        <SectionTitle>Active Model</SectionTitle>
        {model ? (
          <div className="ls-row ls-start ls-gap-3 ls-card" style={{ padding: 12, borderRadius: "var(--ls-radius)" }}>
            <span className="icon-well">
              <BoxesIcon size={18} />
            </span>
            <div className="ls-grow">
              <div className="ls-row ls-gap-2">
                <span className="ls-truncate ls-medium">{model.id}</span>
                <span className="ls-row ls-gap-1 ls-small ls-accent">
                  <Dot tone="ok" /> {model.loaded ? "Running" : "Selected"}
                </span>
              </div>
              <div className="ls-small ls-muted">
                {model.backend} · {model.context_len.toLocaleString()} context
                {model.supports_tools ? " · tools" : ""}
                {model.supports_vision ? " · vision" : ""}
              </div>
            </div>
            <button type="button" className="ls-link-button" onClick={() => go("models")} aria-label="Models">
              <ChevronRightIcon size={16} />
            </button>
          </div>
        ) : (
          <button type="button" className="choice dashed" onClick={() => go("models")}>
            <span className="icon-well">
              <BoxesIcon size={18} />
            </span>
            <span>
              <div className="ls-medium">No model loaded</div>
              <div className="ls-small ls-muted">{environment?.engine.detail ?? "Choose one in Models"}</div>
            </span>
          </button>
        )}
      </Card>

      <Card className="ls-pad-4">
        <SectionTitle action="Manage" onAction={() => go("tools")}>
          Tools
        </SectionTitle>
        <ul className="ls-list ls-divided">
          <li className="ls-row ls-gap-3" style={{ padding: "10px 0" }}>
            <span className="ls-muted">
              <GlobeIcon size={18} />
            </span>
            <div className="ls-grow">
              <div>Web search and fetch</div>
              <div className="ls-small ls-faint">
                {environment
                  ? environment.network === "online"
                    ? "online: allowed"
                    : environment.network === "ask"
                      ? "asks before each request"
                      : "air-gapped: nothing leaves this machine"
                  : ""}
              </div>
            </div>
            <Switch checked={online} disabled={!mayGoOnline} label="Web search" onChange={(on) => void setNetwork(on ? "ask" : "airgapped")} />
          </li>
          {(environment?.harnesses ?? []).map((h) => {
            const exposed = active?.tools.filter((t) => t.harness === h.id).length ?? 0;
            return (
              <li key={h.id} className="ls-row ls-gap-3" style={{ padding: "10px 0" }}>
                <span className="ls-muted">
                  <NetworkIcon size={18} />
                </span>
                <div className="ls-grow">
                  <div className="ls-truncate">{h.title}</div>
                  <div className="ls-small ls-faint">
                    {h.tool_count} tools, {exposed} in context this turn
                    {h.degraded ? ` · ${h.degraded}` : ""}
                  </div>
                </div>
                <Switch checked={h.enabled} label={`${h.title} enabled`} onChange={(on) => void setEnabled(h.id, on)} />
              </li>
            );
          })}
        </ul>
      </Card>

      <Card className="ls-pad-4">
        <SectionTitle action="Manage" onAction={() => go("agents")}>
          Context
        </SectionTitle>
        <button type="button" className="choice" onClick={() => go("agents")}>
          <span className="icon-well">
            <FileIcon size={18} />
          </span>
          <span className="ls-grow">
            <div className="ls-medium">Current context</div>
            <div className="ls-small ls-muted">
              {environment
                ? `Focus: ${environment.focus ?? "none"} · ${active ? `${active.tools.length} tools, ~${active.token_estimate} of ${active.budget} tokens` : "…"}`
                : ""}
            </div>
            <div className="ls-small ls-muted">{task ? `Task: ${task.goal || "none yet"} · ${task.artifacts.length} artifacts` : "No run yet"}</div>
          </span>
          <ChevronRightIcon size={16} className="ls-muted" />
        </button>

        <h3 className="ls-medium ls-mt-4 ls-mb-2" style={{ fontSize: 14 }}>
          Recent changes
        </h3>
        {recent.length === 0 ? (
          <p className="ls-small ls-faint">Nothing committed yet.</p>
        ) : (
          <ul className="ls-list ls-divided">
            {recent.map((c) => (
              <li key={c.id} className="ls-row ls-gap-3" style={{ padding: "8px 0" }}>
                <HistoryIcon size={16} className="ls-muted" />
                <div className="ls-grow">
                  <div className="ls-truncate">{c.diff_summary || c.tool}</div>
                  <div className="ls-small ls-faint">{c.harness}</div>
                </div>
                <span className="ls-small ls-faint">{timeAgo(c.at_ms)}</span>
              </li>
            ))}
          </ul>
        )}
      </Card>

      <Card className="ls-pad-4">
        <button type="button" className="choice plain" onClick={() => go("history")}>
          <span className="icon-well">
            <HistoryIcon size={18} />
          </span>
          <span className="ls-grow">
            <div className="ls-medium">Version history</div>
            <div className="ls-small ls-muted">
              {history.length} commit{history.length === 1 ? "" : "s"} in this environment; every change is undoable.
            </div>
          </span>
          <ChevronRightIcon size={16} className="ls-muted" />
        </button>
      </Card>
    </aside>
  );
}
