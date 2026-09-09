// Agents: the task ledger every prompt carries (spec §18.1), the approvals
// waiting on the user, what the model will see next turn, and the trace.

import { useEffect, useState } from "react";
import { Check, Circle, CircleDot, Loader2, XCircle } from "lucide-react";
import type { StepStatus } from "../api/generated";
import { useSession } from "../store";
import { Button, Card, Empty, SectionTitle } from "../components/ui";

export function AgentsPage() {
  const { task, approvals, approve, trace, context, previewContext, refreshTask, busy } = useSession();
  const [budget, setBudget] = useState(600);

  useEffect(() => {
    void refreshTask();
    void previewContext(budget);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [budget]);

  return (
    <div className="grid min-h-0 flex-1 grid-cols-[1fr_1fr] gap-4 overflow-y-auto px-6 pb-6">
      <div className="flex flex-col gap-4">
        <Card className="p-5">
          <SectionTitle>Task ledger</SectionTitle>
          {task && (task.goal || task.plan.length > 0 || task.artifacts.length > 0) ? (
            <div className="text-sm">
              <div className="text-xs text-faint">{task.id}</div>
              <p className="mt-1 font-medium">{task.goal || "no goal recorded"}</p>
              {task.plan.length > 0 && (
                <ol className="mt-3 space-y-1.5">
                  {task.plan.map((s, i) => (
                    <li key={i} className="flex items-center gap-2">
                      <StepIcon status={s.status} />
                      <span className="font-mono text-xs text-muted">{s.harness}</span>
                      <span>{s.intent}</span>
                    </li>
                  ))}
                </ol>
              )}
              {task.artifacts.length > 0 && (
                <>
                  <h3 className="mt-4 text-xs font-medium uppercase tracking-wide text-faint">Artifacts</h3>
                  <ul className="mt-1 space-y-1">
                    {task.artifacts.map((a) => (
                      <li key={a.id} className="flex gap-2">
                        <span className="font-mono text-xs text-accent">{a.id}</span>
                        <span className="font-mono text-xs text-muted">{a.kind}</span>
                        <span>{a.summary}</span>
                        <span className="text-xs text-faint">
                          from {a.produced_by} @ {a.commit.slice(0, 7)}
                        </span>
                      </li>
                    ))}
                  </ul>
                </>
              )}
              {task.notes.length > 0 && (
                <>
                  <h3 className="mt-4 text-xs font-medium uppercase tracking-wide text-faint">Notes</h3>
                  <ul className="mt-1 list-disc pl-5 text-muted">
                    {task.notes.map((n, i) => (
                      <li key={i}>{n}</li>
                    ))}
                  </ul>
                </>
              )}
            </div>
          ) : (
            <p className="text-sm text-muted">
              {busy ? "A run is in progress." : "No run yet. The ledger fills as the agent plans, acts and records artifacts."}
            </p>
          )}
        </Card>

        <Card className="p-5">
          <SectionTitle>Approvals</SectionTitle>
          {approvals.length === 0 ? (
            <p className="text-sm text-muted">Nothing is waiting on you.</p>
          ) : (
            <ul className="space-y-3">
              {approvals.map((a) => (
                <li key={a.id} className="rounded-lg border border-warn/30 bg-warn-soft p-3 text-sm">
                  <div className="text-xs font-medium uppercase text-warn">{a.kind.replace("_", " ")}</div>
                  <p className="mt-1">{a.prompt}</p>
                  <div className="mt-2 flex gap-2">
                    <Button kind="primary" onClick={() => void approve(a.id, true)}>
                      Allow
                    </Button>
                    <Button onClick={() => void approve(a.id, false)}>Deny</Button>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </Card>

        <Card className="flex min-h-[200px] flex-col p-5">
          <SectionTitle>Trace</SectionTitle>
          {trace.length === 0 ? (
            <p className="text-sm text-muted">Tool calls and Core's notes appear here as they happen.</p>
          ) : (
            <pre className="max-h-80 overflow-y-auto rounded-lg bg-page p-3 font-mono text-xs leading-relaxed">
              {trace.slice(-100).join("\n")}
            </pre>
          )}
        </Card>
      </div>

      <Card className="flex min-h-0 flex-col p-5">
        <div className="flex items-baseline justify-between">
          <SectionTitle>What the model sees</SectionTitle>
          <label className="text-xs text-muted">
            budget{" "}
            <select
              className="rounded-md border border-line bg-white px-1 py-0.5"
              value={budget}
              onChange={(e) => setBudget(Number(e.target.value))}
            >
              {[300, 600, 1500, 4000].map((b) => (
                <option key={b} value={b}>
                  {b} tokens
                </option>
              ))}
            </select>
          </label>
        </div>
        {context ? (
          <>
            <ul className="mb-3 space-y-1 text-xs text-muted">
              {context.blocks.map((b) => (
                <li key={b.harness}>
                  <span className="font-mono text-ink">{b.harness}</span> · {b.tokens} tokens
                  {b.expandable ? " · expandable with a zoom tool" : ""}
                </li>
              ))}
              {context.blocks.length === 0 && <li>No harness has a context provider active.</li>}
            </ul>
            <pre className="min-h-0 flex-1 overflow-auto rounded-lg bg-page p-3 font-mono text-[11.5px] leading-relaxed">
              {context.prompt}
            </pre>
          </>
        ) : (
          <Empty title="Loading the prompt preview…" />
        )}
      </Card>
    </div>
  );
}

function StepIcon({ status }: { status: StepStatus }) {
  switch (status) {
    case "done":
      return <Check size={14} className="text-accent" />;
    case "active":
      return <Loader2 size={14} className="animate-spin text-accent" />;
    case "failed":
      return <XCircle size={14} className="text-danger" />;
    case "pending":
      return <Circle size={14} className="text-faint" />;
    default:
      return <CircleDot size={14} className="text-faint" />;
  }
}
