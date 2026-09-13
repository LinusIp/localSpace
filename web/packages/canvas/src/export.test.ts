import { test } from "node:test";
import assert from "node:assert/strict";
import { Scene } from "./scene.ts";
import { FixedMeasurer } from "./text.ts";
import { EXPORT_PADDING, SVG_FONT, exportBounds, exportNodes, toSvg } from "./export.ts";
import type { Node } from "./model.ts";

const node = (over: Partial<Node> & Pick<Node, "id" | "kind">): Node => ({
  x: 0,
  y: 0,
  w: 130,
  h: 110,
  fill: "yellow",
  text: "",
  frame: null,
  z: 1,
  locked: false,
  ...over,
});

function board(): Scene {
  const s = new Scene(new FixedMeasurer(8));
  s.set(node({ id: "frame", kind: "frame", x: 0, y: 0, w: 400, h: 300, name: "Risks", z: 0 }));
  s.set(node({ id: "a", kind: "sticky", x: 20, y: 40, text: "Ship the <new> plan & tell everyone", frame: "frame", z: 2 }));
  s.set(node({ id: "b", kind: "rect", x: 220, y: 40, fill: "blue", text: "Review", frame: "frame", z: 3 }));
  s.set(node({ id: "c", kind: "ellipse", x: 600, y: 500, w: 120, h: 80, fill: "green", z: 4 }));
  s.set(node({ id: "label", kind: "text", x: 500, y: 20, w: 200, h: 40, size: 20, text: "Title\nsecond line", z: 5 }));
  s.set(node({ id: "arrow", kind: "arrow", x: 0, y: 0, w: 0, h: 0, fill: "grey", from: "a", to: "b", text: "leads to", z: 6 }));
  s.set(
    node({
      id: "ink",
      kind: "ink",
      x: 700,
      y: 700,
      w: 50,
      h: 50,
      fill: "red",
      points: [
        [700, 700],
        [720, 730],
        [750, 750],
      ],
      z: 7,
    }),
  );
  return s;
}

test("an export covers the selection, a selected frame brings its contents, none means all", () => {
  const s = board();
  assert.deepEqual(
    exportNodes(s, new Set()).map((n) => n.id),
    s.all().map((n) => n.id),
  );
  assert.deepEqual(exportNodes(s, new Set(["frame"])).map((n) => n.id), ["frame", "a", "b"]);
  assert.deepEqual(exportNodes(s, new Set(["c", "label"])).map((n) => n.id), ["c", "label"]);
});

test("the bounds pad the union of what is exported and follow an arrow to its ends", () => {
  const s = board();
  const one = exportBounds(s, [s.get("a")!]);
  assert.deepEqual(one, { x: 20 - EXPORT_PADDING, y: 40 - EXPORT_PADDING, w: 130 + 2 * EXPORT_PADDING, h: 110 + 2 * EXPORT_PADDING });
  const all = exportBounds(s, exportNodes(s, new Set()));
  assert.equal(all.x, -EXPORT_PADDING);
  assert.equal(all.y, -EXPORT_PADDING);
  assert.ok(all.w >= 750 + EXPORT_PADDING, `${all.w}`);
  assert.ok(all.h >= 750 + EXPORT_PADDING, `${all.h}`);
  const arrowOnly = exportBounds(s, [s.get("arrow")!], 0);
  assert.ok(arrowOnly.x >= 150 && arrowOnly.x + arrowOnly.w <= 220, JSON.stringify(arrowOnly));
  assert.deepEqual(exportBounds(s, []), { x: 0, y: 0, w: 0, h: 0 });
});

test("an SVG has every kind in place, text wrapped as the canvas wraps it, and nothing that runs", () => {
  const s = board();
  const nodes = exportNodes(s, new Set());
  const svg = toSvg(s, nodes);
  const box = exportBounds(s, nodes);

  assert.ok(svg.startsWith('<svg xmlns="http://www.w3.org/2000/svg"'), svg.slice(0, 80));
  assert.ok(svg.includes(`viewBox="${box.x} ${box.y} ${box.w} ${box.h}"`), svg.slice(0, 200));
  assert.ok(svg.includes(`font-family="${SVG_FONT}"`));
  assert.ok(svg.endsWith("</svg>"));

  // Rects: the background, the frame, the sticky, the white rect and its
  // colour chip, the clips around the two texts that wrap and the label
  // that does not, and the connector's own label.
  assert.equal((svg.match(/<rect /g) ?? []).length, 9, svg);
  assert.ok(svg.includes(">Risks</text>"));
  assert.ok(svg.includes('rx="4" fill="#fbe8a6"/>'), "the sticky's pastel fill, no border");
  assert.ok(svg.includes('rx="10" fill="#ffffff" stroke="#3e6fa8"'), "a white rect with the blue stroke");
  assert.ok(svg.includes('width="14" height="14" fill="#cde3f5"'), "the blue chip");
  assert.equal((svg.match(/<ellipse /g) ?? []).length, 1);
  // The connector joins two shapes: a curve in the connector grey, not a line.
  assert.equal((svg.match(/<line /g) ?? []).length, 0);
  assert.equal((svg.match(/<path d="M /g) ?? []).length, 1);
  assert.ok(svg.includes('stroke="#9a9da1" stroke-width="2.2"'), "the connector's colour and width");
  assert.equal((svg.match(/<polygon /g) ?? []).length, 1);
  assert.ok(svg.includes('text-anchor="middle" xml:space="preserve">leads to</text>'));
  assert.equal((svg.match(/<polyline /g) ?? []).length, 1);
  assert.ok(svg.includes('points="700,700 720,730 750,750"'));

  // Text: escaped, one positioned tspan per wrapped line, the size written.
  assert.ok(svg.includes("&lt;new&gt;") && svg.includes("&amp;"), "escaped");
  assert.ok(!svg.includes("<new>"));
  const stickyLayout = s.stickyLayout(s.get("a")!);
  assert.ok(stickyLayout.title.lines.length > 1, "the fixture wraps");
  assert.equal(stickyLayout.body, null, "one paragraph: a title and no body");
  const stickyText = svg.slice(svg.indexOf('<clipPath id="clip1">'), svg.indexOf("</text>", svg.indexOf('<clipPath id="clip1">')));
  assert.equal((stickyText.match(/<tspan /g) ?? []).length, stickyLayout.title.lines.length);
  assert.ok(stickyText.includes('font-size="13.5" font-weight="600"'), "the title, in bold");
  const labelIndex = svg.indexOf('font-size="20"');
  assert.ok(labelIndex > 0, "the label at its own size");
  assert.equal((svg.slice(labelIndex, svg.indexOf("</text>", labelIndex)).match(/<tspan /g) ?? []).length, 2, "one tspan per line of the label");

  // Nothing runs, nothing is fetched.
  assert.ok(!/<script|onload=|href=|xlink:|<image/i.test(svg));
  assert.equal(svg.indexOf("http"), svg.lastIndexOf("http"), "the namespace is the only URL");

  // The same board is the same file.
  assert.equal(toSvg(s, nodes), svg);
});

test("an empty export is refused rather than an empty picture", () => {
  const s = board();
  assert.throws(() => toSvg(s, []), /nothing to export/);
});
