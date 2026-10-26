import { test } from "node:test";
import assert from "node:assert/strict";
import { ACCEPT, MOST_ATTACHMENTS, TEXT_MAX_BYTES, aboutGigabytes, aboutMegabytes, isPicture, mayBeText, partReadText, refusalText } from "./attachments.ts";
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

test("the file chooser offers pictures and text files; Core decides what a file is", () => {
  assert.match(ACCEPT, /image\/png/);
  assert.match(ACCEPT, /\.csv/);
  assert.equal(TEXT_MAX_BYTES, 2 * 1024 * 1024);
});

const file = (chars: number, read: number, words: number, readWords: number) => ({
  id: "a", kind: "text" as const, name: "long.txt", mime: "text/plain", bytes: 100, width: 0, height: 0,
  text: { encoding: "UTF-8", chars, words, read_chars: read, read_words: readWords, read_tokens: 4000 },
});

test("a text file read only in part says how much, in the person's language; one read whole says nothing", () => {
  for (const language of ["en", "ru", "uz"] as const) {
    const s = STRINGS[language];
    assert.equal(partReadText(s, file(1000, 1000, 22000, 22000), language), null);
    const said = partReadText(s, file(1000, 230, 22000, 5000), language) ?? "";
    assert.match(said, /23/, language);
    assert.match(said, /22[\s,.  ]?000/, language);
    // Under 1%: one decimal; under 0.1%, "<0.1"; never "0%" and never more than was read.
    assert.match(partReadText(s, file(1000, 4, 22000, 90), language) ?? "", /0[.,]4/, language);
    const tiny = partReadText(s, file(1_000_000, 50, 220_000, 9), language) ?? "";
    assert.match(tiny, /<0[.,]1/, language);
    assert.doesNotMatch(tiny, /(^|[^<\d.,])0%/, language);
    // Read none of it: said plainly, without a percentage.
    assert.equal(partReadText(s, file(1000, 0, 200, 0), language), s.fileNotRead);
    // Too few words to count: only the percentage.
    assert.equal(partReadText(s, file(1_500_000, 16_000, 1, 1), language), s.filePartRead("1", null), language);
  }
  // Russian says "слова" after a number ending in 1, but not 11.
  assert.match(partReadText(STRINGS.ru, file(1000, 230, 22001, 5000), "ru") ?? "", /слова\)/);
  assert.match(partReadText(STRINGS.ru, file(1000, 230, 22011, 5000), "ru") ?? "", /слов\)/);
});

test("a file too large to send is told the size limit only when it may be text", () => {
  assert.ok(mayBeText({ type: "text/csv", name: "a.csv" }));
  assert.ok(mayBeText({ type: "", name: "README" }));
  assert.ok(mayBeText({ type: "", name: "big.log" }));
  assert.ok(!mayBeText({ type: "application/pdf", name: "contract.pdf" }));
  assert.ok(!mayBeText({ type: "", name: "film.mp4" }));
});

test("the words are left out when they disagree with the share read", () => {
  // 1% of the characters but 99% of the words: a picture inside a Markdown file.
  assert.equal(partReadText(STRINGS.en, file(1_400_000, 15_875, 1523, 1503), "en"), STRINGS.en.filePartRead("1", null));
});

test("a file's size is said in megabytes to one decimal, rounded up", () => {
  assert.equal(aboutMegabytes(2 * 1024 * 1024 + 1, "en"), "2.1");
  assert.equal(aboutMegabytes(2_100_000, "ru"), "2,1");
  assert.equal(aboutMegabytes(3 * 1024 * 1024, "en"), "3.0");
});

test("every refusal says why in the person's language, with its numbers", () => {
  for (const language of ["en", "ru", "uz"] as const) {
    const s = STRINGS[language];
    assert.equal(refusalText(s, "not_a_picture"), s.notAPicture);
    assert.equal(refusalText(s, "unreadable"), s.pictureUnreadable);
    assert.equal(refusalText(s, "not_found"), s.pictureNotFound);
    assert.equal(refusalText(s, "not_text"), s.notText);
    assert.equal(refusalText(s, "empty_file"), s.emptyFile);
    assert.match(refusalText(s, { text_too_large: { bytes: 2 * 1024 * 1024 + 1 } }, undefined, null, language), /2[.,]1/);
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
