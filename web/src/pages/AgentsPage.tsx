// Agents: the task ledger every prompt carries (spec §18.1), the approvals
// waiting on the user, what the model will see next turn, and the trace.

import { useEffect, useState } from "react";
import { Button, Card, CheckIcon, CircleIcon, CloseIcon, Empty, SectionTitle, Select, SpinnerIcon } from "@localspace/ui";
import type { StepStatus } from "../api/generated";
import { useSession } from "../store";

export function AgentsPage() {
  const { task, approvals, approve, trace, context, previewContext, refreshTask, busy } = useSession();
  const [budget, setBudget] = useState(600);

  useEffect(() => {
    void refreshTask();
    void previewContext(budget);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [budget]);

  return (
    <div className="page page-grid-2">
      <div className="ls-col ls-gap-4">
        <Card className="ls-pad-4">
          <SectionTitle>Task ledger</SectionTitle>
          {task && (task.goal || task.plan.length > 0 || task.artifacts.length > 0) ? (
            <div>
              <div className="ls-small ls-faint">{task.id}</div>
              <p className="ls-mt-1 ls-medium">{task.goal || "no goal recorded"}</p>
              {task.plan.length > 0 && (
                <ol className="ls-list ls-col ls-gap-1 ls-mt-3">
                  {task.plan.map((s, i) => (
                    <li key={i} className="ls-row ls-gap-2">
                      <StepIcon status={s.status} />
                      <span className="ls-mono ls-muted">{s.harness}</span>
                      <span>{s.intent}</span>
                    </li>
                  ))}
                </ol>
              )}
              {task.artifacts.length > 0 && (
                <>
                  <h3 className="ls-mt-4 ls-small ls-medium ls-faint" style={{ textTransform: "uppercase", letterSpacing: "0.04em" }}>
                    Artifacts
                  </h3>
                  <ul className="ls-list ls-col ls-gap-1 ls-mt-1">
                    {task.artifacts.map((a) => (
                      <li key={a.id} className="ls-row ls-gap-2 ls-wrap">
                        <span className="ls-mono ls-accent">{a.id}</span>
                        <span className="ls-mono ls-muted">{a.kind}</span>
                        <span>{a.summary}</span>
                        <span className="ls-small ls-faint">
                          from {a.produced_by} @ {a.commit.slice(0, 7)}
                        </span>
                      </li>
                    ))}
                  </ul>
                </>
              )}
              {task.notes.length > 0 && (
                <>
                  <h3 className="ls-mt-4 ls-small ls-medium ls-faint" style={{ textTransform: "uppercase", letterSpacing: "0.04em" }}>
                    Notes
                  </h3>
                  <ul className="ls-mt-1 ls-muted" style={{ paddingLeft: 20 }}>
                    {task.notes.map((n, i) => (
                      <li key={i}>{n}</li>
                    ))}
                  </ul>
                </>
              )}
            </div>
          ) : (
            <p className="ls-muted">{busy ? "A run is in progress." : "No run yet. The ledger fills as the agent plans, acts and records artifacts."}</p>
          )}
        </Card>

        <Card className="ls-pad-4">
          <SectionTitle>Approvals</SectionTitle>
          {approvals.length === 0 ? (
            <p className="ls-muted">Nothing is waiting on you.</p>
          ) : (
            <ul className="ls-list ls-col ls-gap-3">
              {approvals.map((a) => (
                <li key={a.id} className="chat-approval">
                  <div className="chat-approval-kind">{a.kind.replace("_", " ")}</div>
                  <p className="ls-mt-1" style={{ marginBottom: 0 }}>
                    {a.prompt}
                  </p>
                  <div className="ls-row ls-gap-2 ls-mt-2">
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

        <Card className="ls-pad-4 ls-col" style={{ minHeight: 200 }}>
          <SectionTitle>Trace</SectionTitle>
          {trace.length === 0 ? <p className="ls-muted">Tool calls and Core's notes appear here as they happen.</p> : <pre className="code">{trace.slice(-100).join("\n")}</pre>}
        </Card>
      </div>

      <Card className="ls-pad-4 ls-col" style={{ minHeight: 0 }}>
        <div className="ls-row ls-between ls-start">
          <SectionTitle>What the model sees</SectionTitle>
          <label className="ls-row ls-gap-1 ls-small ls-muted">
            budget
            <Select value={budget} onChange={(e) => setBudget(Number(e.target.value))} style={{ width: "auto", padding: "2px 24px 2px 6px", fontSize: 12 }}>
              {[300, 600, 1500, 4000].map((b) => (
                <option key={b} value={b}>
                  {b} tokens
                </option>
              ))}
            </Select>
          </label>
        </div>
        {context ? (
          <>
            <ul className="ls-list ls-col ls-gap-1 ls-mb-3 ls-small ls-muted">
              {context.blocks.map((b) => (
                <li key={b.harness}>
                  <span className="ls-mono" style={{ color: "var(--ls-ink)" }}>
                    {b.harness}
                  </span>{" "}
                  · {b.tokens} tokens
                  {b.expandable ? " · expandable with a zoom tool" : ""}
                </li>
              ))}
              {context.blocks.length === 0 && <li>No harness has a context provider active.</li>}
            </ul>
            <pre className="code tall">{context.prompt}</pre>
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
      return <CheckIcon size={14} className="ls-accent" />;
    case "active":
      return <SpinnerIcon size={14} className="ls-spin ls-accent" />;
    case "failed":
      return <CloseIcon size={14} className="ls-danger" />;
    case "pending":
      return <CircleIcon size={14} className="ls-faint" />;
    default:
      return <CircleIcon size={14} className="ls-muted" />;
  }
}
