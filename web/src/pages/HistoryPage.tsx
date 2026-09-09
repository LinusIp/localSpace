// History: the version DAG. Every write is a commit; agent runs are branches
// that can be dropped whole.

import { useEffect } from "react";
import { Redo2, Undo2 } from "lucide-react";
import { useSession } from "../store";
import { Button, Card, Pill, SectionTitle, clock } from "../components/ui";

export function HistoryPage() {
  const { history, refreshHistory, undo, redo, dropRun } = useSession();

  useEffect(() => {
    void refreshHistory();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-6">
      <Card className="p-5">
        <div className="flex items-center justify-between">
          <SectionTitle>Commits</SectionTitle>
          <div className="flex gap-2">
            <Button onClick={() => void undo()}>
              <span className="flex items-center gap-1">
                <Undo2 size={14} /> Undo
              </span>
            </Button>
            <Button onClick={() => void redo()}>
              <span className="flex items-center gap-1">
                <Redo2 size={14} /> Redo
              </span>
            </Button>
          </div>
        </div>
        {history.length === 0 ? (
          <p className="text-sm text-muted">Nothing has been written yet.</p>
        ) : (
          <table className="w-full text-sm">
            <thead className="text-left text-xs text-faint">
              <tr>
                <th className="pb-2 font-medium">When</th>
                <th className="pb-2 font-medium">Harness</th>
                <th className="pb-2 font-medium">Tool</th>
                <th className="pb-2 font-medium">Change</th>
                <th className="pb-2 font-medium">By</th>
                <th className="pb-2 font-medium">Run</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {history.map((c) => (
                <tr key={c.id} className="align-top">
                  <td className="py-2 pr-3 text-xs text-muted">{clock(c.at_ms)}</td>
                  <td className="py-2 pr-3 text-xs">{c.harness}</td>
                  <td className="py-2 pr-3 font-mono text-xs">{c.tool}</td>
                  <td className="py-2 pr-3">{c.diff_summary}</td>
                  <td className="py-2 pr-3">
                    <Pill tone={c.author === "agent" ? "ok" : "neutral"}>{c.author}</Pill>
                  </td>
                  <td className="py-2 text-xs">
                    {c.run ? (
                      <button
                        className="text-danger hover:underline"
                        onClick={() => void dropRun(c.run!)}
                        title="Drop this whole agent run"
                      >
                        drop {c.run.slice(0, 8)}
                      </button>
                    ) : (
                      <span className="text-faint">—</span>
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
