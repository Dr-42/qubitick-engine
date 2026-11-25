/**
 * live/wire/schedule.js — who runs, in what order, and why a loop is refused.
 *
 * The wire layer moves *values* by subscription: push, instantaneous, no order.
 * That is right for a corpus IDE, where a selection or a buffer edit just lands.
 * It cannot express "run these nodes once per frame, upstream first" — which is
 * what a clock, a counter or a JS transform actually wants.
 *
 * So a kind may declare `tick(pane, ins, rt, env)`. Those kinds form a schedule:
 * the value wires between them become dependencies, a Kahn topological sort
 * orders them, and any cycle that has no `stateful` node in it is cut — the
 * back-edges are marked illegal and reported, exactly like a compiler refusing
 * a combinational loop. `stateful` is how a delay/clock/counter says "I break
 * the loop on purpose"; without one, a cycle is a bug and is treated as one.
 *
 * Pure except for the callbacks it is handed: no DOM, no globals, so it can be
 * driven from a test.
 */

/**
 * Build the schedule.
 *
 * @param {object}   opts
 * @param {string[]} opts.panes  pane ids that take part
 * @param {object[]} opts.wires  { id, from:{p,port}, to:{p,port}, kind }
 * @param {function} opts.kindOf (paneId) -> kind descriptor
 * @returns {{order:string[], illegal:Set<string>, deps:Map<string,Set<string>>, typed:object[]}}
 */
export function compileSchedule({ panes, wires, kindOf }) {
  const deps = new Map(panes.map((p) => [p, new Set()]));
  const illegal = new Set();
  const byId = new Map();

  const typed = (wires || []).map((w) => {
    const src = kindOf(w.from?.p);
    const dst = kindOf(w.to?.p);
    const port = src && (src.ports || []).find((p) => p.id === w.from.port && p.dir === 'out');
    const dead = !port || !dst || !deps.has(w.from.p) || !deps.has(w.to.p);
    const kind = (w.kind || port?.kind || 'value');
    const entry = { ...w, kind, dead, illegal: false };
    byId.set(w.id, entry);
    return entry;
  });

  // value wires between *ticked* panes are the only thing that constrains order
  for (const w of typed) {
    if (w.dead || w.kind !== 'value') continue;
    const consumer = kindOf(w.to.p);
    if (!consumer || typeof consumer.tick !== 'function') continue;
    if (consumer.stateful) continue;              // a delay breaks the dependency
    deps.get(w.to.p)?.add(w.from.p);
  }

  let order = kahn(deps);
  if (order.length < deps.size) {
    // cut every value edge inside the surviving knot, then retry once
    const left = new Set([...deps.keys()].filter((id) => !order.includes(id)));
    for (const w of typed) {
      if (w.dead || w.kind !== 'value') continue;
      if (left.has(w.from.p) && left.has(w.to.p)) {
        w.illegal = true;
        illegal.add(w.id);
        deps.get(w.to.p)?.delete(w.from.p);
      }
    }
    order = kahn(deps);
    for (const id of deps.keys()) if (!order.includes(id)) order.push(id);
  }

  return { order, illegal, deps, typed };
}

function kahn(deps) {
  const indeg = new Map();
  const rev = new Map();
  for (const [id, set] of deps) {
    indeg.set(id, set.size);
    for (const s of set) {
      if (!rev.has(s)) rev.set(s, []);
      rev.get(s).push(id);
    }
  }
  const q = [...deps.keys()].filter((id) => indeg.get(id) === 0);
  const out = [];
  while (q.length) {
    const n = q.shift();
    out.push(n);
    for (const m of rev.get(n) || []) {
      indeg.set(m, indeg.get(m) - 1);
      if (indeg.get(m) === 0) q.push(m);
    }
  }
  return out;
}

