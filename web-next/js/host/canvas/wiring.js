/*
 * host/canvas/wiring.js — the canvas's own object graph: registry, mounts, tiles, graph, themes, world.
 *
 * The third and last `attach`: `core` is state and helpers, `runtime` is what runs, and this is
 * the wiring BETWEEN the modules — the registry that knows the kinds, the table that mounts them,
 * the tile factory and its header actions, the node graph, the theme machine, and the clusters.
 * Each of those is already a module; this file is the app object they are given.
 *
 * The ORDER here is load-bearing and is the reason it is one file:
 *
 *   REGISTRY      before anything that mounts a pane, because the mount table asks it for kinds;
 *   MOUNTS        before the tile factory, because a tile's body is built through it;
 *   TILES         before the graph and the themes, because both add tiles;
 *   PANE_HOST     last of the wiring, because it is the ctx a pane gets — the surface every other
 *                 entry in this file already exists to serve.
 *
 * A name that is only used inside a lambda can be defined later; a name that is READ while
 * wiring (a registry lookup, a mount table entry) cannot. That is the whole discipline of this
 * file, and the boot spec is what checks it.
 */

import { createRegistry } from './panes.js';
import { createAppMounts } from '../../panes/app-mounts.js';
import { createTiles } from './tiles.js';
import { createGraphRun } from '../../app/graph-run.js';
import { createThemesApp } from './themes-app.js';
import { createWorldHost } from './world.js';
import { createTileState } from './tile-state.js';
import { createLivePanes } from '../../panes/app-live.js';
import { saveModeOf } from '../../app/prefs.js';
import { createPaneHost } from '../../app/pane-host.js';
import { canvasServices } from './services.js';
import { debounce } from '../../app/util.js';

/**
 * @param {object} app   the shared app object; this function fills the wiring names on it
 * @param {object} env   { $, fieldEl(), loadKatex }
 */
