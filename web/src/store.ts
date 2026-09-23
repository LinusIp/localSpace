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
  Computer,
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
  Present,
  SurfaceKind,
  Task,
  ToolOutcome,
  TurnState,
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

/** This window's name in Core's presence: two windows of one person are two announcements. */
const PEER = Array.from(crypto.getRandomValues(new Uint8Array(8)), (b) => b.toString(16).padStart(2, "0")).join("");

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
  /** Each chat's answer in progress, from `assistant_delta` events. */
  streaming: Record<string, string>;
  /** The chats whose answers are being written, or wait, and where they
   *  stand. An answer that ended is not here. */
  turns: Record<string, TurnState>;
  /** Each chat's tool calls at work for its answer in progress. */
  liveCalls: Record<string, LiveToolCall[]>;
  /** The chats whose stopped answer is being carried on: its words join it. */
  continuing: Record<string, boolean>;
  approvals: Approval[];
  trace: string[];
  task: Task | null;
  active: ActiveSet | null;
  history: Commit[];
  catalog: CatalogEntry[];
  models: ModelInfo[];
  catalogModels: ModelCatalogEntry[];
  /** What this computer is, in plain words; asked once, on a person's own computer. */
  computer: Computer | null;
  /** The question about the computer was answered, or failed: the window stops waiting for it. */
  computerAsked: boolean;
  /** The first run was finished or put off in this window. */
  firstRunLeft: boolean;
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
  /** Who has each board open, from Core, for the boards this window was told about. */
  present: Record<string, Present[]>;

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
  /** Tell Core which board this window shows, or that it left; the people on it hear. */
  announcePresence: (board: string | null) => Promise<void>;

  refreshEnvironment: () => Promise<void>;
  refreshTranscript: () => Promise<void>;
  /** A message in the chat on screen; its answer comes by events. */
  send: (text: string) => Promise<void>;
  /** Stop the answer in the chat on screen. */
  cancel: () => Promise<void>;
  /** Carry on the stopped answer in the chat on screen. */
  continueAnswer: () => Promise<void>;
  refreshTurns: () => Promise<void>;
  approve: (id: string, granted: boolean) => Promise<void>;
  refreshTask: () => Promise<void>;
  refreshActive: () => Promise<void>;
  refreshHistory: () => Promise<void>;
  refreshCatalog: () => Promise<void>;
  refreshModels: () => Promise<void>;
  refreshModelCatalog: () => Promise<void>;
  refreshComputer: () => Promise<void>;
  leaveFirstRun: () => void;
  refreshConversations: () => Promise<void>;
  newConversation: () => Promise<void>;
  selectConversation: (id: string) => Promise<void>;
  deleteConversation: (id: string) => Promise<void>;
  refreshEngineLog: () => Promise<void>;
  downloadModel: (id: string) => Promise<void>;
  /** Stop a download, or take one out of the line; what came is kept. */
  stopDownload: (id: string) => Promise<void>;
  /** Remove a model's files from this computer. */
  deleteModel: (id: string) => Promise<void>;
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

/** An answer that is over: its chat has nothing in progress any more. */
const ENDED: TurnState[] = ["done", "stopped", "cut"];

/** A copy of `map` without `key`. */
function without<T>(map: Record<string, T>, key: string): Record<string, T> {
  const next = { ...map };
  delete next[key];
  return next;
}

