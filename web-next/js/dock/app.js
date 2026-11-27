/**
 * dock/app.js — the docking shell, assembled.
 *
 * The shell is the workspace's DEFAULT page: centre editor, docks around it, a status strip
 * under it. The canvas is still there at `/index.html` and the strip links to it — a layout
 * mode, not a different product.
 *
 * What is here and what is not:
 *
 *   · the PANES are the workspace's own (js/panes/*), mounted through the same instance
 *     manager and wired by the same flows the shell page has always used
 *     (js/defaults/wiring/_flows.js). A pane does not know which page it is in.
 *   · the LAYOUT is `js/dock/model.js` + `render.js` — five regions, tab groups, sashes.
 *   · the REMEMBERING is per project (js/dock/persist.js), so two corpora get two layouts.
 *   · nothing here is the world, the camera or the HUD: those belong to the canvas, and the
 *     docking shell is what you get when you do not want a canvas.
 */

import { createGovernor } from '../core/frame.js';
import { createBus } from '../core/bus.js';
import { createScope } from '../core/scope.js';
import { createWirer } from '../core/wire.js';
import { createEndpointRegistry } from '../net/endpoint-node.js';
import { defineEndpoints } from '../defaults/endpoints.js';
import { createPaneRegistry } from '../panes/registry.js';
import { createWorkspace } from '../core/ws.js';
import { createInstanceManager } from '../panes/instances.js';
import { createProjectServices } from '../app/project-services.js';
import {
  flowsSelection, flowsCheckBuild, flowsChrome, flowsData,
} from '../defaults/wiring/_flows.js';

import {
  DOCKS, PANELS, PRESETS, fromPreset, normalize, paneMap, visiblePanels, clampSize, homeOf,
} from './model.js';
import { createDockView } from './render.js';
import { loadDock, saveDock, forgetDock } from './persist.js';
import { installDockKeys, BINDINGS } from './keys.js';
import { createStatusStrip, createBuildService, buildCounts } from './status.js';
import { createPalette } from './palette.js';
import { applyTheme, initialTheme, themeIndex, rememberTheme } from './theme.js';

/** The custom slot's storage key: the live arrangement, kept beside the layout. */
const customKey = (projectId) => 'qubitick.dock.custom.' + (projectId || 'default');

function loadCustom(projectId) {
  try {
    const raw = window.localStorage.getItem(customKey(projectId));
    return raw ? normalize(JSON.parse(raw)) : null;
  } catch { return null; }
}

function saveCustom(projectId, doc) {
  try { window.localStorage.setItem(customKey(projectId), JSON.stringify(doc)); } catch { /* nothing */ }
}

/**
 * Boot the docking shell into `root`.
 *
 * @param {object} opts
 * @param {HTMLElement} opts.root - the `#dock` element
 * @param {string} [opts.preset] - a preset to start with (`?preset=minimal`)
 * @returns {Promise<object>} the app surface (used by tests and `window.__dock`)
 */
