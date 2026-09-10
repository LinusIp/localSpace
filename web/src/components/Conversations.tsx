// The conversation list beside the chat (v2 §8): several, switchable, the
// current one highlighted, a new one a click away, an old one deletable.

import { useEffect } from "react";
import { Button, PlusIcon, TrashIcon } from "@localspace/ui";
import { useSession } from "../store";
import { timeAgo } from "../lib/time";

export function Conversations() {
  const { conversations, currentConversation, refreshConversations, newConversation, selectConversation, deleteConversation } =
    useSession();

  useEffect(() => {
    void refreshConversations();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <aside className="conversations">
      <div className="ls-pad-3">
        <Button block onClick={() => void newConversation()}>
          <PlusIcon size={16} /> New chat
        </Button>
      </div>
      <ul className="ls-list ls-grow ls-scroll" style={{ padding: "0 8px 8px" }}>
        {conversations.map((c) => {
          const current = c.id === currentConversation;
          return (
            <li key={c.id} className={`conversation${current ? " current" : ""}`}>
              <button type="button" className="pick" onClick={() => void selectConversation(c.id)}>
                <div className="title ls-truncate">{c.title}</div>
                <div className="ls-tiny ls-faint">
                  {c.messages === 0 ? "empty" : `${c.messages} message${c.messages === 1 ? "" : "s"}`} · {timeAgo(c.updated_ms)}
                </div>
              </button>
              {conversations.length > 1 && (
                <button
                  type="button"
                  className="delete"
                  title="Delete this conversation"
                  aria-label={`Delete ${c.title}`}
                  onClick={() => {
                    if (window.confirm(`Delete "${c.title}"? This cannot be undone.`)) void deleteConversation(c.id);
                  }}
                >
                  <TrashIcon size={14} />
                </button>
              )}
            </li>
          );
        })}
      </ul>
    </aside>
  );
}
