/**
 * panes/symbols.js — Symbols pane.
 *
 * Which maths a .qbit file actually uses: one chip per glyph with its count, and a
 * density strip showing where in the file the maths sits. Selecting a node in the tree
 * retargets it; with nothing selected it falls back to the corpus's symbol list.
 *
 * In:  `treeFile` (value path), `selection` (value), `file` (value path), `buffer` (value)
 * Out: `picked` (value {glyph})   Call: `refresh`
 *
 * The glyphs are derived from the text and nothing else — there is no privileged symbol
 * table, so a macro the engine has never heard of still shows up with its count. That is
 * the point of the pane: it says what is in the file, not what the file was supposed to
 * contain.
 */

import { valuePort, callPort } from '../core/ports.js';

/** Every `\macro` in the text, most used first. */
export function glyphsFromText(text) {
  const m = new Map(), re = /\\[a-zA-Z]+/g;
  let x;
  while ((x = re.exec(text))) {
    const g = x[0];
    m.set(g, (m.get(g) || 0) + 1);
  }
  return [...m.entries()].map(([g, n]) => ({ g, n })).sort((a, b) => b.n - a.n || a.g.localeCompare(b.g));
}

export default {
  kind: 'symbols',
  title: 'Symbols',
  ports: {
    in: { treeFile: 'value', selection: 'value', file: 'value', buffer: 'value' },
    out: { picked: 'value' },
    call: { refresh: 'call' },
  },
  create(ctx) {
    const esc = (ctx.ui && ctx.ui.esc) || ((s) => String(s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])));

    let el = null, head = null, chips = null, hist = null;
    let treePath = '', activePath = '', buffer = '';
    let selection = null;

    const portPicked = valuePort(null);
    const inTreeFile = valuePort('');
    const inSelection = valuePort(null);
    const inFile = valuePort('');
    const inBuffer = valuePort('');

    inTreeFile.subscribe((v) => { treePath = v || ''; render(); });
    inSelection.subscribe((v) => { selection = v; render(); });
    inFile.subscribe((v) => { activePath = v || ''; render(); });
    inBuffer.subscribe((v) => { buffer = v || ''; render(); });

    function show(glyphs, label, text) {
      if (!head) return;
      const total = glyphs.reduce((a, g) => a + (g.n || 1), 0);
      const mx = glyphs.length ? (glyphs[0].n || 1) : 1;
      head.innerHTML = '<b>' + esc(label) + '</b> <i>' + glyphs.length + ' glyphs · ' + total + ' uses</i>';
      chips.innerHTML = glyphs.map((g) => '<span class="s" data-g="' + esc(g.g) + '" title="' + esc(g.g) + ' × ' + g.n + '">'
        + '<b>' + esc(g.g) + '</b><i>' + g.n + '</i>'
        + '<u style="width:' + Math.round(100 * (g.n || 1) / mx) + '%"></u></span>').join('');
      if (text) {
        const lines = text.split('\n');
        const counts = lines.map((l) => (l.match(/\\[a-zA-Z]+/g) || []).length);
        const m2 = Math.max(1, ...counts);
        hist.style.display = '';
        hist.innerHTML = '<div class="sh-head"><span>glyph density</span><i>' + lines.length + ' lines · max ' + m2 + '/line</i></div>'
          + '<div class="sh-track">' + counts.map((c, i) => '<i style="height:' + (6 + 94 * c / m2).toFixed(1) + '%" data-z="' + (c ? 1 : 0)
            + '" title="L' + (i + 1) + ' · ' + c + ' glyphs"></i>').join('') + '</div>';
      } else {
        hist.style.display = 'none';
        hist.innerHTML = '';
      }
    }

    function render() {
      if (!el) return;
      /* The tree's selection wins: a node picked in the tree is what the reader is looking
         at, and the editor's active file is only the fallback. */
      const path = treePath || activePath || '';
      const text = (path && ctx.fileText ? ctx.fileText(path) : '') || buffer || '';
      const data = ctx.data || {};
      if (text && text.indexOf('\\') >= 0) {
        show(glyphsFromText(text), String(path).split(/[.\/]/).pop() || 'file', text);
      } else if ((data.symbols || []).length) {
        // no file text in hand: the corpus's own symbol table is still worth showing
        const glyphs = (data.symbols || []).slice(0, 300)
          .map((s) => ({ g: String(s.symbol || s.name || ''), n: 1 })).filter((s) => s.g);
        show(glyphs, 'corpus', null);
      } else {
        show([], 'no selection', null);
      }
    }

    const callRefresh = callPort(() => { render(); return { ok: true }; });

    return {
      mount(host) {
        el = document.createElement('div');
        el.className = 'symbols';
        head = document.createElement('div');
        head.className = 'sym-head';
        chips = document.createElement('div');
        chips.className = 'sym-chips';
        hist = document.createElement('div');
        hist.className = 'sym-hist';
        el.append(head, chips, hist);
        // a glyph is a thing you can pick; the port says what was picked
        chips.addEventListener('click', (ev) => {
          const s = ev.target.closest && ev.target.closest('.s');
          if (!s) return;
          portPicked.set({ glyph: s.dataset.g });
        });
        host.appendChild(el);
        render();
      },
      unmount() {
        if (el && el.parentNode) el.parentNode.removeChild(el);
        el = head = chips = hist = null;
      },
      in: { treeFile: inTreeFile, selection: inSelection, file: inFile, buffer: inBuffer },
      out: { picked: portPicked },
      call: { refresh: callRefresh },
      get ports() { return { in: this.in, out: this.out, call: this.call }; },
    };
  },
};