// The chat (screens 2 and 3): a greeting and three things to try until the
// first message, then the conversation. Tool calls appear as what they did,
// in words; anything that needs the person's say-so appears as a card.

import { useEffect, useRef, useState } from "react";
import { ArrowUpIcon, CheckIcon, ChevronDownIcon, CloseIcon, SpinnerIcon, StopIcon } from "@localspace/ui";
import type { ChatMessage, ToolCallRecord } from "../api/generated";
import { initialsOf, outcomeLine, useSession } from "../store";
import type { LiveToolCall } from "../store";
import { Markdown } from "../components/Markdown";
import { Mark } from "../components/Mark";
import { TopBar } from "../components/TopBar";
import { greeting } from "../lib/time";
import { modelLabel } from "../lib/models";

/** The answer in progress in the chat on screen: its words so far, where it
 *  stands, the tool calls at work for it, and whether it carries on a
 *  stopped answer. */
function useChatInProgress() {
  const chat = useSession((s) => s.currentConversation);
  const streaming = useSession((s) => s.streaming[chat] ?? "");
  const turn = useSession((s) => s.turns[chat] ?? null);
  const liveCalls = useSession((s) => s.liveCalls[chat] ?? NO_CALLS);
  const continuing = useSession((s) => s.continuing[chat] ?? false);
  return { streaming, turn, liveCalls, continuing };
}

const NO_CALLS: LiveToolCall[] = [];

/** Said while an answer waits for the person's other chat (ruled 2026-09-23):
 *  without the second sentence people send the message again. */
const WAITS_FOR_ANOTHER_CHAT = "Waiting for the answer in your other chat to finish. This one will start by itself.";

/** Said while every slot of the organisation's server is taken (ruled
 *  2026-09-24): "server", since the person has no choice to make about the
 *  model here. */
const WAITS_FOR_THE_SERVER = "The server is answering other people right now. This one will start by itself.";

/** Said where a proposal the person declined stood (ruled 2026-09-24). */
const DECLINED = "You declined this change. The answer stopped here.";

/** What the chat shows: the person's messages, the answers, and the
 *  proposals the person declined; other tool records are the model's. */
function shownOf(transcript: ChatMessage[]): ChatMessage[] {
  return transcript.filter(
    (m) => m.role === "user" || m.role === "assistant" || (m.role === "tool" && m.tool_calls.some((c) => "declined" in c.outcome)),
  );
}

export function ChatPage() {
  const { conversations, currentConversation, transcript } = useSession();
  const { streaming } = useChatInProgress();
  const shown = shownOf(transcript);
  const empty = shown.length === 0 && !streaming;
  const title = conversations.find((c) => c.id === currentConversation)?.title;
  return (
    <>
      <TopBar title={empty ? undefined : title} bordered={!empty} />
      <Conversation />
    </>
  );
}

