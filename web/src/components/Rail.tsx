// The left rail (the app screens, 1–7): the mark and the name, New chat, the
// chats, the workspace's pages, and Admin, Settings and Help at the bottom.
// It collapses to icons — Ctrl/Cmd+B, or the toggle — and the choice is
// remembered with the user, not the browser.

import { useEffect, useState } from "react";
import { BoardIcon, ChatIcon, Dialog, FileIcon, HelpIcon, PeopleIcon, PlusIcon, SettingsIcon, SidebarIcon, SpinnerIcon, StoreIcon } from "@localspace/ui";
import { documents } from "../api/client";
import { useSession } from "../store";
import { bus } from "../surfaces/bus";
import { Brand } from "./Mark";
import { timeAgo } from "../lib/time";

const CHATS_SHOWN = 8;

/**
 * Whether this workspace holds a document yet. Until it does, "Documents" is
 * not in the rail: on a fresh install it led to an empty page (nothing is
 * visible unless it works; docs/DECISIONS.md, 2026-09-20). A board becomes a
 * document the moment it is opened, and the rail hears of it.
 */
function useHasDocuments(installed: number): boolean {
  const [has, setHas] = useState(false);
  useEffect(() => {
    const look = () => {
      void documents()
        .then((docs) => setHas(docs.length > 0))
        .catch(() => setHas(false));
    };
    look();
    return bus.on("doc_changed", look);
  }, [installed]);
  return has;
}

