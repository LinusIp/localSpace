// The shell (v2 §6.1; the app screens of 2026-09-13): sign in, or a one-time
// link, then the rail and the pages. The chat is the home page.

import { useEffect, useState } from "react";
import { authMode, events, login, me } from "./api/client";
import { useSession } from "./store";
import { Rail } from "./components/Rail";
import { ChatPage } from "./pages/ChatPage";
import { BoardPage } from "./pages/BoardPage";
import { DocumentsPage } from "./pages/DocumentsPage";
import { StorePage } from "./pages/StorePage";
import { SettingsPage } from "./pages/SettingsPage";
import { AdminPage } from "./pages/AdminPage";
import { HelpPage } from "./pages/HelpPage";
import { FirstRunPage } from "./pages/FirstRunPage";
import { SignInPage } from "./pages/SignInPage";
import { InvitePage } from "./pages/InvitePage";

export default function App() {
  const session = useSession();
  const [checked, setChecked] = useState(false);
  const [invite, setInvite] = useState<string | null>(() => {
    const m = /^\/invite\/([0-9a-f]+)\/?$/.exec(location.pathname);
    return m ? m[1] : null;
  });
  // A shared link to a board: opened once the shell is up, then the address is plain again.
  const [boardLink] = useState<string | null>(() => {
    const m = /^\/board\/([^/]+)\/?$/.exec(location.pathname);
    return m ? decodeURIComponent(m[1]) : null;
  });

  // A token in the URL is the desktop app signing its window in. It is
  // exchanged for the cookie and removed from the address at once.
  useEffect(() => {
    const url = new URL(location.href);
    const token = url.searchParams.get("token");
    const settle = async () => {
      try {
        session.setAuthMode(await authMode().catch(() => null));
        if (token) {
          await login(token);
          url.searchParams.delete("token");
          history.replaceState(null, "", url.pathname + url.search + url.hash);
        }
        session.signIn(await me());
      } catch {
        // not signed in: the sign-in page follows
      } finally {
        setChecked(true);
      }
    };
    void settle();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // The tab is named after the organisation when the settings name one.
  useEffect(() => {
    const name = session.authMode?.organisation;
    document.title = name ? `localSpace · ${name}` : "localSpace";
  }, [session.authMode]);

  if (invite) {
    return (
      <InvitePage
        token={invite}
        onDone={() => {
          history.replaceState(null, "", "/");
          setInvite(null);
        }}
      />
    );
  }
  if (!checked) {
    return <div className="auth ls-muted">Connecting…</div>;
  }
  if (!session.me) return <SignInPage />;
  return <Shell boardLink={boardLink} />;
}

function Shell({ boardLink }: { boardLink: string | null }) {
  const { me, page, onEvent, setLive, refreshEnvironment, refreshTranscript, refreshModels, refreshModelCatalog, refreshComputer, refreshTask, refreshConversations, loadPreferences, notices, computer, computerAsked, firstRunLeft } = useSession();

  // The event stream keeps everything current while the shell is open; the
  // first state is fetched outright.
  useEffect(() => {
    const stop = events(onEvent, setLive);
    void refreshEnvironment().then(() => {
      if (!boardLink) return;
      history.replaceState(null, "", "/");
      const s = useSession.get();
      if (s.environment?.harnesses.some((h) => h.id === boardLink)) s.openBoard(boardLink);
      else s.notify("warn", "That board is not in this workspace.");
    });
    void refreshTranscript();
    void refreshModels();
    void refreshModelCatalog();
    void refreshTask();
    void refreshTask();
    void refreshConversations();
    void loadPreferences();
    // The catalog's verdicts and the first run both need a look at the
    // machine, which takes a moment the first time: asked last, so nothing
    // else waits behind it.
    void refreshModelCatalog();
    // On a person's own computer: looking at the machine takes a
    // moment, and nothing else should wait behind it. In an organisation the
    // computer that matters is the server, and this window never asks.
    if (me?.topology === "personal") void refreshComputer();
    return stop;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const latest = notices[notices.length - 1];
  const admin = me?.topology === "organisation" && me.roles.includes("admin");

  // On a person's own computer the window waits a moment for what the
  // computer is, so that a first run never flashes the empty chat first.
  if (me?.topology === "personal" && !computerAsked) return <div className="auth ls-muted">Looking at this computer…</div>;
  // The first run: no model is on this computer yet.
  if (me?.topology === "personal" && computer?.first_run && !firstRunLeft) {
    return (
      <>
        <FirstRunPage />
        {latest && Date.now() - latest.at < 8000 && <div className={`toast${latest.level === "error" ? " error" : latest.level === "warn" ? " warn" : ""}`}>{latest.text}</div>}
      </>
    );
  }

  return (
    <div className="shell">
      <Rail />
      <div className="shell-main">
        {admin && me?.provider === "local" && <div className="banner">This pilot signs people in with local accounts. Connect your identity provider before the wider rollout.</div>}
        {page === "chat" && <ChatPage />}
        {page === "board" && <BoardPage />}
        {page === "documents" && <DocumentsPage />}
        {page === "store" && <StorePage />}
        {page === "settings" && <SettingsPage />}
        {page === "admin" && (admin ? <AdminPage /> : <ChatPage />)}
        {page === "help" && <HelpPage />}
        {latest && Date.now() - latest.at < 8000 && <div className={`toast${latest.level === "error" ? " error" : latest.level === "warn" ? " warn" : ""}`}>{latest.text}</div>}
      </div>
    </div>
  );
}
