import { test } from "node:test";
import assert from "node:assert/strict";
import { fit, pan, toBoard, toScreen, visible, zoomAt, zoomTo, MAX_ZOOM, MIN_ZOOM } from "./camera.ts";

const close = (a: number, b: number) => Math.abs(a - b) < 1e-9;

test("screen and board coordinates round-trip", () => {
  const c = { x: 120, y: -40, z: 1.75 };
  const p = { x: 333, y: 97 };
  const back = toBoard(c, toScreen(c, p));
  assert.ok(close(back.x, p.x) && close(back.y, p.y));
});

test("zooming at a point keeps the board under that point still", () => {
  const c = { x: 10, y: 20, z: 1 };
  const at = { x: 300, y: 200 };
  const before = toBoard(c, at);
  const zoomed = zoomAt(c, at, 2);
  const after = toBoard(zoomed, at);
  assert.ok(close(before.x, after.x) && close(before.y, after.y));
  assert.equal(zoomed.z, 2);
});

test("zoom is clamped to the range", () => {
  const c = { x: 0, y: 0, z: 1 };
  assert.equal(zoomAt(c, { x: 0, y: 0 }, 1000).z, MAX_ZOOM);
  assert.equal(zoomAt(c, { x: 0, y: 0 }, 1e-6).z, MIN_ZOOM);
  assert.equal(zoomTo(c, 3, 800, 600).z, 3);
});

test("pan moves the camera by screen pixels scaled by the zoom", () => {
  const c = pan({ x: 0, y: 0, z: 2 }, 100, -50);
  assert.equal(c.x, -50);
  assert.equal(c.y, 25);
});

test("fit frames the extent with a margin and never zooms in past 1:1", () => {
  const c = fit({ x: 0, y: 0, w: 4000, h: 1000 }, 800, 600, 40);
  const v = visible(c, 800, 600);
  assert.ok(v.x <= 0 && v.y <= 0 && v.x + v.w >= 4000 && v.y + v.h >= 1000, "the extent is inside the view");
  const small = fit({ x: 100, y: 100, w: 50, h: 50 }, 800, 600);
  assert.equal(small.z, 1, "a small board is shown at 1:1, centred");
  const centre = toBoard(small, { x: 400, y: 300 });
  assert.ok(close(centre.x, 125) && close(centre.y, 125));
});
