/**
 * main.js — the client's one entry.
 *
 * The page decides which app boots: a canvas page has the world element the tiles live in
 * (`js/host/canvas/main.js`), and a shell page has a `#root` for the engine's layout tree.
 * Everything below this line is the SHELL's half — the query string, the storage adapter, the
 * endpoints, the pane registry, the preset, the wiring script — and everything that is the
 * ENGINE's (the camera, the world, the renderer, the HUD, the modals, the input, the design
 * layer) is `js/engine/index.js`'s `createEngine`, which this calls.
 *
 * That split is the point of the module: a pane module runs in either app because both give
 * it the same ctx, and `index.html`/`shell.html` are two pages over one entry.
 */
import { createStore } from './layout/store.js';
import { createGovernor } from './core/frame.js';
import { createBus } from './core/bus.js';
import { createScope } from './core/scope.js';
import { createWirer } from './core/wire.js';
import { emptyLayout } from './layout/schema.js';
import { createEndpointRegistry } from './net/endpoint-node.js';
import { defineEndpoints } from './defaults/endpoints.js';
import { createPaneRegistry } from './panes/registry.js';
import { createDeviceSelection, suggestDevice } from './design/device.js';
import { presetUrl, PRESET_IDS, presetId } from './layout/presets.js';
import { createLoader } from './net/loader.js';
import { createEngine } from './engine/index.js';
import { createProjectServices } from './app/project-services.js';
import { installDebug as installDebugSurface, debugEnabled } from './core/debug.js';

// Pane implementations

const ROOT_ID = 'root';

/* `?debug=1` is js/core/debug.js's question ("the page was asked for its debug surface"), and
   the surface itself is that module's `installDebug`: it was written inline here and the
   module sat beside it with the same job. */
function parseQuery() {
  if (typeof window === 'undefined') return { safe: false, debug: false, layout: null };
  const params = new URLSearchParams(window.location.search);
  return {
    safe: params.get('safe') === '1',
    debug: debugEnabled(window.location.search),
    layout: params.get('layout'),
  };
}

/**
 * Create a storage adapter using settings.get / settings.save endpoints.
 * Returns { ready, get, put, snapshot, flush, mode }
 */
function createSettingsStorage({ endpoints, safe }) {
  const settingsGet = endpoints.endpoint('settings.get');
  const settingsSave = endpoints.endpoint('settings.save');
  let data = {};
  let mode_ = safe ? 'local' : 'server';
  let readyPromise = null;

  async function load() {
    if (safe) {
      return {};
    }
    try {
      const result = await settingsGet.call.invoke.invoke({});
      if (result && typeof result === 'object') {
        return result;
      }
    } catch (err) {
      console.warn('settings.get failed, using local:', err.message);
    }
    return {};
  }

  async function save(obj) {
    if (safe) {
      return;
    }
    try {
      await settingsSave.call.invoke.invoke(obj);
    } catch (err) {
      console.warn('settings.save failed:', err.message);
    }
  }

  readyPromise = load().then((loaded) => {
    data = loaded;
    mode_ = safe ? 'local' : 'server';
  }).catch(() => {
    data = {};
    mode_ = 'local';
  });

  function get(key) {
    return readyPromise.then(() => data[key]);
  }

  function put(key, value) {
    data = { ...data, [key]: value };
    if (!save._pending) {
      save._pending = true;
      setTimeout(async () => {
        save._pending = false;
        await save({ ...data });
      }, 400);
    }
    return readyPromise.then(() => undefined);
  }

  function snapshot() {
    return { ...data };
  }

  async function flush() {
    if (safe) return;
    await readyPromise;
    try {
      await Promise.race([save({ ...data }), new Promise(r => setTimeout(r, 5000))]);
    } catch {}
  }

  function mode() {
    return mode_;
  }

  return { ready: readyPromise, get, put, snapshot, flush, mode };
}

