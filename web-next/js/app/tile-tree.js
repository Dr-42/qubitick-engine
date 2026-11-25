/*
 * app/tile-tree.js — the split tree a cluster keeps, as JSON.
 *
 * A cluster's tiling is a binary tree of ratios, and it travels in the workspace document so a
 * cluster comes back with the sash positions the user dragged to. Two rules make the round trip
 * safe, and both are about what to do with a document that is not quite right:
 *
 *   A RATIO IS ROUNDED, NOT TRUSTED. A saved fraction of 0.5000000001 is the same layout as
 *   0.5, and an un-rounded value makes two workspaces that look identical differ byte for byte.
 *
 *   A BROKEN SUBTREE IS DROPPED WHOLE. A node with a missing or unknown kind, or with only one
 *   child, cannot be applied to a cluster — and half-applying it (one leaf in, one out) is worse
 *   than falling back to the cluster's normal tiling. So a subtree that does not parse is `null`
 *   all the way up, and the caller tiles the cluster itself.
 */

/** A leaf names a TILE by kind; an inner node is a split with a ratio in [0,1]. */
export function treeToJSON(node) {
  if (!node) return null;
  if (node.k === 'leaf') return { k: 'leaf', pane: node.pane };
  return { k: node.k, r: Math.round(node.r * 1000) / 1000, a: treeToJSON(node.a), b: treeToJSON(node.b) };
}

/**
 * @param {object} j  a saved tree
 * @returns {object|null} the tree, or null when the document's subtree cannot be applied
 */
export function treeFromJSON(j) {
  if (!j || (j.k !== 'leaf' && j.k !== 'h' && j.k !== 'v')) return null;
  if (j.k === 'leaf') return typeof j.pane === 'string' ? { k: 'leaf', pane: j.pane } : null;
  const a = treeFromJSON(j.a), b = treeFromJSON(j.b);
  if (!a || !b) return null;                      // one child is not a split
  return { k: j.k, r: Number(j.r) || 0.5, a, b };
}