/** Every conversation, when the rail shows only the latest few. */
function AllChats({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { conversations, currentConversation, selectConversation, deleteConversation } = useSession();
  return (
    <Dialog open={open} title="All chats" onClose={onClose}>
      <ul className="chat-list">
        {conversations.map((c) => (
          <li key={c.id}>
            <button
              type="button"
              className={`rail-chat${c.id === currentConversation ? " on" : ""}`}
              onClick={() => {
                onClose();
                void selectConversation(c.id);
              }}
            >
              {c.messages === 0 ? "New chat" : c.title}
            </button>
            <span className="when">{timeAgo(c.updated_ms)}</span>
            {conversations.length > 1 && (
              <button
                type="button"
                className="link"
                onClick={() => {
                  if (window.confirm(`Delete "${c.title}"? This cannot be undone.`)) void deleteConversation(c.id);
                }}
              >
                Delete
              </button>
            )}
          </li>
        ))}
      </ul>
    </Dialog>
  );
}

export function Rail() {
  const { me, page, board, environment, conversations, currentConversation, turns, railCollapsed, boardRailOpen, setRail, go, goSettings, goAdmin, openBoard, newConversation, selectConversation } =
    useSession();
  const admin = me?.roles.includes("admin") && me.topology === "organisation";
  // On the board the rail is icons by default (screen 4); elsewhere it is
  // what the person chose, remembered with them.
  const collapsed = page === "board" ? !boardRailOpen : railCollapsed;
  const harnesses = (environment?.harnesses ?? []).filter((h) => h.views.some((v) => v.kind === "web"));
  const [allChats, setAllChats] = useState(false);
  const hasDocuments = useHasDocuments(harnesses.length);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "b") {
        e.preventDefault();
        const s = useSession.get();
        void setRail(s.page === "board" ? s.boardRailOpen : !s.railCollapsed);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setRail]);

  if (collapsed) {
    return (
      <nav className="rail collapsed" aria-label="Main">
        <button type="button" className="rail-toggle" onClick={() => void setRail(false)} aria-label="Show the sidebar" title="Show the sidebar (Ctrl+B)">
          <Brand size={22} nameless />
        </button>
        <button type="button" className={`rail-icon${page === "chat" ? " on" : ""}`} onClick={() => go("chat")} aria-label="Chat" title="Chat">
          <ChatIcon size={18} />
        </button>
        {harnesses.map((h) => (
          <button
            key={h.id}
            type="button"
            className={`rail-icon${page === "board" && board === h.id ? " on" : ""}`}
            onClick={() => openBoard(h.id)}
            aria-label={h.title}
            title={h.title}
          >
            <BoardIcon size={18} />
          </button>
        ))}
        {hasDocuments && (
          <button type="button" className={`rail-icon${page === "documents" ? " on" : ""}`} onClick={() => go("documents")} aria-label="Documents" title="Documents">
            <FileIcon size={18} />
          </button>
        )}
        <button type="button" className={`rail-icon${page === "store" ? " on" : ""}`} onClick={() => go("store")} aria-label="Store" title="Store">
          <StoreIcon size={18} />
        </button>
        <div className="rail-spacer" />
        <div className="rail-bottom">
          {admin && (
            <button type="button" className={`rail-icon${page === "admin" ? " on" : ""}`} onClick={() => goAdmin("people")} aria-label="Admin" title="Admin">
              <PeopleIcon size={18} />
            </button>
          )}
          <button type="button" className={`rail-icon${page === "settings" ? " on" : ""}`} onClick={() => goSettings("general")} aria-label="Settings" title="Settings">
            <SettingsIcon size={18} />
          </button>
          <button type="button" className={`rail-icon${page === "help" ? " on" : ""}`} onClick={() => go("help")} aria-label="Help" title="Help">
            <HelpIcon size={18} />
          </button>
        </div>
      </nav>
    );
  }

  const shown = conversations.slice(0, CHATS_SHOWN);
  const hidden = conversations.length - shown.length;

  return (
    <nav className="rail" aria-label="Main">
      <div className="rail-head">
        <Brand size={22} />
        <button type="button" className="rail-toggle" onClick={() => void setRail(true)} aria-label="Hide the sidebar" title="Hide the sidebar (Ctrl+B)">
          <SidebarIcon size={18} />
        </button>
      </div>
      <button type="button" className="rail-new" onClick={() => void newConversation()}>
        <PlusIcon size={16} /> New chat
      </button>

      <div className="rail-section chats">
        <div className="rail-cap">Chats</div>
        {conversations.length === 0 || (conversations.length === 1 && conversations[0].messages === 0) ? (
          <div className="rail-empty">Your conversations will appear here.</div>
        ) : (
          <div className="rail-chats">
            {shown.map((c) => (
              <button
                key={c.id}
                type="button"
                className={`rail-chat${c.id === currentConversation && page === "chat" ? " on" : ""}`}
                title={turns[c.id] ? `${c.title} (an answer is being written)` : c.title}
                onClick={() => void selectConversation(c.id)}
              >
                {/* Its answer is being written, or waits: seen from any other chat. */}
                {turns[c.id] && <SpinnerIcon size={12} className="ls-spin rail-writing" />}
                {c.messages === 0 ? "New chat" : c.title}
              </button>
            ))}
            {hidden > 0 && (
              <button type="button" className="rail-chat more" onClick={() => setAllChats(true)}>
                All chats
              </button>
            )}
          </div>
        )}
      </div>
      <AllChats open={allChats} onClose={() => setAllChats(false)} />

      <div className="rail-section">
        <div className="rail-cap">Workspace</div>
        <WorkspaceName />
        {harnesses.map((h) => (
          <button key={h.id} type="button" className={`rail-item${page === "board" && board === h.id ? " on" : ""}`} onClick={() => openBoard(h.id)}>
            <BoardIcon size={17} /> {h.title}
          </button>
        ))}
        {hasDocuments && (
          <button type="button" className={`rail-item${page === "documents" ? " on" : ""}`} onClick={() => go("documents")}>
            <FileIcon size={17} /> Documents
          </button>
        )}
        <button type="button" className={`rail-item${page === "store" ? " on" : ""}`} onClick={() => go("store")}>
          <StoreIcon size={17} /> Store
        </button>
      </div>

      <div className="rail-bottom">
        {admin && (
          <button type="button" className={`rail-item${page === "admin" ? " on" : ""}`} onClick={() => goAdmin("people")}>
            <PeopleIcon size={17} /> Admin
          </button>
        )}
        <button type="button" className={`rail-item${page === "settings" ? " on" : ""}`} onClick={() => goSettings("general")}>
          <SettingsIcon size={17} /> Settings
        </button>
        <button type="button" className={`rail-item${page === "help" ? " on" : ""}`} onClick={() => go("help")}>
          <HelpIcon size={17} /> Help
        </button>
      </div>
    </nav>
  );
}

/** In an organisation the workspace the user is in, and the way to another they belong to. */
function WorkspaceName() {
  const { me, environment, workspaces, refreshWorkspaces, selectWorkspace } = useSession();
  const organisation = me?.topology === "organisation";
  useEffect(() => {
    if (organisation) void refreshWorkspaces();
  }, [organisation, refreshWorkspaces]);
  if (!organisation || !environment) return null;
  const mine = workspaces.filter((w) => w.mine !== null);
  if (mine.length <= 1) return null;
  return (
    <select
      className="rail-workspace"
      aria-label="Workspace"
      value={environment.workspace_id}
      onChange={(e) => {
        if (e.target.value !== environment.workspace_id) void selectWorkspace(e.target.value);
      }}
    >
      {mine.map((w) => (
        <option key={w.id} value={w.id}>
          {w.personal_to ? "Personal" : w.name}
        </option>
      ))}
    </select>
  );
}
