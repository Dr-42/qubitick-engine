/**
 * tiles-to-layout.js — the canvas's arrangement, as an ENGINE LAYOUT DOCUMENT.
 *
 * The two apps describe a workspace differently, and this is the bridge: the canvas has TILES with
 * free rects, gathered (or not) into CLUSTERS; the engine has ROOTS, each a rect holding a
 * split/tab tree. The mapping is nearly one to one, which is what makes "the canvas is a layout
 * mode of the engine" a thing that can be said with evidence rather than intent:
 *
 *   a loose tile   -> a root of its own, whose tree is a single leaf
 *   a cluster      -> a root over the cluster's INNER rect, whose tree is the cluster's own tiling
 *                     tree (js/app/tiling.js's binary `{k:'h'|'v', r, a, b}`) written as the
 *                     engine's n-ary `{split, sizes, children}`
 *
 * One honest difference, and it is named rather than hidden: the canvas lays a cluster's tiles out
 * with a GAP between them (its sashes live in it), and the engine's splits are proportional with no
 * gap (its seams are overlays drawn on the boundary). So a converted document is the same
 * arrangement with its seams closed — the rects are the same up to the gap, which is what the test
 * pins down.
 *
 * Nothing here changes either app: it is the conversion a "canvas mode" of the engine would run,
 * and the proof that the document model can hold what the canvas draws.
 */

const INNER = { x: 16, y: 32 };

/**
 * The engine's tree for one of the canvas's tiling nodes.
 *
 * A CLUSTER'S TREE NAMES ITS MEMBERS BY THEIR CLUSTER KEY ('tree', 'graph'), not by tile id
 * ('c0:tree'): the cluster's own `panes` map is the translation. Without it a converted document
 * held a root for the cluster AND a root for every member (the member keys matched no tile, so
 * nothing looked "already in a cluster"), plus phantom panes named after the keys.
 */
function treeOf(node, tileIdOf) {
  if (!node) return null;
  if (node.k === 'leaf') return { pane: tileIdOf(node.pane) };
  const r = typeof node.r === 'number' ? node.r : 0.5;
  const split = node.k === 'h' ? 'h' : 'v';
  const a = treeOf(node.a, tileIdOf);
  const b = treeOf(node.b, tileIdOf);
  const children = [a, b].filter(Boolean);
  if (!children.length) return null;
  if (children.length === 1) return children[0];
  return { split, sizes: [r, 1 - r], children };
}

/** The pane instances a tiling tree names, as tile ids. */
function panesOf(node, tileIdOf, out = []) {
  if (!node) return out;
  if (node.k === 'leaf') { if (node.pane) out.push(tileIdOf(node.pane)); return out; }
  panesOf(node.a, tileIdOf, out);
  panesOf(node.b, tileIdOf, out);
  return out;
}

/**
 * The canvas's workspace as an engine layout document.
 *
 * @param {object} canvas
 * @param {Array} canvas.panes      the tiles: {id, kind, wr:{x,y,w,h}, home}
 * @param {Array} [canvas.clusters] the clusters: {id, x, y, w, h, tree, panes}
 * @param {{x:number,y:number,z:number}} [canvas.cam]
 * @param {string} [canvas.id]
 * @returns {object} a layout document (see js/layout/schema.js)
 */
export function tilesToLayout({ panes = [], clusters = [], cam, id = 'canvas' } = {}) {
  const inCluster = new Set();
  const roots = [];
  const docPanes = {};

  for (const c of clusters) {
    /* a cluster's tree names its members by their key in `c.panes`; the world knows them by tile id */
    const tileIdOf = (key) => {
      const tile = c.panes && c.panes[key];
      return (tile && tile.id) || key;
    };
    const tree = treeOf(c.tree, tileIdOf);
    if (!tree) continue;
    const members = panesOf(c.tree, tileIdOf);
    members.forEach((paneId) => inCluster.add(paneId));
    roots.push({
      id: 'cluster:' + c.id,
      x: c.x + INNER.x,
      y: c.y + INNER.y,
      w: c.w,
      h: c.h,
      tree,
    });
  }

  for (const p of panes) {
    const kind = p.kind || (typeof p.id === 'string' ? p.id.split(':').pop() : '');
    docPanes[p.id] = { kind };
    if (inCluster.has(p.id)) continue;              // it is inside its cluster's tree
    roots.push({
      id: p.id,
      x: p.wr.x,
      y: p.wr.y,
      w: p.wr.w,
      h: p.wr.h,
      tree: { pane: p.id },
    });
  }
  // the cluster members are tiles too, and their kinds travel with them
  for (const c of clusters) {
    const tileIdOf = (key) => {
      const tile = c.panes && c.panes[key];
      return (tile && tile.id) || key;
    };
    for (const paneId of panesOf(c.tree, tileIdOf)) {
      const tile = panes.find((x) => x.id === paneId) || (c.panes && c.panes[paneId]);
      docPanes[paneId] = { kind: (tile && tile.kind) || 'editor' };
    }
  }

  return {
    version: 1,
    id,
    mode: 'canvas',
    screen: 'desktop',
    wiring: 'standard',
    camera: { x: (cam && cam.x) || 0, y: (cam && cam.y) || 0, z: (cam && cam.z) || 1, lock: 'free' },
    panes: docPanes,
    roots,
    anchors: [],
    hud: [],
    modals: [],
  };
}