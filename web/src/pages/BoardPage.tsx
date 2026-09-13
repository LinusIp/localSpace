// The board (screen 4): canvas first. The rail is icons, the board fills the
// page under a white bar with its name, whether it is saved, the people on
// it, Export and Share; the chat is a drawer on the right. The board itself
// is the harness's own surface, in its frame on its own origin; its tools
// and zoom float inside that frame.

import { useEffect, useState } from "react";
import { ChevronDownIcon, ChevronLeftIcon, CloseIcon, Dialog, Menu, MessageIcon } from "@localspace/ui";
import type { Present } from "../api/generated";
import { avatarColour, initialsOf, useSession } from "../store";
import type { Panel } from "../store";
import { HarnessFrame } from "../components/HarnessFrame";
import { WidgetView } from "../components/WidgetView";
import { NetworkChip, ReadinessChip } from "../components/TopBar";
import { Conversation } from "./ChatPage";
import { bus } from "../surfaces/bus";

/** How often this window says it is still on the board; Core forgets a window quiet for longer. */
const ANNOUNCE_EVERY_MS = 20_000;
const SHOWN_PEOPLE = 4;

export function BoardPage() {
  const { me, board, panels, environment, pendingWrites, chatBeside, toggleChatBeside, go, docJson, present, announcePresence, workspaces, refreshWorkspaces } = useSession();
  const panel = panels.find((p) => p.harness === board) ?? null;
  const harness = environment?.harnesses.find((h) => h.id === board) ?? null;
  const [docTitle, setDocTitle] = useState<string | null>(null);
  const [sharing, setSharing] = useState(false);
  const organisation = me?.topology === "organisation";

  // The document's own title when it has one, as the board's name.
  useEffect(() => {
    if (!board) return;
    let cancelled = false;
    const read = () =>
      void docJson(board).then((json) => {
        if (cancelled || !json || typeof json !== "object" || Array.isArray(json)) return;
        const title = (json as { title?: unknown }).title;
        setDocTitle(typeof title === "string" && title.trim() ? title.trim() : null);
      });
    read();
    const off = bus.on("doc_changed", read);
    return () => {
      cancelled = true;
      off();
    };
  }, [board, docJson]);

  // This window says which board it shows, again every so often, and
  // nothing when it leaves; everyone on the board hears who is there.
  useEffect(() => {
    if (!board) return;
    void announcePresence(board);
    const timer = setInterval(() => void announcePresence(board), ANNOUNCE_EVERY_MS);
    const leave = () => void announcePresence(null);
    window.addEventListener("pagehide", leave);
    return () => {
      clearInterval(timer);
      window.removeEventListener("pagehide", leave);
      leave();
    };
  }, [board, announcePresence]);

  useEffect(() => {
    if (organisation) void refreshWorkspaces();
  }, [organisation, refreshWorkspaces]);

  if (!panel || !harness) {
    return (
      <div className="ls-empty">
        <p className="ls-muted">There is no board open.</p>
      </div>
    );
  }

  const people = present[panel.harness] ?? [];
  const workspace = workspaces.find((w) => w.current) ?? null;
  // A board in a shared workspace is shared with the workspace; a personal
  // workspace takes no members, so there is nothing to share it with.
  const shareable = organisation && workspace !== null && workspace.personal_to === null;

  return (
    <>
      <header className="topbar board-bar">
        <button type="button" className="rail-toggle" onClick={() => go("chat")} aria-label="Back to the chat" title="Back to the chat">
          <ChevronLeftIcon size={18} />
        </button>
        <span className="topbar-title">{docTitle ?? harness.title}</span>
        <span className="board-status">{pendingWrites > 0 ? "Saving…" : "All changes saved"}</span>
        <div className="topbar-right">
          <ReadinessChip />
          <NetworkChip />
          {organisation && people.length > 0 && <People people={people} />}
          {panel.kind === "web" && <ExportMenu panel={panel} />}
          {shareable && (
            <button type="button" className="btn solid" onClick={() => setSharing(true)}>
              Share
            </button>
          )}
        </div>
      </header>
      <div className="board">
        <div className="board-stage">
          {panels.map((p) => (
            <div key={p.key} className="board-layer" hidden={p.key !== panel.key}>
              {p.kind === "web" && <HarnessFrame panel={p} active={p.key === panel.key} />}
              {p.kind === "widgets" && <WidgetView harness={p.harness} view={p.view} />}
              {(p.kind === "egui" || p.kind === "stream" || p.kind === "native") && (
                <div className="ls-empty">
                  <p className="ls-muted">{p.title} cannot be shown in the browser yet.</p>
                </div>
              )}
            </div>
          ))}
          {!chatBeside && (
            <button type="button" className="drawer-tab" onClick={toggleChatBeside} aria-label="Open the chat">
              <span>
                <MessageIcon size={15} style={{ transform: "rotate(-90deg)" }} /> Chat
              </span>
            </button>
          )}
        </div>
        {chatBeside && (
          <aside className="drawer" aria-label="Chat">
            <div className="drawer-head">
              Chat
              <button type="button" className="rail-toggle" onClick={toggleChatBeside} aria-label="Close the chat">
                <CloseIcon size={16} />
              </button>
            </div>
            <Conversation compact />
          </aside>
        )}
      </div>
      {shareable && workspace && (
        <ShareDialog open={sharing} board={panel.harness} title={docTitle ?? harness.title} workspaceName={workspace.name} members={workspace.members.length} admin={me?.roles.includes("admin") ?? false} onClose={() => setSharing(false)} />
      )}
    </>
  );
}

