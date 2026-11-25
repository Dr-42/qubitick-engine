/**
 * panes/render.js — Render pane.
 *
 * The .qbit document as a reader sees it: headings, labelled rows, displayed maths, prose.
 * This is the pane that shows whether the source is legible, so it is deliberately the
 * place where the line classification is decided — `QBIT.classifyLines` reads each line by
 * its ROLE (a label, a displayed formula, a comment, prose), not by "does it look like
 * maths", which is what makes a line of prose with a symbol in it stay prose.
 *
 * In:  `buffer` (value text), `file` (value path), `syntax` (value Bool)
 * Out: `rendered` (value {path, lines})   Call: `redraw`
 *
 * The maths switch has three states and says which one it is in: on (KaTeX), off (plain
 * source), and wired — when a boolean arrives on the `syntax` port the switch is a readout
 * and refuses the click rather than silently fighting the wire.
 *
 * KaTeX is loaded by js/app/math.js; if it never arrives the pane still renders, with the
 * source of each formula instead of the type — a renderer that shows nothing when the
 * fonts fail is worse than one that shows the source.
 */

import { valuePort, callPort } from '../core/ports.js';
import { classifyLines } from '../app/qbit-lines.js';
import { loadKatex, mathHtml, proseHtml, citeHtml } from '../app/math.js';

