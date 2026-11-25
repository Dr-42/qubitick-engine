/*
 * live/wire/host.js — arming the wire layer: registration, delivery, and the schedule.
 *
 * Extracted from the inline script in web-next/index.html. The four wire modules next to
 * this one each do one thing (kinds registers what a port IS, connect owns the drag and the
 * dots, layer draws, schedule orders the ticks). This is the host: it registers the app's
 * own pane kinds, hands every pane the values that arrived on its wires, and decides when
 * the wired scheduler takes over from the plain cascade.
 *
 * Three rules, each of them a bug:
 *
 *   THE LAYER IS LATE. Its modules are dynamically imported, so a workspace can be restored
 *   — wires and all — before there is anything to draw them. The restored wires are held and
 *   replayed at arm time rather than dropped, and until then the page's wire count says 0
 *   (which is honest: there is no layer) instead of the number of declarations.
 *
 *   DELIVERY IS BY CHANGE. Every pane that declares inputs is handed the values arriving on
 *   its wires, and only the ones that CHANGED — a pane is allowed to treat an input as an
 *   event, and re-delivering an unchanged value on the heartbeat would make it run forever.
 *   A wire that is removed puts its port back to "unset" for the same reason.
 *
 *   A TICKING PANE RESOLVES THE TICK ORDER, AND ONLY A TICKING PANE DOES. With no wired kind
 *   that declares `tick`, no scheduler is armed at all — a corpus-only layout pays nothing —
 *   and a loop through kinds that are not `stateful` is cut and reported instead of spinning.
 */

/** The value a pane's output port holds right now, for sources that do not tick. */
const defaultValue = (ctx) => (paneId, port) => {
  const inst = ctx.panes.find((x) => x.id === paneId);
  if (!inst) return undefined;
  // a tile that publishes its own value wins over the kind-level source: two switches, or
  // two editors, do not have to agree about what they are saying
  const own = ctx.sources[inst.id + '_' + port];
  if (own) return own.get();
  const key = inst.kind + '_' + port;
  if (ctx.sources[key]) return ctx.sources[key].get();
  if (port === 'selection' && (inst.kind === 'tree' || inst.kind === 'graph')) return ctx.selection();
  if (port === 'file') return ctx.treeFile() || ctx.editorFile();
  const res = ctx.results ? ctx.results[inst.kind] : null;
  return res ? res[port] : undefined;
};

/**
 * @param {object} host
 * @param {Array} host.panes                 the live tiles
 * @param {object} host.registry             kind → pane record (ports come from here)
 * @param {Array} host.wires                 the DECLARED wires (by kind)
 * @param {Array} host.dyn                   the wires drawn by hand (by instance) — mutated in place
 * @param {() => Array} host.pendingWires    wires restored before the layer armed, or null
 * @param {() => void} host.pendingDone      clear them
 * @param {() => Array} host.resolveWires    expand the declarations into live pairs
 * @param {() => Array} host.instancesOf     the tile ids of one kind
 * @param {object} host.graph                js/app/graph-run.js
 * @param {object} host.elements             { viewport, world, canvas }
 * @param {() => object} host.cam
 * @param {object} host.sources              PORT_SRC: named port sources and pane ports
 * @param {() => any} [host.selection]       the corpus selection, for a `selection` port
 * @param {() => any} [host.treeFile]
 * @param {() => any} [host.editorFile]
 * @param {() => void} host.updateCount      the HUD's wire count
 * @param {() => void} [host.save]
 * @param {(m:string) => void} [host.toast]
 * @param {() => Promise<object[]>} host.loadModules   the four wire modules, in order
 * @param {() => number} [host.now]
 * @param {object} [host.document]
 */
