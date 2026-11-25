/**
 * serialize.js — pure layout serialization (M1.21).
 *
 * normalize(layout, opts) → new layout:
 *   - normalises every split's sizes via normalizeSizes
 *   - drops unreferenced panes (unless opts.keepUnplaced)
 *   - rewrites anchored roots' x,y via resolveAnchors
 *   - fills defaults (cycle:'fixed', active:0, camera.lock:'free')
 * serialize(layout) → pretty JSON string (2-space, stable key order)
 * parse(text) → {ok:true, layout} | {ok:false, error} (never throws)
 */

import { normalizeSizes } from './tree-ops.js';
import { resolveAnchors } from './anchors.js';
import { isLeaf, isSplit, isGroup } from './schema.js';

const KEY_ORDER = [
  'version', 'id', 'mode', 'screen', 'wiring', 'camera',
  'panes', 'roots', 'anchors', 'hud', 'modals',
];

function deep(v) { return JSON.parse(JSON.stringify(v)); }

function hasTabsKey(n) {
  return n !== null && typeof n === 'object' && !Array.isArray(n) && 'tabs' in n;
}

// Collect all referenced pane ids from a tree.
function collectPaneRefs(node, out) {
  if (isLeaf(node)) { out.add(node.pane); return; }
  if (hasTabsKey(node)) { node.tabs.forEach((c) => collectPaneRefs(c, out)); return; }
  if (isSplit(node)) { node.children.forEach((c) => collectPaneRefs(c, out)); return; }
  if (isGroup(node)) { collectPaneRefs(node.group, out); return; }
}

// Fill defaults recursively across a tree.
function fillTreeDefaults(node) {
  if (hasTabsKey(node)) {
    if (node.cycle === undefined) node.cycle = 'fixed';
    if (!Number.isFinite(node.active)) node.active = 0;
    node.tabs.forEach(fillTreeDefaults);
    return;
  }
  if (isSplit(node)) {
    node.children.forEach(fillTreeDefaults);
    return;
  }
  if (isGroup(node)) {
    fillTreeDefaults(node.group);
    return;
  }
}

// Normalize all split sizes in a tree, mutating the (cloned) node.
function normalizeTreeSizes(node) {
  if (isSplit(node)) {
    node.sizes = normalizeSizes(node.sizes);
    node.children.forEach(normalizeTreeSizes);
    return;
  }
  if (hasTabsKey(node)) { node.tabs.forEach(normalizeTreeSizes); return; }
  if (isGroup(node)) { normalizeTreeSizes(node.group); return; }
}

export function normalize(layout, opts = {}) {
  const l = deep(layout);

  /* THE DOCUMENT'S OWN SHAPE FIRST. `js/layout/schema.js` says a layout has roots, anchors, hud,
     modals and panes; a PRESET arrives with only the ones it fills in (no preset carries
     `anchors`, since a shipped arrangement has none), so every reader of a normalized document
     had to defend itself — `anchor.set` did not, and threw where it meant to refuse. */
  if (!Array.isArray(l.roots)) l.roots = [];
  if (!Array.isArray(l.anchors)) l.anchors = [];
  if (!Array.isArray(l.hud)) l.hud = [];
  if (!Array.isArray(l.modals)) l.modals = [];
  if (!l.panes || typeof l.panes !== 'object') l.panes = {};

  // camera defaults
  if (!l.camera) l.camera = {};
  if (l.camera.lock === undefined) l.camera.lock = 'free';

  // roots, hud, modals: normalize sizes, fill tab defaults, collect refs
  const allRefs = new Set();
  const containers = [];
  if (Array.isArray(l.roots)) containers.push(...l.roots.map((r) => ({ tree: r.tree })));
  if (Array.isArray(l.hud)) containers.push(...l.hud.map((h) => ({ tree: h.tree })));
  if (Array.isArray(l.modals)) containers.push(...l.modals.map((m) => ({ tree: m.tree })));

  containers.forEach(({ tree }) => {
    if (!tree) return;
    normalizeTreeSizes(tree);
    fillTreeDefaults(tree);
    collectPaneRefs(tree, allRefs);
  });

  // drop unreferenced panes unless keepUnplaced
  if (!opts.keepUnplaced && l.panes && typeof l.panes === 'object') {
    for (const id of Object.keys(l.panes)) {
      if (!allRefs.has(id)) delete l.panes[id];
    }
  }

  // rewrite anchored roots' x,y via resolveAnchors
  if (Array.isArray(l.roots) && Array.isArray(l.anchors)) {
    const rootList = l.roots.filter((r) => r && typeof r.id === 'string' && typeof r.x === 'number' && typeof r.y === 'number');
    const resolved = resolveAnchors(rootList, l.anchors);
    for (const [id, pos] of resolved.entries()) {
      const root = l.roots.find((r) => r && r.id === id);
      if (root) {
        root.x = pos.x;
        root.y = pos.y;
      }
    }
  }

  return l;
}

export function serialize(layout) {
  const ordered = {};
  for (const k of KEY_ORDER) {
    if (k in layout) ordered[k] = layout[k];
  }
  for (const k of Object.keys(layout)) {
    if (!KEY_ORDER.includes(k)) ordered[k] = layout[k];
  }
  return JSON.stringify(ordered, null, 2);
}

export function parse(text) {
  try {
    const layout = JSON.parse(text);
    return { ok: true, layout };
  } catch (err) {
    const msg = err && err.message ? err.message : String(err);
    return { ok: false, error: msg };
  }
}
