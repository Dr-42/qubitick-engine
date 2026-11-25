/**
 * panes/output.js — Output pane.
 *
 * What the page loaded, in four lines: the corpus files, the graph, the symbols and the
 * live wires. It is the pane you read first when something looks empty, so it says the
 * count of each rather than "ready".
 *
 * Mounted by BOTH hosts through the same contract (`create(ctx)`), which is the point:
 *   canvas app   ctx.data from the loaded corpus, ctx.wires from the wire layer
 *   module shell ctx.data absent — the pane then reports what it has rather than lying
 *
 * In:  `log` (event, appended as a line)   Out: {}   Call: `clear()`
 */

import { eventPort, callPort } from '../core/ports.js';

export default {
  kind: 'output',
  title: 'Output',
  ports: {
    in: { log: 'event', clear: 'event' },
    out: {},
    call: { clear: 'call' },
  },
  create(ctx) {
    const lines = [];
    let el = null;
    let logEl = null;

    const esc = (ctx.ui && ctx.ui.esc) || ((s) => String(s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])));

    /** The four facts, as rows. Missing data is a dash, never a zero that looks real. */
    function facts() {
      const data = (ctx.data || {});
      const graph = data.graph || {};
      const n = (v) => (v === undefined || v === null ? '–' : v);
      return [
        ['files', n((data.files || []).length)],
        ['graph', n((graph.nodes || []).length) + ' nodes'],
        ['symbols', n((data.symbols || []).length)],
        ['wires live', n((ctx.wires || []).length)],
      ];
    }

    function paint() {
      if (!logEl) return;
      const rows = facts().map(([k, v]) => `<span class="ok">✓ ${esc(k)}</span> ${esc(v)}`);
      const extra = lines.slice(-20).map((l) => `<span class="ok">${esc(l)}</span>`);
      logEl.innerHTML = rows.concat(extra).join('\n');
    }

    const portLog = eventPort();
    portLog.on((payload) => {
      const text = (payload && payload.text) || String(payload == null ? '' : payload);
      if (!text) return;
      lines.push(text);
      if (lines.length > 200) lines.shift();
      paint();
    });
    const callClear = callPort(() => { lines.length = 0; paint(); return { ok: true }; });
    /* the same clear, as an event: the registry wires an in-port, a program node invokes
       the call — the pane predates this refactor and both surfaces are in use */
    const portClear = eventPort();
    portClear.on(() => { lines.length = 0; paint(); });

    return {
      mount(host) {
        el = document.createElement('div');
        el.className = 'log';
        logEl = el;
        host.appendChild(el);
        paint();
      },
      unmount() {
        if (el && el.parentNode) el.parentNode.removeChild(el);
        el = logEl = null;
      },
      // the registry wires these; the canvas host reads the summary instead
      in: { log: portLog, clear: portClear },
      out: {},
      call: { clear: callClear },
      get ports() { return { in: this.in, out: this.out, call: this.call }; },
    };
  },
};