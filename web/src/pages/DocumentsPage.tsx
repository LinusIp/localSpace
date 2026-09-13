// Documents: everything in this workspace the person may see — each board
// as a document of its own, and the files that exports made. Uploading
// files comes with document search.

import { useCallback, useEffect, useState } from "react";
import { BoardIcon, FileIcon } from "@localspace/ui";
import { bytesLabel, documents, downloadDocument } from "../api/client";
import type { DocumentInfo } from "../api/generated";
import { useSession } from "../store";
import { TopBar } from "../components/TopBar";
import { bus } from "../surfaces/bus";

function describe(d: DocumentInfo, titles: Map<string, string>): string {
  if ("harness" in d.source) return `A ${titles.get(d.source.harness.harness)?.toLowerCase() ?? "board"} document.`;
  const { harness, commit } = d.source.export;
  const size = d.bytes === null ? "its export was undone, so it has no content" : bytesLabel(d.bytes);
  return `${d.mime.split("/")[1]?.toUpperCase() ?? d.mime} · ${size} · exported from the ${titles.get(harness)?.toLowerCase() ?? "board"}${commit ? "" : ""}`;
}

export function DocumentsPage() {
  const { environment, notify, openBoard } = useSession();
  const [docs, setDocs] = useState<DocumentInfo[] | null>(null);
  const installed = environment?.harnesses.length ?? 0;
  const titles = new Map((environment?.harnesses ?? []).map((h) => [h.id, h.title] as const));

  const refresh = useCallback(() => {
    void documents()
      .then(setDocs)
      .catch(() => setDocs([]));
  }, []);
  useEffect(() => {
    refresh();
    return bus.on("doc_changed", refresh);
  }, [refresh, installed]);

  const save = (d: DocumentInfo) => {
    downloadDocument(d.id, d.title).catch((err: unknown) => notify("error", err instanceof Error ? err.message : String(err)));
  };

  return (
    <>
      <TopBar />
      <div className="page">
        <h1 className="page-title">Documents</h1>
        <div className="page-sub">What is in this workspace: your boards, and the files made from them.</div>
        {docs === null ? null : docs.length === 0 ? (
          <p className="ls-muted" style={{ marginTop: 32 }}>
            No documents yet. A board appears here once you open it, and files you export from it follow. Uploading your own files comes with document search.
          </p>
        ) : (
          <div className="row-list">
            {docs.map((d) => {
              const board = "harness" in d.source ? d.source.harness.harness : null;
              return (
                <div key={d.id} className="row-item">
                  <span className={`glyph${board ? "" : " grey"}`} style={{ width: 36, height: 36, borderRadius: 9 }}>
                    {board ? <BoardIcon size={18} /> : <FileIcon size={18} />}
                  </span>
                  <div className="row-main">
                    <div className="row-title">{d.title}</div>
                    <div className="row-body">{describe(d, titles)}</div>
                  </div>
                  {board ? (
                    <button type="button" className="btn" onClick={() => openBoard(board)}>
                      Open
                    </button>
                  ) : (
                    <button type="button" className="btn" disabled={d.head === null} onClick={() => save(d)}>
                      Download
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </>
  );
}
