/**
 * instances.js — Pane instance manager (M2.04).
 *
 * Manages pane instance lifecycle: create, mount, unmount, dispose.
 * Syncs with layout.panes, builds nodes with ports, handles LOD.
 */

import { createScope } from '../core/scope.js';
import { createNode } from '../core/node.js';
import { valuePort, eventPort, callPort } from '../core/ports.js';
import { bindPanePorts } from '../app/pane-ports.js';

/**
 * Create a pane instance manager.
 * @param {object} opts
 * @param {object} opts.ws - Workspace API
 * @param {object} opts.registry - Pane registry
 * @param {object} opts.governor - Frame governor
 * @param {object} opts.scope - Parent scope for disposal
 * @param {object} [opts.services] - the HOST's services (projects, ui), see `ctx` below
 * @returns {object} Instance manager API
 */
export function createInstanceManager({ ws, registry, governor, scope, services = {} }) {
  const instances = new Map(); // id -> { def, ctx, node, element, scope, mounted, lod }
  const nodeMap = new Map();   // id -> node (for ws.pane(id) lookup)

  function makePlaceholderInstance(kind, id) {
    // Fallback placeholder for unknown kinds
    return {
      kind,
      id,
      summary() { return 'unknown pane kind: ' + kind; },
      mount(el) {},
      unmount() {},
      onLOD() {},
      onSettle() {},
      tick() {},
    };
  }

  function createInstance(id, def, layout) {
    // Create child scope for this pane
    const paneScope = scope.child();

    // Build node with ports from def.ports
    const nodeIn = {};
    const nodeOut = {};
    const nodeCall = {};

    for (const [name, type] of Object.entries(def.ports?.in || {})) {
      if (type === 'value') nodeIn[name] = valuePort();
      else if (type === 'event') nodeIn[name] = eventPort();
      else if (type === 'call') nodeIn[name] = callPort();
    }
    for (const [name, type] of Object.entries(def.ports?.out || {})) {
      if (type === 'value') nodeOut[name] = valuePort();
      else if (type === 'event') nodeOut[name] = eventPort();
      else if (type === 'call') nodeOut[name] = callPort();
    }
    for (const [name, type] of Object.entries(def.ports?.call || {})) {
      if (type === 'call') nodeCall[name] = callPort();
    }

    const node = createNode({
      id,
      kind: 'pane',
      in: nodeIn,
      out: nodeOut,
      call: nodeCall,
    });

    // Create instance object first (so ctx can reference it)
    const instance = {
      id,
      def,
      ctx: null, // will be set below
      node,
      element: null,
      scope: scope.child(),
      mounted: false,
      lod: 'live',
      size: { w: 0, h: 0 },
      paneInstance: null,
      unsubs: [],
      pendingFrames: [],
    };

    /* THE HOST'S SERVICES, as forwarders. A pane asks its ctx for what only the app knows — which
       projects the server can serve, how to wire a path in, where a message goes — and BOTH hosts
       have to answer, or "one pane module, two apps" is true only of the panes that happen to need
       nothing. This host had none of it: a projects tile here drew "no projects found" over a server
       with projects in it, and every control in it was inert (js/app/project-services.js is the
       table; the canvas host supplies the same one). Forwarders and not values, because the list
       arrives after the pane is built and a snapshot is the bug that table exists to prevent. */
    const projectServices = {
      projects: () => (services.projects ? services.projects() : { list: [], active: {} }),
      useProject: (id) => (services.useProject ? services.useProject(id) : Promise.resolve(false)),
      loadProjects: () => (services.loadProjects ? services.loadProjects() : Promise.resolve(null)),
      addProject: (path) => (services.addProject ? services.addProject(path) : Promise.resolve(false)),
      forgetProject: (path) => (services.forgetProject ? services.forgetProject(path) : Promise.resolve(false)),
      subscribeProjects: (fn) => (services.subscribeProjects ? services.subscribeProjects(fn) : () => {}),
    };

    /* THE REST OF THE HOST'S ANSWERS, forwarded and not snapshotted. A pane's ctx is part of its
       contract: a question it asks and the host does not answer is a panel that looks mounted and
       does nothing — `ctx.url` missing meant the Goal panel could never ask the engine for the
       obligation at the caret (it drew its local fallback: a node name and a caret position), and
       `ctx.check`/`ctx.runCheck` missing meant the Diagnostics panel's button was inert. Same
       shape as `projectServices` above, and for the same reason: the values arrive later than the
       pane does. */
    const hostServices = {
      url: (name) => (services.url ? services.url(name) : null),
      data: () => (services.data ? services.data() : null),
      check: () => (services.check ? services.check() : null),
      runCheck: () => { if (services.runCheck) services.runCheck(); },
      counts: () => (services.counts ? services.counts() : null),
      /* A CHECK FINISHES AFTER THE PANE IS MOUNTED, so a panel that only reads `ctx.check()`
         once shows "not checked" forever. `onBuild(fn)` is the subscription: the host calls
         `fn(state)` whenever a report lands, and returns an unsubscribe. */
      onBuild: (fn) => (services.onBuild ? services.onBuild(fn) : () => {}),
      openFile: (path) => { if (services.openFile) services.openFile(path); },
      /* the text of a corpus file the app may already have fetched: `js/panes/symbols.js`
         asks for it, and a host that cannot answer shows an empty panel over a corpus it has. */
      fileText: (path) => (services.fileText ? services.fileText(path) : ''),
      endpoint: (name) => (services.endpoint ? services.endpoint(name) : null),
    };

    // Build ctx per D-030/D-031 - references instance via closure
    const ctx = {
      ws,
      id,
      ...projectServices,
      ...hostServices,
      ui: {
        ...(services.ui || {}),
        /* a message with nowhere to go is dropped quietly rather than thrown: the panes guard their
           toasts, and a host that has no surface for one (this one, until it grows one) must not be
           the thing that makes a pane fail to mount */
        toast: (m) => (services.ui && services.ui.toast ? services.ui.toast(m) : undefined),
      },
      params: layout.panes[id]?.params || {},
      // value(name): returns the node's own OUT value port
      value(name) {
        const port = node.out[name];
        if (!port) throw new Error('No out port ' + name + ' on pane ' + id);
        return port;
      },
      // event(name): returns the node's own OUT event port
      event(name) {
        const port = node.out[name];
        if (!port) throw new Error('No out port ' + name + ' on pane ' + id);
        return port;
      },
      // call(name, fn): binds a call implementation
      call(name, fn) {
        const port = node.call[name];
        if (!port) throw new Error('No call port ' + name + ' on pane ' + id);
        port.bind(fn);
      },
      // lod(): returns current LOD level
      lod() {
        return instance.lod || 'live';
      },
      // size(): returns pane body size in layout px
      size() {
        return instance.size || { w: 0, h: 0 };
      },
      // requestFrame(cb): defers while LOD isn't 'live'
      requestFrame(cb) {
        const owner = 'pane:' + id;
        if (instance.lod === 'live') {
          governor.requestFrame(owner, cb);
        } else {
          // Hold until LOD becomes live
          instance.pendingFrames.push({ cb, owner });
        }
      },
    };

    // Define lod and size as non-writable properties to prevent accidental overwrite by pane callbacks
    Object.defineProperty(ctx, 'lod', {
      get() { return instance.lod || 'live'; },
      configurable: false,
      enumerable: true
    });
    Object.defineProperty(ctx, 'size', {
      get() { return instance.size || { w: 0, h: 0 }; },
      configurable: false,
      enumerable: true
    });

    instance.ctx = ctx;

    // Call def.create(ctx) to get the pane instance
    let paneInstance;
    try {
      paneInstance = def.create(ctx);
    } catch (err) {
      console.error('Pane ' + id + ' (' + def.kind + ') create failed:', err);
      paneInstance = makePlaceholderInstance(def.kind, id);
    }

    instance.paneInstance = paneInstance;

    /* THE PORTS ARE CONNECTED IN ONE PLACE (js/app/pane-ports.js): the node's in/out/call ports
       are the "other side" of whatever the pane declares, and the shared loop understands both
       conventions a pane may use — a PORT (the contract) and a HANDLER (the older form). This
       used to be written here a second time, and the two copies drifted: this one only understood
       handlers, so every wire from js/defaults/wiring/*.js stopped at the node. */
    instance.unsubs = [];
    instance.unbindPorts = bindPanePorts(paneInstance, { in: nodeIn, out: nodeOut, call: nodeCall }, {
      onBind: (off) => instance.unsubs.push(off),
    });

    instances.set(id, instance);
    nodeMap.set(id, node);

    return instance;
  }

  function mountInstance(id, element) {
    const instance = instances.get(id);
    if (!instance) return;
    
    // If already mounted with a different element, unmount first
    // (renderer recreates DOM on every render pass)
    if (instance.mounted) {
      if (instance.element === element) return; // Already mounted to this element
      unmountInstance(id);
    }
    
    instance.element = element;
    instance.mounted = true;
    
    // Call pane's mount
    if (instance.paneInstance && typeof instance.paneInstance.mount === 'function') {
      try {
        instance.paneInstance.mount(element);
      } catch (err) {
        console.error('Pane ' + id + ' mount failed:', err);
      }
    }
    
    // Flush pending frames now that it's live
    if (instance.lod === 'live' && instance.pendingFrames.length > 0) {
      for (const { cb, owner } of instance.pendingFrames) {
        governor.requestFrame(owner, cb);
      }
      instance.pendingFrames = [];
    }
  }

  function unmountInstance(id) {
    const instance = instances.get(id);
    if (!instance || !instance.mounted) return;
    
    // Call pane's unmount
    if (instance.paneInstance && typeof instance.paneInstance.unmount === 'function') {
      try {
        instance.paneInstance.unmount();
      } catch (err) {
        console.error('Pane ' + id + ' unmount failed:', err);
      }
    }
    
    instance.mounted = false;
    instance.element = null;
  }

  function disposeInstance(id) {
    const instance = instances.get(id);
    if (!instance) return;
    
    // Unmount if mounted
    if (instance.mounted) {
      unmountInstance(id);
    }
    
    // Unsubscribe all
    for (const unsub of instance.unsubs) {
      try { unsub(); } catch {}
    }
    
    // Dispose scope
    if (instance.scope && typeof instance.scope.dispose === 'function') {
      instance.scope.dispose();
    }
    
    // Dispose node
    if (instance.node && typeof instance.node.dispose === 'function') {
      instance.node.dispose();
    }
    
    instances.delete(id);
    nodeMap.delete(id);
  }

  function updateLOD(id, level) {
    const instance = instances.get(id);
    if (!instance) return;
    
    const oldLod = instance.lod;
    instance.lod = level;
    
    // Notify pane
    if (instance.paneInstance && typeof instance.paneInstance.onLOD === 'function') {
      try { instance.paneInstance.onLOD(level); } catch {}
    }
    
    // If transitioning to live, flush pending frames
    if (oldLod !== 'live' && level === 'live' && instance.pendingFrames.length > 0) {
      for (const { cb, owner } of instance.pendingFrames) {
        governor.requestFrame(owner, cb);
      }
      instance.pendingFrames = [];
    }
  }

  function updateSize(id, size) {
    const instance = instances.get(id);
    if (!instance) return;
    instance.size = size;
    if (instance.paneInstance && typeof instance.paneInstance.onSettle === 'function') {
      try { instance.paneInstance.onSettle(); } catch {}
    }
  }

  /**
   * Sync instances with layout.panes.
   * Creates new instances, disposes removed ones, updates LOD/size.
   * @param {object} layout - Full layout document
   */
  function sync(layout) {
    const panes = layout.panes || {};
    const currentIds = new Set(instances.keys());
    const newIds = new Set(Object.keys(panes));

    // Dispose instances that disappeared
    for (const id of currentIds) {
      if (!newIds.has(id)) {
        disposeInstance(id);
      }
    }

    // Create or update instances
    for (const [id, paneDef] of Object.entries(panes)) {
      let instance = instances.get(id);
      if (!instance) {
        // New instance
        const def = registry.getPane(paneDef.kind);
        if (!def) {
          console.warn('Unknown pane kind: ' + paneDef.kind + ' for ' + id);
          // Create placeholder
          const placeholderScope = scope.child();
          const placeholderInstance = {
            id,
            def: { kind: paneDef.kind },
            ctx: {
              ws,
              id,
              params: paneDef.params || {},
              value(name) { throw new Error('No out port ' + name + ' on pane ' + id); },
              event(name) { throw new Error('No out port ' + name + ' on pane ' + id); },
              call(name, fn) { throw new Error('No call port ' + name + ' on pane ' + id); },
              lod() { return 'live'; },
              size() { return { w: 0, h: 0 }; },
              requestFrame(cb) { governor.requestFrame('pane:' + id, cb); },
            },
            node: null,
            element: null,
            scope: placeholderScope,
            mounted: false,
            lod: 'live',
            size: { w: 0, h: 0 },
            paneInstance: makePlaceholderInstance(paneDef.kind, id),
            unsubs: [],
            pendingFrames: [],
          };
          instances.set(id, placeholderInstance);
        } else {
          instance = createInstance(id, def, layout);
        }
      } else {
        // Update existing instance with new params
        if (instance.ctx && instance.ctx.params) {
          instance.ctx.params = { ...paneDef.params };
        }
      }
    }
  }

  return {
    /**
     * Sync instances with layout.
     * @param {object} layout
     */
    sync,

    /**
     * Get a pane instance by id.
     * @param {string} id
     * @returns {object|undefined}
     */
    get(id) {
      return instances.get(id);
    },

    /**
     * Get the node for a pane instance.
     * @param {string} id
     * @returns {object|undefined}
     */
    node(id) {
      return nodeMap.get(id);
    },

    /**
     * Check if a pane instance is mounted.
     * @param {string} id
     * @returns {boolean}
     */
    mounted(id) {
      const instance = instances.get(id);
      return instance?.mounted === true;
    },

    /**
     * Destroy all instances.
     */
    destroyAll() {
      for (const id of instances.keys()) {
        disposeInstance(id);
      }
    },

    // Internal methods for renderer
    _mount(id, element) { mountInstance(id, element); },
    _unmount(id) { unmountInstance(id); },
    _updateLOD(id, level) { updateLOD(id, level); },
    _updateSize(id, size) { updateSize(id, size); },
  };
}
