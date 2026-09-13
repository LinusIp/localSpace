// Admin (screen 7): People, and Workspaces. For administrators of an
// organisation server. Every action here is one request Core checks.

import { useEffect, useState } from "react";
import { Dialog, Menu, MoreIcon, PlusIcon, SearchIcon } from "@localspace/ui";
import type { AccessLevel, Invite, UserInfo, UserRole, WorkspaceInfo } from "../api/generated";
import { avatarColour, initialsOf, useSession } from "../store";
import type { AdminPane } from "../store";
import { TopBar } from "../components/TopBar";
import { whenLabel } from "../lib/time";

const PANES: Array<{ id: AdminPane; label: string }> = [
  { id: "people", label: "People" },
  { id: "workspaces", label: "Workspaces" },
];

const ROLES: Array<{ role: UserRole; label: string }> = [
  { role: "admin", label: "Administrator" },
  { role: "member", label: "Member" },
  { role: "viewer", label: "Can view only" },
];

const LEVELS: Array<{ level: AccessLevel; label: string }> = [
  { level: "view", label: "Can view" },
  { level: "comment", label: "Can comment" },
  { level: "edit", label: "Can edit" },
  { level: "owner", label: "Owner" },
];

function roleOf(user: UserInfo): { role: UserRole; label: string } {
  return ROLES.find((r) => user.roles.includes(r.role)) ?? ROLES[1];
}

export function AdminPage() {
  const { adminPane, goAdmin } = useSession();
  return (
    <>
      <TopBar />
      <div className="page">
        <nav className="tabs-quiet" aria-label="Admin">
          {PANES.map((p) => (
            <button key={p.id} type="button" className={`tab-quiet${adminPane === p.id ? " on" : ""}`} aria-current={adminPane === p.id ? "page" : undefined} onClick={() => goAdmin(p.id)}>
              {p.label}
            </button>
          ))}
        </nav>
        {adminPane === "people" ? <People /> : <Workspaces />}
      </div>
    </>
  );
}

/** A one-time link, shown once, with a way to copy it. */
function LinkDialog({ invite, onClose, what }: { invite: Invite | null; onClose: () => void; what: string }) {
  const [copied, setCopied] = useState(false);
  const [shownAt] = useState(() => Date.now());
  if (!invite) return null;
  const link = `${location.origin}/invite/${invite.token}`;
  const hours = Math.max(1, Math.round((invite.expires_ms - shownAt) / 3_600_000));
  return (
    <Dialog
      open
      title={what}
      onClose={onClose}
      actions={
        <button type="button" className="btn solid" onClick={onClose}>
          Done
        </button>
      }
    >
      <p style={{ marginTop: 0 }}>
        Give this link to {invite.email || "them"}. It works once and for {hours} hours; after that, send a new one from here.
      </p>
      <div className="ls-row ls-gap-2">
        <input className="input mono" readOnly value={link} onFocus={(e) => e.currentTarget.select()} aria-label="The link" />
        <button
          type="button"
          className="btn"
          onClick={() => {
            void navigator.clipboard?.writeText(link).then(() => setCopied(true));
          }}
        >
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
    </Dialog>
  );
}

