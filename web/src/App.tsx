// The shell (v2 §6.1): login, then the pages behind the rail. The chat harness
// is the home page; the rest are the environment's own views.

import { useEffect, useState } from "react";
import { Button, Card, Input, Toast } from "@localspace/ui";
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
    return (
      <div className="ls-root ls-screen ls-row ls-middle ls-muted">Connecting…</div>
    );
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
    <div className="ls-root ls-screen ls-row ls-middle">
      <Card className="ls-w-96 ls-pad-6" style={{ boxShadow: "var(--ls-shadow)" }}>
        <form onSubmit={submit}>
          <div className="ls-row ls-gap-3 ls-mb-3" style={{ marginBottom: 24 }}>
            <Mark />
            <h1 style={{ margin: 0, fontSize: 18, fontWeight: 600 }}>localSpace</h1>
          </div>
          <label className="ls-small ls-muted" htmlFor="token">
            Access token
          </label>
          <Input id="token" mono className="ls-mt-1" value={token} onChange={(e) => setToken(e.target.value)} autoComplete="off" autoFocus />
          <p className="ls-mt-2 ls-small ls-faint">
            The server prints its token at start and writes it to <code>token</code> in its data directory. In an organisation your administrator gives
            it to you.
          </p>
          {error && <p className="ls-mt-3 ls-danger">{error}</p>}
          <Button type="submit" kind="primary" block className="ls-mt-4" disabled={busy || token.trim() === ""} busy={busy}>
            {busy ? "Signing in…" : "Sign in"}
          </Button>
        </form>
      </Card>
    </div>
  );
}

function Shell() {
  const { page, onEvent, setLive, refreshEnvironment, refreshTranscript, refreshModels, refreshTask, notices } = useSession();

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
    <div className="ls-root ls-screen ls-row ls-start" style={{ alignItems: "stretch" }}>
      <Rail />
      <div className="ls-col ls-grow">
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
          <Toast level={latest.level === "error" ? "error" : latest.level === "warn" ? "warn" : "info"}>{latest.text}</Toast>
        )}
      </div>
    </div>
  );
}
