// A one-time link, `/invite/<token>`: set a password and you are in. The
// first administrator's link asks for a name and an email as well, since
// the server knows none yet.

import { useEffect, useState } from "react";
import { ShieldIcon } from "@localspace/ui";
import { ApiError, inviteStatus, me, setPassword } from "../api/client";
import type { InviteStatus } from "../api/client";
import { useSession } from "../store";

export function InvitePage({ token, onDone }: { token: string; onDone: () => void }) {
  const signIn = useSession((s) => s.signIn);
  const [status, setStatus] = useState<InviteStatus | null | "unreachable">(null);
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPasswordText] = useState("");
  const [again, setAgain] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    inviteStatus(token)
      .then(setStatus)
      .catch(() => setStatus("unreachable"));
  }, [token]);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    if (password !== again) {
      setError("The two passwords are not the same.");
      return;
    }
    setBusy(true);
    try {
      const body = status && status !== "unreachable" && status.first_admin ? { token, password, email: email.trim(), name: name.trim() } : { token, password };
      await setPassword(body);
      signIn(await me());
      onDone();
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "The server could not be reached.");
    } finally {
      setBusy(false);
    }
  };

  let body: React.ReactNode;
  if (status === null) {
    body = <div className="auth-sub">Checking the link…</div>;
  } else if (status === "unreachable") {
    body = <div className="auth-sub">The server could not be reached. Try again in a moment.</div>;
  } else if (!status.valid) {
    body = (
      <>
        <div className="auth-title">This link no longer works</div>
        <div className="auth-sub">It has been used or has expired. Ask your administrator for a new one.</div>
      </>
    );
  } else {
    const first = status.first_admin;
    body = (
      <form onSubmit={submit}>
        <div className="auth-title">{first ? "You are the first administrator" : "Set your password"}</div>
        <div className="auth-sub">{first ? "Say who you are, and choose a password." : `For ${status.email}. At least 12 characters; spaces are fine.`}</div>
        {first && (
          <>
            <label className="field" style={{ marginTop: 24 }}>
              <span>Your name</span>
              <input className="input" value={name} onChange={(e) => setName(e.target.value)} autoFocus />
            </label>
            <label className="field">
              <span>Work email</span>
              <input className="input" type="email" autoComplete="username" value={email} onChange={(e) => setEmail(e.target.value)} />
            </label>
          </>
        )}
        <label className="field" style={first ? undefined : { marginTop: 24 }}>
          <span>Password</span>
          <input className="input" type="password" autoComplete="new-password" value={password} onChange={(e) => setPasswordText(e.target.value)} autoFocus={!first} />
        </label>
        <label className="field">
          <span>Password, again</span>
          <input className="input" type="password" autoComplete="new-password" value={again} onChange={(e) => setAgain(e.target.value)} />
        </label>
        {error && <div className="error">{error}</div>}
        <button type="submit" className="auth-button" disabled={busy || !password || !again || (first && (!name.trim() || !email.trim()))}>
          {busy ? "One moment…" : first ? "Set the password and sign in" : "Set the password and sign in"}
        </button>
        {first && <div className="auth-help">At least 12 characters; spaces are fine.</div>}
      </form>
    );
  }

  return (
    <div className="auth">
      <div className="auth-column">
        <img src="/brand/localspace-lockup.png" alt="localSpace" style={{ width: 224, height: "auto", display: "block" }} />
        <div className="auth-card">{body}</div>
        <div className="auth-promise">
          <ShieldIcon size={15} className="ls-accent" />
          <span>Runs on your organisation's own server. Nothing you type leaves your network.</span>
        </div>
      </div>
    </div>
  );
}
