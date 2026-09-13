// Application state (v2.1 §6.1: the own store for the app, Automerge for content).
// Everything here is what Core last said, kept current by its events; every
// action is one request to Core, which decides.

import { createStore } from "@localspace/ui";
import { ApiError, call, pick } from "./api/client";
import type { AuthMode, Me } from "./api/client";
import type {
  AccessLevel,
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
  Invite,
  Json,
  ModelCatalogEntry,
  ModelInfo,
  NetworkMode,
  NoticeLevel,
  SurfaceKind,
  Task,
  ToolOutcome,
  UserInfo,
  UserRole,
  ViewDesc,
  WorkspaceInfo,
} from "./api/generated";
import { bus } from "./surfaces/bus";

/** An open view of a harness: the board page shows one at a time and keeps the rest mounted. */
export type Panel = {
  key: string;
  harness: string;
  view: string;
  title: string;
  kind: SurfaceKind;
  /** The zoom the shell last asked a `web` view for; 1 is 100%. */
  zoom: number;
};

/** v2 §6.1: the shell keeps at most this many views open. */
export const MAX_PANELS = 6;

export type Page = "chat" | "board" | "documents" | "store" | "settings" | "admin" | "help";
export type SettingsPane = "general" | "assistant" | "network" | "tools" | "about" | "advanced";
export type AdminPane = "people" | "workspaces";

export type Notice = { level: NoticeLevel; text: string; at: number };
export type LiveToolCall = {
  id: string;
  tool: string;
  params: Json;
  outcome: ToolOutcome | null;
  at: number;
};
export type Approval = { id: string; kind: ApprovalKind; prompt: string };
export type InstallPrompt = { harness: string; token: string; diff: string[]; native_reason: string | null };

export type Session = {
  me: Me | null;
  authMode: AuthMode | null;
  page: Page;
  settingsPane: SettingsPane;
  adminPane: AdminPane;
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
  /** The harness whose view the board page shows. */
  board: string | null;
  panels: Panel[];
  /** The chat drawer beside the board. */
  chatBeside: boolean;
  /** Writes from a frame that Core has not answered yet; zero means everything is saved. */
  pendingWrites: number;
  railCollapsed: boolean;
  /** On the board the rail is icons unless opened for this visit. */
  boardRailOpen: boolean;
  users: UserInfo[];
  workspaces: WorkspaceInfo[];

  signIn: (me: Me) => void;
  signOut: () => void;
  setAuthMode: (mode: AuthMode | null) => void;
  go: (page: Page) => void;
  goSettings: (pane: SettingsPane) => void;
  goAdmin: (pane: AdminPane) => void;
  setLive: (live: boolean) => void;
  notify: (level: NoticeLevel, text: string) => void;
  traceLine: (text: string) => void;
  onEvent: (event: Event) => void;

  openBoard: (harness: string) => void;
  closeBoard: (harness: string) => void;
  toggleChatBeside: () => void;
  noteWrite: (delta: number) => void;
  setZoom: (key: string, zoom: number) => void;
  /** A surface saying what it shows, so the top bar stays true. */
  reportZoom: (key: string, zoom: number) => void;
  setRail: (collapsed: boolean) => Promise<void>;
  loadPreferences: () => Promise<void>;

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
  /** Install a package; when it widens what a harness may do, the prompt comes back for the user to answer. */
  install: (path: string) => Promise<InstallPrompt | null>;
  approveInstall: (harness: string, token: string) => Promise<void>;
  uninstall: (harness: string) => Promise<void>;
  callTool: (tool: string, params: Json) => Promise<ToolOutcome | null>;
  findCapability: (need: string) => Promise<CapabilityHit[]>;
  docJson: (harness: string) => Promise<Json | null>;

  refreshUsers: () => Promise<void>;
  createUser: (email: string, name: string, roles: UserRole[]) => Promise<Invite | null>;
  setUserRoles: (user: string, roles: UserRole[]) => Promise<void>;
  disableUser: (user: string, disabled: boolean) => Promise<void>;
  resetPassword: (user: string) => Promise<Invite | null>;
  unlockUser: (user: string) => Promise<void>;
  revokeSessions: (user: string) => Promise<void>;
  refreshWorkspaces: () => Promise<void>;
  createWorkspace: (name: string) => Promise<void>;
  setMember: (workspace: string, user: string, level: AccessLevel) => Promise<void>;
  removeMember: (workspace: string, user: string) => Promise<void>;
  selectWorkspace: (workspace: string, reason?: string) => Promise<boolean>;
};

const KEEP_NOTICES = 50;
const KEEP_TRACE = 300;

