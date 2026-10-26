import { test } from "node:test";
import assert from "node:assert/strict";
import { endOfAnswer, workedBefore } from "./answers.ts";
import { STRINGS } from "./strings.ts";
import type { ChatMessage } from "../api/generated";

const answer = (over: object) => ({ stopped: false, cut_short: null, content: "Some words.", ...over });
const plain = { didWork: false, mayLoadModel: true };

test("every way an answer ends names its real cause, with what to do, in each language", () => {
  for (const language of ["en", "ru", "uz"] as const) {
    const s = STRINGS[language];
    const end = (over: object) => endOfAnswer(s, answer(over), plain);
    assert.equal(end({}), null, language);
    assert.deepEqual(end({ stopped: true }), { text: s.answerStoppedHere, action: "continue" });
    assert.deepEqual(end({ stopped: true, cut_short: "length" }), { text: s.atLimit, action: "continue" });
    assert.deepEqual(end({ stopped: true, cut_short: "length", content: "" }), { text: s.thoughtTooLong, action: "try_again" });
    assert.deepEqual(end({ stopped: true, cut_short: "chat_full" }), { text: s.chatFull, action: "new_chat" });
    assert.deepEqual(end({ stopped: true, cut_short: "chat_full", content: "" }), { text: s.chatFullNoRoom, action: "new_chat" });
    assert.deepEqual(end({ stopped: true, cut_short: "too_long" }), { text: s.tooLong, action: null });
    assert.deepEqual(end({ stopped: true, cut_short: "too_long", content: "" }), { text: s.tooLongNoRoom, action: null });
    assert.deepEqual(end({ stopped: true, cut_short: "failed", content: "" }), { text: s.modelFailed, action: "try_again" });
    assert.deepEqual(end({ cut_short: "repeated" }), { text: s.repeated, action: null });
    assert.deepEqual(end({ content: "" }), { text: s.noAnswer, action: null });
  }
});

test("a full chat is never offered Continue, and an answer at its own limit never says the chat is full", () => {
  const s = STRINGS.en;
  for (const content of ["", "words"]) {
    assert.notEqual(endOfAnswer(s, answer({ stopped: true, cut_short: "chat_full", content }), plain)?.action, "continue");
    assert.doesNotMatch(endOfAnswer(s, answer({ stopped: true, cut_short: "length", content }), plain)?.text ?? "", /full/);
  }
});

test("a message too long for the model is not called a full chat, and a new chat is not offered", () => {
  for (const language of ["en", "ru", "uz"] as const) {
    const s = STRINGS[language];
    for (const content of ["", "words"]) {
      const end = endOfAnswer(s, answer({ stopped: true, cut_short: "too_long", content }), plain);
      assert.notEqual(end?.text, s.chatFull, language);
      assert.notEqual(end?.text, s.chatFullNoRoom, language);
      assert.equal(end?.action, null, language);
    }
  }
});

test("a line with a button never says the button's words again", () => {
  for (const language of ["en", "ru", "uz"] as const) {
    const s = STRINGS[language];
    for (const text of [s.thoughtTooLong, s.modelFailed, s.modelFailedAskAdmin]) assert.ok(!text.includes(s.tryAgain), `${language}: ${text}`);
  }
});

test("someone who cannot load a model is not told to, when the model fails", () => {
  const s = STRINGS.en;
  const member = endOfAnswer(s, answer({ stopped: true, cut_short: "failed", content: "" }), { didWork: false, mayLoadModel: false });
  assert.deepEqual(member, { text: s.modelFailedAskAdmin, action: "try_again" });
  assert.doesNotMatch(member?.text ?? "", /Settings/);
});

test("an answer whose turn did tool work, and has no words, has no line", () => {
  const call = { id: "c", tool: "t", params: {}, outcome: { ok: { diff_summary: "", result: {}, commit: null } } };
  const message = (role: ChatMessage["role"], content: string, tool_calls: object[] = []) =>
    ({ role, content, tool_calls, stopped: false, thinking: "", cut_short: null }) as unknown as ChatMessage;
  // As Core keeps it: the call is a message of its own, before the answer.
  const transcript = [message("user", "Put a plan on the board."), message("tool", "", [call]), message("assistant", ""), message("user", "Thanks."), message("assistant", "")];
  const worked = workedBefore(transcript);
  assert.ok(worked.has(transcript[2]));
  assert.ok(!worked.has(transcript[4]), "the next turn did no work");
  assert.equal(endOfAnswer(STRINGS.en, transcript[2], { didWork: true, mayLoadModel: true }), null);
  assert.deepEqual(endOfAnswer(STRINGS.en, transcript[4], plain), { text: STRINGS.en.noAnswer, action: null });
});
