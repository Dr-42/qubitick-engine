/**
 * panes/layout-json.js — the layout document, as JSON.
 *
 * The other half of the design pair: `layout-outline` reads the document as a tree, this shows
 * the document itself, so a wiring can hand the same value to both (`defaults/wiring/_common.js`
 * does exactly that). Read-only on purpose — the document is edited by dragging tiles, and a
 * pane that rewrote it from a text box would be a second editor for the workspace with no undo
 * and no gesture to compare it against.
 *
 * In:  `layout` (value — the document)
 * Out: `json` (value — the pretty-printed text)
 * Call: `save` (nothing to save: it says so on the wire rather than pretending)
 *
 * The kind used to be declared with an empty `create` and implemented by a stub that returned
 * `{id, kind, destroy}` — no DOM, nothing drawn. This is what the pane's own ports said it
 * should be.
 */

import { valuePort, callPort } from '../core/ports.js';

/** The text the pane shows and publishes. */
export function layoutJson(layout) {
  if (!layout) return '';
  try {
    return JSON.stringify(layout, null, 2);
  } catch (e) {
    return '<<the layout cannot be printed: ' + (e && e.message) + '>>';
  }
}

export default {
  kind: 'layout-json',
  title: 'Layout JSON',
  ports: {
    in: { layout: 'value' },
    out: { json: 'value' },
    call: { save: 'call' },
  },
  create(ctx) {
    const inLayout = valuePort(null);
    const outJson = valuePort('');

    let host = null;
    let panel = null;
    let pre = null;
    let note = null;
    let current = null;

    function draw() {
      if (!panel) return;
      const text = layoutJson(current);
      if (pre) pre.textContent = text || 'no layout: this pane reads its `layout` in-port';
      outJson.set(text);
      if (note) {
        note.textContent = current
          ? (Object.keys(current.panes || {}).length + ' panes · ' + (current.roots || []).length + ' roots')
          : '';
      }
    }

    function mount(el) {
      host = el;
      panel = document.createElement('div');
      panel.className = 'layout-json';
      panel.style.cssText =
        'position:absolute; inset:0; display:flex; flex-direction:column;'
        + ' background:#161a24; color:#d6e2ee; font-size:11px;';

      note = document.createElement('div');
      note.className = 'lj-note';
      note.style.cssText = 'padding:3px 8px; border-bottom:1px solid #2a2f3e; color:#8ea2c0; flex-shrink:0;';

      pre = document.createElement('pre');
      pre.className = 'lj-body';
      pre.style.cssText =
        'flex:1; margin:0; padding:8px; overflow:auto; white-space:pre;'
        + ' font-family:ui-monospace, monospace; tab-size:2;';

      panel.append(note, pre);
      host.appendChild(panel);
      draw();
    }

    function unmount() {
      if (panel && panel.parentNode) panel.parentNode.removeChild(panel);
      panel = null; pre = null; note = null; host = null;
    }

    inLayout.subscribe((layout) => { current = layout || null; draw(); });

    return {
      mount,
      unmount,
      refresh: draw,
      in: { layout: inLayout },
      out: { json: outJson },
      call: { save: callPort(async () => { throw new Error('the layout pane does not write the document'); }) },
      get ports() { return { in: this.in, out: this.out, call: this.call }; },
    };
  },
};
