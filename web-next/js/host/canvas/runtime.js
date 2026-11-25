/*
 * host/canvas/runtime.js — the canvas's runtime: view, input, chrome, wire switch, prefs, storage.
 *
 * This is the half of the assembly that RUNS — the frame loop, the listeners, the camera
 * consequences, the wire switch and the workspace document — as one `attach(app)` that puts its
 * names on the shared app object. The other half (`core.js`) is what the runtime reads: the
 * registry, the tiles, the graph, the themes.
 *
 * The split is by LIFETIME, not by subject. Everything here either lives for the page (listeners,
 * the frame loop) or is rebuilt from the document (the restore, the prefs). Everything in `core`
 * is built once and then mutated (tiles, clusters, the camera, the registry).
 *
 * Three rules that only exist in this half:
 *
 *   ONE FRAME SCHEDULER. The camera, the chrome loop and the fly-to all ask `app.requestFrame`,
 *   so exactly one thing schedules animation frames and a test can drive the loop.
 *
 *   A CAMERA MOVE IS ALSO A GRIP-SIZE CHANGE, and a SOLO tile is pinned to the viewport — the
 *   view module owns those consequences, and this half only hands it the frame scheduler.
 *
 *   A RESTORE IS NOT A PREFERENCE CHANGE. The prefs are applied inside an `applying` window so
 *   the wire switch's own callback does not save the document it is being restored FROM.
 */

import { createView } from './view.js';
import { createWorld } from '../../engine/world.js';
import { createCanvasInput } from '../../world/canvas-input.js';
import { createChrome } from '../../world/chrome.js';
import { createRestore } from '../../world/restore.js';
import { createPersistence } from './persistence.js';
import { applyEditorPrefs as applyEditorPrefsTo, viewState } from '../../app/prefs.js';

/**
 * @param {object} app  the shared app object; this function fills the runtime names on it
 * @param {object} env  { viewport, world, $, lookups… }
 */
