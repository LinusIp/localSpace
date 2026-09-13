import { test } from "node:test";
import assert from "node:assert/strict";
import { Scene } from "./scene.ts";
import { cubicPoint } from "./geometry.ts";
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

test("a connector between two shapes leaves the side that faces the other and curves; a free end is straight", () => {
  const s = new Scene(new FixedMeasurer());
  s.set(sticky("a", 0, 0));
  s.set(sticky("b", 400, 200));
  s.set(sticky("below", 0, 400));
  s.set({ ...sticky("side", 0, 0, 5), kind: "arrow", w: 0, h: 0, fill: "grey", from: "a", to: "b" });
  s.set({ ...sticky("down", 0, 0, 6), kind: "arrow", w: 0, h: 0, fill: "grey", from: "a", to: "below" });
  s.set({ ...sticky("free", 0, 0, 7), kind: "arrow", w: 0, h: 0, fill: "grey", from: "a", to: null, end: [300, 300] });

  // Side by side: from the middle of a's right side to the middle of b's left side, as an S.
  const side = s.arrowCurve(s.get("side") as Node);
  assert.ok(side);
  assert.deepEqual(side.a, { x: 130, y: 55 });
  assert.deepEqual(side.b, { x: 400, y: 255 });
  assert.deepEqual(side.c1, { x: 265, y: 55 }, "leaves a level");
  assert.deepEqual(side.c2, { x: 265, y: 255 }, "arrives level");
  // The curve is what is hit, not the chord between its ends.
  const onCurve = cubicPoint(side.a, side.c1, side.c2, side.b, 0.25);
  assert.equal(hitTest(s, onCurve, 0)?.id, "side");
  assert.equal(hitTest(s, { x: 197.5, y: 105 }, 0), null, "a quarter of the way along the chord is off the curve");

  // One above the other: from the middle of a's bottom to the middle of below's top, straight down.
  const down = s.arrowCurve(s.get("down") as Node);
  assert.ok(down);
  assert.deepEqual(down.a, { x: 65, y: 110 });
  assert.deepEqual(down.b, { x: 65, y: 400 });
  assert.deepEqual(down.c1, { x: 65, y: 255 });
  assert.deepEqual(down.c2, { x: 65, y: 255 });
  assert.equal(hitTest(s, { x: 65, y: 300 }, 0)?.id, "down");

  // A free end: no curve, and the shape's edge towards the end point.
  assert.equal(s.arrowCurve(s.get("free") as Node), null);
  const [start, end] = s.endpoints(s.get("free") as Node);
  assert.deepEqual(end, { x: 300, y: 300 });
  assert.ok(start.x > 65 && start.y > 55, `leaves a towards the end: ${JSON.stringify(start)}`);
});

test("a note's first line is its title and the rest its body, wrapped inside the padding", () => {
  const s = new Scene(new FixedMeasurer(8));
  const note = { ...sticky("n", 0, 0), w: 196, text: "Research\nInterview six customers before we commit to the date" };
  s.set(note);
  const l = s.stickyLayout(note);
  assert.deepEqual(l.title.lines, ["Research"]);
  assert.ok(l.body);
  // 196 - 28 of padding = 168 px, 21 characters a line at 8 px.
  assert.equal(l.body.lines[0], "Interview six");
  assert.equal(l.body.lineHeight, 13.5 * 1.45);
  assert.equal(s.stickyLayout({ ...note, text: "Title only" }).body, null);
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
