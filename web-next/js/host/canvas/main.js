/*
 * host/canvas/main.js — the canvas app's entry: everything that assembles it.
 *
 * The page that used to hold this is now 33 lines: a #viewport, a #rail, a toast, a wire canvas
 * and one script tag. This module is what that script loads, and it is also the canvas's half of
 * the convergence: `js/main.js` is the ONE entry, it looks up the DOM, and it calls `bootCanvas`
 * here when the page is the canvas (or the engine's boot when the page is the shell).
 *
 * The assembly is three passes over one app object — `core` (state and helpers), `wiring` (the
 * object graph: registry, tiles, graph, themes, clusters) and `runtime` (what runs: view, input,
 * chrome, storage) — and then a boot that starts the loop and restores the workspace. The order
 * is load-bearing: a pass may only READ what an earlier pass has put on the app, and anything a
 * later pass owns is asked for through a lambda.
 *
 * Element lookups, the frame scheduler and browser storage are the ENTRY's business, not this
 * module's: they arrive in `env`, which is what makes one module the only place that touches
 * them.
 */

import { esc, clamp, debounce, durText } from '/js/app/util.js';
import * as LOD from '/js/app/lod.js';
import { ctlRow, ctlSec, ctlNote, ctlBtn, ctlPill, ctlCheck, ctlGroup } from '/js/app/ctl.js';
import { loadKatex, mathHtml, proseHtml } from '/js/app/math.js';
import { createPaneHost } from '/js/app/pane-host.js';
import { tilesToLayout } from '/js/app/tiles-to-layout.js';
import { createDocs } from '/js/app/docs.js';
import { createSaver } from '/js/app/workspace-save.js';
import { resolveThemeIdentity } from '/js/app/themes.js';
import { resolveValue, portsOf as portsOfRec, typeOfPort as typeOfPortRec, pickPort as pickPortRec,
         expandWires } from '/js/app/wires.js';
import { autoTree as buildTree, solveRects, clusterUnder as underCluster, nearestCluster as nearCluster,
         kindOrder as tilingOrder, kindWeight as tilingWeight, GAP as TILE_GAP } from '/js/app/tiling.js';
import { serializeLayout as serializeLayoutDoc, collectWorkspace as collectWorkspaceDoc } from '/js/app/layout-doc.js';
import { mountOf as userMountOf, behaviourMount as behaviourMountOf, loadManifests, portsOf } from '/js/panes/user-panes.js';
import { SNAP, snapBand, spanGap, snapMove, snapEdge } from '/js/app/snap.js';
import { createCameraHost } from '/js/app/camera.js';
import { createConsole } from '/js/world/console.js';
import { createGrips, GRIP_EDGES as GRIP_EDGE_LIST, GRIP as GRIP_BASE } from '/js/world/grips.js';
import { actionsFor, buildShell } from '/js/world/tile-shell.js';
import { disposeTile, removeTile } from '/js/world/tile-life.js';
import { createClusters, FRAME_PAD, INNER as CLUSTER_INNER } from '/js/world/clusters.js';
import { createTileGestures, RESIZE_MIN } from '/js/world/tile-gestures.js';
import { createRestore } from '/js/world/restore.js';
import { createThemeHost } from '/js/live/theme-host.js';
import { createRoutes } from '/js/net/routes.js';
import { createGraphRun } from '/js/app/graph-run.js';
import { createWireHost } from '/js/live/wire/host.js';
import { createCanvasInput } from '/js/world/canvas-input.js';
import { createHome } from '/js/world/home.js';
import { createFieldHost } from '/js/live/field-host.js';
import { createChrome } from '/js/world/chrome.js';
import { createView } from '/js/host/canvas/view.js';
import { createLivePanes } from '/js/panes/app-live.js';
import { createTileState } from '/js/host/canvas/tile-state.js';
import { createData } from '/js/host/canvas/data.js';
import { createPersistence } from '/js/host/canvas/persistence.js';
import { createTiles } from '/js/host/canvas/tiles.js';
import { createWorldHost } from '/js/host/canvas/world.js';
import { createRegistry } from '/js/host/canvas/panes.js';
import { createThemesApp } from '/js/host/canvas/themes-app.js';
import { canvasServices } from '/js/host/canvas/services.js';
import { createNet } from '/js/host/canvas/net-app.js';
import { attachRuntime } from '/js/host/canvas/runtime.js';
import { attachWiring } from '/js/host/canvas/wiring.js';
import { createMirror } from '/js/core/storage.js';
import { realDeps } from '/js/core/frame.js';

