// The chat (screens 2 and 3): a greeting and three things to try until the
// first message, then the conversation. Tool calls appear as what they did,
// in words; anything that needs the person's say-so appears as a card.

import { Fragment, useEffect, useRef, useState } from "react";
import { ArrowUpIcon, CheckIcon, ChevronDownIcon, ClipIcon, CloseIcon, Dialog, SpinnerIcon, StopIcon } from "@localspace/ui";
import type { Attachment, ChatMessage, ToolCallRecord } from "../api/generated";
import { addAttachment, attachmentUrl } from "../api/client";
import { ACCEPT, MOST_ATTACHMENTS, TEXT_MAX_BYTES, aboutMegabytes, isPicture, leftOutText, mayBeText, partReadText, refusalText } from "../lib/attachments";
import { useLanguage, useStrings } from "../useStrings";
import { initialsOf, outcomeLine, useSession } from "../store";
import type { LiveToolCall } from "../store";
import { Markdown } from "../components/Markdown";
import { Mark } from "../components/Mark";
import { TopBar } from "../components/TopBar";
import { greeting } from "../lib/time";
import { endOfAnswer, workOf } from "../lib/answers";
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
  const thinkingSince = useSession((s) => s.thinking[chat] ?? null);
  return { streaming, turn, liveCalls, continuing, thinkingSince };
}

const NO_CALLS: LiveToolCall[] = [];

/** Said in the answer's place from the send until the model's thinking or
 *  first word is heard of, and while it thinks, with the seconds (ruled
 *  2026-10-06 and 2026-10-09). */
