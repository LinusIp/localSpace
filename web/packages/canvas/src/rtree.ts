// A dynamic R-tree over string keys (architecture v2.1 §6.4: the spatial
// index behind viewport culling and hit-testing). Quadratic split, insert,
// remove and box search; nothing is allocated on a search that does not
// have to be.

import { area, enlargement, intersects, union, type Box } from "./geometry.ts";

const MAX_ENTRIES = 9;
const MIN_ENTRIES = 4;

interface Leaf {
  leaf: true;
  box: Box;
  key: string;
}

interface Inner {
  leaf: false;
  box: Box;
  children: Entry[];
  height: number;
}

type Entry = Leaf | Inner;

function isLeaf(e: Entry): e is Leaf {
  return e.leaf;
}

function boxOf(children: readonly Entry[]): Box {
  let b = children[0].box;
  for (let i = 1; i < children.length; i++) b = union(b, children[i].box);
  return b;
}

export class RTree {
  private root: Inner = { leaf: false, box: { x: 0, y: 0, w: 0, h: 0 }, children: [], height: 1 };
  private boxes = new Map<string, Box>();

  get size(): number {
    return this.boxes.size;
  }

  has(key: string): boolean {
    return this.boxes.has(key);
  }

  boxOf(key: string): Box | undefined {
    return this.boxes.get(key);
  }

  /** Insert, or move if the key is already indexed. */
  insert(key: string, b: Box): void {
    if (this.boxes.has(key)) this.remove(key);
    const stored: Box = { x: b.x, y: b.y, w: b.w, h: b.h };
    this.boxes.set(key, stored);
    const leaf: Leaf = { leaf: true, box: stored, key };
    const path: Inner[] = [];
    let node = this.root;
    // Descend to the leaf level choosing the child that grows least.
    while (node.height > 1) {
      path.push(node);
      let best: Inner | null = null;
      let bestGrowth = Infinity;
      let bestArea = Infinity;
      for (const child of node.children) {
        if (isLeaf(child)) continue;
        const growth = enlargement(child.box, stored);
        const a = area(child.box);
        if (growth < bestGrowth || (growth === bestGrowth && a < bestArea)) {
          best = child;
          bestGrowth = growth;
          bestArea = a;
        }
      }
      if (!best) break;
      node = best;
    }
    path.push(node);
    node.children.push(leaf);
    node.box = node.children.length === 1 ? { ...stored } : union(node.box, stored);
    // Split upwards while full.
    for (let i = path.length - 1; i >= 0; i--) {
      const n = path[i];
      if (n.children.length <= MAX_ENTRIES) {
        for (let j = i - 1; j >= 0; j--) path[j].box = union(path[j].box, stored);
        return;
      }
      const sibling = split(n);
      if (i === 0) {
        this.root = {
          leaf: false,
          box: union(n.box, sibling.box),
          children: [n, sibling],
          height: n.height + 1,
        };
      } else {
        path[i - 1].children.push(sibling);
        path[i - 1].box = boxOf(path[i - 1].children);
      }
    }
  }

  remove(key: string): boolean {
    const b = this.boxes.get(key);
    if (!b) return false;
    this.boxes.delete(key);
    const removed = removeFrom(this.root, key, b);
    if (removed) {
      // Collapse a root with one inner child.
      while (this.root.children.length === 1 && !isLeaf(this.root.children[0])) {
        this.root = this.root.children[0];
      }
      if (this.root.children.length === 0) this.root.height = 1;
      else this.root.box = boxOf(this.root.children);
      // Re-insert entries from nodes that fell under the minimum.
      for (const orphan of orphans.splice(0)) this.insert(orphan.key, orphan.box);
    }
    return removed;
  }

  /** Keys whose boxes intersect `b`, in no particular order. */
  search(b: Box, out: string[] = []): string[] {
    const stack: Entry[] = [this.root];
    while (stack.length) {
      const e = stack.pop() as Entry;
      if (!intersects(e.box, b)) continue;
      if (isLeaf(e)) out.push(e.key);
      else for (const c of e.children) stack.push(c);
    }
    return out;
  }

