// Sign in (screen 1): email and password. The line under the card is the
// promise the product sells. On a personal computer the app signs its own
// window in; a browser opened by hand takes the token the app printed.

import { useState } from "react";
import { ShieldIcon } from "@localspace/ui";
import { ApiError, login, loginWithPassword, me } from "../api/client";
import { useSession } from "../store";

export function SignInPage() {
  const { authMode, signIn } = useSession();
  const organisation = authMode?.mode !== "personal";
  const organisationName = authMode?.organisation ?? null;
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [token, setToken] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      if (organisation) await loginWithPassword(email.trim(), password);
      else await login(token.trim());
      signIn(await me());
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "The server could not be reached.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="auth">
      <div className="auth-column">
        <img src="/brand/localspace-lockup.png" alt="localSpace" style={{ width: 224, height: "auto", display: "block" }} />
        {organisationName && <div className="auth-org">{organisationName}</div>}
        <form className="auth-card" onSubmit={submit}>
          <div className="auth-title">Sign in</div>
          <div className="auth-sub">{organisation ? "Use your work email address." : "Use the token this computer's app printed at start."}</div>
          {organisation ? (
            <>
              <label className="field" style={{ marginTop: 24 }}>
                <span>Email</span>
                <input className="input" type="email" autoComplete="username" value={email} onChange={(e) => setEmail(e.target.value)} autoFocus />
              </label>
              <label className="field">
                <span>Password</span>
                <input className="input" type="password" autoComplete="current-password" value={password} onChange={(e) => setPassword(e.target.value)} />
              </label>
            </>
          ) : (
            <label className="field" style={{ marginTop: 24 }}>
              <span>Token</span>
              <input className="input mono" autoComplete="off" value={token} onChange={(e) => setToken(e.target.value)} autoFocus />
            </label>
          )}
          {error && <div className="error">{error}</div>}
          <button type="submit" className="auth-button" disabled={busy || (organisation ? !email.trim() || !password : !token.trim())}>
            {busy ? "Signing in…" : "Sign in"}
          </button>
          <div className="auth-help">{organisation ? "Can't sign in? Ask your IT team to send you a new link." : "The token is in the file named token in the app's data folder."}</div>
        </form>
        <div className="auth-promise">
          <ShieldIcon size={15} className="ls-accent" />
          <span>{organisation ? "Runs on your organisation's own server. Nothing you type leaves your network." : "Runs on this computer. Nothing you type leaves it unless you allow the network."}</span>
        </div>
      </div>
    </div>
  );
}