/**
 * A runnable scheduler over one compiled graph.
 *
 * @param {object}   opts
 * @param {object[]} opts.panes   [{ id, kind }] or plain ids
 * @param {object[]} opts.wires
 * @param {function} opts.kindOf  (paneId) -> kind descriptor
 * @param {function} [opts.onError] (paneId, phase, error)
 */
export function createScheduler({ panes, wires, kindOf, onError, onStructure } = {}) {
  const ids = (panes || []).map((p) => (typeof p === 'string' ? p : p.id));
  const runtime = new Map();   // paneId -> per-pane scratch object (rt)
  const outVal = new Map();    // paneId -> { portId: value }  last written outputs
  let plan = { order: [], illegal: new Set(), typed: [] };
  let time = 0;
  let evals = 0;

  const rtFor = (id) => {
    if (!runtime.has(id)) runtime.set(id, { out: {}, last: {} });
    return runtime.get(id);
  };
  const outsOf = (id) => {
    if (!outVal.has(id)) outVal.set(id, {});
    return outVal.get(id);
  };

  function compile() {
    plan = compileSchedule({ panes: ids, wires, kindOf });
    for (const w of plan.typed) if (w.dead) plan.illegal.delete(w.id);
    onStructure && onStructure({ order: plan.order, illegal: plan.illegal });
    return plan;
  }

  /** Current output value of a port, as last written by its pane's tick. */
  function read(portPath) {
    const [pid, port] = String(portPath).split('.');
    return outsOf(pid)[port];
  }

  function readInputs(paneId) {
    const kind = kindOf(paneId);
    const ins = {};
    const rt = rtFor(paneId);
    for (const port of kind?.ports || []) {
      if (port.dir !== 'in' || port.kind !== 'value') continue;
      const w = plan.typed.find((x) => !x.dead && x.kind === 'value' && x.to.p === paneId && x.to.port === port.id);
      const v = w ? outsOf(w.from.p)[w.from.port] : undefined;
      ins[port.id] = v;
      rt.last[port.id] = v === undefined ? null : v;
    }
    return ins;
  }

  /** One frame: tick in order, then drain the event wires the ticks emitted. */
  function step(dt) {
    time += dt;
    const queue = [];
    let done = 0;
    for (const pid of plan.order) {
      const kind = kindOf(pid);
      if (!kind || typeof kind.tick !== 'function') continue;
      const rt = rtFor(pid);
      const ins = readInputs(pid);
      const env = {
        t: time,
        dt,
        set: (port, value) => { outsOf(pid)[port] = value; rt.out[port] = value; },
        emit: (port, payload) => { if (queue.length < 256) queue.push({ p: pid, port, payload }); },
      };
      try {
        kind.tick({ id: pid }, ins, rt, env);
        done++;
      } catch (err) {
        if (onError) onError(pid, 'tick', err);
        else console.error('[wire] tick', pid, err);
      }
    }

    let guard = 0;
    while (queue.length && guard++ < 512) {
      const ev = queue.shift();
      for (const w of plan.typed) {
        if (w.dead || w.kind !== 'event') continue;
        if (w.from.p !== ev.p || w.from.port !== ev.port) continue;
        const kind = kindOf(w.to.p);
        if (!kind || typeof kind.onEvent !== 'function') continue;
        const rt = rtFor(w.to.p);
        const env = { t: time, dt, emit: (port, payload) => { if (queue.length < 256) queue.push({ p: w.to.p, port, payload }); } };
        try {
          kind.onEvent({ id: w.to.p }, w.to.port, ev.payload, rt, env);
        } catch (err) {
          if (onError) onError(w.to.p, 'event', err);
          else console.error('[wire] event', w.to.p, err);
        }
      }
    }

    evals = done;
    return done;
  }

  return {
    compile,
    step,
    read,
    readInputs,
    runtime: rtFor,
    outputs: outsOf,
    get plan() { return plan; },
    get order() { return plan.order; },
    get illegal() { return plan.illegal; },
    get evals() { return evals; },
    get time() { return time; },
  };
}