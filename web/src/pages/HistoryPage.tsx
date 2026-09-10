// History: the version DAG. Every write is a commit; agent runs are branches
// that can be dropped whole.

import { useEffect } from "react";
import { Button, Card, Pill, RedoIcon, SectionTitle, UndoIcon } from "@localspace/ui";
import { useSession } from "../store";
import { clock } from "../lib/time";

export function HistoryPage() {
  const { history, refreshHistory, undo, redo, dropRun } = useSession();

  useEffect(() => {
    void refreshHistory();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="page">
      <Card className="ls-pad-4">
        <div className="ls-row ls-between ls-start">
          <SectionTitle>Commits</SectionTitle>
          <div className="ls-row ls-gap-2">
            <Button onClick={() => void undo()}>
              <UndoIcon size={14} /> Undo
            </Button>
            <Button onClick={() => void redo()}>
              <RedoIcon size={14} /> Redo
            </Button>
          </div>
        </div>
        {history.length === 0 ? (
          <p className="ls-muted">Nothing has been written yet.</p>
        ) : (
          <table className="ls-table">
            <thead>
              <tr>
                <th>When</th>
                <th>Harness</th>
                <th>Tool</th>
                <th>Change</th>
                <th>By</th>
                <th>Run</th>
              </tr>
            </thead>
            <tbody>
              {history.map((c) => (
                <tr key={c.id}>
                  <td className="ls-small ls-muted">{clock(c.at_ms)}</td>
                  <td className="ls-small">{c.harness}</td>
                  <td className="ls-mono">{c.tool}</td>
                  <td>{c.diff_summary}</td>
                  <td>
                    <Pill tone={c.author === "agent" ? "ok" : "neutral"}>{c.author}</Pill>
                  </td>
                  <td className="ls-small">
                    {c.run ? (
                      <button type="button" className="ls-link-button ls-danger" onClick={() => void dropRun(c.run as string)} title="Drop this whole agent run">
                        drop {c.run.slice(0, 8)}
                      </button>
                    ) : (
                      <span className="ls-faint">—</span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>
    </div>
  );
}
