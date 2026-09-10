// Tools: the active set Core computed for the next turn (spec §4: focused,
// pinned, touched, core builtins), a capability search over everything
// installed, and a way to run a tool by hand through the same door the agent
// uses.

import { useEffect, useState } from "react";
import { Button, Card, IconButton, Input, PanelsIcon, Pill, PinIcon, PlayIcon, SearchIcon, SectionTitle, Textarea } from "@localspace/ui";
import type { CapabilityHit, ToolOutcome } from "../api/generated";
import { outcomeLine, useSession } from "../store";

export function ToolsPage() {
  const { active, environment, refreshActive, setPinned, setFocus, findCapability, callTool, openView } = useSession();
  const [need, setNeed] = useState("");
  const [hits, setHits] = useState<CapabilityHit[] | null>(null);
  const [tool, setTool] = useState("");
  const [params, setParams] = useState("{}");
  const [outcome, setOutcome] = useState<ToolOutcome | string | null>(null);

  useEffect(() => {
    void refreshActive();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const search = async () => {
    if (!need.trim()) return;
    setHits(await findCapability(need.trim()));
  };

  const run = async () => {
    let parsed: unknown;
    try {
      parsed = JSON.parse(params || "{}");
    } catch {
      setOutcome("the parameters are not valid JSON");
      return;
    }
    const result = await callTool(tool.trim(), parsed as never);
    setOutcome(result);
    void refreshActive();
  };

  return (
    <div className="page page-grid-21">
      <Card className="ls-pad-4">
        <SectionTitle>In context this turn</SectionTitle>
        {active ? (
          <>
            <p className="ls-mb-3 ls-small ls-muted">
              {active.tools.length} tools, about {active.token_estimate} of {active.budget} tokens
              {active.dropped.length > 0 ? ` · dropped for budget: ${active.dropped.join(", ")}` : ""} · grammar <span className="ls-mono">{active.grammar_hash.slice(0, 8)}</span>
            </p>
            <table className="ls-table">
              <thead>
                <tr>
                  <th>Tool</th>
                  <th>Why</th>
                  <th>Kind</th>
                  <th>Summary</th>
                </tr>
              </thead>
              <tbody>
                {active.tools.map((t) => (
                  <tr key={t.name}>
                    <td className="ls-mono">
                      <button type="button" className="ls-link-button ls-mono" onClick={() => setTool(t.name)} title="Use below">
                        {t.name}
                      </button>
                    </td>
                    <td>
                      <Pill tone={t.reason === "focused" ? "ok" : "neutral"}>{t.reason.replace("_", " ")}</Pill>
                    </td>
                    <td className="ls-small ls-muted">
                      {t.kind}
                      {t.undoable ? " · undoable" : ""}
                      {t.confirm !== "never" ? ` · confirm ${t.confirm}` : ""}
                    </td>
                    <td className="ls-muted">{t.summary}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </>
        ) : (
          <p className="ls-muted">Loading…</p>
        )}
      </Card>

      <div className="ls-col ls-gap-4">
        <Card className="ls-pad-4">
          <SectionTitle>Harnesses</SectionTitle>
          <ul className="ls-list ls-col ls-gap-2">
            {(environment?.harnesses ?? []).map((h) => {
              const focused = environment?.focus === h.id;
              const pinned = environment?.pinned.includes(h.id) ?? false;
              return (
                <li key={h.id}>
                  <div className="ls-row ls-gap-2">
                    <span className="ls-grow ls-truncate">{h.title}</span>
                    <Button onClick={() => void setFocus(focused ? null : h.id)} kind={focused ? "primary" : "ghost"}>
                      {focused ? "Focused" : "Focus"}
                    </Button>
                    <IconButton label={pinned ? "Unpin" : "Pin: keep its front-door tools in context"} on={pinned} onClick={() => void setPinned(h.id, !pinned)}>
                      <PinIcon size={14} />
                    </IconButton>
                  </div>
                  {h.views.length > 0 && (
                    <div className="ls-row ls-wrap ls-gap-1 ls-mt-1">
                      {h.views.map((v) => (
                        <Button key={v.id} size="small" onClick={() => openView(h.id, v)} title={`Open the ${v.kind} view "${v.id}" as a panel beside the chat`}>
                          <PanelsIcon size={12} /> {v.title}
                          <span className="ls-faint">· {v.kind}</span>
                        </Button>
                      ))}
                    </div>
                  )}
                </li>
              );
            })}
          </ul>
        </Card>

        <Card className="ls-pad-4">
          <SectionTitle>Find a capability</SectionTitle>
          <div className="ls-row ls-gap-2">
            <Input placeholder="what do you need to do?" value={need} onChange={(e) => setNeed(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void search()} />
            <IconButton label="Search" onClick={() => void search()}>
              <SearchIcon size={14} />
            </IconButton>
          </div>
          {hits && (
            <ul className="ls-list ls-col ls-gap-1 ls-mt-3">
              {hits.length === 0 && <li className="ls-muted">Nothing installed does that.</li>}
              {hits.map((h) => (
                <li key={h.tool} className="ls-row ls-gap-2">
                  <button type="button" className="ls-link-button ls-mono ls-accent" onClick={() => setTool(h.tool)}>
                    {h.tool}
                  </button>
                  <span className="ls-muted">{h.summary}</span>
                </li>
              ))}
            </ul>
          )}
        </Card>

        <Card className="ls-pad-4">
          <SectionTitle>Run a tool</SectionTitle>
          <p className="ls-mb-2 ls-small ls-muted">Same door as the agent: permission check, confirmation, a commit in the history.</p>
          <Input mono className="ls-mb-2" placeholder="canvas.add_sticky" value={tool} onChange={(e) => setTool(e.target.value)} />
          <Textarea mono className="ls-mb-2" rows={3} value={params} onChange={(e) => setParams(e.target.value)} />
          <Button kind="primary" onClick={() => void run()} disabled={!tool.trim()}>
            <PlayIcon size={14} /> Run
          </Button>
          {outcome && (
            <pre className="code ls-mt-3">
              {typeof outcome === "string" ? outcome : `${outcomeLine(outcome)}${"ok" in outcome ? `\n${JSON.stringify(outcome.ok.result, null, 2)}` : ""}`}
            </pre>
          )}
        </Card>
      </div>
    </div>
  );
}
