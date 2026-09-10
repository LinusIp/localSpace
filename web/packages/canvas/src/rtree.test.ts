import { test } from "node:test";
import assert from "node:assert/strict";
import { RTree } from "./rtree.ts";
import { intersects, type Box } from "./geometry.ts";

function random(seed: number): () => number {
  let s = seed >>> 0;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return s / 0x100000000;
  };
}

test("search finds exactly the boxes that intersect, at any size", () => {
  const rnd = random(7);
  const tree = new RTree();
  const boxes = new Map<string, Box>();
  for (let i = 0; i < 5000; i++) {
    const b = { x: rnd() * 10000, y: rnd() * 10000, w: 10 + rnd() * 200, h: 10 + rnd() * 200 };
    boxes.set(`n${i}`, b);
    tree.insert(`n${i}`, b);
  }
  assert.equal(tree.size, 5000);
  assert.ok(tree.check(), "every node's box contains its children");
  for (let q = 0; q < 50; q++) {
    const probe = { x: rnd() * 10000, y: rnd() * 10000, w: rnd() * 1500, h: rnd() * 1500 };
    const expected = [...boxes].filter(([, b]) => intersects(b, probe)).map(([k]) => k).sort();
    const got = tree.search(probe).sort();
    assert.deepEqual(got, expected);
  }
});

test("remove takes a key out and the rest stays searchable", () => {
  const rnd = random(11);
  const tree = new RTree();
  const boxes = new Map<string, Box>();
  for (let i = 0; i < 2000; i++) {
    const b = { x: rnd() * 5000, y: rnd() * 5000, w: 5 + rnd() * 100, h: 5 + rnd() * 100 };
    boxes.set(`k${i}`, b);
    tree.insert(`k${i}`, b);
  }
  for (let i = 0; i < 2000; i += 3) {
    assert.ok(tree.remove(`k${i}`));
    boxes.delete(`k${i}`);
  }
  assert.equal(tree.size, boxes.size);
  assert.ok(tree.check());
  assert.equal(tree.remove("never"), false);
  const everything = { x: -1, y: -1, w: 6000, h: 6000 };
  assert.deepEqual(tree.search(everything).sort(), [...boxes.keys()].sort());
  const probe = { x: 1000, y: 1000, w: 800, h: 800 };
  const expected = [...boxes].filter(([, b]) => intersects(b, probe)).map(([k]) => k).sort();
  assert.deepEqual(tree.search(probe).sort(), expected);
});

test("insert on an existing key moves it", () => {
  const tree = new RTree();
  tree.insert("a", { x: 0, y: 0, w: 10, h: 10 });
  tree.insert("a", { x: 500, y: 500, w: 10, h: 10 });
  assert.equal(tree.size, 1);
  assert.deepEqual(tree.search({ x: 0, y: 0, w: 20, h: 20 }), []);
  assert.deepEqual(tree.search({ x: 495, y: 495, w: 20, h: 20 }), ["a"]);
  assert.deepEqual(tree.boxOf("a"), { x: 500, y: 500, w: 10, h: 10 });
});

test("a culling query over a large board touches a small share of the tree", () => {
  const tree = new RTree();
  let i = 0;
  for (let y = 0; y < 100; y++) for (let x = 0; x < 100; x++) tree.insert(`s${i++}`, { x: x * 150, y: y * 130, w: 130, h: 110 });
  const viewport = { x: 7000, y: 6000, w: 1600, h: 900 };
  const hits = tree.search(viewport);
  // 1600 / 150 columns by 900 / 130 rows, give or take an edge.
  assert.ok(hits.length >= 88 && hits.length <= 108, `${hits.length} of 10000 in view`);
});
