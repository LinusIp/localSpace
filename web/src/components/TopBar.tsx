// The top bar: a title when the page has one, and on the right the two
// things that are always visible — the network mode, and whether the
// assistant is ready when it is not — and the person.

import { Menu, ShieldIcon } from "@localspace/ui";
import { logout } from "../api/client";
import { initialsOf, useSession } from "../store";

export function NetworkChip() {
  const environment = useSession((s) => s.environment);
  if (!environment) return null;
  const mode = environment.network;
  const label = mode === "airgapped" ? "Offline" : mode === "ask" ? "Online, asks first" : "Online";
  const title =
    mode === "airgapped"
      ? "Nothing leaves your organisation's server."
      : mode === "ask"
        ? "The assistant asks before it reaches the internet."
        : "The assistant may reach the internet.";
  return (
    <span className={`chip ${mode === "airgapped" ? "green" : "quiet"}`} title={title}>
      <ShieldIcon size={13} />
      {label}
    </span>
  );
}

/** Shown only when the assistant is not ready: the one status besides the network. */
export function ReadinessChip() {
  const { environment, live, me } = useSession();
  if (!environment) return null;
  if (!live) return <span className="chip amber">Reconnecting…</span>;
  if (environment.engine.loading) return <span className="chip amber">Starting up…</span>;
  if (environment.model) return null;
  const admin = me?.roles.includes("admin") || me?.topology === "personal";
  return <span className="chip amber">{admin ? "No model — choose one in Settings" : "No model — ask your administrator"}</span>;
}

export function Person() {
  const { me, signOut, goSettings } = useSession();
  const label = me?.name || me?.email || me?.user || "?";
  return (
    <Menu
      align="right"
      trigger={(open) => (
        <button type="button" className="avatar" onClick={open} aria-label="Account" title={label}>
          {initialsOf(label)}
        </button>
      )}
      items={[
        { id: "who", label: label, disabled: true, onSelect: () => undefined },
        { id: "settings", label: "Settings", onSelect: () => goSettings("general") },
        { id: "out", label: "Sign out", onSelect: () => void logout().then(signOut) },
      ]}
    />
  );
}

export function TopBar({ title, bordered }: { title?: string; bordered?: boolean }) {
  return (
    <header className={`topbar${bordered ? " bordered" : ""}`}>
      {title && <span className="topbar-title">{title}</span>}
      <div className="topbar-right">
        <ReadinessChip />
        <NetworkChip />
        <Person />
      </div>
    </header>
  );
}