/** The state that is one user's, cleared when they sign out. */
const EMPTY = {
  environment: null,
  notices: [] as Notice[],
  transcript: [] as ChatMessage[],
  streaming: {} as Record<string, string>,
  turns: {} as Record<string, TurnState>,
  liveCalls: {} as Record<string, LiveToolCall[]>,
  continuing: {} as Record<string, boolean>,
  approvals: [] as Approval[],
  trace: [] as string[],
  task: null,
  active: null,
  history: [] as Commit[],
  catalog: [] as CatalogEntry[],
  models: [] as ModelInfo[],
  catalogModels: [] as ModelCatalogEntry[],
  computer: null as Computer | null,
  computerAsked: false,
  firstRunLeft: false,
  engineLog: [] as string[],
  context: null,
  conversations: [] as ConversationSummary[],
  currentConversation: "",
  board: null,
  panels: [] as Panel[],
  present: {} as Record<string, Present[]>,
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
    setLive: (live) => {
      const was = get().live;
      set({ live });
      // The events sent while the stream was down are lost: ask where the
      // answers stand, and read the chat on screen again.
      if (live && !was) {
        void get().refreshTurns();
        void get().refreshTranscript();
      }
    },
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
    announcePresence: async (board) => {
      // Quietly: a window announces every twenty seconds, and a server that
      // is away for one of them is back for the next.
      try {
        await call({ presence: { board, peer: PEER } });
      } catch {
        // the next announcement tries again
      }
    },
    loadPreferences: async () => {
      await attempt(async () => {
        const preferences = pick(await call("get_preferences"), "preferences");
        if (preferences) set({ railCollapsed: preferences.values["rail_collapsed"] === "true" });
      });
    },

    onEvent: (event) => {
      if (typeof event === "string") {
        if (event === "environment_outdated") void get().refreshEnvironment();
        return;
      }
      if ("environment_changed" in event) set({ environment: event.environment_changed });
      else if ("assistant_delta" in event) {
        const { conversation, text } = event.assistant_delta;
        set((s) => ({ streaming: { ...s.streaming, [conversation]: (s.streaming[conversation] ?? "") + text } }));
      } else if ("assistant_done" in event) {
        const chat = event.assistant_done.conversation;
        const clear = () =>
          set((s) => ({ streaming: without(s.streaming, chat), liveCalls: without(s.liveCalls, chat), continuing: without(s.continuing, chat) }));
        // The chat on screen keeps its words until the kept answer replaces
        // them, so that nothing blinks.
        if (chat === get().currentConversation) void get().refreshTranscript().then(clear);
        else clear();
        void get().refreshConversations();
        void get().refreshTask();
        void get().refreshHistory();
      } else if ("turn_changed" in event) {
        const { conversation, state } = event.turn_changed;
        set((s) => ({ turns: ENDED.includes(state) ? without(s.turns, conversation) : { ...s.turns, [conversation]: state } }));
      } else if ("tool_call_started" in event) {
        const { conversation, id, tool, params } = event.tool_call_started;
        set((s) => ({
          liveCalls: { ...s.liveCalls, [conversation]: [...(s.liveCalls[conversation] ?? []), { id, tool, params, outcome: null, at: Date.now() }] },
          trace: [...s.trace.slice(1 - KEEP_TRACE), `→ ${tool}`],
        }));
      } else if ("tool_call_finished" in event) {
        const { conversation, id, tool, outcome } = event.tool_call_finished;
        set((s) => ({
          liveCalls: { ...s.liveCalls, [conversation]: (s.liveCalls[conversation] ?? []).map((c) => (c.id === id ? { ...c, outcome } : c)) },
          trace: [...s.trace.slice(1 - KEEP_TRACE), `← ${tool}: ${outcomeLine(outcome)}`],
        }));
      } else if ("approval_withdrawn" in event) {
        const { id } = event.approval_withdrawn;
        set((s) => ({ approvals: s.approvals.filter((a) => a.id !== id) }));
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
        // "checked": files that were already on this computer have been looked at.
        // "paused" and "stopped": the person stopped it, and what is here says the rest.
        if (["done", "checked", "paused", "stopped"].includes(stage) || stage.startsWith("failed")) void get().refreshModelCatalog();
      } else if ("presence" in event) {
        const { board, people } = event.presence;
        set((s) => ({ present: { ...s.present, [board]: people } }));
      } else if ("conversation_changed" in event) {
        // This or another client switched, created or deleted one. What is
        // in progress in each chat stays that chat's.
        set({ currentConversation: event.conversation_changed.current });
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
      const chat = get().currentConversation;
      // Being written until Core says where it stands, so that nothing is
      // sent twice meanwhile.
      set((s) => ({
        turns: { ...s.turns, [chat]: "writing" },
        transcript: [...s.transcript, { role: "user", content: text, tool_calls: [], stopped: false }],
      }));
      const transcript = await attempt(async () => pick(await call({ send_message: { text, conversation: chat } }), "transcript"));
      if (!transcript) {
        // Refused, or not reached: the chat as Core has it.
        set((s) => ({ turns: without(s.turns, chat) }));
        void get().refreshTranscript();
        return;
      }
      if (get().currentConversation === chat) set({ transcript: transcript.messages });
      void get().refreshConversations();
    },
    cancel: async () => {
      const chat = get().currentConversation;
      await attempt(() => call({ cancel_turn: { conversation: chat } }));
    },
    continueAnswer: async () => {
      const chat = get().currentConversation;
      set((s) => ({ turns: { ...s.turns, [chat]: "writing" }, continuing: { ...s.continuing, [chat]: true } }));
      const answered = await attempt(() => call({ continue_answer: { conversation: chat } }));
      if (!answered) set((s) => ({ turns: without(s.turns, chat), continuing: without(s.continuing, chat) }));
    },
    refreshTurns: async () => {
      await attempt(async () => {
        const turns = pick(await call("list_turns"), "turns");
        if (turns) set({ turns: Object.fromEntries(turns.list.map((t) => [t.conversation, t.state])) });
      });
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
        if (c) set({ conversations: c.list, currentConversation: c.current, transcript: [], task: null, page: "chat" });
      });
    },
    selectConversation: async (id) => {
      await attempt(async () => {
        const c = pick(await call({ select_conversation: { id } }), "conversations");
        if (c) set({ conversations: c.list, currentConversation: c.current, page: "chat" });
      });
      await get().refreshTranscript();
    },
    deleteConversation: async (id) => {
      await attempt(async () => {
        const c = pick(await call({ delete_conversation: { id } }), "conversations");
        if (c) set({ conversations: c.list, currentConversation: c.current });
      });
      await get().refreshTranscript();
    },
    refreshModelCatalog: async () => {
      await attempt(async () => takeModelCatalog(await call("list_model_catalog")));
    },
    refreshComputer: async () => {
      await attempt(async () => {
        const computer = pick(await call("describe_computer"), "computer");
        if (computer) set({ computer });
        // The model that was in use here last is started again as the window
        // opens, unless one is running or starting already.
        const engine = get().environment?.engine;
        if (computer?.last_model && !computer.first_run && !engine?.running && !engine?.loading) void get().loadModel(computer.last_model);
      });
      set({ computerAsked: true });
    },
    leaveFirstRun: () => set({ firstRunLeft: true }),
    refreshEngineLog: async () => {
      await attempt(async () => {
        const log = pick(await call({ engine_log: { lines: 60 } }), "engine_log");
        if (log) set({ engineLog: log.lines });
      });
    },
    downloadModel: async (id) => {
      await attempt(async () => takeModelCatalog(await call({ download_model: { id } })));
    },
    stopDownload: async (id) => {
      await attempt(async () => takeModelCatalog(await call({ stop_download: { id } })));
    },
    deleteModel: async (id) => {
      await attempt(async () => takeModelCatalog(await call({ delete_model: { id } })));
      // The model in use may have been the one: what the bar says comes from Core.
      await get().refreshEnvironment();
      await get().refreshModels();
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
        set({ transcript: [], panels: [], board: null, page: "chat" });
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
/** The colours the screens give people's initials, one per person, the same everywhere they appear. */
const AVATAR_COLOURS = ["#1D7A55", "#8A5A2B", "#3E5C8A", "#6B4E8A", "#8A3E4E"];
export function avatarColour(id: string): string {
  let h = 0;
  for (const c of id) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return AVATAR_COLOURS[h % AVATAR_COLOURS.length];
}

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