/**
 * Load a layout by preset id.
 *
 * Resolution order:
 *  1. `layout` query param (explicit override, e.g. ?layout=power-phone)
 *  2. device-class selection (mode + screen) derived from viewport/pointer
 *  3. fall back to the bundled empty layout
 *
 * Preset layouts ship as static JSON under /js/defaults/presets/<id>.json, so we
 * load them directly (no server round-trip) rather than via /api/layout — that
 * endpoint returns the *last-user-saved* layout on the Rust server and is not a
 * preset store. `?layout=` still takes precedence over the device-class preset.
 */
async function resolvePreset(deviceSelection, layoutOverride, endpoints) {
  // 1. Explicit ?layout= override (validated against known preset ids).
  if (layoutOverride) {
    const want = String(layoutOverride).replace(/\.json$/i, '');
    if (PRESET_IDS.includes(want)) {
      const fetched = await fetchPresetJson(presetUrl(want));
      if (fetched) return { id: want, layout: fetched };
    }
    console.warn('[main] unknown layout override "' + layoutOverride + '", falling back to device preset');
  }

  // 2. Device-class preset from stored selection.
  if (deviceSelection) {
    const id = presetId(deviceSelection.mode || 'standard', deviceSelection.screen || 'desktop');
    if (PRESET_IDS.includes(id)) {
      const fetched = await fetchPresetJson(presetUrl(id));
      if (fetched) return { id, layout: fetched };
    }
  }

  return { id: 'standard-desktop', layout: null };
}

async function fetchPresetJson(url) {
  try {
    const res = await fetch(url, { cache: 'no-store' });
    if (!res.ok) return null;
    const layout = await res.json();
    return layout && layout.version === 1 ? layout : null;
  } catch (err) {
    console.warn('[main] preset fetch failed for ' + url + ':', err.message);
    return null;
  }
}

let app = null;