export async function bootDock({ root, preset }) {
  /* ---------------- the document ---------------- */
  const governor = createGovernor();
  const bus = createBus();
  const scope = createScope();
  const wirer = createWirer({ bus });
  const endpoints = createEndpointRegistry({
    fetch: typeof fetch !== 'undefined' ? fetch : null,
    WebSocket: typeof WebSocket !== 'undefined' ? WebSocket : null,
    location: window.location,
    bus,
  });
  defineEndpoints(endpoints);

  /* the project this layout belongs to. `projectServices` answers asynchronously (the list
     is a request), so the first paint uses the 'default' key and the document is re-pointed
     when the answer arrives — a layout that waited for the network before drawing would be a
     blank page for a second every reload. */
  const projects = createProjectServices({
    url: (name) => endpoints.url(name),
    fetch: (u, init) => fetch(u, init),
    toast: (m) => { bus.emit('ui:toast', m); console.info('[project]', m); },
    onSwitched: () => { window.location.reload(); },
  });
  let projectId = 'default';
  const projectKeyOf = () => projectId;

  const saved = loadDock(projectId);
  let doc = saved || (preset && PRESETS[preset] ? fromPreset(preset) : fromPreset('standard'));
  doc = normalize(doc);

  /* ---------------- panes ---------------- */
  const registry = createPaneRegistry();
  const buildService = createBuildService({ fetch: (u, i) => fetch(u, i), bus });

  /* THE CORPUS SNAPSHOT THE PANES READ. Filled once at boot from the routes the engine serves,
     and refreshed when a check runs (a build can add files). One object, so `ctx.data()` is the
     same answer for every panel that asks. */
  const corpusData = { files: [], stats: null, symbols: null, graph: null };
  async function refreshCorpusData() {
    const get = async (name) => {
      try {
        const r = await fetch('/api/' + name);
        return r.ok ? await r.json() : null;
      } catch { return null; }
    };
    const [files, stats, symbols] = await Promise.all([get('files'), get('stats'), get('symbols')]);
    if (files) corpusData.files = files.files || files || [];
    if (stats) corpusData.stats = stats;
    if (symbols) corpusData.symbols = symbols;
  }

  const services = {
    projects: () => projects.get(),
    url: (name) => endpoints.url(name),
    data: () => corpusData,
    counts: () => ({ docks: DOCKS.length, panels: Object.keys(PANELS).length }),
    endpoint: (name) => endpoints.endpoint(name),
    onBuild: (fn) => buildService.onChange(fn),
    openFile: (path) => {
      // opening a file is what the tree's click does: tell the editor, then focus it
      const editor = ws.pane('editor');
      if (editor && editor.in && editor.in.file && editor.in.file.set) editor.in.file.set(path);
      view && view.focusPanel('editor1');
    },
    useProject: (id) => projects.use(id),
    loadProjects: () => projects.load(),
    addProject: (p) => projects.add(p),
    forgetProject: (p) => projects.forget(p),
    subscribeProjects: (fn) => projects.subscribe(fn),
    ui: { toast: (m) => { bus.emit('ui:toast', m); console.info('[dock]', m); } },
    check: () => buildService.state,
    runCheck: () => buildService.run(),
  };

  /* a fake camera and modal port, because the workspace facade asks for both and the docking
     shell has neither: the camera is the canvas's, and a modal here is a dialog on top of the
     docks. Present, and honestly inert. */
  const camera = { get: () => ({ x: 0, y: 0, z: 1 }), set: () => {}, fit: () => {} };
  const modals = { get: () => null, add: () => {}, open: () => {}, close: () => {}, toggle: () => {} };
  const clocks = { clock: () => ({}), delay: () => () => {}, pauseAll() {}, resumeAll() {}, stepAll() {}, isPaused: () => false };

  const ws = createWorkspace({
    bus, storage: null, endpoints, governor,
    layoutStore: { get: () => ({ panes: paneMap(doc) }), subscribe: () => () => {}, dispatch: () => {} },
    camera, modals, clocks, wirer,
    paneRegistry: registry, scope, safe: false, debug: false,
  });
  const instances = createInstanceManager({ ws, registry, governor, scope, services });
  ws._setInstances(instances);
  instances.sync({ panes: paneMap(doc) });

  /* ---------------- the view ---------------- */
  let dirtyFromUser = false;
  /** Any change that is not "switch to preset" makes the layout Custom, and saves it. */
  const customise = () => {
    dirtyFromUser = true;
    doc.key = 'custom';
    doc.name = 'Custom';
    if (status) status.presetSelect.value = 'custom';
    saveCustom(projectKeyOf(), doc);
  };
  const persistNow = () => {
    saveDock(projectKeyOf(), doc);
    doc.key = doc.key || 'standard';
    if (dirtyFromUser || doc.key === 'custom') saveCustom(projectKeyOf(), doc);
  };

  const view = createDockView({
    root, doc, instances,
    onChange: (next, meta) => {
      doc = next;
      // THE CUSTOM SLOT IS WHAT THE USER IS LOOKING AT. Any rearrangement fills it; switching
      // presets does not (that is `setPreset`, below).
      if (meta && meta.source !== 'preset') customise();
      instances.sync({ panes: paneMap(doc) });
      persistNow();
      if (status) status.presetSelect.value = doc.key;
    },
  });

  /* ---------------- wiring: the same flows the shell page uses ---------------- */
  function wire() {
    try {
      flowsSelection(ws);
      flowsCheckBuild(ws);
      flowsChrome(ws);
      flowsData(ws);
    } catch (err) {
      console.error('[dock] wiring failed:', err);
    }
  }
  wire();

  /* the corpus snapshot, then the tree is told: its `files` in-port is filled by the data flow,
     and both read the same object. */
  await refreshCorpusData().catch(() => {});
  try {
    const tree = ws.pane('tree');
    if (tree && tree.in && tree.in.files && tree.in.files.set) tree.in.files.set(corpusData.files);
    const nodes = ws.pane('nodes');
    if (nodes && nodes.in && nodes.in.corpus && nodes.in.corpus.set) nodes.in.corpus.set(corpusData.files);
  } catch (err) {
    console.warn('[dock] corpus data:', err.message);
  }

  /* the diag pane's own "check corpus" button reaches the same service the strip does */
  bus.on && bus.on('ui:build', () => { buildService.run(); });

  /* ---------------- status, palette, keys ---------------- */
  const idx = await themeIndex().catch(() => null);
  const themeList = idx ? (idx.themes || []).map((t) => ({ id: t.id })) : [];
  let currentTheme = await initialTheme().catch(() => null);
  if (currentTheme) await applyTheme(currentTheme).catch((e) => console.warn('[dock] theme:', e.message));

  const setPreset = (key) => {
    if (key === 'custom') {
      const custom = loadCustom(projectKeyOf()) || savedCustom();
      if (custom) { doc = custom; view.setDoc(doc); instances.sync({ panes: paneMap(doc) }); }
      else { doc = fromPreset('standard'); view.setDoc(doc); instances.sync({ panes: paneMap(doc) }); }
      status && (status.presetSelect.value = 'custom');
      persistNow();
      return;
    }
    if (!PRESETS[key]) return;
    dirtyFromUser = false;
    doc = fromPreset(key);
    view.setDoc(doc);
    instances.sync({ panes: paneMap(doc) });
    persistNow();
    if (status) status.presetSelect.value = key;
  };
  const savedCustom = () => loadCustom(projectKeyOf());

  const toggleDock = (dockName) => {
    const d = doc.docks[dockName];
    if (!d) return;
    if (d.hidden) { d.hidden = false; d.collapsed = false; }
    else if (d.collapsed) { d.collapsed = false; }
    else { d.collapsed = true; }
    view.render();
    instances.sync({ panes: paneMap(doc) });
    customise();
    persistNow();
  };

  const resetLayout = () => {
    forgetDock(projectKeyOf());
    try { window.localStorage.removeItem(customKey(projectKeyOf())); } catch { /* nothing */ }
    const key = PRESETS[doc.key] ? doc.key : 'standard';
    dirtyFromUser = false;
    doc = fromPreset(key);
    view.setDoc(doc);
    instances.sync({ panes: paneMap(doc) });
    persistNow();
    if (status) status.presetSelect.value = key;
  };

  const status = createStatusStrip({
    doc, build: buildService, presetKey: doc.key,
    themes: { list: themeList, current: currentTheme },
    actions: {
      setPreset,
      reset: resetLayout,
      runBuild: () => buildService.run(),
      setTheme: async (id) => {
        currentTheme = id;
        rememberTheme(id);
        await applyTheme(id).catch((e) => console.warn('[dock] theme:', e.message));
      },
    },
  });
  root.appendChild(status.el);

  const palette = createPalette({ commands: paletteCommands() });

  function paletteCommands() {
    const cmds = [
      { id: 'build', label: 'Run corpus check', hint: 'Ctrl+Enter', run: () => buildService.run() },
      { id: 'reset', label: 'Reset layout', hint: 'Ctrl+Alt+R', run: () => resetLayout() },
      { id: 'canvas', label: 'Open the canvas workspace', hint: '', run: () => { window.location.href = '/index.html'; } },
    ];
    for (const b of BINDINGS.filter((x) => x.id.startsWith('preset.'))) {
      const key = b.id.split('.')[1];
      cmds.push({ id: b.id, label: b.label, hint: b.chord, run: () => setPreset(key) });
    }
    for (const dockName of ['left', 'bottom', 'right']) {
      cmds.push({ id: 'dock.' + dockName, label: 'Toggle ' + dockName + ' dock', hint: '', run: () => toggleDock(dockName) });
    }
    /* every panel, and the chord that focuses it when it is already on screen */
    for (const [id, def] of Object.entries(PANELS)) {
      const at = visiblePanels(doc).indexOf(id);
      cmds.push({
        id: 'panel.' + id,
        label: 'Open ' + def.title,
        hint: at >= 0 && at < 9 ? 'Ctrl+' + (at + 1) : 'panel',
        run: () => openPanel(id),
      });
    }
    return cmds;
  }

  /** Show a panel, docking it in its home when it is not already visible. */
  function openPanel(id) {
    if (view.focusPanel(id)) return true;
    const def = PANELS[id];
    if (!def) return false;
    const home = homeOf(id);
    const d = doc.docks[home];
    d.hidden = false;
    d.collapsed = false;
    if (!d.groups.length) d.groups = [{ tabs: [id], active: 0 }];
    else d.groups[d.groups.length - 1].tabs.push(id);
    d.sizes = d.groups.map(() => 1 / d.groups.length);
    view.render();
    instances.sync({ panes: paneMap(doc) });
    customise();
    persistNow();
    return true;
  }

  const uninstallKeys = installDockKeys({
    view, doc,
    apply: () => {},
    setPreset,
    toggleDock,
    palette: () => palette.toggle(),
    build: () => buildService.run(),
    reset: resetLayout,
  });

  /* A resize re-clamps every px size — a window narrowed past a dock's max would otherwise
     leave the editor a sliver. Sizes are the user's; the CLAMP is the viewport's. */
  let resizeFrame = null;
  const onResize = () => {
    if (resizeFrame) return;
    resizeFrame = requestAnimationFrame(() => {
      resizeFrame = null;
      const vp = { width: root.clientWidth || window.innerWidth || 1400 };
      for (const d of ['left', 'right', 'bottom']) {
        const dock = doc.docks[d];
        if (dock && dock.size) dock.size = clampSize(d, dock.size, vp);
      }
      view.render();
    });
  };
  window.addEventListener('resize', onResize);

  /* the project the layout belongs to, once the server answers */
  projects.load().then(() => {
    const active = projects.get();
    const id = active && active.active && active.active.id ? active.active.id : 'default';
    if (id === projectId) return;
    projectId = id;
    const perProject = loadDock(projectId);
    if (perProject) {
      doc = perProject;
      view.setDoc(doc);
      instances.sync({ panes: paneMap(doc) });
    } else {
      persistNow();  // the current layout becomes this project's starting point
    }
  }).catch((e) => console.warn('[dock] projects:', e.message));

  const app = {
    root, doc: () => doc, view, instances, ws, bus, buildService, status, palette,
    setPreset, resetLayout, toggleDock, openPanel, panels: visiblePanels,
    /** Wear another theme (the strip's selector uses this, and a screenshot run needs it). */
    setTheme: async (id) => {
      currentTheme = id;
      rememberTheme(id);
      await applyTheme(id);
      return id;
    },
    destroy() {
      uninstallKeys();
      window.removeEventListener('resize', onResize);
      view.destroy();
      instances.destroyAll();
    },
  };
  return app;
}