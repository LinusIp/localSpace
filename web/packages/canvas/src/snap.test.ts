import { test } from "node:test";
import assert from "node:assert/strict";
import { GRID, SNAP_PX, snapMove, snapReach, snapResize, snapTargets } from "./snap.ts";

const shapesOnly = { reach: SNAP_PX, shapes: true, grid: 0 };
const a = { x: 100, y: 0, w: 100, h: 50 };
const moving = { x: 0, y: 200, w: 80, h: 40 };

test("a box moved to within reach of another's edge comes to rest on it, with a guide across both", () => {
  const s = snapMove(moving, 96, 0, snapTargets([a]), shapesOnly);
  assert.equal(s.dx, 100);
  assert.equal(s.dy, 0);
  assert.deepEqual(s.guides, [{ axis: "x", at: 100, from: 0, to: 240 }]);
});

test("beyond reach nothing snaps and no guide is drawn", () => {
  const s = snapMove(moving, 90, 0, snapTargets([a]), shapesOnly);
  assert.deepEqual(s, { dx: 90, dy: 0, guides: [] });
});

test("a centre comes to rest on a centre", () => {
  const s = snapMove(moving, 107, 0, snapTargets([a]), shapesOnly);
  assert.equal(s.dx, 110);
  assert.deepEqual(s.guides, [{ axis: "x", at: 150, from: 0, to: 240 }]);
});

test("the nearest line wins over a farther one", () => {
  const c = { x: 105, y: 400, w: 60, h: 20 };
  const s = snapMove(moving, 103, 0, snapTargets([a, c]), shapesOnly);
  assert.equal(s.dx, 105);
  assert.deepEqual(s.guides, [{ axis: "x", at: 105, from: 200, to: 420 }]);
});

test("a guide spans every shape that shares the line, one guide per line", () => {
  const d = { x: 100, y: 500, w: 40, h: 40 };
  const s = snapMove(moving, 96, 0, snapTargets([a, d]), shapesOnly);
  assert.equal(s.dx, 100);
  assert.deepEqual(s.guides, [
    { axis: "x", at: 100, from: 0, to: 540 },
    { axis: "x", at: 140, from: 200, to: 540 },
  ]);
});

test("reach is in screen pixels: at twice the zoom, half the distance on the board", () => {
  assert.equal(snapReach(1), SNAP_PX);
  assert.equal(snapReach(2), SNAP_PX / 2);
  const targets = snapTargets([a]);
  assert.equal(snapMove(moving, 95, 0, targets, { ...shapesOnly, reach: snapReach(2) }).dx, 95);
  assert.equal(snapMove(moving, 95, 0, targets, { ...shapesOnly, reach: snapReach(1) }).dx, 100);
});

test("with the grid on and no shape near, the top-left corner comes to rest on the grid", () => {
  const s = snapMove({ x: 0, y: 0, w: 80, h: 40 }, 30, 10, null, { reach: SNAP_PX, shapes: true, grid: GRID });
  assert.deepEqual(s, { dx: 24, dy: 0, guides: [] });
});

test("a shape within reach wins over the grid, and the grid takes the other axis", () => {
  const s = snapMove(moving, 97, 0, snapTargets([a]), { reach: SNAP_PX, shapes: true, grid: GRID });
  assert.equal(s.dx, 100);
  assert.equal(s.dy, -8);
  assert.deepEqual(s.guides, [{ axis: "x", at: 100, from: 0, to: 232 }]);
});

test("with shape snapping off, shapes are not rested on", () => {
  const s = snapMove(moving, 96, 0, snapTargets([a]), { reach: SNAP_PX, shapes: false, grid: 0 });
  assert.deepEqual(s, { dx: 96, dy: 0, guides: [] });
});

test("a resize by the east handle brings the right edge onto a target line and leaves the height alone", () => {
  const t = { x: 200, y: 100, w: 50, h: 50 };
  const s = snapResize({ x: 0, y: 0, w: 100, h: 50 }, "e", 95, 7, snapTargets([t]), shapesOnly);
  assert.equal(s.dx, 100);
  assert.equal(s.dy, 7);
  assert.deepEqual(s.guides, [{ axis: "x", at: 200, from: 0, to: 150 }]);
});

test("a resize by the south-west handle rests the left and bottom edges, each on its own line", () => {
  const t = { x: 0, y: 60, w: 50, h: 30 };
  const s = snapResize({ x: 100, y: 0, w: 100, h: 50 }, "sw", -47, 8, snapTargets([t]), shapesOnly);
  assert.equal(s.dx, -50);
  assert.equal(s.dy, 10);
  assert.deepEqual(s.guides, [
    { axis: "x", at: 50, from: 0, to: 90 },
    { axis: "y", at: 60, from: 0, to: 200 },
  ]);
});
