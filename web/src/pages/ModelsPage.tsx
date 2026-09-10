// Models (architecture v2 §4): the catalog with the placement planner's
// verdict for this machine, downloads with their progress, the sidecar with
// its state and log, a file the user already has, and — still — any
// OpenAI-compatible endpoint.

import { useEffect, useState } from "react";
import { BoxesIcon, Button, Card, Dot, DownloadIcon, FileIcon, Input, KeyValue, Pill, PlayIcon, SectionTitle, SpinnerIcon, StopIcon } from "@localspace/ui";
import type { ModelCatalogEntry } from "../api/generated";
import { useSession } from "../store";

export function ModelsPage() {
  const {
    environment,
    models,
    catalogModels,
    engineLog,
    refreshModels,
    refreshModelCatalog,
    refreshEngineLog,
    downloadModel,
    loadModel,
    unloadModel,
    importModel,
    selectModel,
  } = useSession();
  const [endpoint, setEndpoint] = useState("http://localhost:8080/v1");
  const [name, setName] = useState("");
  const [importPath, setImportPath] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    void refreshModels();
    void refreshModelCatalog();
    void refreshEngineLog();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const engine = environment?.engine;
  const model = environment?.model ?? null;

  const connect = async () => {
    if (!name.trim()) return;
    setBusy(true);
    await selectModel(`${endpoint.trim().replace(/\/$/, "")}|${name.trim()}`);
    setBusy(false);
  };

  return (
    <div className="page page-grid-32">
      <div className="ls-col ls-gap-4">
        <Card className="ls-pad-4">
          <SectionTitle action="Refresh" onAction={() => void refreshModelCatalog()}>
            Catalog
          </SectionTitle>
          <p className="ls-mb-3 ls-small ls-muted">
            Each entry carries the placement planner's verdict for this machine and its estimate before anything is downloaded. Downloads come from
            Hugging Face and go into the data directory; they are refused in an air-gapped environment, where a file is imported instead.
          </p>
          {catalogModels.length === 0 ? (
            <p className="ls-muted">Loading the catalog…</p>
          ) : (
            <ul className="ls-list ls-divided">
              {catalogModels.map((m) => (
                <CatalogRow key={m.id} entry={m} onDownload={() => void downloadModel(m.id)} onLoad={() => void loadModel(m.id)} onUnload={() => void unloadModel()} />
              ))}
            </ul>
          )}
        </Card>

        <Card className="ls-pad-4">
          <SectionTitle>Import a file</SectionTitle>
          <p className="ls-mb-2 ls-small ls-muted">
            A GGUF you already have joins the catalog where it is. Without a tensor map the planner sizes it from the file, as a dense model.
          </p>
          <div className="ls-row ls-gap-2">
            <Input
              mono
              placeholder="C:\\models\\my-model.gguf"
              value={importPath}
              onChange={(e) => setImportPath(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && importPath.trim() && void importModel(importPath.trim())}
            />
            <Button onClick={() => void importModel(importPath.trim())} disabled={!importPath.trim()}>
              <FileIcon size={14} /> Import
            </Button>
          </div>
        </Card>
      </div>

      <div className="ls-col ls-gap-4">
        <Card className="ls-pad-4">
          <SectionTitle action="Log" onAction={() => void refreshEngineLog()}>
            Engine
          </SectionTitle>
          <div className="ls-row ls-start ls-gap-3">
            <span className="icon-well">
              <BoxesIcon size={20} />
            </span>
            <div className="ls-grow">
              <div className="ls-row ls-gap-2 ls-medium">
                {engine?.model ?? model?.id ?? "No model"}
                {engine && (
                  <span className={`ls-row ls-gap-1 ls-small ${engine.running ? "ls-accent" : engine.loading ? "ls-warn" : "ls-muted"}`}>
                    <Dot tone={engine.running ? "ok" : engine.loading ? "warn" : "neutral"} />
                    {engine.running ? "running" : engine.loading ? "loading" : "stopped"}
                  </span>
                )}
              </div>
              <div className="ls-break ls-small ls-muted">{engine?.detail}</div>
              {model && (
                <div className="ls-mt-2">
                  <KeyValue
                    rows={[
                      ["backend", model.backend],
                      ["context", `${model.context_len.toLocaleString()} tokens`],
                      ["tool calls", model.supports_tools ? "native" : "through the grammar"],
                    ]}
                  />
                </div>
              )}
              {engine?.running && (
                <Button className="ls-mt-3" onClick={() => void unloadModel()}>
                  <StopIcon size={14} /> Stop
                </Button>
              )}
            </div>
          </div>
          {engineLog.length > 0 && (
            <pre className="code ls-mt-3" style={{ maxHeight: "14rem" }}>
              {engineLog.join("\n")}
            </pre>
          )}
        </Card>

        <Card className="ls-pad-4">
          <SectionTitle>This machine</SectionTitle>
          <KeyValue
            rows={[
              ["hardware", environment?.machine ?? "…"],
              ["profile", environment?.profile ?? "…"],
              ["Tier B", environment?.tier_b_permitted ? "permitted" : "off"],
            ]}
          />
          <p className="ls-mt-3 ls-small ls-faint">
            The sidecar is <span className="ls-mono">llama-server</span>, found under the data directory's <span className="ls-mono">engines/</span>, on
            PATH, or by <span className="ls-mono">--llama-server</span>.
          </p>
        </Card>

        <Card className="ls-pad-4">
          <SectionTitle>Or an endpoint</SectionTitle>
          <p className="ls-mb-3 ls-small ls-muted">Any OpenAI-compatible server already running: tool calls go through Core's grammar either way.</p>
          <Input mono className="ls-mb-2" value={endpoint} onChange={(e) => setEndpoint(e.target.value)} aria-label="Endpoint" />
          <Input
            mono
            className="ls-mb-3"
            placeholder="model name as the server knows it"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void connect()}
            aria-label="Model name"
          />
          <Button kind="primary" onClick={() => void connect()} disabled={busy || !name.trim()}>
            {busy ? "Connecting…" : "Use this endpoint"}
          </Button>
          {models.length > 0 && (
            <ul className="ls-list ls-divided ls-mt-3 ls-small">
              {models.map((m) => (
                <li key={m.id} className="ls-row ls-gap-2" style={{ padding: "6px 0" }}>
                  <span className="ls-grow ls-truncate ls-mono">{m.id}</span>
                  <span className="ls-muted">{m.backend}</span>
                </li>
              ))}
            </ul>
          )}
        </Card>
      </div>
    </div>
  );
}

