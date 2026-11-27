/**
 * pane-ports.js — THE place a pane's declared ports are connected to a host's ports.
 *
 * Two hosts mount the same `js/panes/*.js` modules and each had its own loop doing this: the
 * canvas fills a pane's in-ports BY NAME from the app's own ports (`js/app/pane-host.js`), the
 * engine from the node ports its wires write (`js/panes/instances.js`). Written twice it drifted:
 * one copy understood a pane's in-port as a PORT, the other only as a HANDLER, so a pane written to
 * the contract received nothing in the engine — and every wiring preset in
 * `js/defaults/wiring/*.js` was inert for it. One loop cannot drift from itself.
 *
 * A pane declares three maps, and this is what each one is connected to:
 *
 *   in    a PORT the pane owns (the contract: `in: { buffer: valuePort() }`), or — the older
 *         convention — a HANDLER the host calls with each value.
 *   out   a PORT the pane publishes on, or a plain value it starts with.
 *   call  a FUNCTION the pane offers; the host binds it for whoever calls.
 *
 * The other side is whatever the host has: `{ in: {name: port}, out: {...}, call: {...} }`. A
 * source may be an engine value/event port (`subscribe`/`on`) or the canvas's app ports (`on` +
 * `get`, which do not replay), so both spellings are read and the current value is delivered once
 * when the source only has `on`.
 */

/**
 * Write one value into a pane's port or handler.
 * @param {object|Function} sink - a port, or the older handler
 * @param {*} value
 */
export function writeInto(sink, value) {
  if (typeof sink === 'function') { sink(value); return; }
  if (!sink) return;
  if (sink.kind === 'value' || typeof sink.set === 'function') sink.set(value);
  else if (typeof sink.emit === 'function') sink.emit(value);
}

/**
 * Connect a pane instance to a host's ports.
 *
 * @param {object} inst - the pane instance: `{ in, out, call }` (or `{ ports: { in, ... } }`)
 * @param {object} sources - the host side: `{ in, out, call }` maps of name → port
 * @param {object} [opts]
 * @param {(name: string, value: *, inst: object) => *} [opts.map] - compose a value per port name
 *   before delivery (the canvas gives `cursor` its file's path this way)
 * @param {() => void} [opts.onBind] - called with the unbind of each connection as it is made
 * @returns {() => void} unbind everything
 */
export function bindPanePorts(inst, sources, { map, onBind } = {}) {
  const offs = [];
  const keep = (off) => {
    const un = typeof off === 'function' ? off : () => {};
    offs.push(un);
    if (onBind) onBind(un);
    return un;
  };
  const portsOf = (dir) => (inst && inst[dir]) || (inst && inst.ports && inst.ports[dir]) || {};
  const mine = {
    in: portsOf('in'), out: portsOf('out'), call: portsOf('call'),
  };
  const theirs = {
    in: (sources && sources.in) || {}, out: (sources && sources.out) || {}, call: (sources && sources.call) || {},
  };

  /* ---- in: the host writes into the pane ---- */
  for (const [name, sink] of Object.entries(mine.in)) {
    const src = theirs.in[name];
    if (!src || !sink) continue;
    const deliver = (v) => {
      const value = map ? map(name, v, inst) : v;
      if (value === undefined) return;                // a mapper that has nothing to say
      writeInto(sink, value);
    };
    if (typeof src.subscribe === 'function') {
      keep(src.subscribe(deliver));                   // a value port replays on subscribe
    } else if (typeof src.on === 'function') {
      keep(src.on(deliver));
      // the canvas's app ports do not replay: hand over what they hold now
      if (typeof src.get === 'function') deliver(src.get());
    }
  }

  /* ---- out: the pane publishes, the host's port follows ---- */
  for (const [name, src] of Object.entries(mine.out)) {
    const port = theirs.out[name];
    if (!port) continue;
    if (src && typeof src.subscribe === 'function') {
      keep(src.subscribe((v) => { if (port.set) port.set(v); }));
      if (typeof src.hasValue === 'function' && src.hasValue() && port.set) port.set(src.get());
    } else if (src && typeof src.on === 'function') {
      keep(src.on((v) => { if (port.emit) port.emit(v); }));
    } else if (port.set) {
      port.set(src);                                  // a plain starting value
    }
  }

  /* ---- call: the pane offers a function, the host's call port invokes it ----
   *
   * TWO CONVENTIONS, and only one of them was handled. A pane may hand back a HANDLER
   * (`call: { save: doSave }`) or a PORT it built itself (`call: { save: callPort(doSave) }` —
   * js/panes/editor.js, diag.js, build.js all do this). The port is an object, so
   * `typeof fn === 'function'` was false and NOTHING BOUND: every call surface in the engine
   * host — the editor's save/goto, the Diagnostics panel's rerun, the Build panel's rerun, the
   * Goal panel's citation insert — invoked a port whose impl was never set, and the failure
   * read `call port not bound` at the moment a person pressed the button. */
  for (const [name, fn] of Object.entries(mine.call)) {
    const port = theirs.call[name];
    if (!port || typeof port.bind !== 'function') continue;
    if (typeof fn === 'function') {
      port.bind(fn);
    } else if (fn && typeof fn.invoke === 'function') {
      port.bind((args) => fn.invoke(args));
    }
  }

  return () => { for (const off of offs) { try { off(); } catch (e) {} } };
}