export default {
  kind: 'render',
  title: 'Render',
  ports: {
    in: { buffer: 'value', file: 'value', syntax: 'value' },
    out: { rendered: 'value' },
    call: { redraw: 'call', print: 'call', pdf: 'call' },
  },
  create(ctx) {
    const esc = (ctx.ui && ctx.ui.esc) || ((s) => String(s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])));

    let el = null, view = null, tog = null, bar = null;
    let text = '', path = '';
    let syntax = true;
    let wired = false;

    const inBuffer = valuePort('');
    const inFile = valuePort('');
    const inSyntax = valuePort(undefined);
    const outRendered = valuePort(null);

    inBuffer.subscribe((v) => { text = v || ''; render(); });
    inFile.subscribe((v) => { path = v || ''; render(); });
    inSyntax.subscribe((v) => {
      wired = v !== undefined;
      if (wired) syntax = !!v;
      paintTog();
      render();
    });

    function paintTog() {
      if (!tog) return;
      tog.textContent = syntax ? '∑' : 'A';
      tog.classList.toggle('on', !!syntax);
      tog.classList.toggle('wired', wired);
      tog.disabled = wired;
      tog.title = wired ? 'maths: driven by the wire on this tile’s syntax port'
        : 'maths rendering: ' + (syntax ? 'on (KaTeX)' : 'off (plain source)');
    }

    /** One classified line, as the element its role asks for. */
    function lineEl(r) {
      if (r.kind === 'blank') return document.createElement('div');
      const d = document.createElement('div');
      if (r.kind === 'comment') { d.className = 'lcomment'; d.textContent = r.line; return d; }
      if (r.kind === 'head') {
        d.className = 'lhead';
        d.innerHTML = '<span class="lab">' + esc(r.label) + '</span><span class="name">' + esc(r.value) + '</span>';
        return d;
      }
      if (r.kind === 'label') {
        // a label whose body runs on below it (`Sema:`, `Proof:`) is a row with no value
        d.className = 'lrow';
        d.innerHTML = '<span class="lchip">' + esc(r.label) + '</span>'
          + (r.value ? ('<span class="lval">' + (r.math ? mathHtml(r.value, false) : esc(r.value)) + '</span>') : '');
        return d;
      }
      if (r.kind === 'math') {
        d.className = 'lmath';
        /* THE LABEL IS TYPESET WHEN IT IS TeX: `[\ulcornerA1.1 b b a\urcorner]` is a citation
           whose corner quotes are notation, and escaping it printed the backslashes (js/app/math.js
           ::citeHtml says which labels are which). */
        d.innerHTML = mathHtml(r.body, true) + (r.cite ? ('<span class="lcite">' + citeHtml(r.cite) + '</span>') : '');
        return d;
      }
      d.className = 'lprose';
      d.innerHTML = (r.label ? ('<span class="lchip">' + esc(r.label) + '</span>') : '') + proseHtml(r.text || '');
      return d;
    }

    function render() {
      if (!view) return;
      if (!text) {
        view.innerHTML = '<div class="empty">edit a file in the editor to render it</div>';
        outRendered.set({ path, ok: false, empty: true });
        return;
      }
      const head = String(path || '').split('/').pop();
      if (!syntax) {
        view.innerHTML = '<div class="rhead">' + esc(head) + ' · source</div>'
          + '<pre class="code">' + esc(text) + '</pre>';
        outRendered.set({ path, ok: true, syntax: false, lines: text.split('\n').length });
        return;
      }
      view.innerHTML = '<div class="rhead">' + esc(head) + '</div>';
      for (const r of classifyLines(text)) view.appendChild(lineEl(r));
      outRendered.set({ path, ok: true, syntax: true, lines: text.split('\n').length });
    }

    const callRedraw = callPort(() => { render(); return { ok: true }; });

    /* Printing what is on screen is the browser's job, and a PDF is the engine's: the
       route answers with a file or with an error, and the call reports which. */
    const callPrint = callPort(() => {
      try {
        if (typeof window === 'undefined' || typeof window.print !== 'function') return { ok: false, error: 'no printer' };
        window.print();
        return { ok: true };
      } catch (e) { return { ok: false, error: e.message }; }
    });
    const callPdf = callPort(async () => {
      if (!path) return { ok: false, error: 'no file' };
      const route = ctx.url && ctx.url('pdf.get');
      if (!route) return { ok: false, error: 'no route' };
      try {
        const r = await fetch(route + '?path=' + encodeURIComponent(path));
        if (!r.ok) return { ok: false, error: 'HTTP ' + r.status };
        return { ok: true, path, body: await r.text() };
      } catch (e) { return { ok: false, error: e.message }; }
    });

    return {
      mount(host) {
        /* THE BAR IS THE PANE'S OWN ROW, NOT A CHILD OF THE HOST. `.ed-host` is a flex row
           that wraps one content element, so a toolbar mounted inside it became a
           full-height left column: the document was pushed right by the bar's width and the
           maths switch floated in the middle of the gutter. As a direct child of
           `.pane-body` the existing `.pane-body:has(> .ed-bar)` rule makes the body a
           column — bar on top, host scrolling underneath it. */
        bar = document.createElement('div');
        bar.className = 'ed-bar';
        const want = document.createElement('span');
        want.className = 'ed-kind';
        want.textContent = '∑ katex';
        tog = document.createElement('button');
        tog.className = 'ed-tog';
        tog.type = 'button';
        tog.addEventListener('click', (e) => {
          e.stopPropagation();
          if (wired && ctx.ui && ctx.ui.toast) { ctx.ui.toast('this tile’s syntax port is wired'); return; }
          syntax = !syntax;
          paintTog();
          render();
        });
        bar.append(want, tog);
        view = document.createElement('div');
        view.className = 'render';
        el = document.createElement('div');
        el.className = 'ed-host';
        el.append(view);
        host.append(bar, el);
        paintTog();
        // KaTeX arrives when it arrives; the pane draws the source until then
        loadKatex().then(() => render(), () => render());
        render();
      },
      unmount() {
        if (bar && bar.parentNode) bar.parentNode.removeChild(bar);
        if (el && el.parentNode) el.parentNode.removeChild(el);
        bar = el = view = tog = null;
      },
      in: { buffer: inBuffer, file: inFile, syntax: inSyntax },
      out: { rendered: outRendered },
      call: { redraw: callRedraw, print: callPrint, pdf: callPdf },
      get ports() { return { in: this.in, out: this.out, call: this.call }; },
    };
  },
};