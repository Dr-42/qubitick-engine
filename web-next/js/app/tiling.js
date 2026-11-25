/*
 * tiling.js — how a cluster lays its tiles out.
 *
 * Extracted from the inline script in web-next/index.html. A cluster is a box holding one
 * pane per kind, and the layout is a SPLIT TREE: each node either holds a pane (a leaf) or
 * divides its box in two (a cut), carrying the ratio the cut sits at. The tree is what the
 * sashes move — dragging a sash does not resize a pane, it changes one ratio — and that is
 * why the tree, not a list of rectangles, is the thing worth saving.
 *
 * Everything here is arithmetic on kinds and boxes: which pane goes where, at what ratio,
 * and which rectangles that produces. No DOM, so the rules are testable:
 *
 *   · a cluster has a READING ORDER (PIPELINE): a corpus tree, then the graph, then the
 *     sources, then what is computed from them. Tiles are laid out in that order whatever
 *     order they were added in;
 *   · each cut takes the axis that keeps the resulting boxes nearest a readable aspect
 *     ratio, so a wide box splits sideways and a tall one stacks, and nobody chooses per
 *     layout;
 *   · the cut goes where the running WEIGHT passes half, but never at an edge — a cut that
 *     isolates one small tile is a cut that wasted the box.
 *
 * The weights are preferences, not promises: an unknown kind weighs the default, so a user
 * pane tiles without being listed anywhere.
 */

/** Reading order: the order tiles appear in a cluster, and the order they are split in. */
export const PIPELINE = [
  'tree', 'graph', 'symbols', 'editor', 'handwrite', 'render', 'ir', 'lean', 'goal',
  'output', 'log', 'stats', 'palette', 'constant', 'js-node', 'clock', 'track',
];

/** Relative area a kind asks for, and the default for a kind nobody listed. */
export const KIND_WEIGHT = {
  tree: 0.5, graph: 1.1, stats: 0.7, symbols: 0.85, editor: 1.6, render: 1.1,
  ir: 0.9, lean: 0.9, goal: 0.7, output: 0.8, handwrite: 0.9, palette: 0.8,
  constant: 0.7, 'js-node': 0.9, clock: 0.7, track: 0.7,
};
const DEFAULT_WEIGHT = 0.9;

/** A box wider than this (over its height) splits sideways rather than stacking. */
export const TARGET_ASPECT = 1.35;

export const kindOrder = (k) => {
  const i = PIPELINE.indexOf(k);
  return i < 0 ? PIPELINE.length : i;
};
export const kindWeight = (k) => (KIND_WEIGHT[k] !== undefined ? KIND_WEIGHT[k] : DEFAULT_WEIGHT);

/** The gap between two neighbours, in layout px. */
export const GAP = 2;

/** The narrowest a pane may be laid out to, in layout px: enough to be a tile. */
export const MIN_SIDE = 24;

/**
 * Build the split tree for a list of kinds in a box.
 * A leaf is {k:'leaf', pane}; a cut is {k:'h'|'v', r, a, b} where `r` is the fraction the
 * FIRST child takes (of width for 'h', of height for 'v').
 */
export function autoTree(kinds, boxW = 1600, boxH = 900) {
  const list = kinds.slice().sort((a, b) => kindOrder(a) - kindOrder(b) || a.localeCompare(b));

  function build(items, w, h) {
    if (items.length === 1) return { k: 'leaf', pane: items[0] };
    const total = items.reduce((a, k) => a + kindWeight(k), 0);
    // cut where the running weight passes half, but never at an edge
    let acc = 0, cut = 1;
    for (let i = 0; i < items.length - 1; i++) {
      acc += kindWeight(items[i]);
      if (acc >= total / 2) { cut = i + 1; break; }
    }
    const A = items.slice(0, cut), B = items.slice(cut);
    const share = A.reduce((a, k) => a + kindWeight(k), 0) / total;
    if ((w / h) > TARGET_ASPECT) {
      const ww = Math.max(60, Math.round(w * share));
      return { k: 'h', r: ww / w, a: build(A, ww, h), b: build(B, w - ww, h) };
    }
    const hh = Math.max(40, Math.round(h * share));
    return { k: 'v', r: hh / h, a: build(A, w, hh), b: build(B, w, h - hh) };
  }
  return build(list, boxW, boxH);
}