  clear(): void {
    this.root = { leaf: false, box: { x: 0, y: 0, w: 0, h: 0 }, children: [], height: 1 };
    this.boxes.clear();
  }

  /** The tree's own invariants, for tests: every box contains its children. */
  check(): boolean {
    const walk = (e: Entry): boolean => {
      if (isLeaf(e)) return true;
      for (const c of e.children) {
        if (!containsBoxLoose(e.box, c.box) || !walk(c)) return false;
      }
      return true;
    };
    return walk(this.root);
  }
}

const orphans: Leaf[] = [];

function containsBoxLoose(outer: Box, inner: Box): boolean {
  const eps = 1e-9;
  return (
    inner.x >= outer.x - eps &&
    inner.y >= outer.y - eps &&
    inner.x + inner.w <= outer.x + outer.w + eps &&
    inner.y + inner.h <= outer.y + outer.h + eps
  );
}

function removeFrom(node: Inner, key: string, b: Box): boolean {
  if (!intersects(node.box, b) && node.children.length > 0) return false;
  for (let i = 0; i < node.children.length; i++) {
    const c = node.children[i];
    if (isLeaf(c)) {
      if (c.key === key) {
        node.children.splice(i, 1);
        if (node.children.length) node.box = boxOf(node.children);
        return true;
      }
    } else if (removeFrom(c, key, b)) {
      if (c.children.length < MIN_ENTRIES) {
        // Too small: take it out and let its leaves be re-inserted.
        node.children.splice(i, 1);
        collectLeaves(c, orphans);
      }
      if (node.children.length) node.box = boxOf(node.children);
      return true;
    }
  }
  return false;
}

function collectLeaves(e: Entry, into: Leaf[]): void {
  if (isLeaf(e)) into.push(e);
  else for (const c of e.children) collectLeaves(c, into);
}

/** Quadratic split: seed with the two entries that waste the most space together. */
function split(node: Inner): Inner {
  const entries = node.children;
  let seedA = 0;
  let seedB = 1;
  let worst = -Infinity;
  for (let i = 0; i < entries.length; i++) {
    for (let j = i + 1; j < entries.length; j++) {
      const waste = area(union(entries[i].box, entries[j].box)) - area(entries[i].box) - area(entries[j].box);
      if (waste > worst) {
        worst = waste;
        seedA = i;
        seedB = j;
      }
    }
  }
  const groupA: Entry[] = [entries[seedA]];
  const groupB: Entry[] = [entries[seedB]];
  let boxA = entries[seedA].box;
  let boxB = entries[seedB].box;
  const rest = entries.filter((_, i) => i !== seedA && i !== seedB);
  while (rest.length) {
    // If one group must take the rest to reach the minimum, give it all.
    if (groupA.length + rest.length === MIN_ENTRIES) {
      for (const e of rest) {
        groupA.push(e);
        boxA = union(boxA, e.box);
      }
      break;
    }
    if (groupB.length + rest.length === MIN_ENTRIES) {
      for (const e of rest) {
        groupB.push(e);
        boxB = union(boxB, e.box);
      }
      break;
    }
    // Otherwise the entry with the strongest preference goes first.
    let pick = 0;
    let pickDiff = -Infinity;
    for (let i = 0; i < rest.length; i++) {
      const diff = Math.abs(enlargement(boxA, rest[i].box) - enlargement(boxB, rest[i].box));
      if (diff > pickDiff) {
        pickDiff = diff;
        pick = i;
      }
    }
    const [e] = rest.splice(pick, 1);
    const growA = enlargement(boxA, e.box);
    const growB = enlargement(boxB, e.box);
    if (growA < growB || (growA === growB && groupA.length <= groupB.length)) {
      groupA.push(e);
      boxA = union(boxA, e.box);
    } else {
      groupB.push(e);
      boxB = union(boxB, e.box);
    }
  }
  node.children = groupA;
  node.box = boxA;
  return { leaf: false, box: boxB, children: groupB, height: node.height };
}