export function attachRuntime(app, env) {
  const { cam, panes, clusters, UI } = app;
  const viewport = env.viewport, world = env.world;
  const $ = env.$;
  const requestFrame = env.requestFrame;

  /* ---------------- the world element: one writer for both apps ---------------- */
  /* The canvas's `#viewport > #world` come from the page's markup, so the module ADOPTS them:
     the same `js/engine/world.js` that makes the shell's from scratch owns the pair here too,
     and its `setTransform` is the one place the transform, `--z` and `--inv` are written. The
     shell reaches it through its frame; this app calls it synchronously from `CAM.apply`,
     because its camera moves and its DOM is read in the same tick. */
  const worldHost = createWorld({
    root: viewport, viewportEl: viewport, worldEl: world,
    governor: { requestFrame: (id, cb) => requestFrame(cb) },
    camera: cam,
  });

  /* ---------------- the view: camera, grips, guides, rungs ---------------- */
  const VIEW = createView({
    cam, world, worldHost, panes, clusters,
    viewport: () => app.viewport(),
    viewportWorld: () => app.viewportWorld(),
    el: (id) => $('#' + id),
    field: () => app.theField(),
    chrome: () => CHROME,
    focus: () => app.FOCUS,
    lod: app.lod,
    lodOn: () => app.lodOn(),
    setRect: (q) => app.setRect(q),
    updateLOD: () => VIEW.updateLOD(),
    relayout: (c) => app.relayout(c),
    detachFromCluster: (q) => app.detachFromCluster(q),
    clusterUnder: (r) => app.clusterUnder(r),
    dockInto: (c, q) => app.dockInto(c, q),
    clusterRect: (c) => app.clusterRect(c),
    sashFor: (q, edge) => app.sashFor(q, edge),
    moveSash: (c, i, e, el) => app.moveSash(c, i, e, el),
    paneMin: (q) => app.paneMin(q),
    toast: (m) => app.toast(m),
    save: () => app.scheduleSave(),
    markDirty: () => app.markDirty(),
    requestFrame,
    setStats: (st) => app.setStats(st),
  });

  /* ---------------- the input policy host ---------------- */
  const INPUT = createCanvasInput({
    viewport, world, cam,
    applyCam: () => VIEW.applyCam(),
    stopZoom: () => VIEW.stopZoom(),
    wheel: (px, x, y, pinch) => VIEW.CAM.wheel(px, x, y, pinch),
    viewportSize: () => app.viewport(),
    inputModules: () => app.inputModules(),
    hooks: () => app.inputHooks(),
    scrollCfg: app.scrollCfg,
    zoomNudge: (f) => VIEW.zoomNudge(f),
    flyTo: (t, ms) => VIEW.flyToCam(t, ms),
    camFor: () => VIEW.camFor(VIEW.bbox(), 40),
    onToggleWires: () => app.toast('wires ' + (app.setWireMode() ? 'shown' : 'hidden')),
    // Ctrl/Cmd+S writes the document the editor is following (js/host/canvas/doc-save.js)
    saveDoc: () => app.saveActiveDoc(),
    focus: () => app.FOCUS,
  });
  INPUT.attach();

  /* ---------------- the frame loop, the HUD and the rail ---------------- */
  const CHROME = createChrome({
    el: (id) => $('#' + id),
    panes,
    cam: () => cam,
    dirty: () => app.dirty(),
    clean: () => app.clean(),
    updateLOD: () => VIEW.updateLOD(),
    mountVisible: () => app.mountVisible(),
    recount: () => VIEW.recountDrawStats(),
    stepScrollers: () => INPUT.step(),
    drawWires: (t) => app.drawWires(t),
    placePorts: (show) => { const ports = app.wireHost() && app.wireHost().ports(); if (ports) ports.place(false, show); },
    stats: () => VIEW.stats(),
    nodes: () => ((app.DATA.graph && app.DATA.graph.nodes) || []).length,
    symbols: () => (app.DATA.symbols || []).length,
    updateWireCount: () => app.updateWireCount(),
    applyInputs: () => app.applyInputs(),
    wiresArmed: () => app.wireHost() && app.wireHost().armed(),
    rail: { cursor: app.EDITOR.cursor, file: app.EDITOR.file, dirty: app.EDITOR.dirty },
    chip: () => app.CHECKS.chip(),
    runCheck: () => app.CHECKS.check(),
    requestFrame,
  });
  CHROME.bindRail();

  /* ---------------- the wire switch, the wire count, a new tile ---------------- */
  let wireOn = false;
  let applyingPrefs = false;
  let lastWireLabel = '', lastOpsLabel = '';

  function updateWireCount() {
    const ops = String(Object.keys(app.NODE_RESULTS).length);
    if (ops !== lastOpsLabel) { lastOpsLabel = ops; const o = $('#h-ops'); if (o) o.textContent = ops; }
    // the resolved wire list is the only honest source, and it exists once the layer arms
    let n = 0;
    try { n = app.resolvedWires().length; } catch (e) { n = 0; }
    const w = String(n);
    if (w !== lastWireLabel) { lastWireLabel = w; const el = $('#h-wires'); if (el) el.textContent = w; }
  }

  /** The canvas layer draws the real wires, so the button and `w` only raise or lower it
   *  (no second, dashed overlay). */
  function setWireMode(on) {
    const ports = app.wireHost() && app.wireHost().ports();
    const next = (on === undefined && ports) ? !ports.wiring : !!on;
    if (ports) ports.setWiring(next);
    wireOn = next;
    const b = $('#b-wires');
    if (b) b.classList.toggle('on', wireOn);
    document.body.classList.toggle('wire-mode', wireOn);
    if (!applyingPrefs) { UI.view.wireMode = wireOn; app.scheduleSave(); }
    return wireOn;
  }

  const addFreeTile = (kind) => app.TILES.free(kind).id;

  /* ---------------- preferences ---------------- */
  function applyEditorPrefs() {
    return applyEditorPrefsTo(document.documentElement.style, app.EDITORS, UI.editor);
  }
  function applyViewPrefs() {
    const v = viewState(UI);
    applyingPrefs = true;              // a restore is not a preference change: do not save it
    app.setLod(v.lod);
    VIEW.SNAP.on = v.snap;
    setWireMode(v.wireMode);
    applyingPrefs = false;
    return v;
  }

  /* ---------------- the workspace document ---------------- */
  const RESTORE = createRestore({
    panes, clusters,
    dispose: (p) => app.disposePane(p),
    detachEl: (p) => p.el.remove(),
    clearFrames: () => app.clearFrames(),
    resetTiles: () => app.TILES.reset(),
    known: (kind) => !!app.PANES[kind],
    addCluster: (ox, oy, kinds, box, reuse, state) => app.addCluster(ox, oy, kinds, box, reuse, state),
    make: (id, kind, wr, home, state) => app.mkPane(id, kind, wr, home, state),
    relayout: (c) => app.relayout(c),
    treeFromJSON: (t) => (STORE ? STORE.treeFromJSON(t) : null),
    docOf: (path, text) => app.docOf(path, text),
    hasDoc: (path) => app.DOCS.has(path),
    setActive: (path) => app.setActive(path),
    cam,
    clampZ: (z) => Math.min(Math.max(z, .04), 5),
    applyCam: () => VIEW.applyCam(),
    updateLOD: () => VIEW.updateLOD(),
    /* The theme host, by name: `restore` is the one that knows what a saved workspace
       recorded (an id, a name, a bare palette) and it is also the one that HOLDS a
       restore that arrives before the theme modules are fetched. The extraction left
       this pointing at a method the page used to have (`app.restoreTheme`) and nothing
       assigned, so a saved workspace that carried a theme threw here instead. */
    theme: (saved) => app.THEMES.restore(saved),
    toast: (m) => app.toast(m),
  });
  const restorePanes = (saved) => RESTORE.apply(saved);

  const STORE = createPersistence({
    panes, clusters, cam, UI, DOCS: app.DOCS,
    safe: app.SAFE,
    theField: () => app.theField(),
    THEMES: app.THEMES,
    wrOf: (p) => app.FOCUS.wrOf(p),
    activeDoc: () => app.activeDoc(),
    url: (name) => app.url(name),
    fetch: (u, init) => app.fetch(u, init),
    projectKey: () => app.projectKey(),
    routesReady: () => app.ROUTES_READY,
    refreshProjects: () => app.loadProjects(),
    mirrorKey: app.LS_KEY,
    mirror: app.mirror,
    restore: (layout) => restorePanes(layout),
    applyEditorPrefs: () => applyEditorPrefs(),
    applyViewPrefs: () => applyViewPrefs(),
    render: (q) => app.render(q),
    toast: (m) => app.toast(m),
    ensureConsole: () => app.ensureConsole(),
    ensureCorpus: () => app.ensureCorpus(),
    loadText: (path) => app.loadText(path),
    setActive: (path) => app.setActive(path),
    reload: () => app.reload(),
    dynWires: () => app.DYN_WIRES,
    wireLayer: () => app.wireLayer(),
  });

  /* ---------------- what the app now knows ---------------- */
  Object.assign(app, {
    VIEW, INPUT, CHROME, RESTORE, STORE,
    // camera and grips, by the names the rest of the app and the debug surface use
    SNAP: VIEW.SNAP, GRIPS: VIEW.GRIPS, GESTURES: VIEW.GESTURES, CAM: VIEW.CAM,
    GRIP_EDGES: VIEW.GRIP_EDGES, GRIP: VIEW.GRIP, ZMIN: VIEW.ZMIN, ZMAX: VIEW.ZMAX,
    applyCam: () => VIEW.applyCam(),
    zoomAt: (mx, my, f) => VIEW.zoomAt(mx, my, f),
    stopZoom: () => VIEW.stopZoom(),
    zoomNudge: (dz) => VIEW.zoomNudge(dz),
    zoom100: () => VIEW.zoom100(),
    camFor: (wr, pad, maxZ) => VIEW.camFor(wr, pad, maxZ),
    /* the camera a first render gets (js/app/viewport.js::firstViewCam): the boot and the default
       view both ask for it by name, and this list is where the app's camera API is assembled */
    firstView: (wr, pad) => VIEW.firstView(wr, pad),
    bbox: () => VIEW.bbox(),
    flyToCam: (t, ms, done) => VIEW.flyToCam(t, ms, done),
    fitRect: (wr, pad, ms) => VIEW.fitRect(wr, pad, ms),
    gripAt: () => VIEW.gripAt(),
    gripBand: (r) => VIEW.gripBand(r),
    syncGrips: (q) => VIEW.syncGrips(q),
    mountGrips: (q) => VIEW.mountGrips(q),
    dropGrips: (q) => VIEW.dropGrips(q),
    snapPos: (q, x, y, g) => VIEW.snapPos(q, x, y, g),
    snapResize: (q, r, edge, g) => VIEW.snapResize(q, r, edge, g),
    snapTargets: (q) => VIEW.snapTargets(q),
    showGuides: (l) => VIEW.showGuides(l),
    hideGuides: () => VIEW.hideGuides(),
    routeToSash: (e, q, edge) => VIEW.routeToSash(e, q, edge),
    updateLOD: () => VIEW.updateLOD(),
    recountDrawStats: () => VIEW.recountDrawStats(),
    visibleOnly: () => VIEW.visibleOnly(),
    setVisibleOnly: (on) => VIEW.setVisibleOnly(on),
    // input
    setHotPane: (el) => INPUT.setHotPane(el),
    stepScrollers: () => { INPUT.step(); },
    input: () => INPUT,
    frame: (t) => CHROME.frame(t),
    startFrameLoop: () => CHROME.start(),
    paintChip: () => CHROME.paintChip(),
    // wire switch and the rest
    setWireMode, wireOn: () => wireOn, updateWireCount, addFreeTile,
    applyEditorPrefs, applyViewPrefs,
    restorePanes,
    collectWorkspace: () => STORE.documentOf(),
    serializeLayout: () => STORE.layoutOf(),
    applyWorkspace: (doc) => STORE.apply(doc),
    scheduleSave: () => STORE.scheduleSave(),
    saveWorkspace: (opts) => STORE.save(opts),
    resetLayout: () => STORE.reset(),
    loadSettings: () => STORE.loadSettings(),
    treeToJSON: (n) => STORE.treeToJSON(n),
    treeFromJSON: (j) => STORE.treeFromJSON(j),
    pendingWires: () => STORE.pendingWires(),
    clearPendingWires: () => STORE.clearPendingWires(),
  });
  return app;}
