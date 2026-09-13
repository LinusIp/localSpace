// The board (screen 4): canvas first. The rail is icons, the board fills the
// page, and the chat is a drawer on the right. The board itself is the
// harness's own surface, in its frame on its own origin.

import { useEffect, useState } from "react";
import { ChevronLeftIcon, CloseIcon, FitIcon, MessageIcon, MinusIcon, PlusIcon } from "@localspace/ui";
import { useSession } from "../store";
import { HarnessFrame } from "../components/HarnessFrame";
import { WidgetView } from "../components/WidgetView";
import { NetworkChip, Person, ReadinessChip } from "../components/TopBar";
import { Conversation } from "./ChatPage";
import { bus } from "../surfaces/bus";

export function BoardPage() {
  const { board, panels, environment, pendingWrites, chatBeside, toggleChatBeside, setZoom, go, docJson } = useSession();
  const panel = panels.find((p) => p.harness === board) ?? null;
  const harness = environment?.harnesses.find((h) => h.id === board) ?? null;
  const [docTitle, setDocTitle] = useState<string | null>(null);

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

  if (!panel || !harness) {
    return (
      <div className="ls-empty">
        <p className="ls-muted">There is no board open.</p>
      </div>
    );
  }

  const zoom = Math.round(panel.zoom * 100);
  return (
    <>
      <header className="topbar board-bar">
        <button type="button" className="rail-toggle" onClick={() => go("chat")} aria-label="Back to the chat" title="Back to the chat">
          <ChevronLeftIcon size={18} />
        </button>
        <span className="topbar-title">{docTitle ?? harness.title}</span>
        <span className="board-status">{pendingWrites > 0 ? "Saving…" : "All changes saved"}</span>
        <div className="topbar-right">
          {panel.kind === "web" && (
            <span className="board-zoom" aria-label="Zoom">
              <button type="button" onClick={() => setZoom(panel.key, panel.zoom - 0.1)} aria-label="Zoom out" title="Zoom out">
                <MinusIcon size={15} />
              </button>
              <button type="button" onClick={() => setZoom(panel.key, 1)} title="Back to 100%" className="ls-tabular">
                {zoom}%
              </button>
              <button type="button" onClick={() => setZoom(panel.key, panel.zoom + 0.1)} aria-label="Zoom in" title="Zoom in">
                <PlusIcon size={15} />
              </button>
              <button type="button" onClick={() => bus.emit("command", { harness: panel.harness, view: panel.view, name: "fit", args: {} })} aria-label="Fit the board" title="Fit the board">
                <FitIcon size={15} />
              </button>
            </span>
          )}
          <ReadinessChip />
          <NetworkChip />
          <Person />
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
    </>
  );
}