/**
 * Turn a tree into rectangles and sash descriptors, inside `box`.
 *
 * The sash is placed ON the cut, at the boundary minus half its own thickness, and it
 * carries the box it is dividing (`pw`/`ph`) so a drag can turn a pointer delta into a
 * ratio without walking the tree again, plus the tree node it moves (`node`) so the drag
 * changes THAT ratio and no other — which is the whole point of a tree.
 */
export function solveRects(tree, box, { gap = GAP, min = MIN_SIDE } = {}) {
  const rects = {}, sashes = [];
  /* The cut is placed where the ratio says, then clamped so BOTH children get a usable
     box. Without the clamp a ratio near the end of its range makes the far side's box
     negative — `w - w1 - gap` — and a pane with a negative width is a pane the browser
     clamps to nothing while the layout carries on as if it were there. The clamp is why a
     sash drag cannot squash a pane flat even if a saved ratio says it should. */
  const cut = (total, frac) => {
    const want = Math.round(total * frac);
    if (total < 3 * min + gap) return Math.max(min, Math.round((total - gap) / 2));  // no room to be fussy
    return Math.min(Math.max(want, min + gap), total - 2 * min);
  };
  (function walk(n, r) {
    if (!n) return;
    if (n.k === 'leaf') { rects[n.pane] = r; return; }
    if (n.k === 'h') {
      const w1 = cut(r.w, n.r);
      walk(n.a, { x: r.x, y: r.y, w: w1 - gap, h: r.h });
      walk(n.b, { x: r.x + w1 + gap, y: r.y, w: r.w - w1 - gap, h: r.h });
      sashes.push({ node: n, dir: 'h', x: r.x + w1 - 4, y: r.y, w: 8, h: r.h, pw: r.w, ph: r.h });
    } else {
      const h1 = cut(r.h, n.r);
      walk(n.a, { x: r.x, y: r.y, w: r.w, h: h1 - gap });
      walk(n.b, { x: r.x, y: r.y + h1 + gap, w: r.w, h: r.h - h1 - gap });
      sashes.push({ node: n, dir: 'v', x: r.x, y: r.y + h1 - 4, w: r.w, h: 8, pw: r.w, ph: r.h });
    }
  })(tree, box);
  return { rects, sashes };
}

/* ---------------------------------------------------------------- finding */
/**
 * The cluster whose frame overlaps `rect` most, or null. `frameOf` returns a cluster's
 * outer frame (the tiles plus their furniture), because a tile dropped near a cluster's
 * edge should land in it, and the overlap threshold is what stops a stray tile that merely
 * clipped a corner from being taken in.
 */
export function clusterUnder(rect, clusters, { frameOf, minShare = 0.35 } = {}) {
  let best = null, bestArea = 0;
  for (const c of clusters) {
    const f = frameOf(c);
    const ox = Math.max(0, Math.min(rect.x + rect.w, f.x + f.w) - Math.max(rect.x, f.x));
    const oy = Math.max(0, Math.min(rect.y + rect.h, f.y + f.h) - Math.max(rect.y, f.y));
    const area = ox * oy;
    if (area > bestArea) { bestArea = area; best = c; }
  }
  const self = rect.w * rect.h;
  return (best && bestArea > self * minShare) ? best : null;
}

/** The cluster whose centre is nearest a tile's centre, for gathering a stray tile. */
export function nearestCluster(rect, clusters) {
  if (!clusters.length) return null;
  const cx = rect.x + rect.w / 2, cy = rect.y + rect.h / 2;
  let best = null, bestD = Infinity;
  for (const c of clusters) {
    const dx = (c.x + c.w / 2) - cx, dy = (c.y + c.h / 2) - cy, d = dx * dx + dy * dy;
    if (d < bestD) { bestD = d; best = c; }
  }
  return best;
}