// Settings (screen 6): General, Assistant, Network, Tools, About — and
// Advanced, one collapsed row at the bottom, where the machinery lives:
// connection details, diagnostics and logs. In an organisation a member
// cannot open Advanced at all.

import { useEffect, useState } from "react";
import { ChevronRightIcon, InfoIcon, Switch } from "@localspace/ui";
import type { ModelCatalogEntry, NetworkMode } from "../api/generated";
import { bytesLabel, call, downloadDocument, logout, pick } from "../api/client";
import { outcomeLine, useSession } from "../store";
import type { SettingsPane } from "../store";
import { TopBar } from "../components/TopBar";
import { modelLabel, modelReason, sizeInWords, willNotFit } from "../lib/models";
import { clock } from "../lib/time";
import type { Json, ToolOutcome } from "../api/generated";

const PANES: Array<{ id: SettingsPane; label: string }> = [
  { id: "general", label: "General" },
  { id: "assistant", label: "Assistant" },
  { id: "network", label: "Network" },
  { id: "tools", label: "Tools" },
  { id: "about", label: "About" },
];

export function SettingsPage() {
  const { me, settingsPane, goSettings } = useSession();
  const mayOpenAdvanced = me?.topology === "personal" || (me?.roles.includes("admin") ?? false);
  return (
    <>
      <TopBar />
      <div className="settings">
        <nav className="subnav" aria-label="Settings">
          <div className="subnav-title">Settings</div>
          <div className="ls-col" style={{ gap: 2 }}>
            {PANES.map((p) => (
              <button key={p.id} type="button" className={`sub${settingsPane === p.id ? " on" : ""}`} onClick={() => goSettings(p.id)}>
                {p.label}
              </button>
            ))}
          </div>
          <div className="subnav-sep">
            <button
              type="button"
              className={`sub quiet${settingsPane === "advanced" ? " on" : ""}`}
              onClick={() => goSettings("advanced")}
              disabled={!mayOpenAdvanced}
              title={mayOpenAdvanced ? undefined : "Only administrators can open Advanced."}
            >
              Advanced
            </button>
          </div>
        </nav>
        <div className="pane">
          <div className="pane-inner">
            {settingsPane === "general" && <GeneralPane />}
            {settingsPane === "assistant" && <AssistantPane />}
            {settingsPane === "network" && <NetworkPane />}
            {settingsPane === "tools" && <ToolsPane />}
            {settingsPane === "about" && <AboutPane />}
            {settingsPane === "advanced" && (mayOpenAdvanced ? <AdvancedPane /> : <p className="ls-muted">Only administrators can open Advanced.</p>)}
            {settingsPane !== "advanced" && <AdvancedRow enabled={mayOpenAdvanced} />}
          </div>
        </div>
      </div>
    </>
  );
}

function AdvancedRow({ enabled }: { enabled: boolean }) {
  const goSettings = useSession((s) => s.goSettings);
  return (
    <button type="button" className="advanced-row" onClick={() => goSettings("advanced")} disabled={!enabled} title={enabled ? undefined : "Only administrators can open Advanced."}>
      <span>
        <div className="t">Advanced</div>
        <div className="b">Connection details, diagnostics and logs. You do not need these for everyday use.</div>
      </span>
      <ChevronRightIcon size={19} className="ls-faint" />
    </button>
  );
}

function roleLabel(roles: string[] | undefined): string {
  if (!roles) return "";
  if (roles.includes("admin")) return "Administrator";
  if (roles.includes("member")) return "Member";
  if (roles.includes("viewer")) return "Can view only";
  return "";
}

function GeneralPane() {
  const { me, environment, workspaces, signOut } = useSession();
  const organisation = me?.topology === "organisation";
  const workspace = workspaces.find((w) => w.id === environment?.workspace_id);
  return (
    <>
      <h1 className="pane-title">General</h1>
      <div className="pane-sub">Your account and where you are working.</div>
      <div className="row-list">
        <div className="row-item">
          <div className="row-main">
            <div className="row-title">{me?.name || me?.user}</div>
            <div className="row-body">
              {organisation ? me?.email : "Signed in on this computer"}
              {organisation && roleLabel(me?.roles) ? ` · ${roleLabel(me?.roles)}` : ""}
            </div>
          </div>
          <button type="button" className="btn" onClick={() => void logout().then(signOut)}>
            Sign out
          </button>
        </div>
        <div className="row-item">
          <div className="row-main">
            <div className="row-title">{environment ? (workspace?.personal_to ? "Your personal workspace" : environment.workspace) : "…"}</div>
            <div className="row-body">
              {organisation
                ? workspace && !workspace.personal_to
                  ? `Shared with ${workspace.members.length - 1 > 0 ? `${workspace.members.length - 1} other ${workspace.members.length - 1 === 1 ? "person" : "people"}` : "nobody else yet"}. What you write here, they see.`
                  : "Only you see what is in it."
                : "Everything here stays on this computer."}
            </div>
          </div>
        </div>
      </div>
    </>
  );
}