/** Who has the board open right now, as the screens show them: a row of initials. */
function People({ people }: { people: Present[] }) {
  const names = people.map((p) => p.name).join(", ");
  const shown = people.slice(0, SHOWN_PEOPLE);
  const rest = people.length - shown.length;
  return (
    <div className="stack" role="img" aria-label={`On this board: ${names}`} title={`On this board: ${names}`}>
      {shown.map((p) => (
        <span key={p.user} className="avatar small" style={{ background: avatarColour(p.user) }}>
          {initialsOf(p.name)}
        </span>
      ))}
      {rest > 0 && <span className="avatar small more">+{rest}</span>}
    </div>
  );
}

/** Export: the surface renders the picture and hands it to Core, which keeps it and says what it named it. */
function ExportMenu({ panel }: { panel: Panel }) {
  const ask = (kind: "image.v1" | "svg.v1") => bus.emit("command", { harness: panel.harness, view: panel.view, name: "export", args: { kind } });
  return (
    <Menu
      align="right"
      trigger={(open, isOpen) => (
        <button type="button" className={`btn${isOpen ? " on" : ""}`} onClick={open}>
          Export <ChevronDownIcon size={13} />
        </button>
      )}
      items={[
        { id: "png", label: "As a PNG image", onSelect: () => ask("image.v1") },
        { id: "svg", label: "As an SVG drawing", onSelect: () => ask("svg.v1") },
      ]}
    />
  );
}

/** Who can open the board, and a link that opens it for them. */
function ShareDialog({ open, board, title, workspaceName, members, admin, onClose }: { open: boolean; board: string; title: string; workspaceName: string; members: number; admin: boolean; onClose: () => void }) {
  const [copied, setCopied] = useState(false);
  const link = `${location.origin}/board/${encodeURIComponent(board)}`;
  return (
    <Dialog
      open={open}
      title={`Share ${title}`}
      onClose={onClose}
      actions={
        <button type="button" className="btn solid" onClick={onClose}>
          Done
        </button>
      }
    >
      <p style={{ marginTop: 0 }}>
        Everyone in {workspaceName} can open this board{members > 0 ? `: ${members} ${members === 1 ? "person" : "people"}` : ""}.{" "}
        {admin ? "Add someone to the workspace under Admin and they can open it too." : "An administrator can add someone to the workspace."}
      </p>
      <div className="ls-row ls-gap-2">
        <input className="input mono" readOnly value={link} onFocus={(e) => e.currentTarget.select()} aria-label="The link to this board" />
        <button
          type="button"
          className="btn"
          onClick={() => {
            void navigator.clipboard?.writeText(link).then(() => setCopied(true));
          }}
        >
          {copied ? "Copied" : "Copy link"}
        </button>
      </div>
      <p className="ls-small ls-muted">The link opens this board for anyone who can already sign in here; it gives nobody new a way in.</p>
    </Dialog>
  );
}
