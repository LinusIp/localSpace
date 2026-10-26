import { test } from "node:test";
import assert from "node:assert/strict";
import { ACCEPT, MOST_ATTACHMENTS, aboutGigabytes, isAttachable, isPicture, isTextFile, partReadText, refusalText } from "./attachments.ts";
import { STRINGS } from "./strings.ts";

test("pictures are taken by their type or, without one, by their name", () => {
  assert.ok(isPicture({ type: "image/png", name: "x" }));
  assert.ok(isPicture({ type: "image/webp", name: "x" }));
  assert.ok(isPicture({ type: "", name: "Photo.JPG" }));
  assert.ok(!isPicture({ type: "image/gif", name: "a.gif" }));
  assert.ok(!isPicture({ type: "application/pdf", name: "a.pdf" }));
});

test("as many pictures and files with one message as Core takes", () => {
  assert.equal(MOST_ATTACHMENTS, 4);
});

test("text files are taken by their type or, without one, by their name; others are not", () => {
  assert.ok(isTextFile({ type: "text/plain", name: "x" }));
  assert.ok(isTextFile({ type: "", name: "Report.CSV" }));
  assert.ok(isTextFile({ type: "", name: "main.rs" }));
  assert.ok(!isTextFile({ type: "application/pdf", name: "a.pdf" }));
  assert.ok(!isTextFile({ type: "", name: "noextension" }));
  assert.ok(isAttachable({ type: "", name: "notes.md" }));
  assert.ok(isAttachable({ type: "image/png", name: "x" }));
  assert.ok(!isAttachable({ type: "application/zip", name: "a.zip" }));
  assert.match(ACCEPT, /image\/png/);
  assert.match(ACCEPT, /\.csv/);
});

test("a text file read only in part says how much, in the person's language; one read whole says nothing", () => {
  const file = (read: number) => ({
    id: "a", kind: "text" as const, name: "long.txt", mime: "text/plain", bytes: 100, width: 0, height: 0,
    text: { encoding: "UTF-8", chars: 1000, words: 22000, read_chars: read, read_words: 5000, read_tokens: 4000 },
  });
  for (const language of ["en", "ru", "uz"] as const) {
    const s = STRINGS[language];
    assert.equal(partReadText(s, file(1000), language), null);
    const said = partReadText(s, file(230), language) ?? "";
    assert.match(said, /23/, language);
    assert.match(said, /22[\s,.  ]?000/, language);
  }
});

test("every refusal says why in the person's language, with its numbers", () => {
  for (const language of ["en", "ru", "uz"] as const) {
    const s = STRINGS[language];
    assert.equal(refusalText(s, "not_a_picture"), s.notAPicture);
    assert.equal(refusalText(s, "unreadable"), s.pictureUnreadable);
    assert.equal(refusalText(s, "not_found"), s.pictureNotFound);
    assert.equal(refusalText(s, "not_text"), s.notText);
    assert.equal(refusalText(s, "empty_file"), s.emptyFile);
    assert.match(refusalText(s, { text_too_large: { megabytes: 3 } }), /3/);
    assert.match(refusalText(s, { too_large: { megabytes: 41 } }), /41/);
    assert.match(refusalText(s, { too_many_pixels: { width: 9000, height: 8000 } }), /9000.*8000/);
    assert.match(refusalText(s, { too_many: { most: 4 } }), /4/);
    assert.match(refusalText(s, "cannot_read_pictures", "Ministral 3 8B"), /Ministral 3 8B/);
    // A picture file left out: what it needs, and the model to switch to when there is one.
    const leftOut = { picture_file_left_out: { needs_mib: 1800 } } as const;
    assert.match(refusalText(s, leftOut, "Qwen3.5 9B", "Qwen3.5 4B", language), /1[.,]8/);
    assert.match(refusalText(s, leftOut, "Qwen3.5 9B", "Qwen3.5 4B", language), /Qwen3\.5 4B/);
    assert.doesNotMatch(refusalText(s, leftOut, "Qwen3.5 9B", null, language), /Qwen/);
    assert.ok(s.tryAgain.length > 0);
  }
});

test("gigabytes are said to one decimal, rounded up, in the language's own figures", () => {
  assert.equal(aboutGigabytes(1800, "en"), "1.8");
  assert.equal(aboutGigabytes(1800, "ru"), "1,8");
  assert.equal(aboutGigabytes(1946, "uz"), "2");
  assert.equal(aboutGigabytes(100, "en"), "0.1");
});
