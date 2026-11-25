/**
 * panes/switch.js — Switch pane: the boolean a wire can carry.
 *
 * A data node whose value is one bit. It publishes on its own out-port, so two switches in
 * one workspace are two independent values — which is the whole reason this is a tile and
 * not a global preference.
 *
 * In:  `on` (value Bool, so a wire can drive it)   Out: `on` (value Bool)   Call: `toggle`
 *
 * The tile also registers a named port source (`<id>_on`) with the app, so a behaviour
 * program that declares `from: '<id>_on'` can read it without a wire. That registration is
 * the app's — this pane asks through ctx — and it is dropped again on unmount, or a closed
 * switch would keep answering for a tile that is gone.
 */

import { valuePort, callPort } from '../core/ports.js';

export default {
  kind: 'switch',
  title: 'Switch',
  ports: { in: { on: 'value' }, out: { on: 'value' }, call: { toggle: 'call' } },
  create(ctx) {
    const ui = ctx.ui || {};
    let el = null, box = null, read = null, summary = null;
    let off = null;

    const outOn = valuePort(false);
    const inOn = valuePort(undefined);

    const paint = () => {
      const v = !!outOn.get();
      if (box) box.checked = v;
      if (read) read.textContent = v ? 'true' : 'false';
      if (summary) summary(v);
    };

    /** Set the value and tell everyone: the out-port, the named source, the tile. */
    function set(v, { quiet = false } = {}) {
      outOn.set(!!v);
      paint();
      if (!quiet && ui.toast) ui.toast('switch → ' + !!v);
    }

    inOn.subscribe((v) => {
      // undefined means nothing is wired; a boolean means the wire drives the switch
      if (v === undefined) return;
      set(v, { quiet: true });
    });

    const callToggle = callPort(() => { set(!outOn.get()); return { ok: true, on: outOn.get() }; });

    return {
      mount(host, p) {
        el = document.createElement('div');
        el.className = 'ctl-sec';
        const row = document.createElement('label');
        row.className = 'ctl-row';
        box = document.createElement('input');
        box.type = 'checkbox';
        const lab = document.createElement('span');
        lab.textContent = 'on';
        row.append(box, lab);
        const meta = document.createElement('div');
        meta.className = 'ctl-meta';
        read = document.createElement('span');
        read.className = 'pill';
        const note = document.createElement('div');
        note.className = 'ctl-note';
        note.textContent = '→ on  ·  Bool';
        meta.append(read, note);
        el.append(row, meta);
        box.addEventListener('change', () => set(box.checked));
        host.appendChild(el);
        // the app owns the named-source table; the pane names the port and gets a remover
        if (ctx.registerSource && p) off = ctx.registerSource(p.id + '_on', outOn);
        if (p) summary = (v) => { p.summary = () => 'bool · ' + v; if (p._summary && p.card && p.card.lastChild) p.card.lastChild.textContent = p.summary(); };
        paint();
        if (summary) summary(outOn.get());
      },
      unmount() {
        if (typeof off === 'function') { try { off(); } catch (e) {} off = null; }
        if (el && el.parentNode) el.parentNode.removeChild(el);
        el = box = read = summary = null;
      },
      refresh: paint,
      in: { on: inOn },
      out: { on: outOn },
      call: { toggle: callToggle },
      get ports() { return { in: this.in, out: this.out, call: this.call }; },
    };
  },
};