export function createWireHost(host) {
  const { panes, registry, wires, dyn, graph, elements, cam } = host;
  const toast = host.toast || (() => {});
  const now = host.now || (() => (typeof performance !== 'undefined' ? performance.now() : Date.now()));
  const doc = host.document || (typeof document !== 'undefined' ? document : null);
  const valueOfPort = host.portValue || defaultValue({
    panes, sources: host.sources || {}, results: graph && graph.results,
    selection: host.selection, treeFile: host.treeFile, editorFile: host.editorFile,
  });

  let layer = null, ports = null;         // the drawing layer, and the dots/drag
  let kinds = null;                        // the kind registry
  let sched = null;                        // the scheduler factory, once its module is in
  let t0 = null;                           // the drawer's clock starts at arm time
  let view = [];                           // the resolved wire list the drawer paints
  let armed = false;
  let lastOrder = [];

  const list = () => host.resolveWires();

  /** What a pane's output port holds, by instance or by kind. */
  const portValue = (paneId, port) => valueOfPort(paneId, port);

  /** The inputs a pane should be handed right now. */
  function collectInputs(paneId, from = view) {
    const ins = {};
    for (const w of from) {
      if (w.to.p !== paneId || !w.to.port) continue;
      const v = portValue(w.from.p, w.from.port);
      if (v !== undefined) ins[w.to.port] = v;
    }
    return ins;
  }

  /** Deliver, by change only: a pane may treat an input as an event. */
  function applyInputs() {
    // resolve here rather than reading the drawer's copy: a wire drawn a moment ago counts
    const list2 = list();
    for (const p of panes) {
      const rec = registry[p.kind];
      if (!rec || !rec.inputs || !rec.inputs.length || !p.onInputs) continue;
      const ins = collectInputs(p.id, list2);
      const last = p._ins || (p._ins = {});
      let changed = false;
      for (const k in ins) {
        if (!Object.is(last[k], ins[k])) { last[k] = ins[k]; changed = true; }
      }
      // a wire that is gone puts its port back to "unset"
      for (const k in last) {
        if (!(k in ins) && last[k] !== undefined) { last[k] = undefined; changed = true; }
      }
      if (changed) {
        try { p.onInputs(ins); }
        catch (e) { console.warn('[input]', p.kind, e && e.message); }
      }
    }
  }

  const kindOfPaneId = (id) => {
    const inst = panes.find((x) => x.id === id);
    return inst && kinds ? kinds.getKind(inst.kind) : null;
  };

  /** Arm the scheduler, or disarm it when nothing ticks. */
  function rebuildSchedule() {
    if (!kinds) return null;
    const ticking = panes.some((inst) => { const k = kinds.getKind(inst.kind); return k && typeof k.tick === 'function'; });
    if (!ticking) { if (graph) graph.setScheduler(null); return null; }
    if (!sched) return null;
    const next = sched.createScheduler({
      panes: panes.map((inst) => inst.id),
      wires: list(), kindOf: kindOfPaneId,
      onError: (pid, phase, err) => console.warn('[sched]', pid, phase, err && err.message),
      onStructure: ({ order, illegal }) => {
        if (illegal.size) toast('wire loop cut: ' + [...illegal].join(', '));
        if (order.length) lastOrder = order;
      },
    });
    next.compile();
    if (graph) graph.setScheduler(next);
    return next;
  }

  /** Register one pane kind: its ports, its category, and — for a behaviour node — how it
   *  ticks when the scheduler asks. */
  function registerKinds(K) {
    for (const [kind, rec] of Object.entries(registry)) {
      K.registerKind(kind, { title: rec.title, category: rec.category || 'view', inputs: rec.inputs, outputs: rec.outputs });
      if (rec.category === 'behaviour') {
        K.registerKind(kind, {
          // `clock` is the one stateful kind: a clock is a delay by definition, so it may sit
          // inside a loop that would otherwise be cut
          stateful: kind === 'clock',
          tick: (ref, ins, rt, env) => {
            const inst = panes.find((x) => x.id === ref.id);
            if (!inst || !inst.tickRun) return;
            /* values from ticked sources arrive in `ins`; a wire from a pane that does NOT
               tick (the editor, the tree) is read from its own signal, so a dragged wire
               reaches the program either way */
            const inputs = {};
            for (const w of view) {
              if (w.to.p !== ref.id || !w.to.port) continue;
              const v = portValue(w.from.p, w.from.port);
              if (v !== undefined) inputs[w.to.port] = v;
            }
            for (const k in ins) if (ins[k] !== undefined) inputs[k] = ins[k];
            const res = inst.tickRun(inputs) || {};
            for (const k in res) if (k[0] !== '_' && k !== '__at') env.set(k, res[k]);
          },
        });
      }
    }
  }

  /**
   * Load the four wire modules and arm everything. Runs after the first paint, so the pane
   * table exists; a pane added later is picked up by the observer below.
   */
  async function arm() {
    if (armed) return api;
    const [K, L, C, S] = await host.loadModules();
    kinds = K; sched = S;
    registerKinds(K);
    const canvas = elements.canvas();
    if (canvas && elements.viewport && elements.world) {
      // wires belong behind the panes but above the grid: #world paints after this
      elements.viewport().insertBefore(canvas, elements.world());
    }
    layer = L.createWireLayer({ canvas, camera: () => cam(), portWorld: (pid, port) => (ports ? ports.portWorld(pid, port) : null) });
    ports = C.createPorts({
      worldEl: elements.world(), camera: () => cam(), layer,
      onConnect(w) {
        dyn.push({ id: 'd' + dyn.length, from: w.from, to: w.to, kind: w.kind, type: w.type });
        host.updateCount();
        applyInputs();
        rebuildSchedule();                     // a new wire can change the order
        toast('wired ' + w.from.port + ' → ' + w.to.port);
        if (host.save) host.save();
      },
      onRefuse(why) { toast('cannot connect: ' + why); },
      onSelect(w) {
        toast(w ? ('wire ' + w.from.p + '.' + w.from.port + ' → ' + w.to.p + '.' + w.to.port) : 'no wire under cursor');
        host.updateCount();
        applyInputs();
      },
    });
    ports.setWires(dyn);
    ports.sync();
    ports.attach();
    armed = true;
    host.updateCount();                        // the layer is armed: the count is knowable now
    const pending = host.pendingWires && host.pendingWires();
    if (pending && pending.length) {
      // the workspace was restored before there was a layer: replay its wires now
      dyn.length = 0;
      for (const w of pending) dyn.push({ id: 'd' + dyn.length, ...w });
      if (host.pendingDone) host.pendingDone();
      ports.sync();
    }
    rebuildSchedule();
    t0 = now();
    setTimeout(applyInputs, 0);                // the tiles can now be told what arrived
    return api;
  }

  /** One frame of the wire layer. Called from the frame loop. */
  function draw(t) {
    if (!armed || !layer) return;
    const canvas = elements.canvas();
    if (!canvas || !canvas.isConnected) return;
    // work mode keeps the wiring visible but quiet; `w` turns it up
    const dim = ports.wiring ? 1 : 0.42;
    view = list();
    // the same list the drawer paints is the one a click can pick
    ports.setWires(view);
    layer.draw(view, (t - t0) / 1000, { flow: 1, dim, labels: !!ports.wiring, anchors: null });
  }

  /** Panes come and go while the layer is armed: the dots follow, and so does the order. */
  function watch(worldEl, { ms = 120, observe } = {}) {
    const run = () => { if (ports) ports.sync(); rebuildSchedule(); };
    if (observe) return observe(run, ms);
    if (!doc || typeof MutationObserver === 'undefined' || !worldEl) return () => {};
    let timer = 0;
    const mo = new MutationObserver(() => { clearTimeout(timer); timer = setTimeout(run, ms); });
    mo.observe(worldEl, { childList: true });
    return () => mo.disconnect();
  }

  const api = {
    arm, draw, applyInputs, rebuildSchedule, watch,
    ports: () => ports,
    layer: () => layer,
    kinds: () => kinds,
    view: () => view,
    order: () => lastOrder,
    armed: () => armed,
    ready: null,
    portValue,
  };
  return api;
}