const STARTING = "Starting…";
const THINKING = "Thinking…";

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
  const { transcript, approvals, readFrom } = useSession();
  const strings = useStrings();
  // The first message the model still reads, once a long chat has been trimmed.
  const firstRead = readFrom > 0 ? transcript[readFrom] : undefined;
  const { streaming, turn, liveCalls, continuing, thinkingSince } = useChatInProgress();
  const shown = shownOf(transcript);
  const bottom = useRef<HTMLDivElement>(null);
  // What is typed here belongs to the chat, and stays with it.
  const chat = useSession((s) => s.currentConversation);
  const draft = useSession((s) => s.drafts[chat] ?? "");
  const setDraft = (text: string) => useSession.get().setDraft(chat, text);
  const empty = shown.length === 0 && !streaming;
  // Carrying on a stopped answer: its new words join it, in its place. One
  // with no words is answered afresh: it gives way to the answer in progress,
  // which says where it stands as any other does.
  const last = shown[shown.length - 1];
  const carried = continuing && last?.role === "assistant" && last.stopped;
  const joining = carried && last.content.trim() !== "";
  const thread = carried && !joining ? shown.slice(0, -1) : shown;
  const work = workOf(transcript);

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
          {thread.map((m, i) => {
            const isLast = i === thread.length - 1;
            return (
              <Fragment key={i}>
                {m === firstRead && (
                  <div className="chat-divider" role="note">
                    {strings.olderMessagesLeftOut}
                  </div>
                )}
                <Message message={m} joined={isLast && joining ? streaming : null} mayContinue={isLast && !turn} work={work.get(m) ?? []} />
              </Fragment>
            );
          })}
          {turn && !joining && (
            <div className="message">
              <AssistantAvatar thinking={turn === "writing" && !streaming && thinkingSince !== null} />
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
                  <ThinkingLine key={thinkingSince ?? "starting"} since={thinkingSince} />
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

/** The mark; while the model thinks, its three nodes pulse in turn, in CSS
 *  alone (ruled 2026-10-06). */
function AssistantAvatar({ thinking }: { thinking?: boolean }) {
  return (
    <span className={`avatar small soft${thinking ? " thinking" : ""}`} aria-hidden="true">
      <Mark size={16} color="#1D7A55" />
    </span>
  );
}

/** "Starting…" until the model's thinking is heard of, then "Thinking…" with
 *  the seconds since, written anew once a second in this one line and
 *  nowhere else: the words give way at the first visible word. */
function ThinkingLine({ since }: { since: number | null }) {
  // Counted by the timer alone, from nought: the line is keyed on `since`
  // by its parent, so a new thinking starts a new count.
  const [seconds, setSeconds] = useState(0);
  useEffect(() => {
    if (since === null) return;
    const timer = window.setInterval(() => setSeconds(Math.max(0, Math.floor((Date.now() - since) / 1000))), 1000);
    return () => window.clearInterval(timer);
  }, [since]);
  return (
    <span className="message-work">
      <SpinnerIcon size={14} className="ls-spin" /> {since === null ? STARTING : `${THINKING} ${seconds}s`}
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
function Message({ message, joined, mayContinue, work }: { message: ChatMessage; joined: string | null; mayContinue: boolean; work: ToolCallRecord[] }) {
  const continueAnswer = useSession((s) => s.continueAnswer);
  const newConversation = useSession((s) => s.newConversation);
  const me = useSession((s) => s.me);
  const strings = useStrings();
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
        <div className="message-body user ls-pre">
          {message.attachments && message.attachments.length > 0 && <SentAttachments attachments={message.attachments} />}
          {message.content}
        </div>
      </div>
    );
  }
  const text = joined === null ? message.content : message.content + joined;
  // On an organisation's server only an administrator loads a model.
  const mayLoadModel = me?.topology !== "organisation" || me.roles.includes("admin");
  const end = endOfAnswer(strings, message, { didWork: work.length > 0, mayLoadModel });
  return (
    <div className="message">
      <AssistantAvatar />
      <div className="message-body">
        {message.tool_calls.map((c) => (
          <Work key={c.id} call={c} />
        ))}
        {/* An answer of no words after tool work: the work is what it did. A
            declined proposal is shown in the chat already. */}
        {!text &&
          work
            .filter((c) => !("declined" in c.outcome))
            .map((c) => <Work key={c.id} call={c} />)}
        {text && (
          <div className="prose-chat">
            <Markdown text={text} />
          </div>
        )}
        {joined === null && end && (
          <div className="message-stopped" role="status">
            {end.text}
            {mayContinue && end.action && (
              <>
                {" "}
                <button
                  type="button"
                  className="link green"
                  onClick={() => void (end.action === "new_chat" ? newConversation() : continueAnswer())}
                >
                  {end.action === "new_chat" ? strings.newChat : end.action === "try_again" ? strings.tryAgain : strings.continueAnswer}
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

/**
 * What a sent message carried: its pictures, small, each opening larger in
 * the app (docs/DECISIONS.md, 2026-10-18; plan 38, P3), and its text files,
 * each with how much the model read when that was not all of it (plan 38,
 * F4) and a way to save it again.
 */
function SentAttachments({ attachments }: { attachments: Attachment[] }) {
  const strings = useStrings();
  const language = useLanguage();
  const pictures = attachments.filter((a) => a.kind === "picture");
  const files = attachments.filter((a) => a.kind === "text");
  return (
    <>
      {pictures.length > 0 && <SentPictures pictures={pictures} />}
      {files.map((f) => {
        const part = partReadText(strings, f, language);
        return (
          <div key={f.id} className="message-file">
            <span className="message-file-name" title={f.name}>
              {f.name}
            </span>
            <a className="link green" href={attachmentUrl(f.id)} download={f.name}>
              {strings.saveFile}
            </a>
            {part && <div className="message-file-note">{part}</div>}
          </div>
        );
      })}
    </>
  );
}

function SentPictures({ pictures }: { pictures: Attachment[] }) {
  const [open, setOpen] = useState<Attachment | null>(null);
  return (
    <>
      <div className="message-pictures">
        {pictures.map((p) => (
          <button key={p.id} type="button" className="message-picture" onClick={() => setOpen(p)} title={p.name}>
            <img src={attachmentUrl(p.id)} alt={p.name} loading="lazy" />
          </button>
        ))}
      </div>
      <Dialog open={open !== null} title={open?.name ?? ""} onClose={() => setOpen(null)}>
        {open && <img className="picture-large" src={attachmentUrl(open.id)} alt={open.name} />}
      </Dialog>
    </>
  );
}

/** A picture or a text file in the composer, before the message goes: on its way to Core, or taken. */
interface Pending {
  key: string;
  name: string;
  /** A picture's preview meanwhile: the file itself, on this computer; none for a text file. */
  preview: string | null;
  attachment: Attachment | null;
}

function letGo(p: Pending) {
  if (p.preview) URL.revokeObjectURL(p.preview);
}

/** The files dropped or pasted, folders left out. */
function filesOf(items: DataTransferItemList): File[] {
  return Array.from(items)
    .filter((item) => item.kind === "file" && !item.webkitGetAsEntry()?.isDirectory)
    .flatMap((item) => {
      const file = item.getAsFile();
      return file ? [file] : [];
    });
}

function Composer({ draft, onDraft, placeholder }: { draft: string; onDraft: (text: string) => void; placeholder: string }) {
  const { send, cancel, environment, catalogModels, goSettings, loadModel, downloadModel } = useSession();
  const strings = useStrings();
  const language = useLanguage();
  const [pending, setPending] = useState<Pending[]>([]);
  const [said, setSaid] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);
  const chooser = useRef<HTMLInputElement>(null);
  const uploading = pending.some((p) => p.attachment === null);
  // Pictures with a model that cannot read them here: said at once, with the
  // model to switch to (docs/DECISIONS.md, 2026-10-18; plan 38, P4).
  const loaded = environment?.model ?? null;
  const blind = pending.some((p) => p.preview !== null) && loaded !== null && !loaded.supports_vision;
  // The model Core found at the load that would read them here, never a
  // larger one (docs/DECISIONS.md, 2026-10-19); none, and only "Try again".
  const suggested = environment?.picture_model ?? null;
  const instead = blind && suggested !== null ? (catalogModels.find((e) => e.id === suggested) ?? null) : null;
  // Its picture file left out at this load, for want of free memory (plan 38, P5).
  const leftOut = environment?.picture_file_left_out_mib ?? null;

  // Each file goes to Core as soon as it is added, which reads it as a
  // picture or as text, whatever its name, and keeps it until the message
  // goes. A refusal is said once, plainly. A text file larger than Core
  // takes is said at once, without sending it.
  const addFiles = (files: File[]) => {
    const taken = files.filter((file) => isPicture(file) || file.size <= TEXT_MAX_BYTES);
    const large = files.find((file) => !taken.includes(file));
    if (large) setSaid(mayBeText(large) ? strings.textTooLarge(aboutMegabytes(large.size, language)) : strings.notText);
    const room = MOST_ATTACHMENTS - pending.length;
    if (taken.length > room) setSaid(strings.tooManyPictures(MOST_ATTACHMENTS));
    for (const file of taken.slice(0, Math.max(0, room))) {
      const key = `${file.name}-${file.size}-${Math.random()}`;
      const preview = isPicture(file) ? URL.createObjectURL(file) : null;
      setPending((all) => [...all, { key, name: file.name || "file", preview, attachment: null }]);
      void addAttachment(file, file.name || "file")
        .then((answer) => {
          if ("attachment" in answer) {
            // The same file twice is attached once: its first chip stays.
            setPending((all) => {
              const twice = all.find((p) => p.key === key && all.some((q) => q.key !== key && q.attachment?.id === answer.attachment.id));
              if (twice) {
                letGo(twice);
                return all.filter((p) => p.key !== key);
              }
              return all.map((p) => (p.key === key ? { ...p, attachment: answer.attachment } : p));
            });
          } else {
            setSaid(refusalText(strings, answer.refusal, undefined, null, language));
            setPending((all) => all.filter((p) => p.key !== key));
            if (preview) URL.revokeObjectURL(preview);
          }
        })
        .catch(() => {
          setSaid(preview ? strings.pictureUnreadable : strings.fileNotAttached);
          setPending((all) => all.filter((p) => p.key !== key));
          if (preview) URL.revokeObjectURL(preview);
        });
    }
  };
  const remove = (key: string) => {
    setPending((all) => {
      const gone = all.find((p) => p.key === key);
      if (gone) letGo(gone);
      return all.filter((p) => p.key !== key);
    });
  };
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

  // Pictures still in the box when it goes (another chat, another page):
  // their previews are let go, each up to 30 MB held by the webview.
  const previews = useRef<Pending[]>([]);
  useEffect(() => {
    previews.current = pending;
  }, [pending]);
  useEffect(
    () => () => {
      for (const p of previews.current) letGo(p);
    },
    [],
  );

  // The box grows with the text, to a limit the stylesheet sets.
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 220)}px`;
  }, [draft]);

  const submit = () => {
    const trimmed = draft.trim();
    if (!trimmed || busy || !model || uploading || blind) return;
    const attachments = pending.flatMap((p) => (p.attachment ? [p.attachment] : []));
    const sent = pending;
    onDraft("");
    setPending([]);
    setSaid(null);
    void send(trimmed, attachments).then((refusal) => {
      if (refusal) {
        // Not taken: the words and the pictures back where they were.
        onDraft(trimmed);
        setPending(sent);
        setSaid(refusalText(strings, refusal, modelLabel(model?.id, catalogModels), instead?.title ?? null, language));
      } else {
        for (const p of sent) letGo(p);
      }
    });
  };

  return (
    <div
      className={`composer-card${dragging ? " dropping" : ""}`}
      onDragOver={(e) => {
        if (Array.from(e.dataTransfer.types).includes("Files")) {
          e.preventDefault();
          setDragging(true);
        }
      }}
      onDragLeave={() => setDragging(false)}
      onDrop={(e) => {
        e.preventDefault();
        setDragging(false);
        addFiles(filesOf(e.dataTransfer.items));
        // A folder is not a file: said as such, never sent.
        if (Array.from(e.dataTransfer.items).some((item) => item.webkitGetAsEntry()?.isDirectory)) setSaid(strings.notText);
      }}
    >
      {dragging && <div className="composer-drop">{strings.dropFileHere}</div>}
      {pending.length > 0 && (
        <div className="composer-pictures">
          {pending.map((p) => (
            <div key={p.key} className={`${p.preview ? "composer-picture" : "composer-file"}${p.attachment ? "" : " uploading"}`}>
              {p.preview ? <img src={p.preview} alt={p.name} /> : <span className="composer-file-name" title={p.name}>{p.name}</span>}
              <button type="button" className="composer-picture-remove" onClick={() => remove(p.key)} aria-label={`${strings.removePicture}: ${p.name}`} title={strings.removePicture}>
                <CloseIcon size={12} />
              </button>
            </div>
          ))}
        </div>
      )}
      {said && (
        <div className="composer-said" role="status">
          {said}
        </div>
      )}
      {blind && (
        <div className="composer-said" role="status">
          {leftOut !== null ? leftOutText(strings, leftOut, language, instead?.title ?? null) : strings.cannotReadPictures(modelLabel(loaded.id, catalogModels))}{" "}
          {leftOut !== null && (
            <button type="button" className="link green" onClick={() => void loadModel(loaded.id)}>
              {strings.tryAgain}
            </button>
          )}{" "}
          {instead && (
            <button type="button" className="link green" onClick={() => void (instead.installed ? loadModel(instead.id) : downloadModel(instead.id))}>
              {instead.installed ? strings.switchForPictures(instead.title) : strings.downloadForPictures(instead.title)}
            </button>
          )}
        </div>
      )}
      <textarea
        onPaste={(e) => {
          const files = filesOf(e.clipboardData.items);
          if (files.length > 0) {
            e.preventDefault();
            addFiles(files);
          }
        }}
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
        placeholder={model ? placeholder : strings.chooseModelToStart}
        aria-label="Message"
        autoFocus
      />
      <div className="composer-row">
        <input
          ref={chooser}
          type="file"
          accept={ACCEPT}
          multiple
          hidden
          onChange={(e) => {
            addFiles(Array.from(e.target.files ?? []));
            e.target.value = "";
          }}
        />
        <button type="button" className="composer-attach" onClick={() => chooser.current?.click()} disabled={pending.length >= MOST_ATTACHMENTS} aria-label={strings.attachFile} title={strings.attachFile}>
          <ClipIcon size={17} />
        </button>
        <div className="ls-row ls-gap-2">
          <button type="button" className="model-chip" onClick={() => goSettings("assistant")} title={strings.chooseModelLabel}>
            {modelLabel(model?.id, catalogModels)}
            <ChevronDownIcon size={13} className="ls-faint" />
          </button>
          {busy ? (
            <button type="button" className="send stop" onClick={() => void cancel()} title={strings.stop} aria-label={strings.stop}>
              <StopIcon size={16} />
            </button>
          ) : (
            <button type="button" className="send" onClick={submit} disabled={!draft.trim() || !model || uploading || blind} aria-label={strings.send} title={strings.send}>
              <ArrowUpIcon size={17} />
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
