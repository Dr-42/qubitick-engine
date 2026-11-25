/**
 * layout-to-canvas.js — an ENGINE LAYOUT DOCUMENT, as the canvas's own workspace document.
 *
 * The other direction of `js/app/tiles-to-layout.js`: the engine describes a workspace as roots
 * whose rects hold split/tab trees, and the canvas describes the same workspace as CLUSTERS with
 * a binary tiling tree each (`{k:'h'|'v', r, a, b}`) plus loose tiles. This converts the first
 * into the second, so a document written by (or for) the engine can be opened here — which is what
 * "one document" has to mean before "the canvas is a layout mode of the engine" is more than a
 * sentence.
 *
 * The mapping, and the two conventions it has to know:
 *
 *   a root whose tree is a single leaf   -> a loose tile at the root's rect
 *   a root with a tree                   -> a cluster: the canvas's cluster rect is the root's
 *                                           rect with the frame's padding taken off, its members
 *                                           are the tree's leaves, and its tree is the engine's
 *                                           n-ary split written as the canvas's binary one
 *
 * One rule of the canvas's model has to be honoured on the way in: A CLUSTER HOLDS ONE TILE PER
 * KIND (js/world/clusters.js keys its members by kind, and `dockInto` refuses a second one). The
 * engine's document has no such rule, so a cluster holding two panes of the same kind converts to a
 * cluster holding the first of each — the members it can represent.
 *   `panes[id].kind`                     -> the tile's kind (a kind this build does not know is
 *                                           dropped by the canvas's own restore plan, not here)
 *   `camera {x,y,z}`                     -> the canvas's `{panX, panY, zoom}`
 *
 * Nothing is invented: a document that names no kind for a pane gets the canvas's editor kind,
 * which is what a tile with no kind is.
 */

/** Is this an engine layout document (roots with trees, a mode, a version)? */
export function isLayoutDocument(x) {
  return !!(x && typeof x === 'object' && Array.isArray(x.roots)
    && typeof x.version === 'number' && typeof x.mode === 'string' && x.canvas === undefined);
}

/** The canvas's frame padding, mirrored from js/world/clusters.js (INNER). */
const FRAME_INNER = { x: 16, y: 32 };

/** Every leaf's pane id, in reading order. */
export function leavesOf(tree, out = []) {
  if (!tree) return out;
  if (typeof tree.pane === 'string') { out.push(tree.pane); return out; }
  if (Array.isArray(tree.tabs)) { for (const t of tree.tabs) leavesOf(t, out); return out; }
  if (Array.isArray(tree.children)) { for (const c of tree.children) leavesOf(c, out); return out; }
  if (tree.group) leavesOf(tree.group, out);
  return out;
}

/**
 * The engine's n-ary split as the canvas's binary tree.
 *
 * The canvas cuts one seam at a time, so a three-way split becomes `a` plus the rest:
 * `{k:'h', r: sizes[0], a: first, b: <the rest, normalised>}`. The ratios are renormalised at each
 * fold, because a child's share of the whole has to become its share of what is left after the
 * cuts before it.
 */
export function binaryTreeOf(tree, keyOf = (paneId) => paneId) {
  if (!tree) return null;
  if (typeof tree.pane === 'string') return { k: 'leaf', pane: keyOf(tree.pane) };
  if (tree.group) return binaryTreeOf(tree.group, keyOf);
  const kids = Array.isArray(tree.tabs) ? tree.tabs : (Array.isArray(tree.children) ? tree.children : []);
  if (!kids.length) return null;
  if (kids.length === 1) return binaryTreeOf(kids[0], keyOf);
  // tabs are a stack in the canvas's model only insofar as it has no tabs node: it shows the
  // active entry, so a tabs node becomes its active child
  if (Array.isArray(tree.tabs)) {
    const active = typeof tree.active === 'number' ? tree.active : 0;
    return binaryTreeOf(tree.tabs[active] || tree.tabs[0], keyOf);
  }
  const split = tree.split === 'v' ? 'v' : 'h';
  const sizes = Array.isArray(tree.sizes) && tree.sizes.length === kids.length
    ? tree.sizes : kids.map(() => 1 / kids.length);
  const first = sizes[0];
  const rest = sizes.slice(1);
  const restTotal = rest.reduce((a, b) => a + b, 0) || 1;
  const tail = kids.length === 2
    ? binaryTreeOf(kids[1], keyOf)
    : binaryTreeOf({ split, sizes: rest.map((s) => s / restTotal), children: kids.slice(1) }, keyOf);
  return { k: split, r: Math.min(0.99, Math.max(0.01, first)), a: binaryTreeOf(kids[0], keyOf), b: tail };
}

/**
 * Convert an engine layout document into the canvas's workspace document.
 *
 * @param {object} layout - see js/layout/schema.js
 * @returns {{ canvas: object, roots: object[], free: object[] }}
 */
export function layoutToCanvas(layout, { inner = FRAME_INNER } = {}) {
  const panes = (layout && layout.panes) || {};
  const kindOf = (paneId) => (panes[paneId] && panes[paneId].kind) || 'editor';
  const roots = [];
  const free = [];

  for (const root of (layout && layout.roots) || []) {
    if (!root) continue;
    const leaves = leavesOf(root.tree);
    if (!leaves.length) continue;
    const isCluster = !(typeof root.tree.pane === 'string');
    /* a cluster's keys are KINDS, one per kind: that is the canvas's own rule */
    const seen = new Set();
    const keyed = [];
    for (const paneId of leaves) {
      const kind = kindOf(paneId);
      if (seen.has(kind)) continue;
      seen.add(kind);
      keyed.push({ key: kind, paneId, kind });
    }
    const keyOf = (paneId) => kindOf(paneId);

    if (!isCluster) {
      free.push({
        id: leaves[0], kind: kindOf(leaves[0]),
        x: root.x || 0, y: root.y || 0, w: root.w || 0, h: root.h || 0,
      });
      continue;
    }

    roots.push({
      /* a converted document names its cluster roots `cluster:<id>` (js/app/tiles-to-layout.js);
         the canvas's own document uses the bare id */
      id: String(root.id || '').replace(/^cluster:/, ''),
      x: (root.x || 0) - inner.x,
      y: (root.y || 0) - inner.y,
      w: root.w || 0,
      h: root.h || 0,
      kinds: keyed.map(({ paneId, kind }) => ({
        id: paneId, kind,
        // the cluster re-tiles from its tree; a rect is only a fallback for an empty tree
        x: (root.x || 0), y: (root.y || 0), w: 0, h: 0,
      })),
      tree: binaryTreeOf(root.tree, keyOf),
    });
  }

  const cam = layout && layout.camera;
  return {
    canvas: { panX: (cam && cam.x) || 0, panY: (cam && cam.y) || 0, zoom: (cam && cam.z) || 1 },
    roots,
    free,
  };
}