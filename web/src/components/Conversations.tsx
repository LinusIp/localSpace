// The conversation list beside the chat (v2 §8): several, switchable, the
// current one highlighted, a new one a click away, an old one deletable.

import { useEffect } from "react";
import { Plus, Trash2 } from "lucide-react";
import { useSession } from "../store";
import { timeAgo } from "./ui";

export function Conversations() {
  const { conversations, currentConversation, refreshConversations, newConversation, selectConversation, deleteConversation } =
    useSession();

  useEffect(() => {
    void refreshConversations();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <aside className="flex w-60 shrink-0 flex-col border-r border-line">
      <div className="p-3">
        <button
          className="flex w-full items-center justify-center gap-2 rounded-lg border border-line bg-white px-3 py-2 text-sm font-medium hover:bg-page"
          onClick={() => void newConversation()}
        >
          <Plus size={16} /> New chat
        </button>
      </div>
      <ul className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
        {conversations.map((c) => {
          const active = c.id === currentConversation;
          return (
            <li key={c.id} className="group relative">
              <button
                onClick={() => void selectConversation(c.id)}
                className={`w-full rounded-lg px-3 py-2 text-left ${active ? "bg-accent-soft" : "hover:bg-page"}`}
              >
                <div className={`truncate text-sm ${active ? "font-medium text-accent" : "text-ink"}`}>{c.title}</div>
                <div className="text-[11px] text-faint">
                  {c.messages === 0 ? "empty" : `${c.messages} message${c.messages === 1 ? "" : "s"}`} · {timeAgo(c.updated_ms)}
                </div>
              </button>
              {conversations.length > 1 && (
                <button
                  className="absolute right-2 top-2 hidden rounded p-1 text-faint hover:bg-white hover:text-danger group-hover:block"
                  title="Delete this conversation"
                  aria-label={`Delete ${c.title}`}
                  onClick={() => {
                    if (window.confirm(`Delete "${c.title}"? This cannot be undone.`)) void deleteConversation(c.id);
                  }}
                >
                  <Trash2 size={14} />
                </button>
              )}
            </li>
          );
        })}
      </ul>
    </aside>
  );
}
