/**
 * ws.js — Workspace API facade (M1.28 + M4.00).
 *
 * Creates the main UI facade: pane lookup (id or kind), endpoints,
 * bus, clock, modal, layout node + store, camera, registerPane.
 *
 * D-037: ws.pane(nameOrKindOrId) — exact instance id first;
 *         else first instance of that kind in layout order.
 * D-039: ws.layout.node — node {id:'layout', kind:'layout', out:{layout: valuePort}}
 *         kept in sync with the store.
 */

import { createWirer } from './wire.js';
import { createBus } from './bus.js';
import { createClocks } from './clocks.js';
import { createNode } from './node.js';
import { valuePort } from './ports.js';

/**
 * Create the Workspace API — the main facade for the UI.
 *
 * @param {object} deps
 * @param {object} deps.bus - event bus
 * @param {object} deps.storage - storage adapter
 * @param {object} deps.endpoints - endpoint registry
 * @param {object} deps.governor - frame governor
 * @param {object} deps.layoutStore - layout store
 * @param {object} deps.camera - camera object
 * @param {object} deps.modals - modal manager
 * @param {object} deps.clocks - clock manager
 * @param {object} deps.wirer - wire function
 * @param {object} deps.paneRegistry - pane kind registry
 * @param {object} deps.scope - root scope
 * @param {object} deps.debug - debug object
 */
export function createWorkspace({
  bus,
  storage,
  endpoints,
  governor,
  layoutStore,
  camera,
  modals,
  clocks,
  wirer,
  paneRegistry,
  scope,
  safe, debug,
} = {}) {
  // Instances is set after ws creation (instances depends on ws)
  let instancesRef = null;

  // --- pane(nameOrKindOrId) --- D-037
  // Exact instance id first; else first instance of that kind in layout order.
  // Returns a facade exposing the node's in/out/call port maps at the top level
  // (so wiring can use `pane('nodes').out.selection` / `pane('tree').in.files.emit`).
  function paneFacade(inst) {
    if (inst && inst.node) {
      return {
        id: inst.id,
        kind: inst.def?.kind || inst.kind,
        def: inst.def,
        node: inst.node,
        in: inst.node.in,
        out: inst.node.out,
        call: inst.node.call,
        // delegate to underlying instance where present
        pane: inst.paneInstance || inst,
      };
    }
    return inst;
  }
  function pane(nameOrKindOrId) {
    // 1. Exact instance id match
    if (instancesRef) {
      const inst = instancesRef.get(nameOrKindOrId);
      if (inst) return paneFacade(inst);
    }
    // 2. Kind name -> first instance of that kind in layout order
    if (instancesRef) {
      const allIds = Object.keys(layoutStore.get().panes || {});
      for (const id of allIds) {
        const inst = instancesRef.get(id);
        if (inst && inst.def && inst.def.kind === nameOrKindOrId) {
          return paneFacade(inst);
        }
      }
    }
    // 3. Fall back to pane kind registry (backward compat)
    const def = paneRegistry.getPane ? paneRegistry.getPane(nameOrKindOrId) : paneRegistry.get(nameOrKindOrId);
    if (def) return def;
    // Not found -> return undefined (wiring already null-guards). Do not throw:
    // pane kinds that only exist as modals (graph, new-node, …) are absent from
    // the world `panes` map and must not abort the whole wiring script.
    return undefined;
  }

  // --- endpoint(name) ---
  function endpoint(name) {
    return endpoints.endpoint(name);
  }

  // --- wire ---
  const wire = wirer;

  // --- on / emit (bus) ---
  const on = bus.on.bind(bus);
  const emit = bus.emit.bind(bus);

  // --- clock / delay / clocks ---
  const clock = clocks.clock;
  const delay = clocks.delay;
  const clocksObj = clocks;

  // --- modal(id) ---
  function modal(id) {
    return modals.get(id);
  }

  // --- registerPane ---
  function registerPane(kind, factory) {
    paneRegistry.register(kind, factory);
  }

  // --- D-039: layout node kept in sync with store ---
  let layoutNode = null;
  let layoutOutPort = null;
  let layoutUnsub = null;

  if (layoutStore && typeof layoutStore.get === 'function' && typeof layoutStore.subscribe === 'function') {
    layoutOutPort = valuePort(layoutStore.get());

    function layoutNodeDispose() {
      layoutUnsub?.();
    }

    layoutNode = createNode({
      id: 'layout',
      kind: 'layout',
      in: {},
      out: { layout: layoutOutPort },
      call: {},
      dispose: layoutNodeDispose,
    });

    layoutUnsub = layoutStore.subscribe((layout) => {
      layoutOutPort.set(layout);
    });
  }

  // --- ws object ---
  const ws = {
    pane,
    endpoint,
    endpoints,  // expose the full registry
    wire,
    on,
    emit,
    bus,  // expose the bus
    clock,
    delay,
    clocks: clocksObj,
    modal,
    layout: Object.assign({}, layoutStore, { node: layoutNode }),
    camera,
    registerPane,
    storage,
    hooks: {
      wheel: undefined,
      scroll: undefined,
      gesture: undefined,
    },
    scoped,
    // Expose safe mode flag
    safe: Boolean(safe),
    // Set instances after creation (called from main.js)
    _setInstances: (inst) => { instancesRef = inst; },
  };

  return ws;

  // --- scoped(scope) ---
  function scoped(scope) {
    const wireScoped = (...args) => {
      const unsub = wire(...args);
      scope.add(unsub);
      return unsub;
    };

    const onScoped = (...args) => {
      const unsub = on(...args);
      scope.add(unsub);
      return unsub;
    };

    const clockScoped = (...args) => {
      const node = clock(...args);
      scope.add(() => node.out.tick.dispose?.());
      return node;
    };

    const delayScoped = (ms) => {
      const port = delay(ms);
      scope.add(() => port.dispose?.());
      return port;
    };

    const registerPaneScoped = (kind, factory) => {
      const unsub = registerPane(kind, factory);
      scope.add(unsub);
      return unsub;
    };

    return {
      pane,
      endpoint,
      endpoints,
      wire: wireScoped,
      on: onScoped,
      emit,
      bus,
      clock: clockScoped,
      delay: delayScoped,
      clocks: clocksObj,
      modal,
      layout: Object.assign({}, layoutStore, { node: layoutNode }),
      camera,
      registerPane: registerPaneScoped,
      storage,
      hooks: ws.hooks,
      safe: ws.safe,
      _setInstances: (inst) => { instancesRef = inst; },
    };
  }
}
