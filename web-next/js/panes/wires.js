/**
 * panes/wires.js — Wires pane.
 *
 * The wire layer's own switch. Wires are drawn over every tile, so the control that hides
 * them is a tile: a boolean, the number of live wires, and a line that says where the
 * boolean is coming from.
 *
 * In:  `show` (value Bool)   Out: `visible` (value Bool)   Call: `toggle`
 *
 * The tile can be driven two ways and says which is in force: a wire into `show` (then the
 * checkbox is a readout, not a control) or the checkbox itself. Drawn by the page, because
 * the wire layer is the page's — `ctx.setWireMode` / `ctx.wireOn()` / `ctx.wireCount()`.
 */

import { valuePort, callPort } from '../core/ports.js';

export default {
  kind: 'wires',
  title: 'Wires',
  ports: {
    in: { show: 'value' },
    out: { visible: 'value' },
    call: { toggle: 'call' },
  },
  create(ctx) {
    let el = null, box = null, count = null, note = null;
    let wired = false;

    const inShow = valuePort(undefined);
    const outVisible = valuePort(false);

    const on = () => !!(ctx.wireOn && ctx.wireOn());
    const wires = () => (ctx.wireCount ? ctx.wireCount() : 0);

    function paint() {
      if (!box) return;
      box.checked = on();
      box.disabled = wired;
      count.textContent = wires() + ' wires';
      note.textContent = wired
        ? 'driven by the wire on this tile’s show port'
        : 'click, or wire a boolean into this tile’s show port';
      outVisible.set(on());
    }

    inShow.subscribe((v) => {
      // undefined means nothing is wired; a boolean means the wire owns the switch
      wired = v !== undefined;
      if (wired) {
        if (ctx.setWireMode) ctx.setWireMode(!!v);
        paint();
      } else {
        paint();
      }
    });

    const callToggle = callPort(() => {
      if (wired) return { ok: false, error: 'wired' };
      if (ctx.setWireMode) ctx.setWireMode(!on());
      paint();
      return { ok: true, visible: on() };
    });

    return {
      mount(host) {
        el = document.createElement('div');
        el.className = 'ctl-sec';
        const row = document.createElement('label');
        row.className = 'ctl-row';
        box = document.createElement('input');
        box.type = 'checkbox';
        const lab = document.createElement('span');
        lab.textContent = 'show wires';
        row.append(box, lab);
        count = document.createElement('span');
        count.className = 'pill';
        const meta = document.createElement('div');
        meta.className = 'ctl-meta';
        note = document.createElement('div');
        note.className = 'ctl-note';
        meta.append(count, note);
        el.append(row, meta);
        box.addEventListener('change', () => {
          if (wired) { paint(); return; }
          if (ctx.setWireMode) ctx.setWireMode(box.checked);
          paint();
        });
        host.appendChild(el);
        paint();
      },
      unmount() {
        if (el && el.parentNode) el.parentNode.removeChild(el);
        el = box = count = note = null;
      },
      // the page repaints its chrome often; this is how it asks a mounted pane to follow
      refresh: paint,
      in: { show: inShow },
      out: { visible: outVisible },
      call: { toggle: callToggle },
      get ports() { return { in: this.in, out: this.out, call: this.call }; },
    };
  },
};