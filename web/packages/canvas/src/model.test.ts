import { test } from "node:test";
import assert from "node:assert/strict";
import { fromDoc, sameNode, toDoc, type BoardDoc } from "./model.ts";
import { layout, FixedMeasurer } from "./text.ts";
import { simplify } from "./editor.ts";

test("a document round-trips through nodes without losing what the logic wrote", () => {
  const doc: BoardDoc = {
    title: "Plan",
    frames: [{ id: "f1", name: "Week 1", x: 20, y: 20, w: 600, h: 420 }],
    shapes: [
      { id: "s1", kind: "sticky", x: 40, y: 40, w: 130, h: 110, fill: "yellow", text: "ship", frame: "f1", z: 1, locked: false },
      { id: "t1", kind: "text", x: 300, y: 40, w: 120, h: 26, size: 20, fill: "none", text: "Title", frame: null, z: 2, locked: true },
      { id: "a1", kind: "arrow", x: 0, y: 0, w: 0, h: 0, fill: "grey", text: "then", frame: null, z: 3, locked: false, from: "s1", to: "t1" },
      { id: "i1", kind: "ink", x: 0, y: 0, w: 0, h: 0, fill: "blue", text: "", frame: null, z: 4, locked: false, points: [[1, 2], [3, 4]] },
      { id: "junk", kind: "hexagon", x: 0, y: 0, w: 1, h: 1, frame: null, z: 9, locked: false },
    ],
    selection: ["s1"],
  };
  const nodes = fromDoc(doc);
  assert.deepEqual(nodes.map((n) => n.id), ["f1", "s1", "t1", "a1", "i1"], "frames first, unknown kinds dropped");
  const back = toDoc(nodes, doc);
  assert.equal(back.title, "Plan");
  assert.deepEqual(back.selection, ["s1"]);
  assert.deepEqual(back.frames, doc.frames);
  const s1 = back.shapes?.find((s) => s.id === "s1");
  assert.equal(s1?.frame, "f1");
  assert.equal(s1?.fill, "yellow");
  const a1 = back.shapes?.find((s) => s.id === "a1");
  assert.equal(a1?.from, "s1");
  assert.equal(a1?.text, "then");
  const i1 = back.shapes?.find((s) => s.id === "i1");
  assert.deepEqual(i1?.points, [[1, 2], [3, 4]]);
  assert.deepEqual(back.shapes?.map((s) => s.z), [1, 2, 3, 4], "z is dense and in order");
});

test("sameNode compares what the document would carry", () => {
  const [a] = fromDoc({ shapes: [{ id: "x", kind: "rect", x: 1, y: 2, w: 3, h: 4, frame: null, z: 1, locked: false }] });
  const b = { ...a, text: "changed" };
  const c = { ...a };
  assert.ok(sameNode(a, c));
  assert.ok(!sameNode(a, b));
});

test("text wraps at words and breaks a word wider than the box", () => {
  const m = new FixedMeasurer(10);
  const l = layout(m, "the quick brown fox", "f", 10, 100);
  assert.deepEqual(l.lines, ["the quick", "brown fox"]);
  const long = layout(m, "abcdefghijklmnop", "f", 10, 50);
  assert.deepEqual(long.lines, ["abcde", "fghij", "klmno", "p"]);
  const blank = layout(m, "a\n\nb", "f", 10, 100);
  assert.deepEqual(blank.lines, ["a", "", "b"]);
});

test("simplify keeps the corners of a stroke and drops the noise", () => {
  const pts = [];
  for (let x = 0; x <= 100; x += 5) pts.push({ x, y: (x % 10 === 0 ? 0.2 : -0.2) });
  for (let y = 5; y <= 100; y += 5) pts.push({ x: 100, y });
  const out = simplify(pts, 0.75);
  assert.ok(out.length <= 4, `${out.length} points left`);
  assert.deepEqual(out[0], pts[0]);
  assert.deepEqual(out[out.length - 1], pts[pts.length - 1]);
  assert.ok(out.some((p) => p.x === 100 && Math.abs(p.y) < 1), "the corner survives");
});
