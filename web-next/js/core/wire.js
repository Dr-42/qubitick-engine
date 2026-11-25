export function createWirer({ bus, maxDepth = 32 } = {}) {
  let depth = 0;

  function wire(src, dst, opts = {}, scope) {
    if (src.kind === 'call') {
      throw new TypeError('call ports cannot be wired');
    }
    const dsts = Array.isArray(dst) ? dst : [dst];

    const { map, filter, dedupe = true } = opts;
    let lastSent;

    function forward(v) {
      if (filter && !filter(v)) return;
      const o = map ? map(v) : v;
      if (dedupe && src.kind === 'value' && lastSent !== undefined && Object.is(lastSent, o)) return;
      lastSent = o;

      if (depth >= maxDepth) {
        console.error('wire loop cut');
        unsub();
        if (bus) {
          bus.emit('ws:wire-cut', {
            src: src.name,
            dst: dsts.map(d => d.name),
          });
        }
        return;
      }

      depth++;
      try {
        for (const d of dsts) {
          // A call-port destination is invoked with the mapped payload (i.e.
          // `out.x -> pane.call.y` means "call y(o)"). Call-port *sources* are
          // still rejected above.
          if (d && d.kind === 'call') {
            /* A call port with nothing bound is a WIRING GAP, not a crash: the pane that
               provides it is not mounted (or was closed), so there is no implementation to
               run. The invoke rejects by design (core/ports.js), and a rejection nobody
               awaits becomes an unhandled rejection — a page error for a wire whose other
               end is simply absent. Report it once per wire and keep going: the rest of the
               graph still has work to do, and a workspace that explodes on a dangling wire
               cannot be edited back into shape. */
            if (typeof d.invoke === 'function') {
              Promise.resolve(d.invoke(o)).catch((err) => {
                const detail = { src: src.name || null, to: d.name || d.kind, error: String((err && err.message) || err) };
                if (bus) bus.emit('ws:wire-unbound', detail);
                else console.warn('[wire] call port not bound', detail);
              });
            }
            continue;
          }
          if (d && d.kind === 'value') {
            d.set(o);
          } else if (d) {
            d.emit(o);
          }
        }
      } finally {
        depth--;
      }
    }

    let unsub = () => {};
    if (src.kind === 'value') {
      unsub = src.subscribe(forward);
    } else {
      unsub = src.on(forward);
    }

    if (scope && typeof scope.add === 'function') {
      scope.add(unsub);
    }

    return unsub;
  }

  return wire;
}