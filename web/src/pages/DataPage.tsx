// Data: every document the session may see — each harness's, Automerge in
// Core, and the files of their own that exports make (uploads and cached
// pages follow in step 6) — and the environment lock, as JSON.

import { useCallback, useEffect, useState } from "react";
import { Button, Card, SectionTitle } from "@localspace/ui";
import { bytesLabel, call, documents, downloadDocument, pick } from "../api/client";
import type { DocumentInfo, Json } from "../api/generated";
import { useSession } from "../store";
import { bus } from "../surfaces/bus";

/** The line under a document's title: what it is and where it came from. */
function describe(d: DocumentInfo): string {
  if ("harness" in d.source) return `${d.kind} · ${d.source.harness.harness}`;
  const { harness, commit } = d.source.export;
  const size = d.bytes === null ? "no content: its export was undone" : bytesLabel(d.bytes);
  return `${d.mime} · ${size} · exported from ${harness} at ${commit ? commit.slice(0, 7) : "no commit"}`;
}

export function DataPage() {
  const { environment, docJson, notify } = useSession();
  const [docs, setDocs] = useState<DocumentInfo[]>([]);
  const [selected, setSelected] = useState<DocumentInfo | null>(null);
  const [json, setJson] = useState<Json | null>(null);
  const [lock, setLock] = useState<Json | null>(null);
  const installed = environment?.harnesses.length ?? 0;

  const refresh = useCallback(() => {
    void documents()
      .then(setDocs)
      .catch(() => undefined);
  }, []);
  // The list follows Core: a new export, an undo, an install.
  useEffect(() => {
    refresh();
    return bus.on("doc_changed", refresh);
  }, [refresh, installed]);
  useEffect(() => {
    void call("get_lock").then((r) => setLock(pick(r, "lock")?.json ?? null));
  }, []);

  const open = async (d: DocumentInfo) => {
    if (!("harness" in d.source)) return;
    setSelected(d);
    setJson(await docJson(d.source.harness.harness));
  };
  const save = (d: DocumentInfo) => {
    downloadDocument(d.id, d.title).catch((err: unknown) => notify("error", err instanceof Error ? err.message : String(err)));
  };

  return (
    <div className="page page-grid-side">
      <div className="ls-col ls-gap-4">
        <Card className="ls-pad-4">
          <SectionTitle>Documents</SectionTitle>
          <ul className="ls-list ls-col ls-gap-1">
            {docs.map((d) =>
              "harness" in d.source ? (
                <li key={d.id}>
                  <button type="button" className={`choice plain${selected?.id === d.id ? " current" : ""}`} onClick={() => void open(d)}>
                    <span>
                      <div className="ls-medium">{d.title}</div>
                      <div className="ls-small ls-muted">{describe(d)}</div>
                    </span>
                  </button>
                </li>
              ) : (
                <li key={d.id} className="ls-row ls-gap-2 ls-between">
                  <span>
                    <div className="ls-medium">{d.title}</div>
                    <div className="ls-small ls-muted">{describe(d)}</div>
                  </span>
                  <Button size="small" disabled={d.head === null} onClick={() => save(d)}>
                    Download
                  </Button>
                </li>
              ),
            )}
            {docs.length === 0 && <li className="ls-muted">No document yet: install a harness, or export something.</li>}
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
          <SectionTitle>{selected?.title ?? "Pick a document"}</SectionTitle>
          {selected && <Button onClick={() => void open(selected)}>Refresh</Button>}
        </div>
        <pre className="code tall">{json ? JSON.stringify(json, null, 2) : selected ? "…" : "A harness document appears here as Core holds it; a file downloads."}</pre>
      </Card>
    </div>
  );
}