/** The conversation itself: the page's body, and the board's drawer. */
export function Conversation({ compact }: { compact?: boolean }) {
  const { transcript, approvals } = useSession();
  const { streaming, turn, liveCalls, continuing } = useChatInProgress();
  const shown = shownOf(transcript);
  const bottom = useRef<HTMLDivElement>(null);
  const [draft, setDraft] = useState("");
  const empty = shown.length === 0 && !streaming;
  // Carrying on a stopped answer: its new words join it, in its place.
  const last = shown[shown.length - 1];
  const joining = continuing && last?.role === "assistant" && last.stopped;

  useEffect(() => {
    bottom.current?.scrollIntoView({ block: "end" });
  }, [shown.length, streaming, liveCalls.length, turn]);

  if (empty) {
    return (
      <div className="chat">
        <div className="chat-hero">
          <Hero compact={compact} onStarter={setDraft} />
          <div className="composer-column">
            <Composer draft={draft} onDraft={setDraft} placeholder="Ask anything…" />
            <Footnote />
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="chat">
      <div className="chat-scroll">
        <div className="chat-thread">
          {shown.map((m, i) => {
            const isLast = i === shown.length - 1;
            return (
              <Message
                key={i}
                message={m}
                joined={isLast && joining ? streaming : null}
                mayContinue={isLast && !turn}
              />
            );
          })}
          {turn && !joining && (
            <div className="message">
              <AssistantAvatar />
              <div className="message-body">
                {liveCalls.map((c) => (
                  <LiveWork key={c.id} call={c} />
                ))}
                {turn === "waits_for_another_chat" ? (
                  <span className="message-work">
                    <SpinnerIcon size={14} className="ls-spin" /> {WAITS_FOR_ANOTHER_CHAT}
                  </span>
                ) : streaming ? (
                  <div className="prose-chat">
                    <Markdown text={streaming} />
                  </div>
                ) : turn === "writing" ? (
                  <span className="message-work">
                    <SpinnerIcon size={14} className="ls-spin" /> Thinking…
                  </span>
                ) : (
                  turn === "waits_for_the_model" && (
                    <span className="message-work">
                      <SpinnerIcon size={14} className="ls-spin" /> {WAITS_FOR_THE_SERVER}
                    </span>
                  )
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
      <div className="chat-composer">
        <div className="composer-column">
          <Composer draft={draft} onDraft={setDraft} placeholder="Ask a follow-up…" />
        </div>
      </div>
    </div>
  );
}

function Hero({ compact, onStarter }: { compact?: boolean; onStarter: (text: string) => void }) {
  const { me, environment } = useSession();
  const first = (me?.name || me?.user || "").trim().split(/\s+/)[0] ?? "";
  const boardInstalled = environment?.harnesses.some((h) => h.id === "io.localspace.whiteboard") ?? false;
  const starters = [
    { title: "Summarise text", body: "Paste a document's text and ask for the key points.", text: "Summarise the key points of the following text:\n\n" },
    { title: "Draft a reply", body: "Paste a message and say how to answer it.", text: "Draft a reply to the following message. Keep it short and polite:\n\n" },
    boardInstalled
      ? { title: "Plan on the board", body: "Ask for a plan and it goes onto your whiteboard as notes.", text: "Put a plan for this on the whiteboard as sticky notes: " }
      : { title: "Explain something", body: "Ask a question in your own words.", text: "Explain, in plain words: " },
  ];
  return (
    <>
      <div>
        <div className="hero-title">
          {greeting()}
          {first ? `, ${first}` : ""}
        </div>
        <div className="hero-sub">What would you like to do?</div>
      </div>
      {!compact && (
        <div className="starters">
          {starters.map((s) => (
            <button key={s.title} type="button" className="starter" onClick={() => onStarter(s.text)}>
              <ArrowUpIcon size={19} className="ls-accent" style={{ transform: "rotate(45deg)" }} />
              <div className="starter-title">{s.title}</div>
              <div className="starter-body">{s.body}</div>
            </button>
          ))}
        </div>
      )}
    </>
  );
}

function Footnote() {
  const { me, environment } = useSession();
  if (!environment?.model) return null;
  const where = environment.engine.running
    ? me?.topology === "organisation"
      ? "Answers come from the model running on your organisation's server."
      : "Answers come from the model running on this computer."
    : "Answers come from the model chosen in Settings.";
  return <div className="composer-note">{where}</div>;
}

function AssistantAvatar() {
  return (
    <span className="avatar small soft" aria-hidden="true">
      <Mark size={16} color="#1D7A55" />
    </span>
  );
}

function PersonAvatar() {
  const me = useSession((s) => s.me);
  return (
    <span className="avatar small" aria-hidden="true">
      {initialsOf(me?.name || me?.email || me?.user)}
    </span>
  );
}

/**
 * One message. An answer that stopped says so under its words, with
 * *Continue* when it is the chat's last and nothing is in progress; while it
 * is carried on, `joined` holds the new words, shown in its place.
 */
function Message({ message, joined, mayContinue }: { message: ChatMessage; joined: string | null; mayContinue: boolean }) {
  const continueAnswer = useSession((s) => s.continueAnswer);
  if (message.role === "tool") {
    return (
      <div className="message">
        <AssistantAvatar />
        <div className="message-body">
          <div className="message-stopped">{DECLINED}</div>
        </div>
      </div>
    );
  }
  if (message.role === "user") {
    return (
      <div className="message">
        <PersonAvatar />
        <div className="message-body user ls-pre">{message.content}</div>
      </div>
    );
  }
  const text = joined === null ? message.content : message.content + joined;
  return (
    <div className="message">
      <AssistantAvatar />
      <div className="message-body">
        {message.tool_calls.map((c) => (
          <Work key={c.id} call={c} />
        ))}
        {text && (
          <div className="prose-chat">
            <Markdown text={text} />
          </div>
        )}
        {joined === null && message.stopped && (
          <div className="message-stopped">
            The answer stopped here.
            {mayContinue && (
              <>
                {" "}
                <button type="button" className="link green" onClick={() => void continueAnswer()}>
                  Continue
                </button>
              </>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

/** What a tool call did, in the words Core gave back; never the tool's name. */
function workLine(tool: string, outcome: ToolCallRecord["outcome"] | null): string {
  if (!outcome) return "Working…";
  if ("ok" in outcome) {
    const summary = outcome.ok.diff_summary.trim();
    return summary ? summary.charAt(0).toUpperCase() + summary.slice(1) : "Done";
  }
  const line = outcomeLine(outcome);
  return line.charAt(0).toUpperCase() + line.slice(1) || tool;
}

function Work({ call }: { call: ToolCallRecord }) {
  const ok = "ok" in call.outcome;
  return (
    <div className="message-work">
      <span className={ok ? "ls-accent" : "ls-danger"}>{ok ? <CheckIcon size={14} /> : <CloseIcon size={14} />}</span>
      {workLine(call.tool, call.outcome)}
    </div>
  );
}

function LiveWork({ call }: { call: LiveToolCall }) {
  return (
    <div className="message-work">
      {call.outcome ? (
        <span className={"ok" in call.outcome ? "ls-accent" : "ls-danger"}>{"ok" in call.outcome ? <CheckIcon size={14} /> : <CloseIcon size={14} />}</span>
      ) : (
        <SpinnerIcon size={14} className="ls-spin" />
      )}
      {workLine(call.tool, call.outcome)}
    </div>
  );
}

function ApprovalCard({ id, kind, prompt }: { id: string; kind: string; prompt: string }) {
  const approve = useSession((s) => s.approve);
  return (
    <div className="approval">
      <div className="approval-kind">{kind === "tool_confirm" ? "The assistant asks" : kind.replace("_", " ")}</div>
      <p className="ls-mt-1" style={{ marginBottom: 0 }}>
        {prompt}
      </p>
      <div className="ls-row ls-gap-2 ls-mt-3">
        <button type="button" className="btn solid" onClick={() => void approve(id, true)}>
          Allow
        </button>
        <button type="button" className="btn" onClick={() => void approve(id, false)}>
          Don't allow
        </button>
      </div>
    </div>
  );
}

function Composer({ draft, onDraft, placeholder }: { draft: string; onDraft: (text: string) => void; placeholder: string }) {
  const { send, cancel, environment, catalogModels, goSettings } = useSession();
  // This chat's answer is being written, or waits: Stop, for this chat only.
  const busy = useChatInProgress().turn !== null;
  const box = useRef<HTMLTextAreaElement>(null);
  const model = environment?.model ?? null;

  // A starter puts words in the box; the cursor follows them.
  useEffect(() => {
    if (draft && box.current) {
      box.current.focus();
      box.current.setSelectionRange(draft.length, draft.length);
    }
  }, [draft]);

  // The box grows with the text, to a limit the stylesheet sets.
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 220)}px`;
  }, [draft]);

  const submit = () => {
    const trimmed = draft.trim();
    if (!trimmed || busy || !model) return;
    onDraft("");
    void send(trimmed);
  };

  return (
    <div className="composer-card">
      <textarea
        ref={box}
        rows={1}
        value={draft}
        onChange={(e) => onDraft(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            submit();
          }
        }}
        placeholder={model ? placeholder : "Choose a model in Settings first, then ask here"}
        aria-label="Message"
        autoFocus
      />
      <div className="composer-row">
        <div className="ls-row ls-gap-2">
          <button type="button" className="model-chip" onClick={() => goSettings("assistant")} title="Choose the assistant">
            {modelLabel(model?.id, catalogModels)}
            <ChevronDownIcon size={13} className="ls-faint" />
          </button>
          {busy ? (
            <button type="button" className="send stop" onClick={() => void cancel()} title="Stop" aria-label="Stop">
              <StopIcon size={16} />
            </button>
          ) : (
            <button type="button" className="send" onClick={submit} disabled={!draft.trim() || !model} aria-label="Send">
              <ArrowUpIcon size={17} />
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
