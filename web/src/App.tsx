// The shell (v2 §6.1): login, then the pages behind the rail. The chat harness
// is the home page; the rest are the environment's own views.

import { useEffect, useState } from "react";
import { ApiError, events, login, me } from "./api/client";
import { useSession } from "./store";
import { Rail } from "./components/Rail";
import { TopBar } from "./components/TopBar";
import { Mark } from "./components/Mark";
import { ChatPage } from "./pages/ChatPage";
import { AgentsPage } from "./pages/AgentsPage";
import { ToolsPage } from "./pages/ToolsPage";
import { ModelsPage } from "./pages/ModelsPage";
import { DataPage } from "./pages/DataPage";
import { HistoryPage } from "./pages/HistoryPage";
import { LibraryPage } from "./pages/LibraryPage";
import { SettingsPage } from "./pages/SettingsPage";
import { HelpPage } from "./pages/HelpPage";

export default function App() {
  const session = useSession();
  const [checked, setChecked] = useState(false);

  // A token in the URL is the desktop shell signing its window in. It is
  // exchanged for the cookie and removed from the address at once.
  useEffect(() => {
    const url = new URL(location.href);
    const token = url.searchParams.get("token");
    const settle = async () => {
      try {
        if (token) {
          await login(token);
          url.searchParams.delete("token");
          history.replaceState(null, "", url.pathname + url.search + url.hash);
        }
        session.signIn(await me());
      } catch {
        // not signed in: the login screen follows
      } finally {
        setChecked(true);
      }
    };
    void settle();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!checked) {
    return <div className="flex h-screen items-center justify-center text-sm text-muted">Connecting…</div>;
  }
  if (!session.me) return <Login />;
  return <Shell />;
}

function Login() {
  const signIn = useSession((s) => s.signIn);
  const [token, setToken] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await login(token.trim());
      signIn(await me());
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "the server could not be reached");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex h-screen items-center justify-center bg-page">
      <form onSubmit={submit} className="w-96 rounded-xl border border-line bg-white p-8 shadow-sm">
        <div className="mb-6 flex items-center gap-3">
          <Mark />
          <h1 className="text-lg font-semibold text-ink">localSpace</h1>
        </div>
        <label className="block text-sm text-muted" htmlFor="token">
          Access token
        </label>
        <input
          id="token"
          className="mt-1 w-full rounded-md border border-line px-3 py-2 font-mono text-sm outline-none focus:border-accent"
          value={token}
          onChange={(e) => setToken(e.target.value)}
          autoComplete="off"
          autoFocus
        />
        <p className="mt-2 text-xs text-faint">
          The server prints its token at start and writes it to <code>token</code> in its data directory.
          In an organisation your administrator gives it to you.
        </p>
        {error && <p className="mt-3 text-sm text-danger">{error}</p>}
        <button
          type="submit"
          disabled={busy || token.trim() === ""}
          className="mt-5 w-full rounded-md bg-accent px-3 py-2 text-sm font-medium text-white disabled:opacity-50"
        >
          {busy ? "Signing in…" : "Sign in"}
        </button>
      </form>
    </div>
  );
}

function Shell() {
  const { page, onEvent, setLive, refreshEnvironment, refreshTranscript, refreshModels, refreshTask, notices } =
    useSession();

  // The event stream keeps everything current while the shell is open; the
  // first state is fetched outright.
  useEffect(() => {
    const stop = events(onEvent, setLive);
    void refreshEnvironment();
    void refreshTranscript();
    void refreshModels();
    void refreshTask();
    return stop;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const latest = notices[notices.length - 1];

  return (
    <div className="flex h-screen bg-page text-ink">
      <Rail />
      <div className="flex min-w-0 flex-1 flex-col">
        <TopBar />
        {page === "chat" && <ChatPage />}
        {page === "agents" && <AgentsPage />}
        {page === "tools" && <ToolsPage />}
        {page === "models" && <ModelsPage />}
        {page === "data" && <DataPage />}
        {page === "history" && <HistoryPage />}
        {page === "library" && <LibraryPage />}
        {page === "settings" && <SettingsPage />}
        {page === "help" && <HelpPage />}
        {latest && Date.now() - latest.at < 8000 && (
          <div
            className={`pointer-events-none fixed bottom-4 left-1/2 -translate-x-1/2 rounded-lg px-4 py-2 text-xs shadow ${
              latest.level === "error"
                ? "bg-danger text-white"
                : latest.level === "warn"
                  ? "bg-warn text-white"
                  : "bg-ink text-white"
            }`}
          >
            {latest.text}
          </div>
        )}
      </div>
    </div>
  );
}