/** The state that is one user's, cleared when they sign out. */
const EMPTY = {
  environment: null,
  notices: [] as Notice[],
  transcript: [] as ChatMessage[],
  streaming: "",
  busy: false,
  liveCalls: [] as LiveToolCall[],
  approvals: [] as Approval[],
  trace: [] as string[],
  task: null,
  active: null,
  history: [] as Commit[],
  catalog: [] as CatalogEntry[],
  models: [] as ModelInfo[],
  catalogModels: [] as ModelCatalogEntry[],
  engineLog: [] as string[],
  context: null,
  conversations: [] as ConversationSummary[],
  currentConversation: "",
  board: null,
  panels: [] as Panel[],
  chatBeside: false,
  pendingWrites: 0,
  users: [] as UserInfo[],
  workspaces: [] as WorkspaceInfo[],
};

export const useSession = createStore<Session>((set, get) => {
  /** Run a request; a failure becomes a notice rather than an unhandled rejection. */
  const attempt = async <T>(work: () => Promise<T>): Promise<T | undefined> => {
    try {
      return await work();
    } catch (err) {
      const text = err instanceof ApiError ? err.message : "The server could not be reached.";
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

  const takeUsers = (response: Awaited<ReturnType<typeof call>>) => {
    const users = pick(response, "users");
    if (users) set({ users });
  };

  const takeWorkspaces = (response: Awaited<ReturnType<typeof call>>) => {
    const workspaces = pick(response, "workspaces");
    if (workspaces) set({ workspaces });
  };

  return {
    me: null,
    authMode: null,
    page: "chat",
    settingsPane: "general",
    adminPane: "people",
    live: false,
    railCollapsed: false,
    boardRailOpen: false,
    ...EMPTY,

    signIn: (me) => set({ me }),
    signOut: () => set({ me: null, live: false, page: "chat", ...EMPTY }),
    setAuthMode: (authMode) => set({ authMode }),
    go: (page) => set({ page }),
    goSettings: (pane) => set({ page: "settings", settingsPane: pane }),
    goAdmin: (pane) => set({ page: "admin", adminPane: pane }),
    setLive: (live) => set({ live }),
    notify: (level, text) =>
      set((s) => ({ notices: [...s.notices.slice(1 - KEEP_NOTICES), { level, text, at: Date.now() }] })),
    traceLine: (text) => set((s) => ({ trace: [...s.trace.slice(1 - KEEP_TRACE), text] })),

    openBoard: (harness) => {
      const summary = get().environment?.harnesses.find((h) => h.id === harness);
      const view: ViewDesc | undefined = summary?.views.find((v) => v.kind === "web") ?? summary?.views[0];
      if (!summary || !view) {
        get().notify("warn", `${summary?.title ?? harness} has no page to open.`);
        return;
      }
      const key = `${harness}/${view.id}`;
      const { panels } = get();
      if (!panels.some((p) => p.key === key)) {
        if (panels.length >= MAX_PANELS) {
          get().notify("warn", `At most ${MAX_PANELS} boards can be open; close one first.`);
          return;
        }
        set({ panels: [...panels, { key, harness, view: view.id, title: view.title, kind: view.kind, zoom: 1 }] });
      }
      set({ board: harness, page: "board" });
      // The open surface is the one the agent works on (spec §4: focus).
      if (get().environment?.focus !== harness) void get().setFocus(harness);
    },
    closeBoard: (harness) =>
      set((s) => {
        const panels = s.panels.filter((p) => p.harness !== harness);
        const board = s.board === harness ? (panels[panels.length - 1]?.harness ?? null) : s.board;
        return { panels, board, page: board ? s.page : "chat" };
      }),
    toggleChatBeside: () => set((s) => ({ chatBeside: !s.chatBeside })),
    noteWrite: (delta) => set((s) => ({ pendingWrites: Math.max(0, s.pendingWrites + delta) })),
    setZoom: (key, zoom) => {
      const value = Math.min(4, Math.max(0.25, Math.round(zoom * 100) / 100));
      set((s) => ({ panels: s.panels.map((p) => (p.key === key ? { ...p, zoom: value } : p)) }));
      const panel = get().panels.find((p) => p.key === key);
      if (panel) bus.emit("command", { harness: panel.harness, view: panel.view, name: "zoom", args: { value } });
    },
    reportZoom: (key, zoom) =>
      set((s) => ({
        panels: s.panels.map((p) => (p.key === key && Math.abs(p.zoom - zoom) > 0.001 ? { ...p, zoom } : p)),
      })),
    setRail: async (collapsed) => {
      // On the board the rail is icons by default; the toggle there opens it
      // for this visit only, and the remembered choice is left alone.
      if (get().page === "board") {
        set({ boardRailOpen: !collapsed });
        return;
      }
      set({ railCollapsed: collapsed });
      await attempt(() => call({ set_preference: { key: "rail_collapsed", value: collapsed ? "true" : "false" } }));
    },
    loadPreferences: async () => {
      await attempt(async () => {
        const preferences = pick(await call("get_preferences"), "preferences");
        if (preferences) set({ railCollapsed: preferences.values["rail_collapsed"] === "true" });
      });
    },

    onEvent: (event) => {
      if (event === "assistant_done") {
        set({ streaming: "" });
        void get().refreshTranscript();
        return;
      }
      if (typeof event === "string") {
        if (event === "environment_outdated") void get().refreshEnvironment();
        return;
      }
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
      } else if ("doc_patch" in event) bus.emit("doc_patch", event.doc_patch);
      else if ("doc_changed" in event) bus.emit("doc_changed", event.doc_changed);
      else if ("harness_message" in event) bus.emit("harness_message", event.harness_message);
      else if ("widget_view_changed" in event) bus.emit("widget_view_changed", event.widget_view_changed);
      else if ("notice" in event) get().notify(event.notice.level, event.notice.text);
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
        // The engine moved: loading, ready, stopped. The model behind
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
      void get().refreshConversations();
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
        if (c) set({ conversations: c.list, currentConversation: c.current, transcript: [], streaming: "", liveCalls: [], task: null, page: "chat" });
      });
    },
    selectConversation: async (id) => {
      await attempt(async () => {
        const c = pick(await call({ select_conversation: { id } }), "conversations");
        if (c) set({ conversations: c.list, currentConversation: c.current, streaming: "", liveCalls: [], page: "chat" });
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
      const prompt = await attempt(async () => {
        const response = await call({ install_harness: { path } });
        takeEnvironment(response);
        return pick(response, "install_prompt");
      });
      await get().refreshEnvironment();
      await get().refreshCatalog();
      return prompt ?? null;
    },
    approveInstall: async (harness, token) => {
      await attempt(() => call({ approve_install: { harness, token } }));
      await get().refreshEnvironment();
      await get().refreshCatalog();
    },
    uninstall: async (harness) => {
      await attempt(async () => takeEnvironment(await call({ uninstall_harness: { harness } })));
      get().closeBoard(harness);
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

    refreshUsers: async () => {
      await attempt(async () => takeUsers(await call("list_users")));
    },
    createUser: async (email, name, roles) => {
      const invite = await attempt(async () => pick(await call({ create_user: { email, name, roles } }), "invite"));
      await get().refreshUsers();
      return invite ?? null;
    },
    setUserRoles: async (user, roles) => {
      await attempt(async () => takeUsers(await call({ set_user_roles: { user, roles } })));
    },
    disableUser: async (user, disabled) => {
      await attempt(async () => takeUsers(await call({ disable_user: { user, disabled } })));
    },
    resetPassword: async (user) => {
      const invite = await attempt(async () => pick(await call({ reset_password: { user } }), "invite"));
      await get().refreshUsers();
      return invite ?? null;
    },
    unlockUser: async (user) => {
      await attempt(async () => takeUsers(await call({ unlock_user: { user } })));
    },
    revokeSessions: async (user) => {
      await attempt(() => call({ revoke_sessions: { user } }));
      await get().refreshUsers();
    },
    refreshWorkspaces: async () => {
      await attempt(async () => takeWorkspaces(await call("list_workspaces")));
    },
    createWorkspace: async (name) => {
      await attempt(async () => takeWorkspaces(await call({ create_workspace: { name } })));
    },
    setMember: async (workspace, user, level) => {
      await attempt(async () => takeWorkspaces(await call({ set_member: { workspace, principal: { user }, level } })));
    },
    removeMember: async (workspace, user) => {
      await attempt(async () => takeWorkspaces(await call({ remove_member: { workspace, principal: { user } } })));
    },
    selectWorkspace: async (workspace, reason) => {
      const ok = await attempt(async () => {
        takeEnvironment(await call({ select_workspace: { workspace, reason: reason ?? null } }));
        return true;
      });
      if (ok) {
        set({ transcript: [], streaming: "", liveCalls: [], panels: [], board: null, page: "chat" });
        await get().refreshConversations();
        await get().refreshTranscript();
        await get().refreshWorkspaces();
      }
      return ok === true;
    },
  };
});

export function outcomeLine(outcome: ToolOutcome): string {
  if ("ok" in outcome) return outcome.ok.diff_summary || "done";
  if ("denied" in outcome) return `not allowed: ${outcome.denied.reason}`;
  if ("awaiting_confirm" in outcome) return `waiting for your answer: ${outcome.awaiting_confirm.prompt}`;
  if ("error" in outcome) return `did not work: ${outcome.error.message}`;
  return `queued as ${outcome.queued.job}`;
}

/** The initials shown for a person: two letters from their name, or their email. */
export function initialsOf(name: string | null | undefined, fallback = "?"): string {
  const parts = (name ?? "")
    .replace(/@.*$/, "")
    .split(/[\s._-]+/)
    .filter(Boolean);
  const letters = parts
    .slice(0, 2)
    .map((w) => w[0]?.toUpperCase() ?? "")
    .join("");
  return letters || fallback;
}
