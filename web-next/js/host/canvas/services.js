/*
 * host/canvas/services.js — what a pane module is allowed to know about the canvas app.
 *
 * Extracted from the inline script in web-next/index.html: 129 lines of ctx that were the app's
 * PUBLIC SURFACE without ever being reviewable as one. Every entry here is a capability a pane
 * may use, and nothing a pane needs should reach for `window` instead — which makes this file the
 * answer to "what can a pane do?", and the place to look when a pane wants something new.
 *
 * Two rules the table encodes, and the sub-tables are the second one:
 *
 *   NOTHING HERE IS A GLOBAL. Each entry reads the app through the scope it is handed, so the
 *   same pane module can run in the canvas, in the shell, or in a test harness that supplies a
 *   smaller scope.
 *
 *   SUB-TABLES ARE NAMESPACES, NOT FLAT NAMES. `themes`, `view`, `field` and `banner` group the
 *   calls a settings pane makes, so a pane reads `ctx.view.tiles.snap()` rather than picking one
 *   of forty loose functions out of the air.
 */

/**
 * @param {object} scope the app's capabilities, destructured below
 * @returns the ctx object js/app/pane-host.js hands to every pane
 */
export function canvasServices(scope) {
  const {
  DATA, DIAG, DYN_WIRES, EDITOR, PORT_SRC, graphRun, CUSTOM_THEMES, projectList, REG, SNAP, THEMES_APP, TREE, UI, WIRES,
  addCluster, addFreeTile, addProject, forgetProject, subscribeProjects, addRoot, applyEditorPrefs, applyPaneSource, applyTheme,
  applyThemeModule, bbox, camFor, cam, clusterFreeTiles, currentThemeMod, currentThemeName,
  defaultView, docBound, durText, esc, flyToCam, forgetTheme, forkTheme, hideGuides, loadMod,
  createNode, loadProjects, lodCounts, lodOn, openFile, openThemeEditor, paneFocus, paneSourceOf, panes,
  populateKinds, registerTheme, renderControlColumns, renderPane, rerenderView, resetLayout,
  runCheck, savePaneSource, scheduleSave, saveWorkspace, setThemeById, setWireMode, theField, applyInputs,
  endpoint, saveActiveDoc, saveMode, setSaveMode,
  themeCards, themeKeys, tidyClusters, toast, url, useProject, wireOn, zoom100, zoomNudge,
  drawStats, clusters, routesReady = () => Promise.resolve()
  } = scope;

  /* THE APP'S PORTS, UNDER THE NAMES THE WIRE LAYER RESOLVES. Registered once, here, because this
     is where the app's own ports are defined and a second list somewhere else is how the two would
     drift; `appSources()` below is that list. See the comment on it for what went wrong without it. */
  for (const [name, port] of Object.entries(APP_SOURCES(EDITOR, TREE))) PORT_SRC[name] = port;

  return {
  data: () => DATA,   // the corpus, plus the atom order the stats bar reads

  editor: () => EDITOR,
  wires: () => WIRES,
  url: (name) => url(name),
  // the route table is a promise: a pane that must fetch when it mounts waits for it
  routesReady: () => routesReady(),
  ui: { esc, toast: (m) => toast(m) },
  /* SAVING THE OPEN DOCUMENT, and what a save does besides writing it (js/host/canvas/doc-save.js). The
     console's own section is the only surface that names either of them, which is why they are here. */
  saveActiveDoc: () => { if (saveActiveDoc) saveActiveDoc(); },
  saveMode: () => (saveMode ? saveMode() : 'none'),
  setSaveMode: (id) => (setSaveMode ? setSaveMode(id) : 'none'),
  /* THE ENDPOINT NODE FOR A ROUTE NAME, for a pane migrated from the engine host.
   *
   * A pane written there asks its workspace facade — `ws.endpoint('slate.post')` — and calls the node
   * it gets back; the canvas answers with the shell's own registry over the shell's own route table
   * (js/net/endpoint-node.js + js/defaults/endpoints.js, built in js/host/canvas/net-app.js).
   *
   * IT IS NOT CALLED `ws`, AND THAT MATTERS. `ctx.ws` is how the panes DETECT the engine host
   * (js/panes/new-node.js gives its form a fixed backdrop and a modal shape when `ctx.ws` is there),
   * so a canvas that offered one would have every such pane believe it had a modal surface and a
   * socket it does not have — measured: the new-node tile rendered as a hidden fixed overlay the
   * moment `ws` was added (tests/ui/m1/new-node.spec.mjs). A capability is named for what it does
   * here; host identity is not something this host can fake. */
  endpoint: (name) => endpoint(name),
  /* THE WIRE LAYER'S OWN RECOMPUTE, for the pane host: a port a wire feeds reads the wire's value,
     and the layer applies its inputs on the HUD's cadence — so an app edge has to be able to say
     "recompute now" or it delivers the value from before the move (js/app/pane-host.js). */
  applyInputs: () => (typeof applyInputs === 'function' ? applyInputs() : undefined),
  // the app's ports, so a pane's in-ports fill themselves by name
  ports: () => ({
    buffer: EDITOR.buffer, file: EDITOR.file, cursor: EDITOR.cursor,
    // the tree's own selection: a node picked there is what a reader is looking at
    treeFile: TREE.file, selection: TREE.selection,
  }),
  // and the tile's own document: a pinned tile follows its file, not the active one
  bindTileDoc: (p, fn) => { docBound(p, fn); return () => { p.retarget = null; }; },
  counts: () => ({ panes: panes.length, roots: clusters.length, ops: WIRES.length + DYN_WIRES.length, wires: (window.__wire ? window.__wire.wires().length : 0) }),
  fileText: (path) => (DATA.fileText && DATA.fileText[path]) || '',
  check: () => DIAG,
  // a pane can register a named port source (the switch does), and gets a remover back
  registerSource: (name, port) => {
    PORT_SRC[name] = port;
    return () => { if (PORT_SRC[name] === port) delete PORT_SRC[name]; };
  },
  projects: () => projectList(),
  useProject: (id) => useProject(id),
  addProject: (path) => addProject(path),
  forgetProject: (path) => forgetProject(path),
  subscribeProjects: (fn) => subscribeProjects(fn),
  loadProjects: () => loadProjects(),
  runCheck: () => runCheck(),
  durText,
  openFile: (path) => openFile(path),
  // creating a corpus node: the pane's form, the app's route (js/app/node-services.js)
  createNode: (node) => createNode(node),
  wireOn: () => !!wireOn,
  wireCount: () => (WIRES.length + DYN_WIRES.length),
  setWireMode: (on) => setWireMode(!!on),
  panes: () => panes,
  paneSource: (pane) => paneSourceOf(pane),
  applyPaneSource: (pane, text) => applyPaneSource(pane, text),
  savePaneSource: (pane) => savePaneSource(pane),
  loadEditor: () => loadMod('/js/live/editor.js', () => import('/js/live/editor.js')),
  controlColumns: (p, host, sections) => renderControlColumns(p, sections),
  editorPrefs: () => UI.editor,
  setEditorPref: (key, value) => { UI.editor[key] = value; },
  applyEditorPrefs: () => applyEditorPrefs(),
  saveWorkspace: () => scheduleSave(),
  editorRange: () => REG.editor,
  addTile: (kind) => addFreeTile(kind),
  clusterLoose: () => { if (!clusterFreeTiles()) return false; return true; },
  addRoot: () => { const i = clusters.length, col = i % 3, row = 1 + Math.floor(i / 3); addCluster(col * 1600, row * 1000 + 200); scheduleSave(); toast('+ tile-cluster'); },
  resetLayout: () => resetLayout(),
  defaultView: () => defaultView(),
  frameAll: () => flyToCam(camFor(bbox(), 40), 400),
  populateKinds: (sel) => populateKinds(sel),
  graph: () => ({ running: graphRun.running, tick: graphRun.tick }),
  graphRun: (running) => { graphRun.running = !!running; toast(running ? 'graph running' : 'graph paused'); },
  graphStep: () => { graphRun.step(); toast('tick ' + graphRun.tick); },
  // the theme subsystem: what exists, what is on, and how a pick is applied
  themes: {
    cards: () => themeCards().map((m) => Object.assign({}, m, { source: CUSTOM_THEMES.some((t) => t.id === m.id) ? 'custom' : 'shipped' })),
    current: () => currentThemeMod(),
    currentId: () => THEMES_APP.THEMES.currentId(),
    currentName: () => currentThemeName(),
    fieldSpace: () => (theField() ? theField().space : (UI.view.field || 'graph')),
    pick: (id) => setThemeById(id, { user: true }),
    keys: () => themeKeys(),
    vars: () => THEMES_APP.readVars(),
    applyVars: (vars) => { applyTheme(vars); scheduleSave(); },
    forget: (id) => forgetTheme(id),
    // the editor's side of the same subsystem: apply a draft, fork a shipped theme, register
    // a custom one, and pick one that already exists
    apply: (mod) => applyThemeModule(mod),
    fork: (mod, name) => forkTheme(mod, name),
    register: (mod) => registerTheme(mod, { custom: true }),
    pick: (id) => setThemeById(id, { user: true }),
    openEditor: () => { const p = panes.find((q) => q.kind === 'palette'); if (p) openThemeEditor(p); },
  },
  // the canvas's own subsystem: camera, tile defaults, theme, field, banner
  view: {
    zoomLabel: () => (Math.round(cam.z * 100) + '%'),
    fps: () => (window.__fps != null ? String(window.__fps) : '–'),
    lodLine: () => (typeof lodCounts === 'object'
      ? (drawStats.drawn + ' drawn · ' + lodCounts.live + ' live · ' + lodCounts.frozen + ' frozen · ' + lodCounts.card + ' card · ' + lodCounts.culled + ' culled')
      : '–'),
    zoomNudge: (f) => zoomNudge(f),
    zoom100: () => zoom100(),
    frameAll: () => flyToCam(camFor(bbox(), 40), 400),
    tidy: () => tidyClusters(),
    themeName: () => currentThemeName(),
    themeBoard: () => {
      const t = panes.find((q) => q.kind === 'palette');
      if (t) { paneFocus(t); return; }
      addFreeTile('palette'); toast('+ theme board');
    },
    tiles: {
      lod: (v) => (v === undefined ? lodOn : (lodOn = !!v, UI.view.lod = lodOn, dirty = true, scheduleSave(), lodOn)),
      snap: (v) => (v === undefined ? SNAP.on : (SNAP.on = !!v, UI.view.snap = SNAP.on, SNAP.on || hideGuides(), scheduleSave(), SNAP.on)),
      newTilePin: (v) => (v === undefined ? !!UI.view.newTilePin : (UI.view.newTilePin = !!v, scheduleSave(), v)),
      newTileSel: (v) => (v === undefined ? UI.view.newTileSel !== false : (UI.view.newTileSel = !!v, scheduleSave(), v)),
      syntax: (v) => (v === undefined ? UI.view.syntax !== false : (UI.view.syntax = !!v, scheduleSave(), v)),
    },
    field: {
      spaces: () => THEMES_APP.fieldSpaces(),
      current: () => (theField() ? theField().space : (UI.view.field || 'graph')),
      set: (id) => {
        UI.view.field = id;
        if (theField()) theField().setSpace(id);
        scheduleSave();
        const v = panes.find((q) => q.kind === 'view');
        if (v) renderPane(v);
        toast('field: ' + id);
      },
    },
    banner: {
      get: () => ((theField() && theField().banner) ? theField().banner() : (UI.view.banner || { rotate: -7, border: true })),
      turn: (d) => {
        const now = (theField() && theField().banner) ? theField().banner() : (UI.view.banner || { rotate: -7, border: true });
        const rot = Math.round(((now.rotate || 0) + d) * 10) / 10;
        UI.view.banner = Object.assign({}, now, { rotate: rot });
        if (theField()) theField().setBanner({ rotate: rot });
        scheduleSave(); rerenderView(); toast('banner: ' + rot + '°');
      },
      rule: () => {
        const now = (theField() && theField().banner) ? theField().banner() : (UI.view.banner || { rotate: -7, border: true });
        const border = !now.border;
        UI.view.banner = Object.assign({}, now, { border });
        if (theField()) theField().setBanner({ border });
        scheduleSave(); rerenderView(); toast('banner: ' + (border ? 'ruled' : 'bare'));
      },
    },
  },
  };
}

