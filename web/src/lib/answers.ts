// How an answer ended, in the person's language, and what they can do next
// (docs/DECISIONS.md, 2026-10-26). Core says why (`stopped`, `cut_short`);
// the words are the client's. The real cause is named: an answer at its own
// length limit is not a full chat, a full chat is never offered Continue, and
// a message too long for the model is not a full chat either, since a new
// chat would read the same.

import type { ChatMessage } from "../api/generated";
import type { Strings } from "./strings";

/** What the line under an answer offers: carry it on, start a new chat, or ask again. */
export type EndAction = "continue" | "new_chat" | "try_again";

export interface EndLine {
  text: string;
  action: EndAction | null;
}

/** What the chat around an answer says of it. */
export interface AnswerAround {
  /** Its turn did tool work: the calls Core keeps as messages of their own, before the answer. */
  didWork: boolean;
  /** The person can load a model: on their own computer, or as an organisation's administrator. */
  mayLoadModel: boolean;
}

/** The line under an answer, or null when it ended as it should. */
export function endOfAnswer(strings: Strings, m: Pick<ChatMessage, "stopped" | "cut_short" | "content">, around: AnswerAround): EndLine | null {
  const words = m.content.trim() !== "";
  switch (m.cut_short) {
    case "repeated":
      return { text: strings.repeated, action: null };
    case "chat_full":
      return { text: words ? strings.chatFull : strings.chatFullNoRoom, action: "new_chat" };
    case "too_long":
      return { text: words ? strings.tooLong : strings.tooLongNoRoom, action: null };
    case "failed":
      return { text: around.mayLoadModel ? strings.modelFailed : strings.modelFailedAskAdmin, action: "try_again" };
    case "length":
      // At its own limit with nothing to carry on: the thinking took it all.
      return words ? { text: strings.atLimit, action: "continue" } : { text: strings.thoughtTooLong, action: "try_again" };
    default:
      break;
  }
  if (m.stopped) return { text: strings.answerStoppedHere, action: "continue" };
  if (!words && !around.didWork) return { text: strings.noAnswer, action: null };
  return null;
}

/** The answers whose turn did tool work: a call Core keeps as a message of its own, between the person's message and the answer. */
export function workedBefore(transcript: ChatMessage[]): Set<ChatMessage> {
  const worked = new Set<ChatMessage>();
  let work = false;
  for (const m of transcript) {
    if (m.role === "user") work = false;
    else if (m.role === "tool") work = true;
    else if (m.role === "assistant" && work) worked.add(m);
  }
  return worked;
}
