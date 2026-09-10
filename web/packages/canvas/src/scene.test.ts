import { test } from "node:test";
import assert from "node:assert/strict";
import { Scene } from "./scene.ts";
import { hitHandle, hitTest, nodesWithin } from "./hit.ts";
import { FixedMeasurer } from "./text.ts";
import type { Node } from "./model.ts";

const sticky = (id: string, x: number, y: number, z = 1): Node => ({
  id,
  kind: "sticky",
  x,
  y,
  w: 130,
  h: 110,
  fill: "yellow",
  text: "",
  frame: null,
  z,
  locked: false,
});

test("query returns what touches the box, bottom to top", () => {
  const s = new Scene(new FixedMeasurer());
  s.set(sticky("a", 0, 0, 2));
  s.set(sticky("b", 50, 50, 1));
  s.set(sticky("c", 1000, 1000, 3));
  assert.deepEqual(
    s.query({ x: 40, y: 40, w: 20, h: 20 }).map((n) => n.id),
    ["b", "a"],
  );
  assert.deepEqual(s.all().map((n) => n.id), ["b", "a", "c"]);
});

test("hit-testing picks the topmost shape, respects ellipses and reaches lines", () => {
  const s = new Scene(new FixedMeasurer());
  s.set(sticky("under", 0, 0, 1));
  s.set(sticky("over", 60, 60, 2));
  assert.equal(hitTest(s, { x: 70, y: 70 }, 0)?.id, "over");
  assert.equal(hitTest(s, { x: 10, y: 10 }, 0)?.id, "under");
  assert.equal(hitTest(s, { x: 900, y: 900 }, 0), null);

  s.set({ ...sticky("e", 300, 300, 3), kind: "ellipse", w: 200, h: 100, fill: "grey" });
  assert.equal(hitTest(s, { x: 400, y: 350 }, 0)?.id, "e", "the centre of the ellipse");
  assert.equal(hitTest(s, { x: 302, y: 302 }, 0), null, "the corner of its box is outside the ellipse");

  s.set({ ...sticky("ink", 0, 0, 4), kind: "ink", w: 0, h: 0, fill: "blue", points: [[500, 500], [600, 500], [600, 600]] });
  assert.equal(hitTest(s, { x: 550, y: 502 }, 0)?.id, "ink");
  assert.equal(hitTest(s, { x: 550, y: 540 }, 0), null);
});

test("an arrow follows the shapes it joins and lets go of one that is deleted", () => {
  const s = new Scene(new FixedMeasurer());
  s.set(sticky("a", 0, 0));
  s.set(sticky("b", 400, 0));
  s.set({ ...sticky("arrow", 0, 0, 5), kind: "arrow", w: 0, h: 0, fill: "grey", from: "a", to: "b" });
  const [start, end] = s.endpoints(s.get("arrow") as Node);
  assert.equal(start.x, 130, "starts on a's right edge");
  assert.equal(end.x, 400, "ends on b's left edge");
  const before = s.bounds("arrow");
  s.set(sticky("b", 800, 0));
  const after = s.bounds("arrow");
  assert.ok(before && after && after.w > before.w, "the arrow's box grew with the move");
  s.delete("b");
  const arrow = s.get("arrow") as Node;
  assert.equal(arrow.to, null);
  assert.deepEqual(arrow.end, [800, 55], "the free end stays where b's edge was");
});

test("marquee selection takes only what lies fully inside", () => {
  const s = new Scene(new FixedMeasurer());
  s.set(sticky("in", 100, 100));
  s.set(sticky("half", 400, 100));
  assert.deepEqual(nodesWithin(s, { x: 50, y: 50, w: 450, h: 300 }).map((n) => n.id), ["in"]);
});

test("handles are found at the corners and edges within reach", () => {
  const b = { x: 100, y: 100, w: 200, h: 100 };
  assert.equal(hitHandle(b, { x: 300, y: 200 }, 6), "se");
  assert.equal(hitHandle(b, { x: 200, y: 99 }, 6), "n");
  assert.equal(hitHandle(b, { x: 200, y: 150 }, 6), null);
});

test("a text label's height follows its wrapped text", () => {
  const s = new Scene(new FixedMeasurer(8));
  const label: Node = { ...sticky("t", 0, 0), kind: "text", w: 96, h: 0, size: 16, fill: "none", text: "one two three four" };
  s.set(label);
  // 96 - 16 padding = 80 px of width, 10 characters per line at 8 px:
  // "one two" and "three four", two lines, plus the padding above and below.
  const stored = s.get("t") as Node;
  assert.equal(stored.h, 2 * 16 * 1.3 + 16);
  s.set({ ...stored, text: "one two three four five six" });
  assert.equal((s.get("t") as Node).h, 3 * 16 * 1.3 + 16, "more text, another line");
});
