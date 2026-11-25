/**
 * summary.js — one-line structural summary of a layout (M3.00).
 *
 * summary(layout) → e.g.
 *   "main: h[logo tabs(controls,output,diag)] | probe: x-probe | hud:ribbon(top,24) | modal:graph"
 *
 * Roots are shown as `id: tree`. HUD roots as `hud:id(dock,size)`.
 * Modals as `modal:id`. Leaves show pane kind (looked up from layout.panes);
 * groups as `group{...}`; splits as `h[...]`/`v[...]` (space-separated
 * children); tabs as `tabs(kind,kind,...)` (comma-separated).
 */

import { isLeaf, isTabs, isSplit, isGroup } from './schema.js';

/** Look up a pane instance's kind by id. */
function paneKind(layout, paneId) {
  const def = layout.panes && layout.panes[paneId];
  return def ? def.kind : '?';
}

/** Render a single tree node to its summary string. */
function treeStr(layout, t) {
  if (!t) return '';
  if (isLeaf(t)) return paneKind(layout, t.pane);
  if (isTabs(t)) {
    return 'tabs(' + t.tabs.map((c) => treeStr(layout, c)).join(',') + ')';
  }
  if (isSplit(t)) {
    return t.split + '[' + t.children.map((c) => treeStr(layout, c)).join(' ') + ']';
  }
  if (isGroup(t)) {
    return 'group{' + treeStr(layout, t.group) + '}';
  }
  return '?';
}

/**
 * Produce a one-line structural summary of the whole layout.
 * @param {object} layout
 * @returns {string}
 */
export function summary(layout) {
  const parts = [];
  for (const root of layout.roots || []) {
    parts.push(root.id + ': ' + treeStr(layout, root.tree));
  }
  for (const hud of layout.hud || []) {
    parts.push('hud:' + hud.id + '(' + hud.dock + ',' + hud.size + ')');
  }
  for (const modal of layout.modals || []) {
    parts.push('modal:' + modal.id);
  }
  return parts.join(' | ');
}