/**
 * The app's own ports, under the names the WIRE LAYER resolves.
 *
 * A wire names its source as `<kind>.<port>` (js/host/canvas/state.js) and the layer resolves that
 * to a value through `PORT_SRC['<kind>_<port>']` (js/live/wire/host.js::defaultValue). Nothing
 * registered the editor's, so the two seeded wires that come from it — `editor.buffer -> render` and
 * `editor.cursor -> goal` — resolved to `undefined`, and `collectInputs` DROPS an undefined source
 * (`if (v !== undefined)`): measured on the shipped workspace, `render._ins === {}` and
 * `goal._ins === {}` with both wires resolved and painted. The tiles were fed by the app's by-name
 * path (`ports()` below), which is exactly why the wiring read as decoration — the wire was on the
 * canvas and carried nothing, so re-pointing it at another tile could not have changed anything.
 *
 * With these registered, a wire FROM the editor or the tree carries what that port holds, and the
 * rule the pane host applies to it ("a connected wire outranks the app", js/app/pane-host.js) decides
 * what the pane sees. `tree_selection` is here for the same reason: a wire drawn from the tree has
 * to be able to carry what the tree is showing.
 */
function APP_SOURCES(EDITOR, TREE) {
  return {
    editor_buffer: EDITOR.buffer, editor_file: EDITOR.file, editor_cursor: EDITOR.cursor,
    tree_file: TREE.file, tree_selection: TREE.selection,
  };
}

/** Build a pane host over the canvas app's scope. */
export function createCanvasPaneHost(createPaneHost, scope) {
  return createPaneHost(canvasServices(scope));
}
