/**
 * panes/ir.js — IR pane.
 *
 * The parse of the editor buffer, as text: what the engine made of the .qbit the editor
 * is showing. A computed pane — the body is whatever `POST /api/parse` answered last.
 *
 * In:  `buffer` (value text)   Out: `applied` (value)   Call: `apply`
 *
 * Two things a pane like this gets wrong, both handled here:
 *
 *   - ANSWERS OUT OF ORDER. Every keystroke schedules a parse; a slow answer for an old
 *     buffer arriving after a fast one for the new buffer would show the old parse under
 *     the new document. Each request carries a sequence number and only the newest lands.
 *   - AN ANSWER AFTER UNMOUNT. The tile can be closed while a parse is in flight; the
 *     reply then has no element to write to, which is a no-op and not an exception.
 */

import { valuePort, callPort } from '../core/ports.js';

export default {
  kind: 'ir',
  title: 'IR',
  ports: {
    in: { buffer: 'value', file: 'value' },
    out: { applied: 'value' },
    call: { apply: 'call' },
  },
  create(ctx) {
    const { url, ui } = ctx;
    const esc = (ui && ui.esc) || ((s) => String(s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])));

    let el = null;
    let bar = null;
    let out = null;
    let path = '';
    let buffer = '';
    let seq = 0;

    const portBuffer = valuePort('');
    const portFile = valuePort('');
    const portApplied = valuePort(null);
    const callApply = callPort(() => compute());

    portBuffer.subscribe((v) => { buffer = v || ''; compute(); });
    portFile.subscribe((v) => { path = v || ''; compute(); });

    function paint(text) {
      if (out) out.textContent = text;
    }

    function compute() {
      if (!out && !el) return { ok: false, error: 'not mounted' };
      const mine = ++seq;                       // only the newest answer may land
      if (!buffer) { paint(''); return { ok: true, empty: true }; }
      paint('parsing…');
      const endpoint = url && url('parse.post');
      if (!endpoint) { paint('parse: no route'); return { ok: false }; }
      fetch(endpoint, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: path || '', content: buffer }),
      })
        .then((r) => { if (!r.ok) throw new Error('HTTP ' + r.status); return r.json(); })
        .then((j) => {
          if (mine !== seq) return;             // a newer parse is already on its way
          paint(j.ast || j.ir || JSON.stringify(j, null, 2));
          portApplied.set({ path, ok: true });
        })
        .catch((e) => {
          if (mine !== seq) return;
          paint('parse error: ' + e.message);
          portApplied.set({ path, ok: false, error: e.message });
        });
      return { ok: true };
    }

    return {
      mount(host) {
        /* THE BAR IS THE PANE'S OWN ROW, NOT A CHILD OF THE HOST. `.ed-host` is a flex row
           that wraps one content element, so a toolbar mounted inside it became a
           full-height left column: the output was pushed right by the bar's width and the
           bar's own contents floated in the middle of the gutter. As a direct child of
           `.pane-body` the existing `.pane-body:has(> .ed-bar)` rule makes the body a
           column — bar on top, host scrolling underneath it. */
        bar = document.createElement('div');
        bar.className = 'ed-bar';
        const pad = document.createElement('select');
        pad.className = 'ed-files';
        pad.disabled = true;
        const o = document.createElement('option');
        o.textContent = '← editor.buffer · parse';
        pad.appendChild(o);
        bar.appendChild(pad);
        out = document.createElement('pre');
        out.className = 'code';
        el = document.createElement('div');
        el.className = 'ed-host';
        el.append(out);
        host.append(bar, el);
        compute();
      },
      unmount() {
        seq++;                                  // the in-flight answer is already stale
        if (bar && bar.parentNode) bar.parentNode.removeChild(bar);
        if (el && el.parentNode) el.parentNode.removeChild(el);
        bar = el = out = null;
      },
      in: { buffer: portBuffer, file: portFile },
      out: { applied: portApplied },
      call: { apply: callApply },
      get ports() { return { in: this.in, out: this.out, call: this.call }; },
    };
  },
};