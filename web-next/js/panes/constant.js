/**
 * panes/constant.js — Constant pane.
 *
 * A data node: a JSON value you type, and the value the rest of the graph sees. It is the
 * smallest pane in the workspace and the one that explains the model — a tile can be a
 * source, not only a view.
 *
 * In:  {}   Out: `value` (value any)   Call: `apply`
 *
 * The text is the source of truth: on every apply the value is re-parsed, and a parse
 * failure is a visible `#ERR` in the body AND in the value the graph receives, so a wire
 * downstream is never fed a stale value that looked fine on screen.
 */

import { valuePort, callPort } from '../core/ports.js';

export default {
  kind: 'constant',
  title: 'Constant',
  ports: { in: {}, out: { value: 'value' }, call: { apply: 'call' } },
  create(ctx) {
    let el = null, input = null, out = null, summary = null;
    const portValue = valuePort(null);

    /** Parse what is in the box; a failure is a value, not an exception. */
    function parse() {
      try {
        return { ok: true, value: JSON.parse(input && input.value ? input.value : 'null') };
      } catch (e) {
        return { ok: false, value: '#ERR ' + e.message };
      }
    }

    function apply() {
      const r = parse();
      portValue.set(r.value);
      if (out) out.textContent = JSON.stringify(r.value, null, 2);
      if (summary) summary(r.value);
    }

    const callApply = callPort(() => { apply(); const r = parse(); return { ok: r.ok, value: r.value }; });

    return {
      mount(host) {
        el = document.createElement('div');
        el.className = 'bnode';
        input = document.createElement('textarea');
        input.className = 'bprog';
        input.spellcheck = false;
        input.value = '"hello qubitick"';
        const row = document.createElement('div');
        row.className = 'brow';
        const btn = document.createElement('button');
        btn.textContent = 'apply value';
        btn.className = 'brun';
        btn.addEventListener('click', apply);
        row.append(btn);
        out = document.createElement('pre');
        out.className = 'code bout';
        el.append(input, row, out);
        host.appendChild(el);
        apply();
      },
      unmount() {
        if (el && el.parentNode) el.parentNode.removeChild(el);
        el = input = out = null;
      },
      /** the page puts its title-bar text through this, so the pane stays out of the tile */
      setSummary(fn) { summary = fn; },
      out: { value: portValue },
      in: {},
      call: { apply: callApply },
      get ports() { return { in: this.in, out: this.out, call: this.call }; },
    };
  },
};