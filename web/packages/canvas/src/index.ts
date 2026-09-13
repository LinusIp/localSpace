// @localspace/canvas: the 2D infinite-canvas engine (architecture v2.1 §6.4).
// Camera, retained scene, R-tree culling, hit-testing, selection and
// handles, snapping with guides, text layout, freehand strokes, and an
// editor that turns input into document changes. Zero dependencies; undo
// is the environment's.

export { Editor, TOOLS, simplify } from "./editor.ts";
export type { Cause, Change, EditorEvents, EditorOptions, Snapping, Tool } from "./editor.ts";
export { Scene, STICKY_LINE_HEIGHT, STICKY_PADDING, STICKY_SIZE, TEXT_PADDING } from "./scene.ts";
export type { Curve, StickyLayout } from "./scene.ts";
export { Renderer, LIGHT } from "./render.ts";
export type { Overlay, Palette, Theme, Viewport } from "./render.ts";
export { RTree } from "./rtree.ts";
export { fit, pan, toBoard, toScreen, visible, zoomAt, zoomTo, MAX_ZOOM, MIN_ZOOM } from "./camera.ts";
export type { Camera } from "./camera.ts";
export { hitHandle, hitTest, nodesWithin, nodeHit, handlePoint, HANDLES } from "./hit.ts";
export type { Handle } from "./hit.ts";
export { DEFAULTS, FILLS, KINDS, boundsOf, cloneNode, fromDoc, sameNode, toDoc } from "./model.ts";
export type { BoardDoc, DocFrame, DocShape, Fill, Kind, Node } from "./model.ts";
export { CanvasMeasurer, FixedMeasurer, fontFor, layout } from "./text.ts";
export type { Layout, TextMeasurer } from "./text.ts";
export { GRID, SNAP_PX, snapMove, snapReach, snapResize, snapTargets } from "./snap.ts";
export type { Guide, SnapOptions, SnapTargets, Snapped } from "./snap.ts";
export { EXPORT_MAX_SIDE, EXPORT_PADDING, EXPORT_SCALE, SVG_FONT, exportBounds, exportNodes, rasterize, toSvg } from "./export.ts";
export type { ExportOptions, Raster } from "./export.ts";
export * from "./geometry.ts";
