import { test } from "node:test";
import assert from "node:assert/strict";
import { featureIn } from "./features.ts";
import { MODES, PALETTES, lookAttributes, oneOf } from "./look.ts";

test("a feature is on only when Core reports it on", () => {
  assert.equal(featureIn(null, "themes"), false);
  assert.equal(featureIn(undefined, "themes"), false);
  assert.equal(featureIn({ on: [], locked: [] }, "themes"), false);
  assert.equal(featureIn({ on: ["translation"], locked: [] }, "themes"), false);
  assert.equal(featureIn({ on: ["themes"], locked: ["themes"] }, "themes"), true);
});

test("a stored palette or mode the client doesn't know falls back to the default", () => {
  assert.equal(oneOf("violet", PALETTES), "violet");
  assert.equal(oneOf("green", PALETTES), "indigo");
  assert.equal(oneOf(undefined, PALETTES), "indigo");
  assert.equal(oneOf("dark", MODES), "dark");
  assert.equal(oneOf("", MODES), "system");
});

test("same as Windows follows Windows; light and dark don't", () => {
  assert.deepEqual(lookAttributes("indigo", "system", true), { palette: "indigo", theme: "dark" });
  assert.deepEqual(lookAttributes("indigo", "system", false), { palette: "indigo", theme: "light" });
  assert.deepEqual(lookAttributes("teal", "light", true), { palette: "teal", theme: "light" });
  assert.deepEqual(lookAttributes("violet", "dark", false), { palette: "violet", theme: "dark" });
});
