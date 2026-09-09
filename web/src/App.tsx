// The shell (v2 §6.1): login, then the environment. Panels, the chat harness,
// the store and settings arrive with build-order steps 3 and 4; today this is
// the empty shell both the browser and the desktop app boot into.

import { useEffect, useState } from "react";
import { ApiError, call, events, login, logout, me } from "./api/client";
import type { HarnessSummary, NetworkMode } from "./api/generated";
import { useSession } from "./store";

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

  if (!checked) return <Centered>Connecting…</Centered>;
  if (!session.me) return <Login />;
  return <Shell />;
}

function Centered({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-screen items-center justify-center text-sm text-muted">{children}</div>
  );
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
          The server prints its token at start and writes it to <code>token</code> in its data
          directory. In an organisation your administrator gives it to you.
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
  const session = useSession();
  const { me: who, environment } = session;

  // The event stream keeps the environment current for as long as the shell
  // is open; the first environment is fetched outright.
  useEffect(() => {
    const stop = events(session.onEvent, session.setLive);
    call("get_environment")
      .then((response) => {
        if (typeof response !== "string" && "environment" in response) {
          session.setEnvironment(response.environment);
        }
      })
      .catch((err: unknown) => {
        const text = err instanceof ApiError ? err.message : "the server could not be reached";
        session.onEvent({ notice: { level: "error", text } });
      });
    return stop;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const setNetwork = async (mode: NetworkMode) => {
    const response = await call({ set_network_mode: { mode } });
    if (typeof response !== "string" && "environment" in response) {
      session.setEnvironment(response.environment);
    }
  };

  const signOut = async () => {
    await logout();
    session.signOut();
  };

  return (
    <div className="flex h-screen flex-col bg-page text-ink">
      <header className="flex items-center gap-4 border-b border-line bg-white px-4 py-2">
        <Mark />
        <span className="font-semibold">localSpace</span>
        <span className="rounded-md border border-line px-2 py-0.5 text-xs text-muted">
          {who?.user}
          {environment ? ` · ${environment.workspace}` : ""}
        </span>
        <span className="text-xs text-faint">{who?.topology}</span>
        <div className="ml-auto flex items-center gap-3 text-xs">
          {environment && (
            <label className="flex items-center gap-1 text-muted">
              network
              <select
                className="rounded-md border border-line bg-white px-1 py-0.5"
                value={environment.network}
                onChange={(e) => void setNetwork(e.target.value as NetworkMode)}
              >
                {(["airgapped", "ask", "online"] as NetworkMode[])
                  .filter((m) => rank(m) <= rank(environment.network_ceiling))
                  .map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
              </select>
            </label>
          )}
          <span className="flex items-center gap-1 text-muted">
            <span
              className={`inline-block h-2 w-2 rounded-full ${session.live ? "bg-accent" : "bg-faint"}`}
            />
            {session.live ? "live" : "reconnecting"}
          </span>
          <button className="text-muted hover:text-ink" onClick={() => void signOut()}>
            Sign out
          </button>
        </div>
      </header>

      <div className="flex min-h-0 flex-1">
        <nav className="flex w-16 flex-col items-center gap-2 border-r border-line bg-white py-3 text-[11px] text-muted">
          {["Chat", "Store", "Settings"].map((item) => (
            <div key={item} className="w-14 rounded-md py-2 text-center hover:bg-page">
              {item}
            </div>
          ))}
        </nav>

        <main className="flex min-w-0 flex-1 flex-col">
          <div className="flex flex-1 items-center justify-center p-8">
            <div className="max-w-md text-center">
              <p className="text-sm text-muted">No panels open.</p>
              <p className="mt-1 text-xs text-faint">
                Harnesses are installed from the Store and open here as panels. The chat harness
                and the panel layout arrive with the next build steps.
              </p>
            </div>
          </div>
          <NoticesStrip />
        </main>

        <aside className="w-80 overflow-y-auto border-l border-line bg-white p-4 text-sm">
          <h2 className="text-xs font-medium uppercase tracking-wide text-faint">Environment</h2>
          {environment ? (
            <>
              <dl className="mt-2 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
                <dt className="text-muted">machine</dt>
                <dd>{environment.machine}</dd>
                <dt className="text-muted">profile</dt>
                <dd>{environment.profile}</dd>
                <dt className="text-muted">engine</dt>
                <dd>{environment.engine.running ? environment.engine.detail : "no model loaded"}</dd>
                <dt className="text-muted">model</dt>
                <dd>{environment.model ? environment.model.id : "none"}</dd>
                <dt className="text-muted">Tier B</dt>
                <dd>{environment.tier_b_permitted ? "permitted" : "off"}</dd>
              </dl>
              <h2 className="mt-5 text-xs font-medium uppercase tracking-wide text-faint">
                Harnesses
              </h2>
              {environment.harnesses.length === 0 ? (
                <p className="mt-2 text-xs text-muted">None installed. Only chat ships in the box.</p>
              ) : (
                <ul className="mt-2 space-y-2">
                  {environment.harnesses.map((h) => (
                    <HarnessCard key={h.id} harness={h} />
                  ))}
                </ul>
              )}
            </>
          ) : (
            <p className="mt-2 text-xs text-muted">Loading…</p>
          )}
        </aside>
      </div>
    </div>
  );
}

function HarnessCard({ harness }: { harness: HarnessSummary }) {
  return (
    <li className="rounded-lg border border-line p-3">
      <div className="flex items-baseline justify-between">
        <span className="font-medium">{harness.title}</span>
        <span className="text-[10px] uppercase text-faint">
          {harness.tier === "wasm" ? "Tier A · wasm" : "Tier B · native"}
        </span>
      </div>
      <div className="mt-0.5 text-xs text-muted">
        {harness.id} · {harness.version} · {harness.publisher}
      </div>
      <div className="mt-1 text-xs text-muted">
        {harness.tool_count} tools, {harness.front_door.length} front door
        {harness.has_context_provider ? ", context provider" : ""}
        {harness.loaded ? " · resident" : " · idle"}
      </div>
      {harness.degraded && <div className="mt-1 text-xs text-danger">{harness.degraded}</div>}
    </li>
  );
}

function NoticesStrip() {
  const notices = useSession((s) => s.notices);
  const recent = notices.slice(-3);
  if (recent.length === 0) return null;
  return (
    <div className="border-t border-line bg-white px-4 py-2 text-xs">
      {recent.map((n) => (
        <div key={n.at + n.text} className={n.level === "error" ? "text-danger" : "text-muted"}>
          {n.text}
        </div>
      ))}
    </div>
  );
}

function Mark() {
  return (
    <span className="inline-flex h-6 w-6 items-center justify-center rounded-full border-2 border-accent">
      <span className="h-2 w-2 rounded-full bg-accent" />
    </span>
  );
}

function rank(mode: NetworkMode): number {
  return mode === "airgapped" ? 0 : mode === "ask" ? 1 : 2;
}
