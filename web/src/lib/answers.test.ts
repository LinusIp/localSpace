import { test } from "node:test";
import assert from "node:assert/strict";
import { endOfAnswer } from "./answers.ts";
import { STRINGS } from "./strings.ts";

const answer = (over: object) => ({ stopped: false, cut_short: null, content: "Some words.", tool_calls: [], ...over });

test("every way an answer ends names its real cause, with what to do, in each language", () => {
  for (const language of ["en", "ru", "uz"] as const) {
    const s = STRINGS[language];
    assert.equal(endOfAnswer(s, answer({})), null, language);
    assert.deepEqual(endOfAnswer(s, answer({ stopped: true })), { text: s.answerStoppedHere, action: "continue" });
    assert.deepEqual(endOfAnswer(s, answer({ stopped: true, cut_short: "length" })), { text: s.atLimit, action: "continue" });
    assert.deepEqual(endOfAnswer(s, answer({ stopped: true, cut_short: "length", content: "" })), { text: s.thoughtTooLong, action: "try_again" });
    assert.deepEqual(endOfAnswer(s, answer({ stopped: true, cut_short: "chat_full" })), { text: s.chatFull, action: "new_chat" });
    assert.deepEqual(endOfAnswer(s, answer({ stopped: true, cut_short: "chat_full", content: "" })), { text: s.chatFullNoRoom, action: "new_chat" });
    assert.deepEqual(endOfAnswer(s, answer({ stopped: true, cut_short: "failed", content: "" })), { text: s.modelFailed, action: "try_again" });
    assert.deepEqual(endOfAnswer(s, answer({ cut_short: "repeated" })), { text: s.repeated, action: null });
    assert.deepEqual(endOfAnswer(s, answer({ content: "" })), { text: s.noAnswer, action: null });
  }
});

test("a full chat is never offered Continue, and an answer at its own limit never says the chat is full", () => {
  const s = STRINGS.en;
  for (const content of ["", "words"]) {
    assert.notEqual(endOfAnswer(s, answer({ stopped: true, cut_short: "chat_full", content }))?.action, "continue");
    assert.doesNotMatch(endOfAnswer(s, answer({ stopped: true, cut_short: "length", content }))?.text ?? "", /full/);
  }
});

test("an answer that is only tool work has no line", () => {
  const call = { id: "c", tool: "t", params: {}, outcome: { ok: { diff_summary: "", result: {}, commit: null } } };
  assert.equal(endOfAnswer(STRINGS.en, answer({ content: "", tool_calls: [call] } as object)), null);
});
