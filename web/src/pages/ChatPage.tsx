// The chat harness's page (v2 §8): the conversation with the agent, its tool
// calls as they happen, approvals it asks for, and the composer.

import { useEffect, useRef, useState } from "react";
import { Button, Card, ChatIcon, CheckIcon, ClipIcon, CloseIcon, IconButton, SendIcon, SlidersIcon, SpinnerIcon, StopIcon } from "@localspace/ui";
import type { ChatMessage, ToolCallRecord } from "../api/generated";
import { outcomeLine, useSession } from "../store";
import type { LiveToolCall } from "../store";
import { Markdown } from "../components/Markdown";
import { Mark } from "../components/Mark";
import { RightPanel } from "../components/RightPanel";
import { Conversations } from "../components/Conversations";
import { Panels } from "../components/Panels";

export function ChatPage() {
  const { transcript, streaming, busy, liveCalls, approvals, environment, panels, details } = useSession();
  const shown = transcript.filter((m) => m.role === "user" || m.role === "assistant");
  const title = shown.find((m) => m.role === "user")?.content.slice(0, 60) ?? "New Chat";
  const bottom = useRef<HTMLDivElement>(null);
  // With a panel open the chat becomes a column beside it and the details
  // column steps aside until asked for; nothing about it is lost.
  const withPanels = panels.length > 0;

  useEffect(() => {
    bottom.current?.scrollIntoView({ block: "end" });
  }, [shown.length, streaming, liveCalls.length]);

  return (
    <div className="page-flex">
      <Card className={`chat-column ${withPanels ? "chat-beside" : "chat-alone"}`}>
        {!withPanels && <Conversations />}
        <div className="ls-col ls-grow">
          <div className="chat-head">
            <span className="ls-muted">
              <ChatIcon size={18} />
            </span>
            <h1 className="ls-truncate">{title}</h1>
            {environment?.focus && <span className="ls-ml-auto ls-small ls-faint">focus: {environment.focus}</span>}
          </div>

          <div className="chat-scroll">
            {shown.length === 0 && !streaming && (
              <div className="ls-empty">
                <Mark size={40} />
                <p className="ls-mt-4 ls-muted">
                  {environment?.model
                    ? "Ask for something. The agent works through the harnesses installed here."
                    : "No model is loaded yet. Choose one in Models, then ask for something."}
                </p>
              </div>
            )}
            <div className="chat-thread">
              {shown.map((m, i) => (
                <Message key={i} message={m} />
              ))}
              {(streaming || (busy && liveCalls.length > 0)) && (
                <div className="chat-message">
                  <Avatar />
                  <div className="ls-bubble">
                    {liveCalls.map((c) => (
                      <LiveCall key={c.id} call={c} />
                    ))}
                    {streaming ? (
                      <Markdown text={streaming} />
                    ) : (
                      <span className="ls-row ls-gap-2 ls-muted">
                        <SpinnerIcon size={14} className="ls-spin" /> working…
                      </span>
                    )}
                  </div>
                </div>
              )}
              {approvals.map((a) => (
                <ApprovalCard key={a.id} id={a.id} kind={a.kind} prompt={a.prompt} />
              ))}
              <div ref={bottom} />
            </div>
          </div>

          <Composer />
        </div>
      </Card>
      {withPanels && <Panels />}
      {(!withPanels || details) && <RightPanel />}
    </div>
  );
}

function Avatar() {
  return (
    <span className="chat-avatar">
      <Mark size={24} />
    </span>
  );
}

function Message({ message }: { message: ChatMessage }) {
  if (message.role === "user") {
    return (
      <div className="ls-row ls-end">
        <div className="ls-bubble-user">{message.content}</div>
      </div>
    );
  }
  return (
    <div className="chat-message">
      <Avatar />
      <div className="ls-bubble">
        {message.tool_calls.map((c) => (
          <ToolCall key={c.id} call={c} />
        ))}
        {message.content && <Markdown text={message.content} />}
      </div>
    </div>
  );
}

function ToolCall({ call }: { call: ToolCallRecord }) {
  const ok = "ok" in call.outcome;
  return (
    <div className="chat-call">
      <span className={ok ? "ls-accent" : "ls-danger"}>{ok ? <CheckIcon size={14} /> : <CloseIcon size={14} />}</span>
      <span className="ls-mono">{call.tool}</span>
      <span className="ls-muted">{outcomeLine(call.outcome)}</span>
    </div>
  );
}

function LiveCall({ call }: { call: LiveToolCall }) {
  return (
    <div className="chat-call">
      {call.outcome ? (
        <span className={"ok" in call.outcome ? "ls-accent" : "ls-danger"}>{"ok" in call.outcome ? <CheckIcon size={14} /> : <CloseIcon size={14} />}</span>
      ) : (
        <SpinnerIcon size={14} className="ls-spin ls-muted" />
      )}
      <span className="ls-mono">{call.tool}</span>
      <span className="ls-muted">{call.outcome ? outcomeLine(call.outcome) : "running"}</span>
    </div>
  );
}

function ApprovalCard({ id, kind, prompt }: { id: string; kind: string; prompt: string }) {
  const approve = useSession((s) => s.approve);
  return (
    <div className="chat-approval">
      <div className="chat-approval-kind">{kind.replace("_", " ")}</div>
      <p className="ls-mt-1" style={{ marginBottom: 0 }}>
        {prompt}
      </p>
      <div className="ls-row ls-gap-2 ls-mt-3">
        <Button kind="primary" onClick={() => void approve(id, true)}>
          Allow
        </Button>
        <Button onClick={() => void approve(id, false)}>Deny</Button>
      </div>
    </div>
  );
}

function Composer() {
  const { busy, send, cancel, go, environment } = useSession();
  const [text, setText] = useState("");
  const box = useRef<HTMLTextAreaElement>(null);

  const submit = () => {
    const trimmed = text.trim();
    if (!trimmed || busy) return;
    setText("");
    void send(trimmed);
  };

  return (
    <div className="composer-wrap">
      <div className="ls-composer">
        <IconButton label="Attach" quiet disabled title="Attaching files arrives with retrieval and upload, build step 6">
          <ClipIcon size={18} />
        </IconButton>
        <textarea
          ref={box}
          rows={1}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              submit();
            }
          }}
          placeholder={environment?.model ? "Type your message…" : "Choose a model in Models first, then type here"}
          style={{ minHeight: 36, padding: "8px 0" }}
        />
        <IconButton label="Model settings" quiet onClick={() => go("models")}>
          <SlidersIcon size={18} />
        </IconButton>
        {busy ? (
          <button type="button" className="send-button stop" onClick={() => void cancel()} title="Stop this turn" aria-label="Stop">
            <StopIcon size={16} />
          </button>
        ) : (
          <button type="button" className="send-button" onClick={submit} disabled={!text.trim()} aria-label="Send">
            <SendIcon size={16} />
          </button>
        )}
      </div>
      <div className="ls-mt-1 ls-tiny ls-faint" style={{ padding: "0 4px" }}>
        Enter sends, Shift+Enter adds a line.
      </div>
    </div>
  );
}
