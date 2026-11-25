/**
 * panes/lean.js — Lean pane.
 *
 * The Lean 4 the engine generates for the file the editor is showing. A computed pane:
 * the body is whatever `GET /api/lean?path=…` answered last, and the file is the tile's
 * own when the tile is pinned.
 *
 * In:  `file` (value path), `buffer` (value text)   Out: `generated` (value)
 * Call: `generate`, `copy`
 *
 * The pane is read-only on purpose: this is what the engine will verify, so being able to
 * type into it would be a lie about what is checked. Selecting the text is enough to copy
 * it by hand, and `copy` does it in one call for a button or a wired program node.
 */

import { valuePort, callPort } from '../core/ports.js';

export default {
  kind: 'lean',
  title: 'Lean',
  ports: {
    in: { file: 'value', buffer: 'value' },
    out: { generated: 'value' },
    call: { generate: 'call', copy: 'call' },
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
    let text = '';

    const portFile = valuePort('');
    const portBuffer = valuePort('');
    const portGenerated = valuePort(null);
    const callGenerate = callPort(() => compute());
    const callCopy = callPort(async () => {
      if (!text) return { ok: false, error: 'nothing generated yet' };
      try {
        if (navigator.clipboard && navigator.clipboard.writeText) {
          await navigator.clipboard.writeText(text);
          return { ok: true, chars: text.length };
        }
        return { ok: false, error: 'no clipboard' };
      } catch (e) {
        return { ok: false, error: e.message };
      }
    });

    portFile.subscribe((v) => { path = v || ''; compute(); });
    portBuffer.subscribe((v) => { buffer = v || ''; });

    function paint(value) {
      text = value == null ? '' : String(value);
      if (out) out.textContent = text;
    }

    function compute() {
      const mine = ++seq;
      if (!path) { paint('open a file to generate Lean'); portGenerated.set({ path:'', ok:false }); return { ok: false, error: 'no file' }; }
      paint('generating…');
      const route = url && url('lean.get');
      if (!route) { paint('lean: no route'); return { ok: false }; }
      /* The generated Lean can also depend on unsaved edits, so the buffer goes along when
         the endpoint accepts a body; the query path is what the route documents. */
      fetch(route + '?path=' + encodeURIComponent(path))
        .then((r) => r.json())
        .then((j) => {
          if (mine !== seq) return;             // a newer request is already in flight
          paint((j && (j.lean || j.content || j.error)) || '');
          portGenerated.set({ path, ok: !(j && j.error), error: (j && j.error) || null });
        })
        .catch((e) => {
          if (mine !== seq) return;
          paint('lean error: ' + e.message);
          portGenerated.set({ path, ok: false, error: e.message });
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
        o.textContent = '← editor.file · generated Lean';
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
        seq++;
        if (bar && bar.parentNode) bar.parentNode.removeChild(bar);
        if (el && el.parentNode) el.parentNode.removeChild(el);
        bar = el = out = null;
      },
      in: { file: portFile, buffer: portBuffer },
      out: { generated: portGenerated },
      call: { generate: callGenerate, copy: callCopy },
      get ports() { return { in: this.in, out: this.out, call: this.call }; },
    };
  },
};