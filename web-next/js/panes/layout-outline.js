/**
 * panes/layout-outline.js — the layout document, as a tree you can read and pick from.
 *
 * Rows: every root with its tree (splits, tabs, groups, leaves), then the HUD roots, then the
 * modals. A row says what it is, and a click PICKS it: the pane highlights the pane instance on
 * the canvas and publishes the pick on its `picked` out-port, which is what a wiring can use.
 *
 * In:  `layout` (value — the document; the design wiring connects it)
 * Out: `outline` (value — the one-line summary, which is what js/layout/summary.js makes),
 *      `picked` (value — the row that was picked: { type, id, path })
 * Call: `focus` (re-draw from the current document)
 *
 * The module came out of the engine's earlier style (`createLayoutOutlinePane({root, store,
 * camera, lock})`) and was never wired to its kind, so the kind rendered nothing while this
 * held row actions — a ✕ that removed a pane, a ⌖ that flew the camera — that reached a store
 * and a camera directly. A pane does not get those: the contract hands it `ctx` (data, editor,
 * ports, ui) and its own ports, and neither host gives a pane a store to dispatch into. So the
 * actions are gone and the PICK is a port: a removal is a wiring's business, not a pane's.
 *
 * The summary line is js/layout/summary.js's — the same one-line description of a document
 * that the M3.00 work wrote, with no caller until now.
 */

import { valuePort, callPort } from '../core/ports.js';
import { summary } from '../layout/summary.js';

/** The tree's own node kinds, for a row's label. */
function treeRows(layout, node, path, out) {
  if (!node) return;
  if (typeof node.pane === 'string') {
    const def = (layout.panes && layout.panes[node.pane]) || {};
    out.push({ type: 'pane', id: node.pane, label: def.kind || node.pane, path: [...path] });
    return;
  }
  if (Array.isArray(node.tabs)) {
    out.push({ type: 'tabs', id: null, label: 'tabs (' + node.tabs.length + ')', path: [...path] });
    node.tabs.forEach((child, i) => treeRows(layout, child, [...path, { tabs: i }], out));
    return;
  }
  if (Array.isArray(node.children)) {
    out.push({ type: 'split', id: null, label: 'split ' + node.split, path: [...path] });
    node.children.forEach((child, i) => treeRows(layout, child, [...path, { children: i }], out));
    return;
  }
  if (node.group) {
    out.push({ type: 'group', id: null, label: 'group', path: [...path] });
    treeRows(layout, node.group, [...path, { group: 0 }], out);
  }
}

export default {
  kind: 'layout-outline',
  title: 'Layout Outline',
  ports: {
    in: { layout: 'value' },
    out: { outline: 'value', picked: 'value' },
    call: { focus: 'call' },
  },
  create(ctx) {
    const esc = (ctx.ui && ctx.ui.esc) || ((s) => String(s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])));

    const inLayout = valuePort(null);
    const outOutline = valuePort(null);
    const outPicked = valuePort(null);

    let panel = null;
    let headEl = null;
    let treeEl = null;
    let current = null;
    let highlightTimer = null;

    const rowsOf = (layout) => {
      const rows = [];
      for (const root of (layout.roots || [])) {
        rows.push({ type: 'root', id: root.id || 'main', label: 'root ' + (root.id || 'main'), path: [{ root: root.id }] });
        treeRows(layout, root.tree, [{ root: root.id }], rows);
      }
      for (const hud of (layout.hud || [])) {
        rows.push({ type: 'hud', id: hud.id, label: 'hud ' + hud.id + ' (' + (hud.dock || 'top') + ')', path: [{ hud: hud.id }] });
      }
      for (const modal of (layout.modals || [])) {
        rows.push({ type: 'modal', id: modal.id, label: 'modal ' + modal.id, path: [{ modal: modal.id }] });
      }
      return rows;
    };

    function clearHighlight() {
      if (highlightTimer) { clearTimeout(highlightTimer); highlightTimer = null; }
      document.querySelectorAll('.outline-hl').forEach((el) => el.classList.remove('outline-hl'));
    }

    /** A pick is visible twice: on the row, and on the tile it names. */
    function pick(row) {
      if (!row || !panel) return;
      clearHighlight();
      panel.querySelectorAll('[data-row]').forEach((el) => el.removeAttribute('aria-selected'));
      const sel = panel.querySelector('[data-row="' + esc(row.type + ':' + (row.id || row.label)) + '"]');
      if (sel) sel.setAttribute('aria-selected', 'true');

      if (row.type === 'pane' && row.id) {
        const el = document.querySelector('[data-pane="' + row.id + '"]');
        if (el) {
          el.classList.add('outline-hl');
          highlightTimer = setTimeout(() => el.classList.remove('outline-hl'), 1500);
        }
      }
      outPicked.set({ type: row.type, id: row.id, label: row.label, path: row.path });
    }

    function draw() {
      if (!panel) return;
      panel.textContent = '';
      const layout = current;
      if (!layout) {
        headEl = document.createElement('div');
        headEl.className = 'outline-empty';
        headEl.textContent = 'no layout: this pane reads its `layout` in-port';
        panel.appendChild(headEl);
        return;
      }

      /* the one-line description (js/layout/summary.js), which is also what leaves on the
         `outline` port: the pane is a reader of the document, and this is the document in
         one line */
      const line = summary(layout);
      headEl = document.createElement('div');
      headEl.className = 'outline-summary';
      headEl.style.cssText = 'padding:4px 6px; border-bottom:1px solid #2a2f3e; color:#8ea2c0; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;';
      headEl.textContent = line;
      headEl.title = line;
      panel.appendChild(headEl);
      outOutline.set(line);

      const rows = rowsOf(layout);
      treeEl = document.createElement('div');
      treeEl.setAttribute('role', 'tree');
      rows.forEach((row, i) => {
        const el = document.createElement('div');
        el.className = 'outline-row';
        el.setAttribute('role', 'treeitem');
        el.setAttribute('tabindex', i === 0 ? '0' : '-1');
        el.dataset.row = row.type + ':' + (row.id || row.label);
        el.dataset.type = row.type;
        el.style.cssText = 'padding:3px 6px; cursor:default; border-bottom:1px solid #1c2130;'
          + ' padding-left:' + (6 + Math.min(row.path.length - 1, 6) * 12) + 'px;';
        el.textContent = row.label;
        el.addEventListener('click', () => pick(row));
        treeEl.appendChild(el);
      });
      panel.appendChild(treeEl);
    }

    function mount(host) {
      panel = document.createElement('div');
      panel.className = 'layout-outline';
      panel.style.cssText =
        'position:absolute; inset:0; overflow:auto; color:#d6e2ee;'
        + ' font-family:inherit; font-size:12px; background:#1c2130; padding:8px; box-sizing:border-box;';
      host.appendChild(panel);
      draw();
    }

    function unmount() {
      clearHighlight();
      if (panel && panel.parentNode) panel.parentNode.removeChild(panel);
      panel = null;
      headEl = null;
      treeEl = null;
    }

    inLayout.subscribe((layout) => { current = layout || null; draw(); });

    const callFocus = callPort(() => draw());

    return {
      mount,
      unmount,
      refresh: draw,
      in: { layout: inLayout },
      out: { outline: outOutline, picked: outPicked },
      call: { focus: callFocus },
      get ports() { return { in: this.in, out: this.out, call: this.call }; },
    };
  },
};
