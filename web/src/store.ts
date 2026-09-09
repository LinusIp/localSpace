// Application state (v2 §6.1: Zustand for the app, Automerge for content).
// Everything here is what Core last said, kept current by its events; every
// action is one request to Core, which decides.

import { create } from "zustand";
import { ApiError, call, pick } from "./api/client";
import type { Me } from "./api/client";
import type {
  ActiveSet,
  ApprovalKind,
  CapabilityHit,
  CatalogEntry,
  ChatMessage,
  Commit,
  ContextBlock,
  ConversationSummary,
  EnvironmentState,
  Event,
  Json,
  ModelCatalogEntry,
  ModelInfo,
  NetworkMode,
  NoticeLevel,
  Task,
  ToolOutcome,
} from "./api/generated";

export type Page =
  | "chat"
  | "agents"
  | "tools"
  | "models"
  | "data"
  | "history"
  | "library"
  | "settings"
  | "help";

export type Notice = { level: NoticeLevel; text: string; at: number };
export type LiveToolCall = {
  id: string;
  tool: string;
  params: Json;
  outcome: ToolOutcome | null;
  at: number;
};
export type Approval = { id: string; kind: ApprovalKind; prompt: string };

export type Session = {
  me: Me | null;
  page: Page;
  environment: EnvironmentState | null;
  live: boolean;
  notices: Notice[];
  transcript: ChatMessage[];
  /** The assistant's message in progress, from `assistant_delta` events. */
  streaming: string;
  busy: boolean;
  liveCalls: LiveToolCall[];
  approvals: Approval[];
  trace: string[];
  task: Task | null;
  active: ActiveSet | null;
  history: Commit[];
  catalog: CatalogEntry[];
  models: ModelInfo[];
  catalogModels: ModelCatalogEntry[];
  engineLog: string[];
  context: { blocks: ContextBlock[]; prompt: string } | null;
  conversations: ConversationSummary[];
  currentConversation: string;

  signIn: (me: Me) => void;
  signOut: () => void;
  go: (page: Page) => void;
  setLive: (live: boolean) => void;
  notify: (level: NoticeLevel, text: string) => void;
  onEvent: (event: Event) => void;

  refreshEnvironment: () => Promise<void>;
  refreshTranscript: () => Promise<void>;
  send: (text: string) => Promise<void>;
  cancel: () => Promise<void>;
  approve: (id: string, granted: boolean) => Promise<void>;
  refreshTask: () => Promise<void>;
  refreshActive: () => Promise<void>;
  refreshHistory: () => Promise<void>;
  refreshCatalog: () => Promise<void>;
  refreshModels: () => Promise<void>;
  refreshModelCatalog: () => Promise<void>;
  refreshConversations: () => Promise<void>;
  newConversation: () => Promise<void>;
  selectConversation: (id: string) => Promise<void>;
  deleteConversation: (id: string) => Promise<void>;
  refreshEngineLog: () => Promise<void>;
  downloadModel: (id: string) => Promise<void>;
  loadModel: (id: string) => Promise<void>;
  unloadModel: () => Promise<void>;
  importModel: (path: string) => Promise<void>;
  previewContext: (budget: number) => Promise<void>;
  selectModel: (id: string) => Promise<void>;
  setNetwork: (mode: NetworkMode) => Promise<void>;
  setEnabled: (harness: string, enabled: boolean) => Promise<void>;
  setPinned: (harness: string, pinned: boolean) => Promise<void>;
  setFocus: (harness: string | null) => Promise<void>;
  undo: () => Promise<void>;
  redo: () => Promise<void>;
  dropRun: (run: string) => Promise<void>;
  install: (path: string) => Promise<void>;
  approveInstall: (harness: string, token: string) => Promise<void>;
  uninstall: (harness: string) => Promise<void>;
  callTool: (tool: string, params: Json) => Promise<ToolOutcome | null>;
  findCapability: (need: string) => Promise<CapabilityHit[]>;
  docJson: (harness: string) => Promise<Json | null>;
};

