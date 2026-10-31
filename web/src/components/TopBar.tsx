// The top bar: a title when the page has one, and on the right the two
// things that are always visible — the network mode, and whether the
// assistant is ready when it is not — and the person.

import { Menu, ShieldIcon } from "@localspace/ui";
import { logout } from "../api/client";
import { initialsOf, useSession } from "../store";
import { useStrings } from "../useStrings";

/**
 * The one indicator of whether this place can talk to the internet (plugin
 * spec §8.1), saying what is true of the product as it is: no web tool
 * exists, so the one thing that ever goes online is the download of a model
 * somebody asked for (docs/DECISIONS.md, 2026-09-20).
 */
export function NetworkChip() {
  const environment = useSession((s) => s.environment);
  const personal = useSession((s) => s.me?.topology === "personal");
  const goSettings = useSession((s) => s.goSettings);
  const strings = useStrings();
  if (!environment) return null;
  const offline = environment.network === "airgapped";
  const hint = offline
    ? personal
      ? strings.offlineHintPersonal
      : strings.offlineHintOrganisation
    : personal
      ? strings.onlineHintPersonal
      : strings.onlineHintOrganisation;
  return (
    <button type="button" className={`chip ${offline ? "green" : "quiet"}`} onClick={() => goSettings("network")} title={hint}>
      <ShieldIcon size={13} />
      {offline ? strings.offline : strings.onlineForDownloads}
    </button>
  );
}

/** Shown only when the assistant is not ready: the one status besides the network. */
export function ReadinessChip() {
  const { environment, live, me, goSettings } = useSession();
  const strings = useStrings();
  if (!environment) return null;
  if (!live) return <span className="chip amber">{strings.reconnecting}</span>;
  if (environment.engine.loading) return <span className="chip amber">{strings.startingUp}</span>;
  if (environment.model) return null;
  const admin = me?.roles.includes("admin") || me?.topology === "personal";
  if (!admin) return <span className="chip amber">{strings.noModelAskAdmin}</span>;
  return (
    <button type="button" className="chip amber" onClick={() => goSettings("assistant")}>
      {strings.chooseModel}
    </button>
  );
}

export function Person() {
  const { me, signOut, goSettings } = useSession();
  const strings = useStrings();
  const label = me?.name || me?.email || me?.user || "?";
  // A person's own computer has no accounts, so nothing to sign out of: there,
  // "Sign out" led to a page asking for a token nobody can obtain, a locked
  // door (docs/DECISIONS.md, 2026-09-20). Without it the menu held a name and
  // a second way into Settings, so the initial opens Settings itself.
  if (me?.topology === "personal") {
    return (
      <button type="button" className="avatar" onClick={() => goSettings("general")} aria-label={strings.menuSettings} title={label}>
        {initialsOf(label)}
      </button>
    );
  }
  return (
    <Menu
      align="right"
      trigger={(open) => (
        <button type="button" className="avatar" onClick={open} aria-label={strings.account} title={label}>
          {initialsOf(label)}
        </button>
      )}
      items={[
        { id: "who", label: label, disabled: true, onSelect: () => undefined },
        { id: "settings", label: strings.menuSettings, onSelect: () => goSettings("general") },
        { id: "out", label: strings.signOut, onSelect: () => void logout().then(signOut) },
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
