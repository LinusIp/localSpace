// Data: the documents — one per harness, Automerge in Core — and the
// environment lock, as JSON.

import { useEffect, useState } from "react";
import { Button, Card, SectionTitle } from "@localspace/ui";
import { call, pick } from "../api/client";
import type { Json } from "../api/generated";
import { useSession } from "../store";

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
    <div className="page page-grid-side">
      <div className="ls-col ls-gap-4">
        <Card className="ls-pad-4">
          <SectionTitle>Documents</SectionTitle>
          <ul className="ls-list ls-col ls-gap-1">
            {(environment?.harnesses ?? []).map((h) => (
              <li key={h.id}>
                <button type="button" className={`choice plain${selected === h.id ? " current" : ""}`} onClick={() => void open(h.id)}>
                  <span>
                    <div className="ls-medium">{h.title}</div>
                    <div className="ls-small ls-muted">
                      {h.doc_kind} · {h.id}
                    </div>
                  </span>
                </button>
              </li>
            ))}
            {(environment?.harnesses.length ?? 0) === 0 && <li className="ls-muted">No harness, so no document yet.</li>}
          </ul>
        </Card>
        <Card className="ls-pad-4">
          <SectionTitle>Environment lock</SectionTitle>
          <p className="ls-mb-2 ls-small ls-muted">Every package, its version and content hash (spec §17.2).</p>
          <pre className="code" style={{ maxHeight: "16rem" }}>
            {lock ? JSON.stringify(lock, null, 2) : "…"}
          </pre>
        </Card>
      </div>
      <Card className="ls-pad-4 ls-col" style={{ minHeight: 0 }}>
        <div className="ls-row ls-between ls-start">
          <SectionTitle>{selected ?? "Pick a document"}</SectionTitle>
          {selected && <Button onClick={() => void open(selected)}>Refresh</Button>}
        </div>
        <pre className="code tall">{doc ? JSON.stringify(doc, null, 2) : selected ? "…" : "The document appears here as Core holds it."}</pre>
      </Card>
    </div>
  );
}