export function attachWiring(app, env) {
  const $ = env.$;

  /* ---------------- tile state, the live panes ---------------- */
  const TILE_STATE = createTileState({
    tree: app.TREE,
    activeDoc: () => app.activeDoc(),
    showDoc: (p, d) => app.showDoc(p, d),
    paintChrome: (p) => TILE_STATE.paintChrome(p),
    toast: (m) => app.toast(m),
    save: () => app.scheduleSave(),
  });
  const LIVE = createLivePanes({
    ui: { esc: (s) => app.esc(s), toast: (m) => app.toast(m) },
    data: () => app.DATA,
    editor: () => app.EDITOR,
    tree: () => app.TREE,
    selOf: (p) => TILE_STATE.selOf(p),
    setSel: (p, id) => TILE_STATE.setSel(p, id),
    linkedSel: (p) => TILE_STATE.linkedSel(p),
    paintChrome: (p) => TILE_STATE.paintChrome(p),
    activeDoc: () => app.activeDoc(),
    docOf: (path, text) => app.docOf(path, text),
    loadText: (path) => app.loadText(path),
    showDoc: (p, d, opts) => app.showDoc(p, d, opts),
    setActive: (path) => app.setActive(path),
    docEdited: (doc, text, view, changes) => app.docEdited(doc, text, view, changes),
    // the editor bar's save button: writes THIS tile's document (js/host/canvas/doc-save.js)
    saveDoc: (p) => app.saveDoc(p),
    watchActive: (p) => app.watchActive(p),
    loadEditor: () => app.loadMod('/js/live/editor.js', () => import('/js/live/editor.js')),
    editorPrefs: () => app.UI.editor,
    applyEditorPrefs: () => app.applyEditorPrefs(),
    addEditor: (ed) => app.EDITORS.add(ed),
    setViewSyntax: (v) => { app.UI.view.syntax = v; },
    save: () => app.scheduleSave(),
    openFile: (path) => app.openFile(path),
    recognizeHandwriting: (canvas) => app.recognizeHandwriting(canvas),
    /* the citation search the editor's Enter asks for, answered by the host that owns the routes */
    autocite: (lhs, rhs) => app.autocite(lhs, rhs),
    /* the check strip's two questions: does this buffer's rows follow, and what can come next */
    rows: (content) => app.rows(content),
    /* where a citation points: the editor's links follow it */
    cite: (name) => app.cite(name),
    steps: (expr, limit) => app.steps(expr, limit),
  });
  const LIVE_MOUNTS = { tree: (p) => LIVE.renderTree(p), graph: (p) => LIVE.renderGraph(p),
    editor: (p) => LIVE.renderEditor(p), handwrite: (p) => LIVE.renderHandwrite(p) };

  /* ---------------- the registry ---------------- */
  const REGISTRY = createRegistry({
    editor: () => app.EDITOR,
    toast: (m) => app.toast(m),
    graph: () => GRAPH,
    fetch: (u, init) => app.fetch(u, init),
    url: (name) => app.url(name),
    render: (q) => app.render(q),
    markDirty: () => app.markDirty(),
    save: () => app.scheduleSave(),
    nodeRuns: () => GRAPH.runs,
    behaviourCtx: () => ({
      ctx: REGISTRY.CTX,
      resolveFrom: (from) => GRAPH.valueOf(from),
      sources: app.PORT_SRC,
      schedule: debounce,
      cascade: () => GRAPH.cascade(),
      onResult: (r, outs) => GRAPH.out(r, outs),
      registerRun: (id, fn) => { GRAPH.runs[id] = fn; },
    }),
    paneFocus: (q) => app.paneFocus(q),
    makeTile: (id, kind, wr, home, state) => app.mkPane(id, kind, wr, home, state),
    nextTileId: (kind) => app.TILES.next(kind),
    setHotPane: (el) => app.setHotPane(el),
    populateKinds: (sel) => app.populateKinds(sel),
    mounts: (kind) => LIVE_MOUNTS[kind] || APP_MOUNTS.mountFor(kind),
  }, { DATA: app.DATA, panes: app.panes, cam: app.cam, viewport: () => app.viewport(), loadKatex: env.loadKatex, openFile: (p) => app.openFile(p) });

  /* ---------------- the mount table ---------------- */
  const APP_MOUNTS = createAppMounts({
    paneHost: { mount: (def, p) => PANE_HOST.mount(def, p) },
    data: () => app.DATA,
    docPath: (p) => app.docPath(p),
    watchActive: (p) => app.watchActive(p),
    redraw: (p) => app.render(p),
    /* THE CORPUS TILE'S PORTS ARE THE APP'S CORPUS. The app does not fetch it (js/host/canvas/
       data.js): it reads the four ports this tile publishes, so the interface follows whatever
       project is wired into it. */
    corpusFrom: (inst, p) => {
      if (p && p._corpusOff) { try { p._corpusOff(); } catch (e) {} }
      const off = app.corpusFrom(inst, p);
      if (p) p._corpusOff = off;
    },
    registerSource: (name, port) => {
      app.PORT_SRC[name] = port;
      return () => { if (app.PORT_SRC[name] === port) delete app.PORT_SRC[name]; };
    },
    results: () => GRAPH.results,
    wireCount: () => (app.WIRES.length + app.DYN_WIRES.length),
    counts: () => app.diagCounts(),
    cursor: () => (app.EDITOR.cursor.get() || {}),
    watchCursor: (fn) => app.EDITOR.cursor.on(fn),
    panes: () => app.panes,
    toast: (m) => app.toast(m),
    save: () => app.scheduleSave(),
  });
  /* every kind the canvas ships, once, from the module's own list */
  REGISTRY.registerBuiltins();

  /* ---------------- the tile factory ---------------- */
  const TILE_WORLD = createTiles({
    registry: () => REGISTRY.PANES,
    viewPrefs: () => app.UI.view,
    tileKinds: { DOC_KINDS: TILE_STATE.DOC_KINDS, LINK_KINDS: TILE_STATE.LINK_KINDS, SYNTAX_KINDS: TILE_STATE.SYNTAX_KINDS },
    lod: app.lod,
    viewport: () => app.viewport(),
    viewportWorld: () => app.viewportWorld(),
    screenRect: (p) => app.screenRect(p),
    visibleOnly: () => app.VIEW.visibleOnly(),
    esc: (x) => app.esc(x),
    el: (sel) => $(sel),
    tree: () => app.TREE,
    editor: () => app.EDITOR,
    /* the pair the home arrangement builds asks for this (js/world/home.js): the workspace
       cannot boot without a corpus, so the wire that feeds the corpus tile is made here and
       the function itself lives in the boot (js/host/canvas/boot.js) */
    ensureProjectWire: () => app.ensureProjectWire(),
    activeDoc: () => app.activeDoc(),
    selOf: (p) => TILE_STATE.selOf(p),
    mountGrips: (p) => app.mountGrips(p),
    gestures: () => app.GESTURES,
    setRect: (q) => app.setRect(q),
    render: (q) => app.render(q),
    fitRect: (wr, pad, ms) => app.fitRect(wr, pad, ms),
    flyToCam: (t, ms) => app.flyToCam(t, ms),
    bbox: () => app.bbox(),
    camFor: (wr, pad) => app.camFor(wr, pad),
    applyCam: () => app.applyCam(),
    updateLOD: () => app.updateLOD(),
    setHotPane: (el) => app.setHotPane(el),
    paneEditSource: (q) => REGISTRY.paneEditSource(q),
    // the focus pair are this factory's own: the app object points back at it
    paneFocus: (q) => TILE_WORLD.paneFocus(q),
    paneFullscreen: (q) => TILE_WORLD.paneFullscreen(q),
    paneClose: (q) => TILE_WORLD.paneClose(q),
    togglePin: (q) => TILE_STATE.togglePin(q),
    toggleSelLink: (q) => TILE_STATE.toggleSelLink(q),
    wirePin: (q) => TILE_STATE.wirePin(q),
    wireSel: (q) => TILE_STATE.wireSel(q),
    removePane: (q) => WORLD.removePane(q),
    disposePane: (q) => WORLD.disposePane(q),
    detachFromCluster: (q) => WORLD.detachFromCluster(q),
    relayout: (c) => WORLD.relayout(c),
    addCluster: (ox, oy, kinds, box) => WORLD.addCluster(ox, oy, kinds, box),
    frameBox: (x, y, w, h, label) => WORLD.frameBox(x, y, w, h, label),
    clearFrames: () => WORLD.clearFrames(),
    behaviourMount: (rec) => REGISTRY.behaviourMount(rec),
    userMount: (render) => REGISTRY.userMount(render),
    toast: (m) => app.toast(m),
    save: () => app.scheduleSave(),
    markDirty: () => app.markDirty(),
  }, { panes: app.panes, clusters: app.clusters, cam: app.cam, world: app.world, REG: app.REG });

  /* ---------------- the node graph ---------------- */
  const GRAPH = createGraphRun({
    /* A NAMED VALUE — and the forwarder this asked for did not exist.
     *
     * `js/app/graph-run.js::valueOf` asks `host.valueOf(name)` first for the whole string, so that a
     * pane's own port (`<tileId>_<port>`, a shape its parser cannot read — a colon is not allowed in
     * it) resolves like any other named value. The canvas passed `app.graphValue`, which is defined
     * NOWHERE: `valueOf` threw `app.graphValue is not a function` for any name at all, so a behaviour
     * tile could not be made (`clock`, `track`, `js-node`), and a behaviour program that resolves a
     * value could not run. Measured by making the tile.
     *
     * A name is a port the canvas publishes — the app's own under `<kind>_<port>` (`editor_buffer`,
     * js/host/canvas/services.js::APP_SOURCES) or a pane's under `<tileId>_<port>` (PORT_SRC) — or a
     * node's published output. Nothing else: an unknown name is `undefined`, which lets
     * `graph-run.js` fall through to its own reader instead of failing the call. */
    valueOf: (name) => {
      const src = app.PORT_SRC[name];
      if (src && typeof src.get === 'function') return src.get();
      return GRAPH.results[name];
    },
  });
  GRAPH.start();

  /* ---------------- the themes and the field ---------------- */
  const THEMES_APP = createThemesApp({
    toast: (m) => app.toast(m),
    render: (q) => app.render(q),
    fieldEl: () => env.fieldEl(),
    viewPrefs: () => app.UI.view,
    cam: () => app.cam,
    save: () => app.scheduleSave(),
    diag: (o) => app.diag(o),
    publishVars: (vars) => { GRAPH.results['palette'] = vars; },
    panes: app.panes,
    addFreeTile: (kind) => app.addFreeTile(kind),
    paneFocus: (q) => app.paneFocus(q),
    dynWires: () => app.DYN_WIRES,
    updateWireCount: () => app.updateWireCount(),
  }, { UI: app.UI });

  /* ---------------- the world element ---------------- */
  const WORLD = createWorldHost({
    world: app.world, panes: app.panes, clusters: app.clusters, cam: app.cam,
    NOCLOSE: TILE_WORLD.NOCLOSE,
    makeTile: (id, kind, wr, home, state) => TILE_WORLD.mkPane(id, kind, wr, home, state),
    syncGrips: (p) => app.syncGrips(p),
    dropGrips: (p) => app.dropGrips(p),
    invalidateGrips: () => app.GRIPS.invalidate(),
    refitSplit: (p) => app.refitSplit(p),
    documents: () => app.DOCS.values(),
    updateLOD: () => app.updateLOD(),
    startSashDrag: (c, i, e, el) => WORLD.moveSash(c, i, e, el),
    toast: (m) => app.toast(m),
    save: () => app.scheduleSave(),
    markDirty: () => app.markDirty(),
  });

  /* ---------------- the pane host: the ctx every pane gets ---------------- */
  const PANE_HOST = createPaneHost(canvasServices({
    /* the wire layer's recompute, so an app-port edge can deliver the wire's CURRENT value
       (js/app/pane-host.js::through): the boot assigns `applyInputs` on the app (js/host/canvas/
       boot.js), and this is read when an edge fires, long after both have been built */
    applyInputs: () => app.applyInputs(),
    // the endpoint node for a route name: the one shell capability `ctx.ws` answers here
    endpoint: (name) => app.endpoint(name),
    // the console's save section: the document, and what a save does besides writing it
    saveActiveDoc: () => app.saveActiveDoc(),
    saveMode: () => saveModeOf(app.UI.view && app.UI.view.saveMode),
    setSaveMode: (id) => app.setSaveMode(id),
    EDITOR: app.EDITOR, TREE: app.TREE, lodOn: app.lodOn(), graphRun: GRAPH.run,
    CUSTOM_THEMES: THEMES_APP.customThemes,
    DATA: app.DATA, DIAG: app.DIAG, DYN_WIRES: app.DYN_WIRES, PORT_SRC: app.PORT_SRC,
    /* A FUNCTION, not `app.projects()`. Wiring is built once, before the boot has asked the
       server which projects it serves — a snapshot taken here is the empty list and stays empty,
       which is why the projects pane said "no projects found" over a server that had one
       (observed with the pane in a workspace, not asserted: no shipped workspace holds one).
       `services.projects` reads through this now. */
    projectList: () => app.projects(), REG: app.REG,
    // the snap rules belong to the view, which the runtime attaches after this wiring
    SNAP: { get on() { return app.SNAP && app.SNAP.on; }, set on(v) { if (app.SNAP) app.SNAP.on = v; } },
    THEMES_APP, UI: app.UI,
    WIRES: app.WIRES, clusters: app.clusters,
    addCluster: (ox, oy, kinds, box, reuse, state) => WORLD.addCluster(ox, oy, kinds, box, reuse, state),
    addFreeTile: (kind) => app.addFreeTile(kind),
    addProject: (path) => app.addProject(path),
    forgetProject: (path) => app.forgetProject(path),
    subscribeProjects: (fn) => app.subscribeProjects(fn),
    applyEditorPrefs: () => app.applyEditorPrefs(),
    applyPaneSource: (t, text) => REGISTRY.applyPaneSource(t, text),
    applyTheme: (vars) => THEMES_APP.applyTheme(vars),
    applyThemeModule: (mod) => THEMES_APP.applyThemeModule(mod),
    bbox: () => app.bbox(), camFor: (wr, pad) => app.camFor(wr, pad), cam: app.cam,
    clusterFreeTiles: () => WORLD.clusterFreeTiles(),
    currentThemeMod: () => THEMES_APP.currentThemeMod(),
    currentThemeName: () => THEMES_APP.currentThemeName(),
    defaultView: () => TILE_WORLD.defaultView(),
    docBound: (q, fn) => app.docBound(q, fn), durText: app.durText, esc: (s) => app.esc(s),
    flyToCam: (t, ms) => app.flyToCam(t, ms),
    forgetTheme: (id) => THEMES_APP.forgetTheme(id),
    forkTheme: (mod, name) => THEMES_APP.forkTheme(mod, name),
    hideGuides: () => app.hideGuides(),
    loadMod: (path, load) => app.loadMod(path, load),
    loadProjects: () => app.loadProjects(),
    lodCounts: app.lodCounts(), openFile: (p) => app.openFile(p),
    /* CREATING A NODE: the pane's form asks for this, the app owns the route
       (js/app/node-services.js). It was missing from this scope — the one place a pane's
       capabilities are listed — so `ctx.createNode` was undefined in the canvas and the New Node
       tile fell through to the engine host's socket, which is not there either. */
    createNode: (node) => app.NET.createNode(node),
    openThemeEditor: (p) => THEMES_APP.openThemeEditor(p),
    paneFocus: (q) => app.paneFocus(q), paneSourceOf: (p) => REGISTRY.paneSourceOf(p),
    panes: app.panes, populateKinds: (sel) => app.populateKinds(sel),
    registerTheme: (mod, opts) => THEMES_APP.registerTheme(mod, opts),
    renderControlColumns: (p, sections) => app.renderControlColumns(p, sections),
    renderPane: (q) => app.render(q), rerenderView: () => app.rerenderView(),
    resetLayout: () => app.resetLayout(), runCheck: () => app.CHECKS.check(),
    savePaneSource: (t) => REGISTRY.savePaneSource(t),
    scheduleSave: () => app.scheduleSave(), saveWorkspace: (opts) => app.saveWorkspace(opts),
    setThemeById: (id, opts) => THEMES_APP.setThemeById(id, opts),
    setWireMode: (on) => app.setWireMode(on),
    theField: () => app.theField(), themeCards: () => THEMES_APP.themeCards(),
    themeKeys: () => THEMES_APP.themeKeys(), tidyClusters: () => WORLD.tidyClusters(),
    toast: (m) => app.toast(m), url: (name) => app.url(name),
    routesReady: () => app.ROUTES_READY,
    useProject: (id) => app.useProject(id),
    // lazy: the wire switch belongs to the runtime, which attaches after this wiring
    wireOn: () => (app.wireOn ? app.wireOn() : false),
    zoom100: () => app.zoom100(), zoomNudge: (f) => app.zoomNudge(f),
    drawStats: app.drawStats(),
  }));

  /* ---------------- what the app now knows ---------------- */
  Object.assign(app, {
    TILE_STATE, LIVE, REGISTRY, APP_MOUNTS, TILE_WORLD, GRAPH, THEMES_APP, WORLD, PANE_HOST,
    PANES: REGISTRY.PANES, CTX: REGISTRY.CTX,
    DOC_KINDS: TILE_STATE.DOC_KINDS, LINK_KINDS: TILE_STATE.LINK_KINDS, SYNTAX_KINDS: TILE_STATE.SYNTAX_KINDS,
    linkedSel: (p) => TILE_STATE.linkedSel(p), selOf: (p) => TILE_STATE.selOf(p),
    setSel: (p, id) => TILE_STATE.setSel(p, id), pinTile: (p, on) => TILE_STATE.pinTile(p, on),
    paintChrome: (p) => TILE_STATE.paintChrome(p), wirePin: (p) => TILE_STATE.wirePin(p),
    wireSel: (p) => TILE_STATE.wireSel(p), toggleSelLink: (p) => TILE_STATE.toggleSelLink(p),
    togglePin: (p) => TILE_STATE.togglePin(p),
    paneSourceOf: (p) => REGISTRY.paneSourceOf(p),
    applyPaneSource: (t, text) => REGISTRY.applyPaneSource(t, text),
    savePaneSource: (t) => REGISTRY.savePaneSource(t),
    paneEditSource: (p) => REGISTRY.paneEditSource(p),
    loadUserPanes: () => REGISTRY.loadUserPanes(),
    userMount: (m) => REGISTRY.userMount(m),
    behaviourMount: (rec) => REGISTRY.behaviourMount(rec),
    renderTree: (p) => LIVE.renderTree(p), renderGraph: (p) => LIVE.renderGraph(p),
    renderEditor: (p) => LIVE.renderEditor(p), renderHandwrite: (p) => LIVE.renderHandwrite(p),
    // tiles
    NOCLOSE: TILE_WORLD.NOCLOSE, TILES: TILE_WORLD.TILES,
    mkPane: (id, kind, wr, home, state) => TILE_WORLD.mkPane(id, kind, wr, home, state),
    HOME: TILE_WORLD.HOME, FOCUS: TILE_WORLD.FOCUS, TILE_MOUNT: TILE_WORLD.TILE_MOUNT,
    buildDefaultWorkspace: () => TILE_WORLD.buildDefaultWorkspace(),
    /* the camera the shipped arrangement opens at (js/world/home.js::defaultCam) */
    defaultCam: () => TILE_WORLD.defaultCam(),
    defaultView: () => TILE_WORLD.defaultView(),
    ensureConsole: () => TILE_WORLD.ensureConsole(),
    /* the corpus pair, for the restore: a workspace saved before the pair existed gets one built
       the same way the boot builds it (js/app/boot-workspace.js, js/world/home.js) */
    ensureCorpus: () => TILE_WORLD.ensureCorpus(),
    paneFocus: (p) => TILE_WORLD.paneFocus(p),
    paneFullscreen: (p) => TILE_WORLD.paneFullscreen(p),
    exitFullscreen: () => TILE_WORLD.exitFullscreen(),
    paneSubject: (p) => TILE_WORLD.paneSubject(p),
    paneClose: (p) => TILE_WORLD.paneClose(p),
    render: (p) => TILE_WORLD.renderPane(p),
    mountVisible: () => TILE_WORLD.mountVisible(),
    populateKinds: (into) => TILE_WORLD.populateKinds(into),
    consoleKinds: () => TILE_WORLD.consoleKinds(),
    setConsoleKinds: (k) => TILE_WORLD.setConsoleKinds(k),
    // graph
    graphRun: GRAPH.run, NODE_RUNS: GRAPH.runs, NODE_RESULTS: GRAPH.results,
    runNodeOut: (rec, outs) => GRAPH.out(rec, outs),
    resolveFrom: (from) => GRAPH.valueOf(from),
    recomputeAll: () => GRAPH.cascade(),
    graphAdvance: (force) => GRAPH.advance(force),
    graphTick: () => GRAPH.advance(false),
    // themes
    THEMES: THEMES_APP.THEMES, THEME_MODS: THEMES_APP.themeMods, CUSTOM_THEMES: THEMES_APP.customThemes,
    theField: () => THEMES_APP.theField(), fieldBoot: () => THEMES_APP.fieldBoot(),
    applyFieldPrefs: () => THEMES_APP.applyFieldPrefs(),
    refreshThemeUI: () => THEMES_APP.refreshThemeUI(),
    openThemeEditor: (p) => THEMES_APP.openThemeEditor(p),
    themeKeys: () => THEMES_APP.themeKeys(), currentThemeMod: () => THEMES_APP.currentThemeMod(),
    currentThemeName: () => THEMES_APP.currentThemeName(),
    applyThemeModule: (m) => THEMES_APP.applyThemeModule(m),
    applyTheme: (v) => THEMES_APP.applyTheme(v),
    setThemeById: (id, opts) => THEMES_APP.setThemeById(id, opts),
    themeBoot: () => THEMES_APP.themeBoot(), registerTheme: (m, o) => THEMES_APP.registerTheme(m, o),
    forgetTheme: (id) => THEMES_APP.forgetTheme(id), forkTheme: (m, n) => THEMES_APP.forkTheme(m, n),
    themeCards: () => THEMES_APP.themeCards(), isShipped: (id) => THEMES_APP.isShipped(id),
    // world
    setRect: (p) => WORLD.setRect(p), disposePane: (p) => WORLD.disposePane(p),
    removePane: (p) => WORLD.removePane(p), CLUSTERS: WORLD.CLUSTERS,
    frameBox: (x, y, w, h, l) => WORLD.frameBox(x, y, w, h, l),
    syncFrame: (c) => WORLD.syncFrame(c), relayout: (c) => WORLD.relayout(c),
    addCluster: (ox, oy, kinds, box, reuse, state) => WORLD.addCluster(ox, oy, kinds, box, reuse, state),
    dockInto: (c, p) => WORLD.dockInto(c, p), detachFromCluster: (p) => WORLD.detachFromCluster(p),
    clusterFreeTiles: () => WORLD.clusterFreeTiles(), tidyClusters: () => WORLD.tidyClusters(),
    sashFor: (p, edge) => WORLD.sashFor(p, edge), clusterRect: (c) => WORLD.clusterRect(c),
    clusterUnder: (r) => WORLD.clusterUnder(r), nearestCluster: (p) => WORLD.nearestCluster(p),
    clearFrames: () => WORLD.clearFrames(), moveSash: (c, i, e, el) => WORLD.moveSash(c, i, e, el),
  });
  return app;}
