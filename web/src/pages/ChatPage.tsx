// The chat harness's page (v2 §8): the conversation with the agent, its tool
// calls as they happen, approvals it asks for, and the composer.

import { useEffect, useRef, useState } from "react";
import { Loader2, MessageSquare, Paperclip, SendHorizontal, SlidersHorizontal, Square, Check, X } from "lucide-react";
import type { ChatMessage, ToolCallRecord } from "../api/generated";
import { outcomeLine, useSession } from "../store";
import type { LiveToolCall } from "../store";
import { Markdown } from "../components/Markdown";
import { Mark } from "../components/Mark";
import { RightPanel } from "../components/RightPanel";
import { Conversations } from "../components/Conversations";
import { Button, Card } from "../components/ui";

export function ChatPage() {
  const { transcript, streaming, busy, liveCalls, approvals, environment } = useSession();
  const shown = transcript.filter((m) => m.role === "user" || m.role === "assistant");
  const title = shown.find((m) => m.role === "user")?.content.slice(0, 60) ?? "New Chat";
  const bottom = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottom.current?.scrollIntoView({ block: "end" });
  }, [shown.length, streaming, liveCalls.length]);

  return (
    <div className="flex min-h-0 flex-1 gap-4 px-6 pb-6">
      <Card className="flex min-w-0 flex-1 flex-row">
        <Conversations />
        <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-center gap-3 border-b border-line px-6 py-4">
          <span className="text-muted">
            <MessageSquare size={18} />
          </span>
          <h1 className="truncate text-[15px] font-medium">{title}</h1>
          {environment?.focus && (
            <span className="ml-auto text-xs text-faint">focus: {environment.focus}</span>
          )}
        </div>

        <div className="flex-1 overflow-y-auto px-6 py-5">
          {shown.length === 0 && !streaming && (
            <div className="flex h-full flex-col items-center justify-center text-center">
              <Mark size={40} />
              <p className="mt-4 text-sm text-muted">
                {environment?.model
                  ? "Ask for something. The agent works through the harnesses installed here."
                  : "No model is loaded yet. Choose one in Models, then ask for something."}
              </p>
            </div>
          )}
          <div className="mx-auto flex max-w-3xl flex-col gap-5">
            {shown.map((m, i) => (
              <Message key={i} message={m} />
            ))}
            {(streaming || (busy && liveCalls.length > 0)) && (
              <div className="flex gap-3">
                <Avatar />
                <div className="min-w-0 flex-1 rounded-xl border border-line bg-white px-5 py-4">
                  {liveCalls.map((c) => (
                    <LiveCall key={c.id} call={c} />
                  ))}
                  {streaming ? (
                    <Markdown text={streaming} />
                  ) : (
                    <span className="flex items-center gap-2 text-sm text-muted">
                      <Loader2 size={14} className="animate-spin" /> working…
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
      <RightPanel />
    </div>
  );
}

function Avatar() {
  return (
    <span className="mt-1 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-white shadow-sm">
      <Mark size={24} />
    </span>
  );
}

function Message({ message }: { message: ChatMessage }) {
  if (message.role === "user") {
    return (
      <div className="flex justify-end">
        <div className="max-w-[80%] rounded-xl bg-accent-soft px-5 py-3 text-[14px] leading-relaxed text-ink">
          {message.content}
        </div>
      </div>
    );
  }
  return (
    <div className="flex gap-3">
      <Avatar />
      <div className="min-w-0 flex-1 rounded-xl border border-line bg-white px-5 py-4">
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
    <div className="mb-2 flex items-start gap-2 rounded-lg bg-page px-3 py-2 text-xs">
      <span className={ok ? "text-accent" : "text-danger"}>{ok ? <Check size={14} /> : <X size={14} />}</span>
      <span className="font-mono text-ink">{call.tool}</span>
      <span className="text-muted">{outcomeLine(call.outcome)}</span>
    </div>
  );
}

function LiveCall({ call }: { call: LiveToolCall }) {
  return (
    <div className="mb-2 flex items-start gap-2 rounded-lg bg-page px-3 py-2 text-xs">
      {call.outcome ? (
        <span className={"ok" in call.outcome ? "text-accent" : "text-danger"}>
          {"ok" in call.outcome ? <Check size={14} /> : <X size={14} />}
        </span>
      ) : (
        <Loader2 size={14} className="animate-spin text-muted" />
      )}
      <span className="font-mono text-ink">{call.tool}</span>
      <span className="text-muted">{call.outcome ? outcomeLine(call.outcome) : "running"}</span>
    </div>
  );
}

function ApprovalCard({ id, kind, prompt }: { id: string; kind: string; prompt: string }) {
  const approve = useSession((s) => s.approve);
  return (
    <div className="rounded-xl border border-warn/30 bg-warn-soft px-5 py-4">
      <div className="text-xs font-medium uppercase tracking-wide text-warn">{kind.replace("_", " ")}</div>
      <p className="mt-1 text-sm">{prompt}</p>
      <div className="mt-3 flex gap-2">
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
    <div className="border-t border-line p-4">
      <div className="flex items-end gap-2 rounded-xl border border-line bg-white px-3 py-2 focus-within:border-accent">
        <button
          className="rounded-lg p-2 text-faint"
          disabled
          title="Attaching files arrives with retrieval and upload, build step 6"
          aria-label="Attach"
        >
          <Paperclip size={18} />
        </button>
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
          className="max-h-40 min-h-[36px] flex-1 resize-none bg-transparent py-2 text-[14px] outline-none"
        />
        <button
          className="rounded-lg p-2 text-muted hover:text-ink"
          onClick={() => go("models")}
          title="Model settings"
          aria-label="Model settings"
        >
          <SlidersHorizontal size={18} />
        </button>
        {busy ? (
          <button
            className="rounded-lg bg-danger p-2.5 text-white"
            onClick={() => void cancel()}
            title="Stop this turn"
            aria-label="Stop"
          >
            <Square size={16} />
          </button>
        ) : (
          <button
            className="rounded-lg bg-accent p-2.5 text-white disabled:opacity-40"
            onClick={submit}
            disabled={!text.trim()}
            aria-label="Send"
          >
            <SendHorizontal size={16} />
          </button>
        )}
      </div>
      <div className="mt-1.5 px-1 text-[11px] text-faint">Enter sends, Shift+Enter adds a line.</div>
    </div>
  );
}