function CatalogRow({ entry, onDownload, onLoad, onUnload }: { entry: ModelCatalogEntry; onDownload: () => void; onLoad: () => void; onUnload: () => void }) {
  const gb = (entry.bytes / 1e9).toFixed(entry.bytes < 5e9 ? 1 : 0);
  const verdictTone = entry.verdict === "resident" ? "ok" : entry.verdict === "does not fit" ? "danger" : entry.verdict === "unknown" ? "neutral" : "warn";
  const downloading = entry.download && entry.download.stage.startsWith("downloading");
  const failed = entry.download && entry.download.stage.startsWith("failed");
  const percent = entry.download && entry.download.total_bytes > 0 ? Math.min(100, (100 * entry.download.done_bytes) / entry.download.total_bytes) : 0;

  return (
    <li style={{ padding: "12px 0" }}>
      <div className="ls-row ls-start ls-gap-3">
        <div className="ls-grow">
          <div className="ls-row ls-wrap ls-gap-2">
            <span className="ls-medium">{entry.title}</span>
            <Pill tone={verdictTone}>{entry.verdict}</Pill>
            {entry.loaded && <Pill tone="ok">loaded</Pill>}
            {entry.installed && !entry.loaded && <Pill tone="neutral">downloaded</Pill>}
          </div>
          <div className="ls-small ls-muted">
            {entry.family} · {entry.params_b}B{entry.active_params_b !== entry.params_b ? ` (${entry.active_params_b}B active)` : ""} · {entry.quant} · about {gb} GB ·{" "}
            {entry.context_len.toLocaleString()} context · {entry.license}
          </div>
          <div className="ls-small ls-muted" title={entry.plan_notes.join("\n")}>
            {entry.verdict === "unknown"
              ? entry.plan_summary
              : entry.verdict === "does not fit"
                ? (entry.plan_notes[0] ?? entry.plan_summary)
                : `about ${entry.estimated_tok_s.toFixed(0)} tokens/s, first token ${(entry.first_token_ms / 1000).toFixed(1)} s`}
          </div>
          {entry.notes && <div className="ls-small ls-faint">{entry.notes}</div>}
          {downloading && entry.download && (
            <div className="ls-mt-2">
              <div className="progress">
                <div style={{ width: `${percent}%` }} />
              </div>
              <div className="ls-mt-1 ls-small ls-muted">
                {(entry.download.done_bytes / 1e9).toFixed(2)} of {(entry.download.total_bytes / 1e9).toFixed(2)} GB · {entry.download.stage}
              </div>
            </div>
          )}
          {failed && entry.download && <div className="ls-mt-1 ls-small ls-danger">{entry.download.stage}</div>}
        </div>
        <div className="ls-col ls-gap-2 ls-shrink0">
          {entry.loaded ? (
            <Button onClick={onUnload}>
              <StopIcon size={14} /> Stop
            </Button>
          ) : entry.installed ? (
            <Button kind="primary" onClick={onLoad} disabled={entry.verdict === "does not fit"}>
              <PlayIcon size={14} /> Load
            </Button>
          ) : downloading ? (
            <Button disabled>
              <SpinnerIcon size={14} className="ls-spin" /> {percent.toFixed(0)}%
            </Button>
          ) : (
            <Button onClick={onDownload} disabled={entry.source === "import"}>
              <DownloadIcon size={14} /> Download
            </Button>
          )}
        </div>
      </div>
    </li>
  );
}