function People() {
  const { me, users, refreshUsers, createUser, setUserRoles, disableUser, resetPassword, unlockUser, revokeSessions } = useSession();
  const [inviting, setInviting] = useState(false);
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [role, setRole] = useState<UserRole>("member");
  const [error, setError] = useState<string | null>(null);
  const [link, setLink] = useState<{ invite: Invite; what: string } | null>(null);
  const [search, setSearch] = useState("");
  const [searching, setSearching] = useState(false);
  // The clock the "locked" pills are read against; refreshed with the list.
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    void refreshUsers().then(() => setNow(Date.now()));
  }, [refreshUsers]);

  const invite = async () => {
    setError(null);
    if (!name.trim() || !email.trim()) {
      setError("Give their name and their work email address.");
      return;
    }
    const made = await createUser(email.trim(), name.trim(), [role]);
    if (made) {
      setInviting(false);
      setName("");
      setEmail("");
      setLink({ invite: made, what: `${name.trim()} is invited` });
    }
  };

  const shown = users.filter((u) => {
    const q = search.trim().toLowerCase();
    return !q || u.name.toLowerCase().includes(q) || u.email.toLowerCase().includes(q);
  });

  return (
    <>
      <div className="page-head">
        <div>
          <h1 className="page-title">People</h1>
          <div className="page-sub">Who can sign in to localSpace here.</div>
        </div>
        <div className="ls-row ls-gap-2">
          {searching ? (
            <input
              className="input"
              style={{ height: 36, width: 240, fontSize: 14 }}
              placeholder="Name or email"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              onBlur={() => {
                if (!search.trim()) setSearching(false);
              }}
              aria-label="Search people"
              autoFocus
            />
          ) : (
            <button type="button" className="btn tall" onClick={() => setSearching(true)}>
              <SearchIcon size={16} /> Search
            </button>
          )}
          <button type="button" className="btn solid tall" onClick={() => setInviting(true)}>
            <PlusIcon size={16} /> Invite people
          </button>
        </div>
      </div>

      <div className="table-card">
        <table className="table">
          <thead>
            <tr>
              <th>Name</th>
              <th>Role</th>
              <th>Last signed in</th>
              <th style={{ textAlign: "right" }}></th>
            </tr>
          </thead>
          <tbody>
            {shown.map((u) => {
              const locked = u.locked_until_ms !== null && u.locked_until_ms > now;
              const self = u.id === me?.user;
              return (
                <tr key={u.id}>
                  <td>
                    <div className="person">
                      <span className="avatar small" style={{ background: u.disabled || !u.has_password ? "#c4c1bc" : avatarColour(u.id) }} aria-hidden="true">
                        {initialsOf(u.name || u.email)}
                      </span>
                      <div>
                        <div className="person-name">
                          {u.name || u.email}
                          {self ? " (you)" : ""}
                        </div>
                        <div className="person-mail">{u.email}</div>
                      </div>
                    </div>
                  </td>
                  <td>
                    <span className="pill">{roleOf(u).label}</span>
                  </td>
                  <td className="ls-muted">
                    {u.disabled ? (
                      <span className="pill red">Disabled</span>
                    ) : locked ? (
                      <span className="pill amber">Locked after failed sign-ins</span>
                    ) : !u.has_password ? (
                      <span className="pill amber">Invitation sent</span>
                    ) : (
                      whenLabel(u.last_login_ms)
                    )}
                  </td>
                  <td style={{ textAlign: "right" }}>
                    <Menu
                      align="right"
                      trigger={(open) => (
                        <button type="button" className="rail-toggle" onClick={open} aria-label={`Actions for ${u.name || u.email}`}>
                          <MoreIcon size={18} />
                        </button>
                      )}
                      items={[
                        ...ROLES.filter((r) => r.role !== roleOf(u).role).map((r) => ({
                          id: `role-${r.role}`,
                          label: `Make ${r.label.toLowerCase() === "can view only" ? "view-only" : r.label.toLowerCase()}`,
                          disabled: self,
                          onSelect: () => void setUserRoles(u.id, [r.role]),
                        })),
                        {
                          id: "link",
                          label: u.has_password ? "Send a new sign-in link (resets their password)" : "Send a new link",
                          onSelect: () => void resetPassword(u.id).then((inv) => inv && setLink({ invite: inv, what: `A new link for ${u.name || u.email}` })),
                        },
                        ...(locked ? [{ id: "unlock", label: "Unlock now", onSelect: () => void unlockUser(u.id) }] : []),
                        { id: "sessions", label: "Sign them out everywhere", disabled: self, onSelect: () => void revokeSessions(u.id) },
                        {
                          id: "disable",
                          label: u.disabled ? "Enable the account" : "Disable the account",
                          danger: !u.disabled,
                          disabled: self,
                          onSelect: () => void disableUser(u.id, !u.disabled),
                        },
                      ]}
                    />
                  </td>
                </tr>
              );
            })}
            {shown.length === 0 && (
              <tr>
                <td colSpan={4} className="ls-muted">
                  {users.length === 0 ? "Nobody yet." : "Nobody matches."}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
      <div className="count-line">
        {users.length} {users.length === 1 ? "person" : "people"}.
      </div>

      <Dialog
        open={inviting}
        title="Invite someone"
        onClose={() => setInviting(false)}
        actions={
          <>
            <button type="button" className="btn" onClick={() => setInviting(false)}>
              Cancel
            </button>
            <button type="button" className="btn solid" onClick={() => void invite()}>
              Make their link
            </button>
          </>
        }
      >
        <p style={{ marginTop: 0 }} className="ls-muted">
          You get a one-time link to give them; it works once and for 24 hours.
        </p>
        <label className="field">
          <span>Name</span>
          <input className="input" value={name} onChange={(e) => setName(e.target.value)} autoFocus />
        </label>
        <label className="field">
          <span>Work email</span>
          <input className="input" type="email" value={email} onChange={(e) => setEmail(e.target.value)} />
        </label>
        <label className="field">
          <span>Role</span>
          <select className="select" value={role} onChange={(e) => setRole(e.target.value as UserRole)}>
            {ROLES.map((r) => (
              <option key={r.role} value={r.role}>
                {r.label}
              </option>
            ))}
          </select>
        </label>
        {error && <div className="error">{error}</div>}
      </Dialog>
      <LinkDialog invite={link?.invite ?? null} what={link?.what ?? ""} onClose={() => setLink(null)} />
    </>
  );
}

function Workspaces() {
  const { workspaces, users, refreshWorkspaces, refreshUsers, createWorkspace } = useSession();
  const [making, setMaking] = useState(false);
  const [name, setName] = useState("");

  useEffect(() => {
    void refreshWorkspaces();
    void refreshUsers();
  }, [refreshWorkspaces, refreshUsers]);

  const shared = workspaces.filter((w) => w.personal_to === null);

  return (
    <>
      <div className="page-head">
        <div>
          <h1 className="page-title">Workspaces</h1>
          <div className="page-sub">Shared workspaces and who is in them. Everyone also has a personal workspace of their own.</div>
        </div>
        <button type="button" className="btn solid tall" onClick={() => setMaking(true)}>
          <PlusIcon size={16} /> New workspace
        </button>
      </div>
      {shared.length === 0 ? (
        <p className="ls-muted" style={{ marginTop: 32 }}>
          No shared workspace yet. Make one, then add people to it with what they may do there.
        </p>
      ) : (
        shared.map((w) => <WorkspaceCard key={w.id} workspace={w} users={users} />)
      )}
      <Dialog
        open={making}
        title="New workspace"
        onClose={() => setMaking(false)}
        actions={
          <>
            <button type="button" className="btn" onClick={() => setMaking(false)}>
              Cancel
            </button>
            <button
              type="button"
              className="btn solid"
              disabled={!name.trim()}
              onClick={() => {
                void createWorkspace(name.trim()).then(() => {
                  setMaking(false);
                  setName("");
                });
              }}
            >
              Make it
            </button>
          </>
        }
      >
        <label className="field" style={{ marginTop: 0 }}>
          <span>Name</span>
          <input className="input" value={name} onChange={(e) => setName(e.target.value)} placeholder="Finance" autoFocus />
        </label>
        <p className="ls-small ls-muted">You own it. Add people afterwards; the assistant proposes changes in a shared workspace rather than making them outright.</p>
      </Dialog>
    </>
  );
}

function WorkspaceCard({ workspace, users }: { workspace: WorkspaceInfo; users: UserInfo[] }) {
  const { setMember, removeMember } = useSession();
  const [adding, setAdding] = useState("");
  const [level, setLevel] = useState<AccessLevel>("edit");
  const members = workspace.members.filter((m): m is { principal: { user: string }; level: AccessLevel } => typeof m.principal === "object" && "user" in m.principal);
  const byId = new Map(users.map((u) => [u.id, u]));
  const candidates = users.filter((u) => !u.disabled && !members.some((m) => m.principal.user === u.id));
  return (
    <div className="table-card">
      <div className="row-item" style={{ borderTop: 0 }}>
        <div className="row-main">
          <div className="row-title">{workspace.name}</div>
          <div className="row-body">
            {members.length} {members.length === 1 ? "person" : "people"} · the assistant {workspace.agent_writes === "proposal" ? "proposes changes here" : "makes changes here directly"}
          </div>
        </div>
      </div>
      <table className="table">
        <tbody>
          {members.map((m) => {
            const u = byId.get(m.principal.user);
            return (
              <tr key={m.principal.user}>
                <td>
                  <div className="person">
                    <span className="avatar small" style={{ background: avatarColour(m.principal.user) }} aria-hidden="true">
                      {initialsOf(u?.name || u?.email || m.principal.user)}
                    </span>
                    <div>
                      <div className="person-name">{u?.name || u?.email || m.principal.user}</div>
                      {u && <div className="person-mail">{u.email}</div>}
                    </div>
                  </div>
                </td>
                <td style={{ width: 180 }}>
                  <select className="select" style={{ height: 36, fontSize: 14 }} value={m.level} onChange={(e) => void setMember(workspace.id, m.principal.user, e.target.value as AccessLevel)} aria-label={`What ${u?.name ?? "they"} may do`}>
                    {LEVELS.map((l) => (
                      <option key={l.level} value={l.level}>
                        {l.label}
                      </option>
                    ))}
                  </select>
                </td>
                <td style={{ textAlign: "right", width: 120 }}>
                  <button type="button" className="link" onClick={() => void removeMember(workspace.id, m.principal.user)}>
                    Remove
                  </button>
                </td>
              </tr>
            );
          })}
          <tr>
            <td>
              <select className="select" style={{ height: 36, fontSize: 14 }} value={adding} onChange={(e) => setAdding(e.target.value)} aria-label="Add a person">
                <option value="">Add a person…</option>
                {candidates.map((u) => (
                  <option key={u.id} value={u.id}>
                    {u.name || u.email}
                  </option>
                ))}
              </select>
            </td>
            <td>
              <select className="select" style={{ height: 36, fontSize: 14 }} value={level} onChange={(e) => setLevel(e.target.value as AccessLevel)} aria-label="What they may do">
                {LEVELS.map((l) => (
                  <option key={l.level} value={l.level}>
                    {l.label}
                  </option>
                ))}
              </select>
            </td>
            <td style={{ textAlign: "right" }}>
              <button
                type="button"
                className="btn"
                disabled={!adding}
                onClick={() => {
                  void setMember(workspace.id, adding, level).then(() => setAdding(""));
                }}
              >
                Add
              </button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  );
}
