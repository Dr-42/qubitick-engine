/**
 * panes/stats.js — Stats pane.
 *
 * The corpus at a glance: one row per count with a bar, and one stacked bar for the
 * composition of the graph by node kind. It answers "is the workspace empty, or is the
 * corpus empty?" without opening a file, which is the question a blank-looking screen
 * actually asks.
 *
 * In:  {}   Out: {}   Call: `refresh`
 *
 * Everything it shows comes from its context: `data` (the corpus the app loaded),
 * `counts` (how many tiles and roots are in the workspace — the app's own state, not the
 * corpus's) and `data.atomOrder`, the corpus's own ordering of node kinds, so the stacked
 * bar reads in the order the corpus uses rather than in whatever order the JSON happened
 * to be in.
 */

import { callPort } from '../core/ports.js';

const DEFAULT_ORDER = ['axiom', 'definition', 'theorem', 'calibration'];

/** One metric row: label, bar, value. A value that does not exist is a dash, not a zero. */
function row(ui, key, value, max) {
  const has = value !== null && value !== undefined && value !== '';
  const n = Number(value);
  const pct = has && Number.isFinite(n) ? Math.max(2, Math.round(100 * n / max)) : 0;
  return '<span class="k">' + ui.esc(key) + '</span>'
    + '<span class="bar"><i style="width:' + pct + '%"></i></span>'
    + '<span class="v' + (has ? '' : ' z') + '">' + (has ? ui.esc(value) : '–') + '</span>';
}

/** How the corpus divides by node kind, in the corpus's own order. */
function distribution(ui, nodes, order) {
  const kinds = {};
  for (const n of nodes) {
    const k = String(n.kind || n.type || '?').toLowerCase();
    kinds[k] = (kinds[k] || 0) + 1;
  }
  const keys = [...order.filter((k) => kinds[k]), ...Object.keys(kinds).filter((k) => !order.includes(k))];
  const total = keys.reduce((a, k) => a + kinds[k], 0);
  if (!total) return null;
  const track = keys.map((k) => '<i data-k="' + ui.esc(k) + '" style="width:' + (100 * kinds[k] / total).toFixed(2)
    + '%" title="' + ui.esc(k) + ' ' + kinds[k] + '"></i>').join('');
  const legend = keys.map((k) => '<span data-k="' + ui.esc(k) + '">' + ui.esc(k) + ' <b>' + kinds[k] + '</b></span>').join('');
  return '<div class="sd-track">' + track + '</div><div class="sd-keys">' + legend + '</div>';
}

export default {
  kind: 'stats',
  title: 'Stats',
  ports: { in: {}, out: {}, call: { refresh: 'call' } },
  create(ctx) {
    const ui = (ctx.ui && ctx.ui.esc) ? ctx.ui : { esc: (s) => String(s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])) };

    let el = null;
    let host = null;

    function paint() {
      if (!el) return;
      const data = ctx.data || {};
      const counts = (ctx.counts && ctx.counts()) || { panes: null, roots: null };
      const graph = data.graph || {};
      const s = data.stats || {};
      const edges = graph.edges || [];
      const nodes = graph.nodes || [];
      const rows = [
        ['nodes', s.nodes != null ? s.nodes : nodes.length],
        ['dependencies', edges.length],
        ['symbols', s.symbols != null ? s.symbols : (data.symbols || []).length],
        ['goedel count', s.godel && s.godel.count != null ? s.godel.count : null],
        ['panels', counts.panes],
        ['roots', counts.roots],
      ];
      const nums = rows.map((r) => Number(r[1])).filter((n) => Number.isFinite(n) && n > 0);
      const max = nums.length ? Math.max(...nums) : 1;

      el.textContent = '';
      const grid = document.createElement('div');
      grid.className = 'stat-grid';
      grid.innerHTML = rows.map(([k, v]) => row(ui, k, v, max)).join('');
      el.appendChild(grid);

      const dist = distribution(ui, nodes, (data.atomOrder || DEFAULT_ORDER));
      if (dist) {
        const d = document.createElement('div');
        d.className = 'stat-dist';
        d.innerHTML = dist;
        el.appendChild(d);
      }
    }

    const callRefresh = callPort(() => { paint(); return { ok: true }; });

    return {
      mount(h) {
        host = h;
        el = document.createElement('div');
        host.appendChild(el);
        paint();
      },
      unmount() {
        if (el && el.parentNode) el.parentNode.removeChild(el);
        el = host = null;
      },
      in: {}, out: {},
      call: { refresh: callRefresh },
      get ports() { return { in: this.in, out: this.out, call: this.call }; },
    };
  },
};