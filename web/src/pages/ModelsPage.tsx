// Models (architecture v2 §4): the catalog with the placement planner's
// verdict for this machine, downloads with their progress, the sidecar with
// its state and log, a file the user already has, and — still — any
// OpenAI-compatible endpoint.

import { useEffect, useState } from "react";
import { Boxes, Download, FileInput, Play, RefreshCw, Square } from "lucide-react";
import type { ModelCatalogEntry } from "../api/generated";
import { useSession } from "../store";
import { Button, Card, Dot, KeyValue, Pill, SectionTitle } from "../components/ui";

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
    <div className="grid min-h-0 flex-1 grid-cols-[3fr_2fr] gap-4 overflow-y-auto px-6 pb-6">
      <div className="flex flex-col gap-4">
        <Card className="p-5">
          <SectionTitle action="Refresh" onAction={() => void refreshModelCatalog()}>
            Catalog
          </SectionTitle>
          <p className="mb-3 text-xs text-muted">
            Each entry carries the placement planner's verdict for this machine and its estimate
            before anything is downloaded. Downloads come from Hugging Face and go into the data
            directory; they are refused in an air-gapped environment, where a file is imported instead.
          </p>
          {catalogModels.length === 0 ? (
            <p className="text-sm text-muted">Loading the catalog…</p>
          ) : (
            <ul className="divide-y divide-line">
              {catalogModels.map((m) => (
                <CatalogRow
                  key={m.id}
                  entry={m}
                  onDownload={() => void downloadModel(m.id)}
                  onLoad={() => void loadModel(m.id)}
                  onUnload={() => void unloadModel()}
                />
              ))}
            </ul>
          )}
        </Card>

        <Card className="p-5">
          <SectionTitle>Import a file</SectionTitle>
          <p className="mb-2 text-xs text-muted">
            A GGUF you already have joins the catalog where it is. Without a tensor map the planner
            sizes it from the file, as a dense model.
          </p>
          <div className="flex gap-2">
            <input
              className="flex-1 rounded-lg border border-line px-3 py-1.5 font-mono text-xs outline-none focus:border-accent"
              placeholder="C:\\models\\my-model.gguf"
              value={importPath}
              onChange={(e) => setImportPath(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && importPath.trim() && void importModel(importPath.trim())}
            />
            <Button onClick={() => void importModel(importPath.trim())} disabled={!importPath.trim()}>
              <span className="flex items-center gap-1">
                <FileInput size={14} /> Import
              </span>
            </Button>
          </div>
        </Card>
      </div>

      <div className="flex flex-col gap-4">
        <Card className="p-5">
          <SectionTitle action="Log" onAction={() => void refreshEngineLog()}>
            Engine
          </SectionTitle>
          <div className="flex items-start gap-3">
            <span className="rounded-lg bg-page p-2 text-muted">
              <Boxes size={20} />
            </span>
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2 font-medium">
                {engine?.model ?? model?.id ?? "No model"}
                {engine && (
                  <span className={`flex items-center gap-1 text-xs ${engine.running ? "text-accent" : engine.loading ? "text-warn" : "text-muted"}`}>
                    <Dot tone={engine.running ? "ok" : engine.loading ? "warn" : "neutral"} />
                    {engine.running ? "running" : engine.loading ? "loading" : "stopped"}
                  </span>
                )}
              </div>
              <div className="break-words text-xs text-muted">{engine?.detail}</div>
              {model && (
                <div className="mt-2">
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
                <Button className="mt-3" onClick={() => void unloadModel()}>
                  <span className="flex items-center gap-1">
                    <Square size={14} /> Stop
                  </span>
                </Button>
              )}
            </div>
          </div>
          {engineLog.length > 0 && (
            <pre className="mt-3 max-h-56 overflow-auto rounded-lg bg-page p-3 font-mono text-[11px] leading-relaxed">
              {engineLog.join("\n")}
            </pre>
          )}
        </Card>

        <Card className="p-5">
          <SectionTitle>This machine</SectionTitle>
          <KeyValue
            rows={[
              ["hardware", environment?.machine ?? "…"],
              ["profile", environment?.profile ?? "…"],
              ["Tier B", environment?.tier_b_permitted ? "permitted" : "off"],
            ]}
          />
          <p className="mt-3 text-xs text-faint">
            The sidecar is <span className="font-mono">llama-server</span>, found under the data
            directory's <span className="font-mono">engines/</span>, on PATH, or by{" "}
            <span className="font-mono">--llama-server</span>.
          </p>
        </Card>

        <Card className="p-5">
          <SectionTitle>Or an endpoint</SectionTitle>
          <p className="mb-3 text-xs text-muted">
            Any OpenAI-compatible server already running: tool calls go through Core's grammar
            either way.
          </p>
          <input
            className="mb-2 w-full rounded-lg border border-line px-3 py-1.5 font-mono text-xs outline-none focus:border-accent"
            value={endpoint}
            onChange={(e) => setEndpoint(e.target.value)}
            aria-label="Endpoint"
          />
          <input
            className="mb-3 w-full rounded-lg border border-line px-3 py-1.5 font-mono text-xs outline-none focus:border-accent"
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
            <ul className="mt-3 divide-y divide-line text-xs">
              {models.map((m) => (
                <li key={m.id} className="flex items-center gap-2 py-1.5">
                  <span className="flex-1 truncate font-mono">{m.id}</span>
                  <span className="text-muted">{m.backend}</span>
                </li>
              ))}
            </ul>
          )}
        </Card>
      </div>
    </div>
  );
}

