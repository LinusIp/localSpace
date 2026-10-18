import { test } from "node:test";
import assert from "node:assert/strict";
import { MOST_PICTURES, isPicture, pictureModel, refusalText } from "./attachments.ts";
import type { ModelCatalogEntry } from "../api/generated";
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
  }
});

const entry = (id: string, params_b: number, more: Partial<ModelCatalogEntry> = {}): ModelCatalogEntry =>
  ({ id, title: id, params_b, reads_pictures: true, verdict: "runs_well", memory_tight: false, installed: false, ...more }) as ModelCatalogEntry;

test("the model suggested for pictures reads them, runs here with room, and is on this computer if one is", () => {
  const nine = entry("9b", 9, { memory_tight: true, installed: true });
  const four = entry("4b", 4);
  const two = entry("2b", 2, { installed: true });
  const blind = entry("8b", 8, { reads_pictures: false, installed: true });
  const huge = entry("27b", 27, { verdict: "will_not_fit" });
  // Installed first, though smaller.
  assert.equal(pictureModel([nine, four, two, blind, huge], "9b")?.id, "2b");
  // None installed: the largest that fits with room.
  assert.equal(pictureModel([nine, four, { ...two, installed: false }, blind, huge], "9b")?.id, "4b");
  // Never the one in use, never one without a picture file, never one that is tight or will not fit.
  assert.equal(pictureModel([nine, blind, huge], "4b"), null);
});
