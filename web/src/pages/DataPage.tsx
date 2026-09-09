// Data: the documents — one per harness, Automerge in Core — and the
// environment lock, as JSON.

import { useEffect, useState } from "react";
import { call, pick } from "../api/client";
import type { Json } from "../api/generated";
import { useSession } from "../store";
import { Button, Card, SectionTitle } from "../components/ui";

export function DataPage() {
  const { environment, docJson } = useSession();
  const [selected, setSelected] = useState<string | null>(null);
  const [doc, setDoc] = useState<Json | null>(null);
  const [lock, setLock] = useState<Json | null>(null);

  useEffect(() => {
    void call("get_lock").then((r) => setLock(pick(r, "lock")?.json ?? null));
  }, []);

  const open = async (harness: string) => {
    setSelected(harness);
    setDoc(await docJson(harness));
  };

  return (
    <div className="grid min-h-0 flex-1 grid-cols-[280px_1fr] gap-4 overflow-y-auto px-6 pb-6">
      <div className="flex flex-col gap-4">
        <Card className="p-5">
          <SectionTitle>Documents</SectionTitle>
          <ul className="space-y-1 text-sm">
            {(environment?.harnesses ?? []).map((h) => (
              <li key={h.id}>
                <button
                  className={`w-full rounded-lg px-2 py-1.5 text-left hover:bg-page ${selected === h.id ? "bg-accent-soft text-accent" : ""}`}
                  onClick={() => void open(h.id)}
                >
                  <div className="font-medium">{h.title}</div>
                  <div className="text-xs text-muted">{h.doc_kind} · {h.id}</div>
                </button>
              </li>
            ))}
            {(environment?.harnesses.length ?? 0) === 0 && (
              <li className="text-muted">No harness, so no document yet.</li>
            )}
          </ul>
        </Card>
        <Card className="p-5">
          <SectionTitle>Environment lock</SectionTitle>
          <p className="mb-2 text-xs text-muted">Every package, its version and content hash (spec §17.2).</p>
          <pre className="max-h-64 overflow-auto rounded-lg bg-page p-3 font-mono text-[11px]">
            {lock ? JSON.stringify(lock, null, 2) : "…"}
          </pre>
        </Card>
      </div>
      <Card className="flex min-h-0 flex-col p-5">
        <div className="flex items-baseline justify-between">
          <SectionTitle>{selected ?? "Pick a document"}</SectionTitle>
          {selected && <Button onClick={() => void open(selected)}>Refresh</Button>}
        </div>
        <pre className="min-h-0 flex-1 overflow-auto rounded-lg bg-page p-3 font-mono text-[11.5px] leading-relaxed">
          {doc ? JSON.stringify(doc, null, 2) : selected ? "…" : "The document appears here as Core holds it."}
        </pre>
      </Card>
    </div>
  );
}
