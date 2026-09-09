// Models: what is loaded, and connecting an OpenAI-compatible endpoint such
// as llama-server. The supervised sidecar and the catalog with download are
// build step 2; until then the endpoint is whatever is already running.

import { useEffect, useState } from "react";
import { Boxes } from "lucide-react";
import { useSession } from "../store";
import { Button, Card, Dot, KeyValue, SectionTitle } from "../components/ui";

export function ModelsPage() {
  const { environment, models, refreshModels, selectModel } = useSession();
  const [endpoint, setEndpoint] = useState("http://localhost:8080/v1");
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    void refreshModels();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const connect = async () => {
    if (!name.trim()) return;
    setBusy(true);
    await selectModel(`${endpoint.trim().replace(/\/$/, "")}|${name.trim()}`);
    setBusy(false);
  };

  const model = environment?.model ?? null;

  return (
    <div className="grid min-h-0 flex-1 grid-cols-[1fr_1fr] gap-4 overflow-y-auto px-6 pb-6">
      <div className="flex flex-col gap-4">
        <Card className="p-5">
          <SectionTitle>Active model</SectionTitle>
          {model ? (
            <div className="flex items-start gap-3">
              <span className="rounded-lg bg-page p-2 text-muted">
                <Boxes size={20} />
              </span>
              <div className="flex-1">
                <div className="flex items-center gap-2 font-medium">
                  {model.id}
                  <span className="flex items-center gap-1 text-xs text-accent">
                    <Dot tone="ok" /> {model.loaded ? "running" : "selected"}
                  </span>
                </div>
                <KeyValue
                  rows={[
                    ["backend", model.backend],
                    ["context", `${model.context_len.toLocaleString()} tokens`],
                    ["tool calls", model.supports_tools ? "native" : "through the grammar"],
                    ["vision", model.supports_vision ? "yes" : "no"],
                  ]}
                />
              </div>
            </div>
          ) : (
            <p className="text-sm text-muted">
              No model is loaded. {environment?.engine.detail ?? ""}
            </p>
          )}
        </Card>

        <Card className="p-5">
          <SectionTitle>Connect an endpoint</SectionTitle>
          <p className="mb-3 text-xs text-muted">
            Any OpenAI-compatible server: <span className="font-mono">llama-server</span>, LM Studio,
            Ollama's compatibility endpoint. Tool calls go through Core's grammar either way.
          </p>
          <label className="block text-xs text-muted">Endpoint</label>
          <input
            className="mb-2 mt-1 w-full rounded-lg border border-line px-3 py-1.5 font-mono text-xs outline-none focus:border-accent"
            value={endpoint}
            onChange={(e) => setEndpoint(e.target.value)}
          />
          <label className="block text-xs text-muted">Model name as the server knows it</label>
          <input
            className="mb-3 mt-1 w-full rounded-lg border border-line px-3 py-1.5 font-mono text-xs outline-none focus:border-accent"
            placeholder="e.g. qwen2.5-32b-instruct-q4_k_m"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void connect()}
          />
          <Button kind="primary" onClick={() => void connect()} disabled={busy || !name.trim()}>
            {busy ? "Connecting…" : "Use this model"}
          </Button>
        </Card>
      </div>

      <div className="flex flex-col gap-4">
        <Card className="p-5">
          <SectionTitle>This machine</SectionTitle>
          <KeyValue
            rows={[
              ["hardware", environment?.machine ?? "…"],
              ["profile", environment?.profile ?? "…"],
              ["engine", environment?.engine.running ? environment.engine.detail : "no local engine resident"],
              ["Tier B", environment?.tier_b_permitted ? "permitted" : "off"],
            ]}
          />
          <p className="mt-3 text-xs text-faint">
            The supervised llama.cpp sidecar, the placement planner's plan as its flags, and the
            model catalog with one-click download are build step 2 of architecture v2.
          </p>
        </Card>

        <Card className="p-5">
          <SectionTitle>Known models</SectionTitle>
          {models.length === 0 ? (
            <p className="text-sm text-muted">None configured yet.</p>
          ) : (
            <ul className="divide-y divide-line text-sm">
              {models.map((m) => (
                <li key={m.id} className="flex items-center gap-2 py-2">
                  <span className="flex-1 font-mono text-xs">{m.id}</span>
                  <span className="text-xs text-muted">{m.backend}</span>
                  <Button onClick={() => void selectModel(m.id)}>Use</Button>
                </li>
              ))}
            </ul>
          )}
        </Card>
      </div>
    </div>
  );
}