function AssistantPane() {
  const { me, environment, catalogModels, refreshModelCatalog, loadModel, downloadModel } = useSession();
  useEffect(() => {
    void refreshModelCatalog();
  }, [refreshModelCatalog]);
  const organisation = me?.topology === "organisation";
  const admin = organisation ? (me?.roles.includes("admin") ?? false) : true;
  const current = environment?.model?.id ?? null;
  const here = catalogModels.filter((m) => m.installed);
  const known = here.some((m) => m.id === current);
  const where = organisation ? "your organisation's server" : "this computer";

  return (
    <>
      <h1 className="pane-title">Assistant</h1>
      <div className="pane-sub">Choose how the assistant works. All of these run on {where}.</div>
      {here.length === 0 && !current ? (
        <p className="ls-muted" style={{ marginTop: 26 }}>
          {admin ? `No model is on ${where} yet.` : "Your administrator has not set up a model yet."}
        </p>
      ) : (
        <div className="options">
          {current && !known && (
            <div className="opt on" role="radio" aria-checked="true">
              <span className="radio on" />
              <span style={{ flex: 1 }}>
                <span className="opt-title">
                  {modelLabel(current, catalogModels)}
                  <span className="pill green">In use</span>
                </span>
                <div className="opt-body">A model on a server you connected under Advanced.</div>
              </span>
            </div>
          )}
          {here.map((m) => {
            const on = m.id === current;
            return (
              <button key={m.id} type="button" className={`opt${on ? " on" : ""}`} role="radio" aria-checked={on} onClick={() => !on && void loadModel(m.id)} disabled={willNotFit(m)}>
                <span className={`radio${on ? " on" : ""}`} />
                <span style={{ flex: 1 }}>
                  <span className="opt-title">
                    {m.title}
                    {on && <span className="pill green">In use</span>}
                    {!on && environment?.engine.loading && environment.engine.model === m.id && <span className="pill amber">Starting up…</span>}
                  </span>
                  <div className="opt-body">{modelReason(m)}</div>
                </span>
              </button>
            );
          })}
        </div>
      )}
      <div className="note">
        <InfoIcon size={17} className="ls-muted" style={{ flexShrink: 0, marginTop: 1 }} />
        <div>{organisation ? "Your administrator decides which of these are available. Changing this affects only you." : "Changing this affects only this computer."}</div>
      </div>
      {admin && <GetAModel entries={catalogModels.filter((m) => !m.installed)} onDownload={(id) => void downloadModel(id)} />}
    </>
  );
}

