// Tools: the active set Core computed for the next turn (spec §4: focused,
// pinned, touched, core builtins), a capability search over everything
// installed, and a way to run a tool by hand through the same door the agent
// uses.

import { useEffect, useState } from "react";
import { Pin, Play, Search } from "lucide-react";
import type { CapabilityHit, ToolOutcome } from "../api/generated";
import { outcomeLine, useSession } from "../store";
import { Button, Card, Pill, SectionTitle } from "../components/ui";

export function ToolsPage() {
  const { active, environment, refreshActive, setPinned, setFocus, findCapability, callTool } = useSession();
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
    <div className="grid min-h-0 flex-1 grid-cols-[2fr_1fr] gap-4 overflow-y-auto px-6 pb-6">
      <Card className="p-5">
        <SectionTitle>In context this turn</SectionTitle>
        {active ? (
          <>
            <p className="mb-3 text-xs text-muted">
              {active.tools.length} tools, about {active.token_estimate} of {active.budget} tokens
              {active.dropped.length > 0 ? ` · dropped for budget: ${active.dropped.join(", ")}` : ""} · grammar{" "}
              <span className="font-mono">{active.grammar_hash.slice(0, 8)}</span>
            </p>
            <table className="w-full text-sm">
              <thead className="text-left text-xs text-faint">
                <tr>
                  <th className="pb-2 font-medium">Tool</th>
                  <th className="pb-2 font-medium">Why</th>
                  <th className="pb-2 font-medium">Kind</th>
                  <th className="pb-2 font-medium">Summary</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-line">
                {active.tools.map((t) => (
                  <tr key={t.name} className="align-top">
                    <td className="py-2 pr-3 font-mono text-xs">
                      <button className="hover:text-accent" onClick={() => setTool(t.name)} title="Use below">
                        {t.name}
                      </button>
                    </td>
                    <td className="py-2 pr-3">
                      <Pill tone={t.reason === "focused" ? "ok" : "neutral"}>{t.reason.replace("_", " ")}</Pill>
                    </td>
                    <td className="py-2 pr-3 text-xs text-muted">
                      {t.kind}
                      {t.undoable ? " · undoable" : ""}
                      {t.confirm !== "never" ? ` · confirm ${t.confirm}` : ""}
                    </td>
                    <td className="py-2 text-muted">{t.summary}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </>
        ) : (
          <p className="text-sm text-muted">Loading…</p>
        )}
      </Card>

      <div className="flex flex-col gap-4">
        <Card className="p-5">
          <SectionTitle>Harnesses</SectionTitle>
          <ul className="space-y-2 text-sm">
            {(environment?.harnesses ?? []).map((h) => {
              const focused = environment?.focus === h.id;
              const pinned = environment?.pinned.includes(h.id) ?? false;
              return (
                <li key={h.id} className="flex items-center gap-2">
                  <span className="min-w-0 flex-1 truncate">{h.title}</span>
                  <Button onClick={() => void setFocus(focused ? null : h.id)} kind={focused ? "primary" : "ghost"}>
                    {focused ? "Focused" : "Focus"}
                  </Button>
                  <button
                    className={`rounded-lg border border-line p-1.5 ${pinned ? "text-accent" : "text-faint"}`}
                    onClick={() => void setPinned(h.id, !pinned)}
                    title={pinned ? "Unpin" : "Pin: keep its front-door tools in context"}
                  >
                    <Pin size={14} />
                  </button>
                </li>
              );
            })}
          </ul>
        </Card>

        <Card className="p-5">
          <SectionTitle>Find a capability</SectionTitle>
          <div className="flex gap-2">
            <input
              className="flex-1 rounded-lg border border-line px-3 py-1.5 text-sm outline-none focus:border-accent"
              placeholder="what do you need to do?"
              value={need}
              onChange={(e) => setNeed(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && void search()}
            />
            <Button onClick={() => void search()}>
              <Search size={14} />
            </Button>
          </div>
          {hits && (
            <ul className="mt-3 space-y-1 text-sm">
              {hits.length === 0 && <li className="text-muted">Nothing installed does that.</li>}
              {hits.map((h) => (
                <li key={h.tool} className="flex gap-2">
                  <button className="font-mono text-xs text-accent" onClick={() => setTool(h.tool)}>
                    {h.tool}
                  </button>
                  <span className="text-muted">{h.summary}</span>
                </li>
              ))}
            </ul>
          )}
        </Card>

        <Card className="p-5">
          <SectionTitle>Run a tool</SectionTitle>
          <p className="mb-2 text-xs text-muted">
            Same door as the agent: permission check, confirmation, a commit in the history.
          </p>
          <input
            className="mb-2 w-full rounded-lg border border-line px-3 py-1.5 font-mono text-xs outline-none focus:border-accent"
            placeholder="canvas.add_sticky"
            value={tool}
            onChange={(e) => setTool(e.target.value)}
          />
          <textarea
            className="mb-2 w-full rounded-lg border border-line px-3 py-1.5 font-mono text-xs outline-none focus:border-accent"
            rows={3}
            value={params}
            onChange={(e) => setParams(e.target.value)}
          />
          <Button kind="primary" onClick={() => void run()} disabled={!tool.trim()}>
            <span className="flex items-center gap-1">
              <Play size={14} /> Run
            </span>
          </Button>
          {outcome && (
            <pre className="mt-3 whitespace-pre-wrap rounded-lg bg-page p-3 font-mono text-xs">
              {typeof outcome === "string"
                ? outcome
                : `${outcomeLine(outcome)}${"ok" in outcome ? `\n${JSON.stringify(outcome.ok.result, null, 2)}` : ""}`}
            </pre>
          )}
        </Card>
      </div>
    </div>
  );
}