/** The browser's frame scheduler, from the one module that owns it. */
const FRAME = realDeps();
import { attachCore } from '/js/host/canvas/core.js';
import { bootCanvasApp } from '/js/host/canvas/boot.js';
import { SAFE, DATA, REG, UI, EDITORS, LS_KEY, panes, clusters, cam, DYN_WIRES, PORT_SRC, WIRES,
  loadRegistry as stateLoadRegistry } from '/js/host/canvas/state.js';
import { createTileMount } from '/js/world/tile-mount.js';
import { bootWorkspace } from '/js/app/boot-workspace.js';
import { applyEditorPrefs as applyEditorPrefsTo, viewState } from '/js/app/prefs.js';
import { createDiagnostics, diagPath } from '/js/app/diagnostics.js';
import { sourceOf, applySource, saveSource } from '/js/panes/source-of.js';
import { createAppMounts } from '/js/panes/app-mounts.js';
import { createTileFactory } from '/js/world/tile-create.js';
import { createFocus } from '/js/world/focus.js';
import { ZMIN as VP_ZMIN, ZMAX as VP_ZMAX, screenRect as screenRectOf, worldAt as worldAtOf,
         onScreen as rectOnScreen, bboxOf, camForRect, zoomAbout, viewportWorld as viewportWorldOf } from '/js/app/viewport.js';
/* ---- safe mode: `?safe=1` looks but does not touch ------------------------------
   The workspace is saved to local storage AND posted to the server, so a layout that
   has been dragged into a state the user cannot recover from is also the layout that
   gets restored on every reload. Safe mode is the way out: the page still restores
   NOTHING (it opens on the default workspace) and still SAVES nothing (neither the
   local mirror nor the server), so whatever is stored survives intact and can be
   repaired in the layout pane without the running page overwriting it. */
/**
 * Boot the canvas app into the elements the entry looked up.
 * @param {object} env  { $, viewport, world, grid, fieldEl, requestFrame }
 */