/** The models that can be brought here, for whoever may bring them. */
function GetAModel({ entries, onDownload }: { entries: ModelCatalogEntry[]; onDownload: (id: string) => void }) {
  const [open, setOpen] = useState(false);
  const environment = useSession((s) => s.environment);
  if (entries.length === 0) return null;
  const offline = environment?.network === "airgapped";
  return (
    <div style={{ marginTop: 26 }}>
      <button type="button" className="link green" onClick={() => setOpen((v) => !v)} aria-expanded={open} style={{ fontSize: 14 }}>
        {open ? "Hide the models that can be added" : "Get another model"}
      </button>
      {open && (
        <div className="row-list" style={{ marginTop: 12 }}>
          {offline && <div className="row-item ls-muted">Downloads need the network. This server is offline, so a model is brought in as a file by your administrator.</div>}
          {entries.map((m) => {
            const downloading = m.download && m.download.stage.startsWith("downloading");
            const failed = m.download && m.download.stage.startsWith("failed");
            const percent = m.download && m.download.total_bytes > 0 ? Math.min(100, (100 * m.download.done_bytes) / m.download.total_bytes) : 0;
            return (
              <div key={m.id} className="row-item">
                <div className="row-main">
                  <div className="row-title">{m.title}</div>
                  <div className="row-body">
                    {modelReason(m)} About {sizeInWords(m.bytes)} to download.
                  </div>
                  {downloading && m.download && (
                    <div style={{ marginTop: 8 }}>
                      <div className="progress">
                        <div style={{ width: `${percent}%` }} />
                      </div>
                      <div className="row-body">{percent.toFixed(0)}% downloaded</div>
                    </div>
                  )}
                  {failed && <div className="error">The download did not finish. Try again.</div>}
                </div>
                <button type="button" className="btn" onClick={() => onDownload(m.id)} disabled={!!downloading || offline || willNotFit(m)}>
                  {downloading ? "Downloading…" : willNotFit(m) ? "Too large" : "Download"}
                </button>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

const MODES: Array<{ mode: NetworkMode; title: string; body: string }> = [
  { mode: "airgapped", title: "Offline", body: "Nothing leaves your organisation's server. The assistant answers from what it has." },
  { mode: "ask", title: "Online, asks first", body: "The assistant asks you before each request to the internet." },
  { mode: "online", title: "Online", body: "The assistant may reach the internet when it needs to." },
];

function rank(mode: NetworkMode): number {
  return mode === "airgapped" ? 0 : mode === "ask" ? 1 : 2;
}

function NetworkPane() {
  const { me, environment, setNetwork } = useSession();
  const ceiling = environment?.network_ceiling ?? "airgapped";
  const organisation = me?.topology === "organisation";
  // The mode is the server's, one for everyone: the administrator sets it.
  const admin = !organisation || (me?.roles.includes("admin") ?? false);
  return (
    <>
      <h1 className="pane-title">Network</h1>
      <div className="pane-sub">{admin ? "Whether the assistant may reach the internet." : "Whether the assistant may reach the internet. Your administrator sets this for the whole server."}</div>
      <div className="options">
        {MODES.map((m) => {
          const allowed = rank(m.mode) <= rank(ceiling);
          const on = environment?.network === m.mode;
          return (
            <button key={m.mode} type="button" className={`opt${on ? " on" : ""}`} role="radio" aria-checked={on} disabled={!allowed || !admin} onClick={() => void setNetwork(m.mode)} title={!admin ? "Only administrators change this." : allowed ? undefined : "Your administrator has not allowed this."}>
              <span className={`radio${on ? " on" : ""}`} />
              <span style={{ flex: 1 }}>
                <span className="opt-title">{m.title}</span>
                <div className="opt-body">{organisation ? m.body : m.body.replace("your organisation's server", "this computer")}</div>
              </span>
            </button>
          );
        })}
      </div>
      {organisation && ceiling !== "online" && (
        <div className="note">
          <InfoIcon size={17} className="ls-muted" style={{ flexShrink: 0, marginTop: 1 }} />
          <div>Your administrator has set the limit at “{MODES.find((m) => m.mode === ceiling)?.title}”.</div>
        </div>
      )}
    </>
  );
}

function ToolsPane() {
  const { environment, setEnabled, go } = useSession();
  const harnesses = environment?.harnesses ?? [];
  return (
    <>
      <h1 className="pane-title">Tools</h1>
      <div className="pane-sub">What the assistant can work with here. Turn one off and the assistant stops using it.</div>
      {harnesses.length === 0 ? (
        <p className="ls-muted" style={{ marginTop: 26 }}>
          No tools are installed yet.{" "}
          <button type="button" className="link green" style={{ fontSize: 14 }} onClick={() => go("store")}>
            Add one from the Store.
          </button>
        </p>
      ) : (
        <div className="row-list">
          {harnesses.map((h) => (
            <div key={h.id} className="row-item">
              <div className="row-main">
                <div className="row-title">{h.title}</div>
                <div className="row-body">
                  {h.tool_count} things it can do{h.degraded ? ` · ${h.degraded}` : ""}
                </div>
              </div>
              <Switch checked={h.enabled} label={`${h.title} on`} onChange={(on) => void setEnabled(h.id, on)} />
            </div>
          ))}
        </div>
      )}
    </>
  );
}

function AboutPane() {
  const { me, environment } = useSession();
  const organisation = me?.topology === "organisation";
  return (
    <>
      <h1 className="pane-title">About</h1>
      <div className="pane-sub">{organisation ? "localSpace runs on your organisation's own server. Nothing you type leaves your network." : "localSpace runs on this computer. Nothing you type leaves it unless you allow the network."}</div>
      <div className="row-list">
        <div className="row-item">
          <div className="row-main">
            <div className="row-title">Version</div>
            <div className="row-body">{me?.version ?? "…"}</div>
          </div>
        </div>
        <div className="row-item">
          <div className="row-main">
            <div className="row-title">{organisation ? "The server" : "This computer"}</div>
            <div className="row-body">{environment?.machine ?? "…"}</div>
          </div>
        </div>
        <div className="row-item">
          <div className="row-main">
            <div className="row-title">Typeface</div>
            <div className="row-body">Figtree, under the SIL Open Font License, served from this app.</div>
          </div>
        </div>
      </div>
    </>
  );
}

// ---------------------------------------------------------------------------
// Advanced: the machinery, for those who run it
// ---------------------------------------------------------------------------

function Section({ title, children, defaultOpen }: { title: string; children: React.ReactNode; defaultOpen?: boolean }) {
  const [open, setOpen] = useState(defaultOpen ?? false);
  return (
    <div className="row-list">
      <button type="button" className="row-item" style={{ width: "100%", border: 0, background: "transparent", font: "inherit", textAlign: "left", cursor: "pointer" }} onClick={() => setOpen((v) => !v)} aria-expanded={open}>
        <span className="row-title" style={{ flex: 1 }}>
          {title}
        </span>
        <ChevronRightIcon size={16} className="ls-faint" style={{ transform: open ? "rotate(90deg)" : undefined }} />
      </button>
      {open && <div style={{ padding: "0 16px 16px" }}>{children}</div>}
    </div>
  );
}

function AdvancedPane() {
  const { me, environment } = useSession();
  return (
    <>
      <h1 className="pane-title">Advanced</h1>
      <div className="pane-sub">Connection details, diagnostics and logs. Everything here is what the server reports about itself.</div>
      {me?.topology === "personal" && <EndpointSection />}
      <EngineSection />
      <DiagnosticsSection />
      <HistorySection />
      <RunToolSection />
      <p className="ls-small ls-faint" style={{ marginTop: 22 }}>
        Model profile: {environment?.profile ?? "…"}. The API this app speaks is described at{" "}
        <a className="ls-accent" href="/api/v1/openapi.json" target="_blank" rel="noreferrer">
          /api/v1/openapi.json
        </a>
        .
      </p>
    </>
  );
}

function EndpointSection() {
  const { models, selectModel } = useSession();
  const [endpoint, setEndpoint] = useState("http://localhost:8080/v1");
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const connect = async () => {
    if (!name.trim()) return;
    setBusy(true);
    await selectModel(`${endpoint.trim().replace(/\/$/, "")}|${name.trim()}`);
    setBusy(false);
  };
  return (
    <Section title="Connect to a model server">
      <p className="ls-small ls-muted" style={{ marginTop: 0 }}>
        Any OpenAI-compatible server already running, on this computer or elsewhere.
      </p>
      <label className="field">
        <span>Address</span>
        <input className="input mono" value={endpoint} onChange={(e) => setEndpoint(e.target.value)} />
      </label>
      <label className="field">
        <span>Model name, as that server knows it</span>
        <input className="input mono" value={name} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void connect()} />
      </label>
      <button type="button" className="btn solid" style={{ marginTop: 16 }} onClick={() => void connect()} disabled={busy || !name.trim()}>
        {busy ? "Connecting…" : "Use this server"}
      </button>
      {models.length > 0 && (
        <ul className="ls-list ls-small ls-muted" style={{ marginTop: 12 }}>
          {models.map((m) => (
            <li key={m.id} className="ls-mono">
              {m.id} · {m.backend}
            </li>
          ))}
        </ul>
      )}
    </Section>
  );
}

function EngineSection() {
  const { environment, engineLog, refreshEngineLog, unloadModel } = useSession();
  useEffect(() => {
    void refreshEngineLog();
  }, [refreshEngineLog]);
  const engine = environment?.engine;
  const model = environment?.model;
  return (
    <Section title="Engine" defaultOpen>
      <div className="ls-small">
        {engine?.running ? "Running" : engine?.loading ? "Starting up" : "Stopped"}
        {engine?.model ? ` · ${engine.model}` : ""}
      </div>
      {engine?.detail && <div className="ls-small ls-muted ls-break" style={{ marginTop: 4 }}>{engine.detail}</div>}
      {model && (
        <div className="ls-small ls-muted" style={{ marginTop: 4 }}>
          {model.backend} · {model.context_len.toLocaleString()} context · tool calls {model.supports_tools ? "native" : "through the grammar"}
        </div>
      )}
      {engine?.running && (
        <button type="button" className="btn" style={{ marginTop: 12 }} onClick={() => void unloadModel()}>
          Stop the engine
        </button>
      )}
      <div className="ls-row ls-between" style={{ marginTop: 16 }}>
        <span className="ls-small ls-muted">Engine log</span>
        <button type="button" className="link" onClick={() => void refreshEngineLog()}>
          Refresh
        </button>
      </div>
      <pre className="code" style={{ marginTop: 6 }}>
        {engineLog.length > 0 ? engineLog.join("\n") : "Nothing logged yet."}
      </pre>
    </Section>
  );
}

function DiagnosticsSection() {
  const { active, task, trace, context, refreshActive, refreshTask, previewContext, notify } = useSession();
  const [budget, setBudget] = useState(600);
  useEffect(() => {
    void refreshActive();
    void refreshTask();
  }, [refreshActive, refreshTask]);
  useEffect(() => {
    void previewContext(budget);
  }, [budget, previewContext]);
  return (
    <Section title="This turn: tools, the task ledger, the prompt, the trace">
      <div className="ls-small">
        {active ? `${active.tools.length} tools in context, about ${active.token_estimate} of ${active.budget} tokens${active.dropped.length ? ` · dropped for budget: ${active.dropped.join(", ")}` : ""}` : "…"}
      </div>
      {active && (
        <ul className="ls-list ls-small ls-muted" style={{ marginTop: 6, columns: 2 }}>
          {active.tools.map((t) => (
            <li key={t.name} className="ls-mono">
              {t.name} <span style={{ fontFamily: "var(--ls-font-sans)" }}>· {t.reason.replace("_", " ")}</span>
            </li>
          ))}
        </ul>
      )}
      <div className="ls-small" style={{ marginTop: 14, fontWeight: 600 }}>
        Task ledger
      </div>
      {task && (task.goal || task.plan.length > 0 || task.artifacts.length > 0) ? (
        <div className="ls-small">
          <div className="ls-muted">{task.goal || "no goal recorded"}</div>
          {task.plan.length > 0 && (
            <ol style={{ margin: "6px 0", paddingLeft: 18 }}>
              {task.plan.map((s, i) => (
                <li key={i}>
                  <span className="ls-mono ls-muted">{s.harness}</span> {s.intent} <span className="ls-faint">· {s.status}</span>
                </li>
              ))}
            </ol>
          )}
          {task.artifacts.length > 0 && (
            <ul className="ls-list" style={{ marginTop: 6 }}>
              {task.artifacts.map((a) => (
                <li key={a.id} className="ls-row ls-gap-2 ls-wrap" style={{ padding: "3px 0" }}>
                  <span className="ls-mono ls-accent">{a.id}</span>
                  <span className="ls-mono ls-muted">{a.kind}</span>
                  <span>{a.summary}</span>
                  {a.file && (
                    <button type="button" className="link green" onClick={() => void downloadDocument(a.doc, a.file?.name ?? "file").catch((err: unknown) => notify("error", err instanceof Error ? err.message : String(err)))}>
                      Download {a.file.name} ({bytesLabel(a.file.bytes)})
                    </button>
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>
      ) : (
        <div className="ls-small ls-muted">No run yet.</div>
      )}
      <div className="ls-row ls-between" style={{ marginTop: 14 }}>
        <span className="ls-small" style={{ fontWeight: 600 }}>
          What the model will see
        </span>
        <label className="ls-small ls-muted">
          budget{" "}
          <select value={budget} onChange={(e) => setBudget(Number(e.target.value))}>
            {[300, 600, 1500, 4000].map((b) => (
              <option key={b} value={b}>
                {b} tokens
              </option>
            ))}
          </select>
        </label>
      </div>
      <pre className="code tall" style={{ marginTop: 6 }}>
        {context ? context.prompt : "…"}
      </pre>
      <div className="ls-small" style={{ marginTop: 14, fontWeight: 600 }}>
        Trace
      </div>
      <pre className="code" style={{ marginTop: 6 }}>
        {trace.length > 0 ? trace.slice(-100).join("\n") : "Tool calls and the server's notes appear here as they happen."}
      </pre>
    </Section>
  );
}

function HistorySection() {
  const { history, refreshHistory, undo, redo, dropRun } = useSession();
  const [lock, setLock] = useState<Json | null>(null);
  useEffect(() => {
    void refreshHistory();
    void call("get_lock").then((r) => setLock(pick(r, "lock")?.json ?? null));
  }, [refreshHistory]);
  return (
    <Section title="History and the environment lock">
      <div className="ls-row ls-gap-2" style={{ marginBottom: 10 }}>
        <button type="button" className="btn" onClick={() => void undo()}>
          Undo
        </button>
        <button type="button" className="btn" onClick={() => void redo()}>
          Redo
        </button>
      </div>
      {history.length === 0 ? (
        <div className="ls-small ls-muted">Nothing has been written yet.</div>
      ) : (
        <table className="table" style={{ fontSize: 13 }}>
          <thead>
            <tr>
              <th>When</th>
              <th>Where</th>
              <th>Change</th>
              <th>By</th>
              <th>Run</th>
            </tr>
          </thead>
          <tbody>
            {history.slice(0, 60).map((c) => (
              <tr key={c.id}>
                <td className="ls-muted" style={{ padding: "6px 8px 6px 0", fontSize: 12.5 }}>
                  {clock(c.at_ms)}
                </td>
                <td style={{ padding: "6px 8px 6px 0", fontSize: 12.5 }}>{c.harness}</td>
                <td style={{ padding: "6px 8px 6px 0", fontSize: 12.5 }}>
                  {c.diff_summary || c.tool}
                </td>
                <td style={{ padding: "6px 8px 6px 0", fontSize: 12.5 }}>{c.author}</td>
                <td style={{ padding: "6px 0", fontSize: 12.5 }}>
                  {c.run ? (
                    <button type="button" className="link" style={{ color: "var(--ls-danger)" }} onClick={() => void dropRun(c.run as string)} title="Drop this whole run">
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
      <div className="ls-small" style={{ marginTop: 14, fontWeight: 600 }}>
        Environment lock
      </div>
      <pre className="code" style={{ marginTop: 6 }}>
        {lock ? JSON.stringify(lock, null, 2) : "…"}
      </pre>
    </Section>
  );
}

function RunToolSection() {
  const { callTool, active } = useSession();
  const [tool, setTool] = useState("");
  const [params, setParams] = useState("{}");
  const [outcome, setOutcome] = useState<ToolOutcome | string | null>(null);
  const run = async () => {
    let parsed: unknown;
    try {
      parsed = JSON.parse(params || "{}");
    } catch {
      setOutcome("the parameters are not valid JSON");
      return;
    }
    setOutcome(await callTool(tool.trim(), parsed as never));
  };
  return (
    <Section title="Run a tool by hand">
      <p className="ls-small ls-muted" style={{ marginTop: 0 }}>
        The same door as the assistant: permission check, confirmation, a commit in the history.
      </p>
      <label className="field">
        <span>Tool</span>
        <input className="input mono" list="tool-names" value={tool} onChange={(e) => setTool(e.target.value)} placeholder="canvas.add_sticky" />
        <datalist id="tool-names">{active?.tools.map((t) => <option key={t.name} value={t.name} />)}</datalist>
      </label>
      <label className="field">
        <span>Parameters (JSON)</span>
        <textarea className="input mono" rows={3} style={{ height: "auto", padding: 10 }} value={params} onChange={(e) => setParams(e.target.value)} />
      </label>
      <button type="button" className="btn solid" style={{ marginTop: 16 }} onClick={() => void run()} disabled={!tool.trim()}>
        Run
      </button>
      {outcome && (
        <pre className="code" style={{ marginTop: 12 }}>
          {typeof outcome === "string" ? outcome : `${outcomeLine(outcome)}${"ok" in outcome ? `\n${JSON.stringify(outcome.ok.result, null, 2)}` : ""}`}
        </pre>
      )}
    </Section>
  );
}
