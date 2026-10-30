import { test } from "node:test";
import assert from "node:assert/strict";
import { featuresIn } from "./features.ts";
import { lookAttributes } from "./look.ts";

test("a feature is on only when the flag names it", () => {
  assert.deepEqual([...featuresIn(null)], []);
  assert.deepEqual([...featuresIn("")], []);
  assert.deepEqual([...featuresIn("themes")], ["themes"]);
  assert.deepEqual([...featuresIn(" themes , translation ")].sort(), ["themes", "translation"]);
});

test("a name the client doesn't know turns nothing on", () => {
  assert.deepEqual([...featuresIn("theme,everything,1")], []);
});

test("same as Windows follows Windows; light and dark don't", () => {
  assert.deepEqual(lookAttributes("indigo", "system", true), { palette: "indigo", theme: "dark" });
  assert.deepEqual(lookAttributes("indigo", "system", false), { palette: "indigo", theme: "light" });
  assert.deepEqual(lookAttributes("teal", "light", true), { palette: "teal", theme: "light" });
  assert.deepEqual(lookAttributes("violet", "dark", false), { palette: "violet", theme: "dark" });
});
