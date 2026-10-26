// How an answer ended, in the person's language, and what they can do next
// (docs/DECISIONS.md, 2026-10-26). Core says why (`stopped`, `cut_short`);
// the words are the client's. The real cause is named: an answer at its own
// length limit is not a full chat, and a full chat is never offered Continue.

import type { ChatMessage } from "../api/generated";
import type { Strings } from "./strings";

/** What the line under an answer offers: carry it on, start a new chat, or ask again. */
export type EndAction = "continue" | "new_chat" | "try_again";

export interface EndLine {
  text: string;
  action: EndAction | null;
}

/** The line under an answer, or null when it ended as it should. */
export function endOfAnswer(strings: Strings, m: Pick<ChatMessage, "stopped" | "cut_short" | "content" | "tool_calls">): EndLine | null {
  const words = m.content.trim() !== "";
  switch (m.cut_short) {
    case "repeated":
      return { text: strings.repeated, action: null };
    case "chat_full":
      return { text: words ? strings.chatFull : strings.chatFullNoRoom, action: "new_chat" };
    case "failed":
      return { text: strings.modelFailed, action: "try_again" };
    case "length":
      // At its own limit with nothing to carry on: the thinking took it all.
      return words ? { text: strings.atLimit, action: "continue" } : { text: strings.thoughtTooLong, action: "try_again" };
    default:
      break;
  }
  if (m.stopped) return { text: strings.answerStoppedHere, action: "continue" };
  if (!words && m.tool_calls.length === 0) return { text: strings.noAnswer, action: null };
  return null;
}