export function bootCanvas(env) {
const $ = env.$, viewport = env.viewport, world = env.world, fieldEl = env.fieldEl;
let vw = innerWidth, vh = innerHeight;

/* The input policy loads as modules (js/input/*) and these two are how the runtime asks for
   it: both are null until the boot's import lands, and the canvas works without them. */
let IN = null;                       // the js/input modules
let wheelHooks = null;               // pane/plugin wheel overrides, consulted first
const SCROLL_CFG = { notchPx:96, notchEaseMs:120, accel:{windowMs:200,perEvent:0.04,max:1.8}, lineHeightPx:18, snapToLine:true };

/* The canvas assembles in three passes over one app object. This is the FIRST — state and
   helpers (documents, corpus, server, console) — in js/host/canvas/core.js. */
async function loadMod(path, load){
  for(let attempt = 0; attempt < 2; attempt++){
    try { return await load(); }
    catch(e){ if(attempt === 1) console.warn('[module]', path, (e && e.message) || e); }
  }
  return null;
}

/** The world rect a tile needs to cover the viewport exactly. */
const viewportWorld = () => viewportWorldOf(cam, vw, vh);

const APP = {
  cam, panes, clusters, lod: LOD, esc: (x) => esc(x), durText: durText,
  /* the wire layer, the debug sink and the browser mirror: the three things the runtime asks
     for that are the page's own (the layer arms after the boot; the mirror is the browser) */
  diag: (o) => Object.assign(window.__diag || (window.__diag = {}), o),
  mirror: createMirror(LS_KEY),
  reload: () => location.reload(),
  fetch: (u, init) => fetch(u, init),
  /* THE HOST'S OWN CAPABILITIES, asked for by name rather than assembled in a pane: the
     handwriting pane wants the OCR of a canvas, and the editor's Enter wants what bridges two
     expressions. Neither pane knows a route — `js/defaults/endpoints.js` is the only place a
     route string lives — and both are attached HERE, because a capability that is left as a
     module function is a button that says `undefined is not a function`. */
  recognizeHandwriting: (canvas) => recognizeHandwriting(canvas),
  autocite: (lhs, rhs) => APP.NET.autocite(lhs, rhs),
  rows: (content) => APP.NET.rows(content),
  cite: (name) => APP.NET.cite(name),
  steps: (expr, limit) => APP.NET.steps(expr, limit),
  loadMod: (path, load) => loadMod(path, load),
  viewportWorld: () => viewportWorld(),
  inputModules: () => IN,
  inputHooks: () => wheelHooks,
  scrollCfg: SCROLL_CFG,
  setInputModules: (mods, hooks) => { IN = mods; wheelHooks = hooks; },
  setScrollCfg: (cfg) => { Object.assign(SCROLL_CFG, cfg); },
};
attachCore(APP, {
  $, viewport, world,
  viewportSize: () => ({ w: vw, h: vh }),
  minSize: { w: 220, h: 150 },
  requestFrame: (cb) => { const id = FRAME.raf(cb); return () => FRAME.caf(id); },
});
/* the document ports and the helpers the rest of this file reads */
const EDITOR = APP.EDITOR, TREE = APP.TREE, DOCS = APP.DOCS;
const docOf = APP.docOf, docEdited = APP.docEdited, setActive = APP.setActive, loadText = APP.loadText;
const showDoc = APP.showDoc, watchActive = APP.watchActive, docBound = APP.docBound;
const docText = APP.docText, docPath = APP.docPath, activeDoc = APP.activeDoc;
const loadRegistry = APP.loadRegistry;
const splitRange = APP.splitRange, refitSplit = APP.refitSplit, paneMin = APP.paneMin;
addEventListener('resize', ()=>{ vw=innerWidth; vh=innerHeight; dirty = true; });

/* A tile's own state — its pin, its selection, and the three kind tables that decide which
   tiles have either — is js/host/canvas/tile-state.js. What stays here is the docs wiring it
   reads and the names the rest of the page calls. */

/* docBound, docText and docPath come from js/app/docs.js (see the top of this module) */
// typed wiring: editor out → consumer module in (the visible logic layer)
/* wires drawn by the canvas layer rather than declared here: one list, counted by the
   console pane and by the wires tile, so it has to exist before either mounts */
/* lazy-load KaTeX (vendored at /vendor/katex.min.js+.css) */
/* KaTeX arrives on first use — js/app/math.js owns the load and its promise. */
/* ---------- a .qbit line has a ROLE, and the role decides how it is read ------
   Every line used to be handed to KaTeX as DISPLAY math unless it was a comment or
   a short title, so `Math: The ledger order …` — a sentence about the node — was
   typeset as a formula, and `Sema:` alone (whose formula is on the NEXT line) was
   typeset as an empty formula. The file says which is which by its shape:

     `A1. Realm`            the node header: a label and a name
     `Sema:`                a label whose body runs on below it
     `Type: \tau \to …`     a label with a value, and the value is mathematics
     `Math: …`              a label with a value, and the value is prose
     `  = a \Delta b  [A1.3]` a proof row: mathematics plus its justification
     `\forall a b : …`      a statement: mathematics, unlabelled

   The label table is the corpus's own (329 `Math:`, 172 `Proof:`, 85 `Type:`,
   65 `Sema:`), and anything unlabelled is classified by what it contains, so a new
   label degrades to "look at the text" rather than to "assume formula". */
/* The role table lives in js/app/qbit-lines.js — pure, so a node-only check reads it
   without a browser. Imported statically at the
   top of this module, so it is in hand before the first render. What stays here is
   the DOM: the renderer that turns a role into elements. */

/* mathHtml and proseHtml live in js/app/math.js now — the text-to-markup half of
   the render tile, with the "a span KaTeX cannot read becomes code, not nothing"
   rule and the lazy KaTeX load. */
/* The corpus loader, the server-facing app and the console splitter are wired in
   js/host/canvas/core.js; these are the names this file still reads. */
const toast = APP.toast, setLoad = APP.setLoad, fetchJSON = APP.fetchJSON;
const openFile = APP.openFile;
const projectKey = APP.projectKey, loadProjects = APP.loadProjects;
const useProject = APP.useProject, addProject = APP.addProject, forgetProject = APP.forgetProject,
  subscribeProjects = APP.subscribeProjects;
/* the chip is painted by the chrome, from the words and tone the check module gives it */
function paintCheckChip(){ CHROME.paintChip(); }
function paintDiag(){ for(const p of panes) if(p.kind === 'diag' && p.chrome) p.chrome(); }


/** Load a module this page only needs once something has asked for it.
 *
 *  A dynamic import REJECTS — on a network blip, on a document that is being torn down
 *  by a navigation, on a file that will not parse — and `await import(...)` with no
 *  catch turns that into an unhandled promise rejection and a tile that never finishes
 *  rendering. Neither is a good answer to "the editor did not load": the tile is worth
 *  something with a plain textarea in it, and the failure is worth one line in the
 *  console rather than a rejection nobody sees.
 *
 *  So every lazy import in this page goes through here. One attempt, one retry (the
 *  first failure is usually the tap that was still opening), and `null` as the answer
 *  instead of a throw — the caller already knows how to be a worse editor, and this is
 *  what lets it.
 *
 *  The module is passed as a THUNK (`() => import('...')`) rather than a path: the
 *  literal has to stay in the source, because tools/dead-modules.mjs decides what is
 *  reachable by matching `import('...')`, and a computed `import(path)` reads as "nothing
 *  imports this file" — which would have listed the editor as dead the moment it was
 *  retried. */

/* ---------- pane kinds ---------- */
/* The four canvas panes that are NOT pane modules — the tree, the graph, the editor and the
   handwriting pad — keep their own wrappers (they need a LIVE object, not a render function),
   and those wrappers are js/panes/app-live.js. What stays here is the app they plug into. */

/* the Stats tile is js/panes/stats.js: the corpus numbers from ctx.data, the workspace's
   own size from ctx.counts, and the kind order the corpus itself uses */


/* every glyph is derived from the file text — there is no privileged symbol table,
   so a macro the engine has never heard of still shows up with its count */
function glyphsFromText(text){
  // glyphs are derived purely from the file text — no privileged symbol table
  const m=new Map(), re=/\\[a-zA-Z]+/g; let x;
  while((x=re.exec(text))){ const g='\\'+x[0].slice(1); m.set(g,(m.get(g)||0)+1); }
  return [...m.entries()].map(([g,n])=>({g,n})).sort((a,b)=>b.n-a.n||a.g.localeCompare(b.g));
}

/* the Symbols tile is js/panes/symbols.js: it follows the tree's selection first and the
   active file second, and derives every glyph from the text it is given */


/* the Goal tile is js/panes/goal.js: the host fills its cursor/file ports from the app
   (by name) and from this tile's own document, and the pane draws itself */

/* the Output tile is js/panes/output.js now: the page keeps the mount call and the
   summary, the pane keeps its own markup */

/* the Render tile is js/panes/render.js: the document comes in on its buffer/file ports,
   the maths switch is its own port, and the line classification lives with the pane */


/* computed module: consumes editor.buffer → /api/parse → IR/AST */
/* the IR tile is js/panes/ir.js: the buffer comes in on its port, the parse goes out to
   /api/parse, and only the newest answer is allowed to land */


/* the Lean tile is js/panes/lean.js: read-only by design — this is what the engine
   verifies, so typing over it would be a lie about what is checked */


/* Handwriting recognition is a wasm build loaded as a GLOBAL script (var createLatexOCR),
   not an ES module — so the loading and the calling are the page's, and the pane's own
   markup is not. */
async function recognizeHandwriting(canvas){
  if (typeof window.createLatexOCR !== 'function') {
    await new Promise((res, rej) => {
      const s = document.createElement('script');
      s.src = '/wasm/latexocr/latexocr.js';
      s.onload = res;
      s.onerror = () => rej(new Error('latexocr script failed'));
      document.head.appendChild(s);
    });
  }
  const hocr = await window.createLatexOCR({ locateFile: (f) => '/wasm/latexocr/' + f });
  const dataUrl = canvas.toDataURL('image/png');
  const res = hocr && typeof hocr.recognize === 'function' ? await hocr.recognize(dataUrl)
    : (hocr && typeof hocr.recognizeImage === 'function' ? await hocr.recognizeImage(dataUrl) : null);
  if (typeof res === 'string' && res.trim()) return '\\' + res.trim().replace(/^\\/, '');
  return '✓ strokes captured';
}

/* ---------- the console, as panes ----------
   These controls used to live in a panel pinned to the corner of the window. They
   are tiles now: the same rows and the same ids, but they belong to the workspace
   like everything else, so they can be moved, docked, re-tiled — and closed, all
   except `core`, which is the workspace's own control surface. */
/* the widget builders are in js/app/ctl.js — ctlRow, ctlSec, ctlNote, ctlBtn,
   ctlPill, ctlCheck, ctlGroup, imported by name above */

/* the Core tile is js/panes/core.js: create, cluster, save, run — and the keyboard the
   canvas listens to. It is the tile that cannot be closed, so it says how to steer. */


/* ---------- the console splitter ----------
   One settings pane is one column until you ask for two; the arrangement — measuring the
   sections, placing the seam, remembering the fraction — is js/world/console.js, and the
   arithmetic under it is js/app/split.js (with node tests). What is here is the wiring: the
   camera it measures in, the store the fractions live in, and the two things it needs from
   the page (re-render this tile, save the workspace). */
/* the View tile is js/panes/view.js: the camera, the tile defaults, the theme and the
   field, all read back from the app on every paint */

/** Re-render the view tile in place: it is the one pane that shows state it also changes. */
function rerenderView(){
  const v = panes.find((q) => q.kind === 'view');
  if (v && v.refreshPane) v.refreshPane();
}

/* the field picker: one button per blueprint space, pressed for the one in use */
function fieldButtons(){
  const spaces = THEMES_APP.fieldSpaces();
  const cur = theField() ? theField().space : (UI.view.field || 'graph');
  return spaces.map((s)=>{
    const b=ctlBtn('f-'+s.id, String(s.name||s.id).toLowerCase(), s.note || ('blueprint: '+s.name));
    b.className='seg'; b.dataset.field=s.id; b.setAttribute('aria-pressed', String(cur===s.id));
    b.addEventListener('click',()=>{
      UI.view.field = s.id;
      if(theField()) theField().setSpace(s.id);
      scheduleSave();
      const v=panes.find(q=>q.kind==='view'); if(v) renderPane(v);
      toast('field: '+(s.name||s.id));
    });
    return b;
  });
}

/* prefs: how the code in an editor tile is drawn */
/* the Editor-prefs tile is js/panes/prefs.js: five knobs — font, line height, tab, wrap and the
   check strip's `suggest rewrites` — one set of values for every editor in the workspace, arranged
   by the page's splitter */


/* ---------- a pane's own source ----------
   "edit source" is about the module: the program a behaviour node runs, the code or
   view a data tile was declared with, or — for the app's own kinds — the module
   file that implements them. The corpus file a tile is *about* is the editor's
   business, not this one's. */
/* Which source a tile has, and what may be done with it, is js/panes/source-of.js: a
   manifest pane's declaration is editable (and saveable, because it is data), an app module
   is opened read-only. What stays here is the shape of the page's registry and the one tile
   that shows a source. */
/* The tile that SHOWS a source: a code view over one pane's own source, read-only for an
   app module and live for a manifest pane (the pane module does the drawing). */

/* The wire layer's own switch. `show` is a boolean input, so a Constant or a
   program node can throw it; with nothing wired, the checkbox here is in charge. */
/* the Wires tile is js/panes/wires.js: the switch belongs to the pane, the wire layer it
   switches belongs to the page, and the tile's own wired input is handed to its port */


/* The pane host: what a pane module is allowed to know about this page. Built once,
   right before the registry that mounts through it — so a pane becomes a file in
   web-next/js/panes and the page stops growing one function per kind. */
DATA.atomOrder = REG.atoms.order;   // the corpus's own order for the composition bar
/* ---------- PaneRegistry: a pane is a manifest + a handler (userspace-extensible).
   Built-in panes register here; user panes (panes/manifests.json) register at boot.
   The engine is generic: titles, add-tile menu and rendering all key off the registry. ---- */



/* The canvas's own object graph — registry, mount table, tile factory, node graph, theme
   machine, clusters — is js/host/canvas/wiring.js: one `attachWiring(app, env)` that puts its
   names on the app object, and the aliases below are the names this file still reads. */
attachWiring(APP, { $, fieldEl, loadKatex });
/* the aliases the assembly and the debug surface still read by name */
const consoleHost = APP.consoleHost, paneBodyW = APP.paneBodyW, renderControlColumns = APP.renderControlColumns;
const ROUTES = APP.ROUTES, CHECKS = APP.CHECKS, diagCounts = APP.diagCounts;
const GRAPH = APP.GRAPH;
const paintChrome = APP.paintChrome, renderTree = APP.renderTree, populateKinds = APP.populateKinds;
const runNodeOut = APP.runNodeOut, graphAdvance = APP.graphAdvance;
const refreshThemeUI = APP.refreshThemeUI, themeKeys = APP.themeKeys;
const openThemeEditor = APP.openThemeEditor;
const forgetTheme = APP.forgetTheme, registerTheme = APP.registerTheme, setThemeById = APP.setThemeById;
const applyThemeModule = APP.applyThemeModule, themeBoot = APP.themeBoot, forkTheme = APP.forkTheme;
const frameBox = APP.frameBox, gripAt = APP.gripAt, showGuides = APP.showGuides, setHotPane = APP.setHotPane;
const PANE_HOST = APP.PANE_HOST;
const REGISTRY = APP.REGISTRY, PANES = APP.PANES, CTX = APP.CTX;
const LIVE = APP.LIVE, TILE_STATE = APP.TILE_STATE, TILE_WORLD = APP.TILE_WORLD;
const DOC_KINDS = APP.DOC_KINDS, LINK_KINDS = APP.LINK_KINDS, SYNTAX_KINDS = APP.SYNTAX_KINDS;
const linkedSel = APP.linkedSel, selOf = APP.selOf, setSel = APP.setSel, pinTile = APP.pinTile;
const toggleSelLink = APP.toggleSelLink, togglePin = APP.togglePin;
const paneSourceOf = APP.paneSourceOf, applyPaneSource = APP.applyPaneSource, savePaneSource = APP.savePaneSource;
const paneEditSource = APP.paneEditSource, loadUserPanes = APP.loadUserPanes, userMount = APP.userMount;
const behaviourMount = APP.behaviourMount;
const NOCLOSE = APP.NOCLOSE, TILES = APP.TILES, mkPane = APP.mkPane, HOME = APP.HOME, FOCUS = APP.FOCUS, TILE_MOUNT = APP.TILE_MOUNT;
const buildDefaultWorkspace = APP.buildDefaultWorkspace, defaultView = APP.defaultView, ensureConsole = APP.ensureConsole;
const paneFocus = APP.paneFocus, paneFullscreen = APP.paneFullscreen, exitFullscreen = APP.exitFullscreen;
const paneSubject = APP.paneSubject, paneClose = APP.paneClose, renderPane = APP.render, mountVisible = APP.mountVisible;
let CONSOLE_KINDS = APP.consoleKinds();
const graphRun = APP.graphRun, NODE_RUNS = APP.NODE_RUNS, NODE_RESULTS = APP.NODE_RESULTS;
const THEMES = APP.THEMES, THEME_MODS = APP.THEME_MODS, CUSTOM_THEMES = APP.CUSTOM_THEMES;
const theField = APP.theField, fieldBoot = APP.fieldBoot, applyFieldPrefs = APP.applyFieldPrefs;
const setRect = APP.setRect, disposePane = APP.disposePane, removePane = APP.removePane, CLUSTERS = APP.CLUSTERS;
const relayout = APP.relayout, syncFrame = APP.syncFrame, gripBand = APP.gripBand, gripAt2 = APP.gripAt;
const hideGuides = APP.hideGuides, spanGap = APP.spanGap, snapBand = APP.snapBand;
const dockInto = APP.dockInto, detachFromCluster = APP.detachFromCluster, clusterFreeTiles = APP.clusterFreeTiles;
const tidyClusters = APP.tidyClusters, sashFor = APP.sashFor, clusterRect = APP.clusterRect;
const clusterUnder = APP.clusterUnder, nearestCluster = APP.nearestCluster;

/* ---------- the runtime: view, input, chrome, wire switch, prefs, storage ----------
   The half of the assembly that RUNS — the frame loop, the listeners, the camera's consequences,
   the wire switch and the workspace document — is js/host/canvas/runtime.js. It puts its names on
   this app object, and the aliases below are the ones the rest of this file still reads. */
attachRuntime(APP, {
  viewport, world, $,
  requestFrame: (cb) => { const id = FRAME.raf(cb); return () => FRAME.caf(id); },
});
/* the names the rest of this file still reads, now owned by the runtime */
const VIEW = APP.VIEW, INPUT = APP.INPUT, CHROME = APP.CHROME, RESTORE = APP.RESTORE, STORE = APP.STORE;
const SNAP = APP.SNAP, GRIPS = APP.GRIPS, GESTURES = APP.GESTURES, CAM = APP.CAM;
const GRIP_EDGES = APP.GRIP_EDGES, GRIP = APP.GRIP, ZMIN = APP.ZMIN, ZMAX = APP.ZMAX;
const applyCam = APP.applyCam, zoomAt = APP.zoomAt, stopZoom = APP.stopZoom, zoomNudge = APP.zoomNudge;
const zoom100 = APP.zoom100, camFor = APP.camFor, bbox = APP.bbox, flyToCam = APP.flyToCam, fitRect = APP.fitRect;
const snapPos = APP.snapPos, snapResize = APP.snapResize, snapTargets = APP.snapTargets;
const updateLOD = APP.updateLOD, recountDrawStats = APP.recountDrawStats, visibleOnlyNow = APP.visibleOnly;
const resolveFrom = APP.resolveFrom, graphTick = APP.graphTick;
const frame = APP.frame;
const setWireMode = APP.setWireMode, updateWireCount = APP.updateWireCount, addFreeTile = APP.addFreeTile;
const applyEditorPrefs = APP.applyEditorPrefs, applyViewPrefs = APP.applyViewPrefs;
const restorePanes = APP.restorePanes;
const collectWorkspace = APP.collectWorkspace, serializeLayout = APP.serializeLayout, applyWorkspace = APP.applyWorkspace;
const scheduleSave = APP.scheduleSave, saveWorkspace = APP.saveWorkspace, resetLayout = APP.resetLayout;
const loadSettings = APP.loadSettings, treeToJSON = APP.treeToJSON, treeFromJSON = APP.treeFromJSON;

window.__spike={cam,panes,clusters,applyCam,camFor,flyToCam,bbox,relayout,updateLOD,addFreeTile,projectKey,data:DATA,PANES,WIRES,NODE_RESULTS,PORT_SRC,resolveFrom,NODE_RUNS,graphRun,serializeLayout,
  /* THE SAME WORKSPACE, IN THE ENGINE'S DOCUMENT SHAPE (js/app/tiles-to-layout.js): tiles become
     roots, clusters become roots holding their tiling tree, and the `mode` says `canvas`. It is the
     bridge a `mode:'canvas'` boot of the engine runs, and the thing to compare against when the two
     documents have to agree — a canvas workspace is expressible as a layout document, which is what
     makes "the canvas is a layout mode of the engine" a statement about the model. */
  layoutDocument: () => tilesToLayout({ panes, clusters, cam, id: 'canvas:' + projectKey() }),
  loadSettings,restorePanes,setWireMode,SNAP,snapPos,snapResize,snapBand,spanGap,hideGuides, autoTree: buildTree, clustersAPI: CLUSTERS, dockInto,clusterUnder,clusterFreeTiles,tidyClusters,zoomNudge,zoom100,zoomAt,stopZoom,ZMIN,ZMAX, zooming:()=>CAM.moving, camHost: CAM, worldAt:(sx,sy)=>({x:(sx-cam.x)/cam.z,y:(sy-cam.y)/cam.z}), UI, scheduleSave, updateWireCount, collectWorkspace, removePane, setRect, clusters, editor: EDITOR, DATA, renderPane, splitRange: (p)=>splitRange(p), paneBodyW, paneMin, refitSplit, get field(){ return theField(); }, themeId: ()=>THEMES.currentId(), themes: ()=>[...THEME_MODS.values()], setTheme: (id,opts)=>setThemeById(id,opts), openThemeEditor, customThemes: ()=>CUSTOM_THEMES.slice(), projects: ()=>APP.projects(), loadProjects, useProject, addProject, forgetProject, subscribeProjects, forgetTheme, registerTheme, saveNow: (opts)=>saveWorkspace(opts), saveWorkspace, safe: SAFE, get themesReady(){ return THEME_MODS.size; }, ensureConsole, CONSOLE_KINDS, defaultView, buildDefaultWorkspace, syncFrame, nearestCluster, drawStats: ()=>drawStats, mountVisible, visibleOnly: (on)=>{ const v = VIEW.setVisibleOnly(on); dirty = true; updateLOD(); return v; }, LOD, onScreen: LOD.onScreen, screenRect: LOD.screenRect, lodPick: LOD.pick, setLOD: LOD.setLOD, DOCS, docOf, setActive, showDoc, togglePin, pinTile, docText, docPath, DOC_KINDS, loadText, openFile, PORT_SRC, setWireMode, wireOn: ()=>APP.wireOn(), applyInputsNow: ()=>APP.applyInputs(), activePath: ()=>activeDoc()?activeDoc().path:null, selOf, setSel, toggleSelLink, linkedSel, LINK_KINDS, input: ()=>IN, hoverPane: ()=>INPUT.hotPane() ? (INPUT.hotPane().dataset.pane||null) : null, paneFocus, paneFullscreen, paneEditSource, paneClose, paneSubject, paneSourceOf, applyPaneSource, savePaneSource, exitFullscreen, userManifests: ()=>REGISTRY.manifests(), solo: ()=>{ const q=FOCUS.solo(); return q?q.id:null; }, NOCLOSE, viewportWorld, relayout, detachFromCluster, scrollConfig: ()=>SCROLL_CFG, scrollState: ()=>[...INPUT.scrollers.keys()].map(el=>(el.className||el.tagName)), hoverHits: (x,y)=>{ const el=document.elementFromPoint(x,y); return el && IN ? IN.hitRegion(el, viewport) : null; }, addWheelHook: (fn, prio)=>wheelHooks ? wheelHooks.register('wheel', fn, prio) : ()=>{}, docs: ()=>[...DOCS.values()].map(d=>({path:d.path, dirty:d.dirty, views:d.views.size, primary:!!d.primary, chars:d.text.length}))};
window.__bus = { projects: ()=>APP.projects(), loadProjects, useProject, addProject, forgetProject, PORT_SRC, EDITOR, TREE, WIRES, NODE_RESULTS, NODE_RUNS, graphRun,
  get theme(){ return THEMES_APP.theme(); }, get themeId(){ return THEMES.currentId(); }, get themes(){ return [...THEME_MODS.values()]; },
  setTheme: (id,opts)=>setThemeById(id,opts), get field(){ return theField(); },
  openThemeEditor, customThemes: ()=>CUSTOM_THEMES.slice(), forgetTheme, registerTheme,
  wireThemeEditor: (fromId)=>openThemeEditor(panes.find(q=>q.id===fromId)) };

/* the pane host (the ctx every pane gets) is built by the wiring, which owns the registry */

/* the boot sequence — the loop, the corpus, the themes, the wire layer and the restore — is
   js/host/canvas/boot.js: it starts what the assembly built. */
bootCanvasApp(APP, { viewport, world, $, requestFrame: (cb) => { const id = FRAME.raf(cb); return () => FRAME.caf(id); } });
}