const KEEP_NOTICES = 50;
const KEEP_TRACE = 300;

export const useSession = create<Session>((set, get) => {
  /** Run a request; a failure becomes a notice rather than an unhandled rejection. */
  const attempt = async <T>(work: () => Promise<T>): Promise<T | undefined> => {
    try {
      return await work();
    } catch (err) {
      const text = err instanceof ApiError ? err.message : "the server could not be reached";
      get().notify("error", text);
      return undefined;
    }
  };

  const takeEnvironment = (response: Awaited<ReturnType<typeof call>>) => {
    const environment = pick(response, "environment");
    if (environment) set({ environment });
  };

  const takeModelCatalog = (response: Awaited<ReturnType<typeof call>>) => {
    const catalog = pick(response, "model_catalog");
    if (catalog) set({ catalogModels: catalog.entries });
  };

  return {
    me: null,
    page: "chat",
    environment: null,
    live: false,
    notices: [],
    transcript: [],
    streaming: "",
    busy: false,
    liveCalls: [],
    approvals: [],
    trace: [],
    task: null,
    active: null,
    history: [],
    catalog: [],
    models: [],
    catalogModels: [],
    engineLog: [],
    context: null,
    conversations: [],
    currentConversation: "",

    signIn: (me) => set({ me }),
    signOut: () =>
      set({
        me: null,
        environment: null,
        notices: [],
        transcript: [],
        streaming: "",
        busy: false,
        liveCalls: [],
        approvals: [],
        trace: [],
        task: null,
        active: null,
        history: [],
        catalog: [],
        models: [],
        catalogModels: [],
        engineLog: [],
        context: null,
        conversations: [],
        currentConversation: "",
        live: false,
      }),
    go: (page) => set({ page }),
    setLive: (live) => set({ live }),
    notify: (level, text) =>
      set((s) => ({ notices: [...s.notices.slice(1 - KEEP_NOTICES), { level, text, at: Date.now() }] })),

    onEvent: (event) => {
      if (event === "assistant_done") {
        set({ streaming: "" });
        void get().refreshTranscript();
        return;
      }
      if (typeof event === "string") return;
      if ("environment_changed" in event) set({ environment: event.environment_changed });
      else if ("assistant_delta" in event)
        set((s) => ({ streaming: s.streaming + event.assistant_delta.text }));
      else if ("tool_call_started" in event) {
        const { id, tool, params } = event.tool_call_started;
        set((s) => ({
          liveCalls: [...s.liveCalls, { id, tool, params, outcome: null, at: Date.now() }],
          trace: [...s.trace.slice(1 - KEEP_TRACE), `→ ${tool}`],
        }));
      } else if ("tool_call_finished" in event) {
        const { id, tool, outcome } = event.tool_call_finished;
        set((s) => ({
          liveCalls: s.liveCalls.map((c) => (c.id === id ? { ...c, outcome } : c)),
          trace: [...s.trace.slice(1 - KEEP_TRACE), `← ${tool}: ${outcomeLine(outcome)}`],
        }));
      } else if ("notice" in event) get().notify(event.notice.level, event.notice.text);
      else if ("trace_line" in event)
        set((s) => ({ trace: [...s.trace.slice(1 - KEEP_TRACE), event.trace_line.text] }));
      else if ("task_changed" in event) set({ task: event.task_changed });
      else if ("approval_request" in event)
        set((s) => ({ approvals: [...s.approvals, event.approval_request] }));
      else if ("model_progress" in event) {
        // A download moved: update the one entry, and refresh the catalog when
        // it finished so "installed" comes from Core, not from arithmetic.
        const { id, done_bytes, total_bytes, stage } = event.model_progress;
        set((s) => ({
          catalogModels: s.catalogModels.map((m) =>
            m.id === id ? { ...m, download: { done_bytes, total_bytes, stage } } : m,
          ),
        }));
        if (stage === "done" || stage.startsWith("failed")) void get().refreshModelCatalog();
      } else if ("conversation_changed" in event) {
        // This or another client switched, created or deleted one.
        set({ currentConversation: event.conversation_changed.current, streaming: "", liveCalls: [] });
        void get().refreshConversations();
        void get().refreshTranscript();
      } else if ("engine_changed" in event) {
        // The sidecar moved: loading, ready, crashed. The model behind
        // `environment.model` follows, so the environment is refreshed too.
        const engine = event.engine_changed;
        set((s) => (s.environment ? { environment: { ...s.environment, engine } } : {}));
        void get().refreshEnvironment();
        void get().refreshModelCatalog();
        if (engine.running) void get().refreshModels();
      }
    },

    refreshEnvironment: async () => {
      await attempt(async () => takeEnvironment(await call("get_environment")));
    },
    refreshTranscript: async () => {
      await attempt(async () => {
        const transcript = pick(await call("get_transcript"), "transcript");
        if (transcript) set({ transcript: transcript.messages });
      });
    },
    send: async (text) => {
      set((s) => ({
        busy: true,
        streaming: "",
        liveCalls: [],
        transcript: [...s.transcript, { role: "user", content: text, tool_calls: [] }],
      }));
      await attempt(async () => {
        const transcript = pick(await call({ send_message: { text } }), "transcript");
        if (transcript) set({ transcript: transcript.messages });
      });
      set({ busy: false, streaming: "" });
      void get().refreshTask();
      void get().refreshHistory();
    },
    cancel: async () => {
      await attempt(() => call("cancel_turn"));
    },
    approve: async (id, granted) => {
      set((s) => ({ approvals: s.approvals.filter((a) => a.id !== id) }));
      await attempt(() => call({ approve: { id, granted } }));
    },
    refreshTask: async () => {
      await attempt(async () => {
        const task = pick(await call("get_task"), "task");
        if (task) set({ task });
      });
    },
    refreshActive: async () => {
      await attempt(async () => {
        const active = pick(await call("get_active_set"), "active");
        if (active) set({ active });
      });
    },
    refreshHistory: async () => {
      await attempt(async () => {
        const history = pick(await call({ get_history: { limit: 200 } }), "history");
        if (history) set({ history: history.commits });
      });
    },
    refreshCatalog: async () => {
      await attempt(async () => {
        const catalog = pick(await call("list_catalog"), "catalog");
        if (catalog) set({ catalog: catalog.entries });
      });
    },
    refreshModels: async () => {
      await attempt(async () => {
        const models = pick(await call("list_models"), "models");
        if (models) set({ models: models.models });
      });
    },
    refreshConversations: async () => {
      await attempt(async () => {
        const c = pick(await call("list_conversations"), "conversations");
        if (c) set({ conversations: c.list, currentConversation: c.current });
      });
    },
    newConversation: async () => {
      await attempt(async () => {
        const c = pick(await call("new_conversation"), "conversations");
        if (c) set({ conversations: c.list, currentConversation: c.current, transcript: [], streaming: "", liveCalls: [], task: null });
      });
    },
    selectConversation: async (id) => {
      await attempt(async () => {
        const c = pick(await call({ select_conversation: { id } }), "conversations");
        if (c) set({ conversations: c.list, currentConversation: c.current, streaming: "", liveCalls: [] });
      });
      await get().refreshTranscript();
    },
    deleteConversation: async (id) => {
      await attempt(async () => {
        const c = pick(await call({ delete_conversation: { id } }), "conversations");
        if (c) set({ conversations: c.list, currentConversation: c.current, streaming: "", liveCalls: [] });
      });
      await get().refreshTranscript();
    },
    refreshModelCatalog: async () => {
      await attempt(async () => takeModelCatalog(await call("list_model_catalog")));
    },
    refreshEngineLog: async () => {
      await attempt(async () => {
        const log = pick(await call({ engine_log: { lines: 60 } }), "engine_log");
        if (log) set({ engineLog: log.lines });
      });
    },
    downloadModel: async (id) => {
      await attempt(async () => takeModelCatalog(await call({ download_model: { id } })));
    },
    loadModel: async (id) => {
      await attempt(() => call({ load_model: { id } }));
      await get().refreshEnvironment();
      await get().refreshModelCatalog();
    },
    unloadModel: async () => {
      await attempt(() => call("unload_model"));
      await get().refreshEnvironment();
      await get().refreshModelCatalog();
      await get().refreshModels();
    },
    importModel: async (path) => {
      await attempt(async () => takeModelCatalog(await call({ import_model: { path } })));
    },
    previewContext: async (budget) => {
      await attempt(async () => {
        const context = pick(await call({ preview_context: { budget } }), "context");
        if (context) set({ context: { blocks: context.blocks, prompt: context.prompt_preview } });
      });
    },
    selectModel: async (id) => {
      await attempt(async () => {
        takeEnvironment(await call({ select_model: { id } }));
        await get().refreshEnvironment();
        await get().refreshModels();
      });
    },
    setNetwork: async (mode) => {
      await attempt(async () => takeEnvironment(await call({ set_network_mode: { mode } })));
    },
    setEnabled: async (harness, enabled) => {
      await attempt(async () => takeEnvironment(await call({ set_harness_enabled: { harness, enabled } })));
      void get().refreshActive();
    },
    setPinned: async (harness, pinned) => {
      await attempt(async () => takeEnvironment(await call({ set_pinned: { harness, pinned } })));
      void get().refreshActive();
    },
    setFocus: async (harness) => {
      await attempt(async () => takeEnvironment(await call({ set_focus: { harness } })));
      void get().refreshActive();
    },
    undo: async () => {
      await attempt(() => call("undo"));
      void get().refreshHistory();
    },
    redo: async () => {
      await attempt(() => call("redo"));
      void get().refreshHistory();
    },
    dropRun: async (run) => {
      await attempt(() => call({ drop_run: { run } }));
      void get().refreshHistory();
    },
    install: async (path) => {
      await attempt(async () => {
        const response = await call({ install_harness: { path } });
        const prompt = pick(response, "install_prompt");
        if (prompt) {
          // Capabilities widened: Core wants a second word. The Library shows
          // the diff and asks; the token is what ties the answer to the ask.
          const lines = prompt.diff.join("\n");
          const reason = prompt.native_reason ? `\n\nNative tier: ${prompt.native_reason}` : "";
          if (window.confirm(`${prompt.harness} asks for:\n${lines}${reason}\n\nInstall?`)) {
            await call({ approve_install: { harness: prompt.harness, token: prompt.token } });
          }
        }
        takeEnvironment(response);
      });
      await get().refreshEnvironment();
      await get().refreshCatalog();
    },
    approveInstall: async (harness, token) => {
      await attempt(() => call({ approve_install: { harness, token } }));
      await get().refreshEnvironment();
      await get().refreshCatalog();
    },
    uninstall: async (harness) => {
      await attempt(async () => takeEnvironment(await call({ uninstall_harness: { harness } })));
      await get().refreshEnvironment();
      await get().refreshCatalog();
    },
    callTool: async (tool, params) => {
      const outcome = await attempt(async () => pick(await call({ call_tool: { tool, params } }), "tool_result"));
      void get().refreshHistory();
      return outcome ?? null;
    },
    findCapability: async (need) => {
      const hits = await attempt(async () => pick(await call({ find_capability: { need } }), "capabilities"));
      return hits?.hits ?? [];
    },
    docJson: async (harness) => {
      const doc = await attempt(async () => pick(await call({ get_doc_json: { harness } }), "doc_json"));
      return doc?.json ?? null;
    },
  };
});

export function outcomeLine(outcome: ToolOutcome): string {
  if ("ok" in outcome) return outcome.ok.diff_summary || "ok";
  if ("denied" in outcome) return `denied: ${outcome.denied.reason}`;
  if ("awaiting_confirm" in outcome) return `waiting for confirmation: ${outcome.awaiting_confirm.prompt}`;
  if ("error" in outcome) return `error: ${outcome.error.message}`;
  return `queued as ${outcome.queued.job}`;
}
