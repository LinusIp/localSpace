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
import { SignInPage } from "./pages/SignInPage";
import { InvitePage } from "./pages/InvitePage";

export default function App() {
  const session = useSession();
  const [checked, setChecked] = useState(false);
  const [invite, setInvite] = useState<string | null>(() => {
    const m = /^\/invite\/([0-9a-f]+)\/?$/.exec(location.pathname);
    return m ? m[1] : null;
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
  return <Shell />;
}

function Shell() {
  const { me, page, onEvent, setLive, refreshEnvironment, refreshTranscript, refreshModels, refreshModelCatalog, refreshTask, refreshConversations, loadPreferences, notices } = useSession();

  // The event stream keeps everything current while the shell is open; the
  // first state is fetched outright.
  useEffect(() => {
    const stop = events(onEvent, setLive);
    void refreshEnvironment();
    void refreshTranscript();
    void refreshModels();
    void refreshModelCatalog();
    void refreshTask();
    void refreshConversations();
    void loadPreferences();
    return stop;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const latest = notices[notices.length - 1];
  const admin = me?.topology === "organisation" && me.roles.includes("admin");

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