export async function createApp() {
  if (app) return app;

  const { safe, debug, layout: layoutName } = parseQuery();

  const root = document.getElementById(ROOT_ID);
  if (!root) throw new Error('Root element #root not found');

  /* What this half of the boot owns: the store the engine renders, the frame governor it
     animates with, the bus and the scope the panes are wired through, the endpoints table,
     and the storage adapter. Everything else is the engine's. */
  const layout = emptyLayout('boot');
  const store = createStore(layout);
  const governor = createGovernor();
  const bus = createBus();
  const scope = createScope();
  const wirer = createWirer({ bus });

  const location = typeof window !== 'undefined' ? window.location : { protocol: 'http:', host: 'localhost:8765' };
  const endpoints = createEndpointRegistry({
    fetch: typeof fetch !== 'undefined' ? fetch : null,
    WebSocket: typeof WebSocket !== 'undefined' ? WebSocket : null,
    location, bus,
  });
  defineEndpoints(endpoints);

  const storage = createSettingsStorage({ endpoints, safe });

  // M5.10: device-class selection
  const deviceSel = createDeviceSelection({ get: storage.get.bind(storage), put: storage.put.bind(storage) });
  let deviceSelection = null;
  await storage.ready.then(async () => {
    deviceSelection = await deviceSel.load();
    if (!deviceSelection) {
      const vp = { w: root.clientWidth || 1440, h: root.clientHeight || 900 };
      const coarse = typeof window !== 'undefined' && window.matchMedia
        ? window.matchMedia('(pointer: coarse)').matches : false;
      deviceSelection = suggestDevice({ width: vp.w, height: vp.h }, coarse);
      await deviceSel.save(deviceSelection);
    }
  });

  /* THE REGISTRY OWNS THE VOCABULARY. `createPaneRegistry()` registers every kind with the
     ports it declares; this used to register ten of them a second time, overwriting the
     registry's own definitions with the same modules — the "Overwriting pane kind" lines in
     the console were this. One place says what a pane is. */
  const registry = createPaneRegistry();

  /* THE HOST'S SERVICES FOR PANES. A pane that needs to know which projects the engine can serve
     asks its ctx, so the page that boots the engine has to answer — the canvas host answers the same
     way through the same module (js/app/project-services.js). Until this existed, a projects tile
     opened in `shell.html` drew "no projects found" and none of its controls did anything. */
  const projectServices = createProjectServices({
    url: (name) => endpoints.url(name),
    fetch: (u, init) => fetch(u, init),
    /* the shell has no toast surface yet: the message goes onto its bus for whatever grows one, and
       to the console, which is where a project failure is looked for today */
    toast: (m) => { bus.emit('ui:toast', m); console.info('[project]', m); },
    onSwitched: () => { if (typeof window !== 'undefined' && window.location) window.location.reload(); },
  });

  /* ---------------- the engine ---------------- */
  const engine = createEngine({
    root, store, governor, bus, endpoints, storage, scope, wirer, registry, safe, debug,
    services: {
      projects: () => projectServices.get(),
      useProject: (id) => projectServices.use(id),
      loadProjects: () => projectServices.load(),
      addProject: (path) => projectServices.add(path),
      forgetProject: (path) => projectServices.forget(path),
      subscribeProjects: (fn) => projectServices.subscribe(fn),
      ui: { toast: (m) => { bus.emit('ui:toast', m); console.info('[project]', m); } },
    },
  });

  /* the list the panes read is filled before the first pane mounts: the projects tile draws from it
     the moment it is built, and an empty first paint is the bug this is here to avoid */
  await projectServices.load();

  /* ---------------- the preset, and its wiring ---------------- */
  const { id: presetId_, layout: presetLayout } = await resolvePreset(deviceSelection, layoutName, endpoints);
  const bootLayout = presetLayout || layout;
  // applying the document is the engine's: it re-renders and mounts what the layout names
  store.reset(bootLayout);

  /* B2: the boot layout names its wiring script (js/defaults/wiring/<name>.js, or one of the
     built-ins in that directory's index.js); running it is what connects the panes to the
     corpus. A wiring script that fails is reported and the app stands. */
  const loader = createLoader({
    ws: engine.app.ws,
    storage: null,
    scope,
    safe,
    importModule: (u) => import(/* @vite-ignore */ u),
  });
  const wiringName = bootLayout.wiring || 'standard';
  await loader.loadWiring(wiringName)
    .catch((err) => console.warn('[main] wiring "' + wiringName + '" failed:', err.message));

  if (typeof document !== 'undefined' && typeof requestAnimationFrame !== 'undefined') {
    await new Promise((resolve) => requestAnimationFrame(resolve));
  }

  if (typeof window !== 'undefined') {
    installDebugSurface(window, {
      ws: engine.app.ws, governor, store, endpoints, bus, camera: engine.app.camera, engine,
    });
  }

  if (typeof document !== 'undefined') {
    document.documentElement.dataset.ready = '1';
  }

  app = {
    root, store, governor, bus, endpoints, registry, storage, safe, debug,
    ws: engine.app.ws, camera: engine.app.camera, engine, destroy,
  };
  return app;
}

function destroy() {
  if (!app) return;
  try { app.engine.destroy(); } catch (e) { console.warn('[main] destroy', e); }
  app = null;
}

if (typeof document !== 'undefined') {
  window.addEventListener('error', (event) => {
    console.error('[main] Uncaught error:', event.message, event.filename, event.lineno);
  });
  window.addEventListener('unhandledrejection', (event) => {
    console.error('[main] Unhandled rejection:', event.reason);
  });

  /* ONE ENTRY, TWO APPS. The page decides which: a canvas page has the world element the tiles
     live in, and a shell page has a #root for the engine's layout tree. The entry is the only
     place that looks elements up and the only place that starts either app, so a pane module
     runs in both without knowing which one it is in. */
  const start = () => {
    const canvas = document.getElementById('viewport') && document.getElementById('world');
    if (canvas) {
      import('./host/canvas/main.js')
        .then((m) => m.bootCanvas({
          $: (sel) => document.querySelector(sel),
          viewport: document.getElementById('viewport'),
          world: document.getElementById('world'),
          fieldEl: () => document.getElementById('grid'),
        }))
        .catch((e) => console.error('[canvas] could not boot:', e));
      return;
    }
    createApp();
  };
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', start);
  } else {
    setTimeout(start, 0);
  }
}