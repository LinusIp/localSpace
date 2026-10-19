import { test } from "node:test";
import assert from "node:assert/strict";
import { MOST_PICTURES, aboutGigabytes, isPicture, refusalText } from "./attachments.ts";
import { STRINGS } from "./strings.ts";

test("pictures are taken by their type or, without one, by their name", () => {
  assert.ok(isPicture({ type: "image/png", name: "x" }));
  assert.ok(isPicture({ type: "image/webp", name: "x" }));
  assert.ok(isPicture({ type: "", name: "Photo.JPG" }));
  assert.ok(!isPicture({ type: "image/gif", name: "a.gif" }));
  assert.ok(!isPicture({ type: "application/pdf", name: "a.pdf" }));
});

test("as many pictures with one message as Core takes", () => {
  assert.equal(MOST_PICTURES, 4);
});

test("every refusal says why in the person's language, with its numbers", () => {
  for (const language of ["en", "ru", "uz"] as const) {
    const s = STRINGS[language];
    assert.equal(refusalText(s, "not_a_picture"), s.notAPicture);
    assert.equal(refusalText(s, "unreadable"), s.pictureUnreadable);
    assert.equal(refusalText(s, "not_found"), s.pictureNotFound);
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