function CatalogRow({
  entry,
  onDownload,
  onLoad,
  onUnload,
}: {
  entry: ModelCatalogEntry;
  onDownload: () => void;
  onLoad: () => void;
  onUnload: () => void;
}) {
  const gb = (entry.bytes / 1e9).toFixed(entry.bytes < 5e9 ? 1 : 0);
  const verdictTone =
    entry.verdict === "resident" ? "ok" : entry.verdict === "does not fit" ? "danger" : entry.verdict === "unknown" ? "neutral" : "warn";
  const downloading = entry.download && entry.download.stage.startsWith("downloading");
  const failed = entry.download && entry.download.stage.startsWith("failed");
  const percent = entry.download && entry.download.total_bytes > 0 ? Math.min(100, (100 * entry.download.done_bytes) / entry.download.total_bytes) : 0;

  return (
    <li className="py-3">
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="font-medium">{entry.title}</span>
            <Pill tone={verdictTone as "ok" | "warn" | "danger" | "neutral"}>{entry.verdict}</Pill>
            {entry.loaded && <Pill tone="ok">loaded</Pill>}
            {entry.installed && !entry.loaded && <Pill tone="neutral">downloaded</Pill>}
          </div>
          <div className="text-xs text-muted">
            {entry.family} · {entry.params_b}B{entry.active_params_b !== entry.params_b ? ` (${entry.active_params_b}B active)` : ""} ·{" "}
            {entry.quant} · about {gb} GB · {entry.context_len.toLocaleString()} context · {entry.license}
          </div>
          <div className="text-xs text-muted" title={entry.plan_notes.join("\n")}>
            {entry.verdict === "unknown"
              ? entry.plan_summary
              : entry.verdict === "does not fit"
                ? entry.plan_notes[0] ?? entry.plan_summary
                : `about ${entry.estimated_tok_s.toFixed(0)} tokens/s, first token ${(entry.first_token_ms / 1000).toFixed(1)} s`}
          </div>
          {entry.notes && <div className="mt-0.5 text-xs text-faint">{entry.notes}</div>}
          {downloading && (
            <div className="mt-2">
              <div className="h-1.5 w-full overflow-hidden rounded-full bg-line">
                <div className="h-full bg-accent transition-[width]" style={{ width: `${percent}%` }} />
              </div>
              <div className="mt-1 text-xs text-muted">
                {(entry.download!.done_bytes / 1e9).toFixed(2)} of {(entry.download!.total_bytes / 1e9).toFixed(2)} GB ·{" "}
                {entry.download!.stage}
              </div>
            </div>
          )}
          {failed && <div className="mt-1 text-xs text-danger">{entry.download!.stage}</div>}
        </div>
        <div className="flex shrink-0 flex-col gap-2">
          {entry.loaded ? (
            <Button onClick={onUnload}>
              <span className="flex items-center gap-1">
                <Square size={14} /> Stop
              </span>
            </Button>
          ) : entry.installed ? (
            <Button kind="primary" onClick={onLoad} disabled={entry.verdict === "does not fit"}>
              <span className="flex items-center gap-1">
                <Play size={14} /> Load
              </span>
            </Button>
          ) : downloading ? (
            <Button disabled>
              <span className="flex items-center gap-1">
                <RefreshCw size={14} className="animate-spin" /> {percent.toFixed(0)}%
              </span>
            </Button>
          ) : (
            <Button onClick={onDownload} disabled={entry.source === "import"}>
              <span className="flex items-center gap-1">
                <Download size={14} /> Download
              </span>
            </Button>
          )}
        </div>
      </div>
    </li>
  );
}
