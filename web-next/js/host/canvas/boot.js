/*
 * host/canvas/boot.js — starting what the assembly built.
 *
 * The canvas is assembled (core, wiring and runtime, then the page's own debug surface); this is
 * the sequence that makes it LIVE, and its order is the app's whole behaviour on arrival:
 *
 *   1. draw the shipped arrangement and FRAME it — something is on screen before any fetch;
 *   2. start the corpus load, the user panes, the vocabulary, the theme modules and the field —
 *      five things that each take a request, none of which the first paint waits for;
 *   3. load the input policy (js/input/*), which the wheel needs and which the canvas works
 *      without;
 *   4. arm the WIRE LAYER — after the pane table exists, because it registers every kind's ports,
 *      and after the corpus, because a restored wire names a tile by id;
 *   5. RESTORE THE WORKSPACE last, so a saved layout replaces what the boot built instead of
 *      being overwritten by it. The promise is kept on `window.__boot`, because a caller that has
 *      to act on the RESTORED workspace (a test, a diagnostic) should be able to wait for it
 *      rather than race it;
 *   6. start the frame loop and say the page is ready.
 *
 * The wire layer's own bootstrap is here because it is the last thing to arm — everything it
 * needs (the pane table, the result table, the port sources) is in place by step 4.
 */

import { portsOf as portsOfRec, typeOfPort as typeOfPortRec, pickPort as pickPortRec, expandWires } from '../../app/wires.js';
import { createWireHost } from '../../live/wire/host.js';

/**
 * @param {object} APP  the assembled app
 * @param {object} env  { viewport, world, $, requestFrame }
 */
export function bootCanvasApp(APP, env) {
  const { viewport, world, $ } = env;
  const {
    cam, panes, camFor, defaultCam, applyCam, updateLOD, mountVisible, buildDefaultWorkspace,
    loadUserPanes, loadRegistry, themeBoot, fieldBoot, updateWireCount, loadSettings,
    CHROME, STORE, GRAPH, PANES, WIRES, DYN_WIRES, PORT_SRC, TREE, EDITOR,
    scheduleSave, toast, graphTick, ROUTES_READY,
  } = APP;

  /* the port arithmetic is js/app/wires.js; these are this app's views of the registry */
  const portsOf = (kind, dir) => portsOfRec(PANES[kind], dir);
  const typeOfPort = (kind, dir, port) => typeOfPortRec(PANES[kind], dir, port);
  const pickPort = (kind, dir, wantType) => pickPortRec(PANES[kind], dir, wantType);
  const instancesOf = (kind) => panes.filter((x) => x.kind === kind && x.el.isConnected).map((x) => x.id);

  /** One declaration becomes every live pairing; the ports are picked when it names only a
   *  kind, and there is a cap on the fan-out (js/app/wires.js). */
  function resolvedWires() {
    return expandWires(WIRES, {
      instancesOf: (kind) => instancesOf(kind),
      registry: (kind) => PANES[kind],
      extra: DYN_WIRES,
    });
  }

  /* THE PAIR IS BUILT IN STEP 1 AND THE LAYER IN STEP 4, so both names have to exist before
     step 1. The home arrangement makes the project/corpus pair and calls `ensureProjectWire`
     while it does (js/world/home.js), and that call has to see a layer host that is still null;
     `armedHost` is what step 4 fills in. */
  let armedHost = null;
  APP.ensureProjectWire = ensureProjectWire;

  /* ---------------- 1. something on screen ---------------- */
  ROUTES_READY.then((r) => { window.__diag.routes = Object.keys(r).length; }).catch(() => {});
  buildDefaultWorkspace();
  /* THE FIRST VIEW IS THE WRITING ROOT, FRAMED — not the whole canvas (js/world/home.js::defaultCam
     owns the rule and says why: a fit of every tile lands at 38%, which is a map, and the legible
     fallback then anchors a corner of a 2420x2160 world). A saved camera is applied later, in the
     restore; this is only what a first visit, or a workspace without one, gets. */
  Object.assign(cam, defaultCam());
  applyCam(); updateLOD(); mountVisible();

  /* ---------------- 2. the corpus, the panes, the words, the look ---------------- */
  /* THE CORPUS IS NOT FETCHED HERE, and there is nothing to bind here either: the corpus TILE
     fetches it for whatever project is wired in (js/panes/corpus.js), and the app subscribes to
     that tile's four ports when the tile MOUNTS (js/panes/app-mounts.js -> data.js::corpusFrom) —
     which is true of the pair step 1 builds, of one restored from a document and of one dropped
     in later. With no project wired into that tile there is no corpus, which is the point. */
  loadUserPanes();            // userspace panes (panes/manifests.json) + the add-tile menu
  loadRegistry();             // the vocabulary: which kinds, which LODs, which tab widths
  themeBoot();                // the theme modules (js/defaults/themes/*.json), one of them on
  fieldBoot();                // the blueprint, drawn from that theme's own ink
  updateWireCount();

  /* ---------------- 3. the input policy ---------------- */
  Promise.all([
    import('/js/input/hooks.js'), import('/js/input/wheel.js'), import('/js/input/scroll.js'),
    import('/js/defaults/scroll-config.js').catch(() => ({})),
  ]).then(([h, w, sc, cfg]) => {
    APP.setInputModules({ ...h, ...w, ...sc }, h.createHooks());
    if (cfg && cfg.DEFAULT_SCROLL_CONFIG) APP.setScrollCfg({ ...APP.scrollCfg, ...cfg.DEFAULT_SCROLL_CONFIG });
    window.__diag = window.__diag || {};
    window.__diag.input = 'ready';
  }).catch((e) => {
    window.__diag = window.__diag || {};
    window.__diag.input = 'failed: ' + (e && e.message);
    console.warn('[input]', e);
  });

  /* ---------------- 4. the wire layer ---------------- */
  const WIRE_HOST = createWireHost({
    panes, registry: PANES, wires: WIRES, dyn: DYN_WIRES,
    pendingWires: () => STORE.pendingWires(),
    pendingDone: () => STORE.clearPendingWires(),
    resolveWires: () => resolvedWires(),
    graph: GRAPH,
    elements: { viewport: () => viewport, world: () => world, canvas: () => $('#wire-canvas') },
    cam: () => cam,
    sources: PORT_SRC,
    selection: () => TREE.selection.get(),
    treeFile: () => TREE.file.get(),
    editorFile: () => EDITOR.file.get(),
    updateCount: () => updateWireCount(),
    save: () => scheduleSave(),
    toast: (m) => toast(m),
    loadModules: () => Promise.all([
      import('/js/live/wire/kinds.js'),
      import('/js/live/wire/layer.js'),
      import('/js/live/wire/connect.js'),
      import('/js/live/wire/schedule.js'),
    ]),
  });
  const wireArmed = WIRE_HOST.arm().catch((e) => console.warn('[wire]', e));
  armedHost = WIRE_HOST;
  wireArmed.then(() => updateWireCount());
  WIRE_HOST.watch(world);

  /* the app's late-bound half: what the boot started, for everything that attached earlier */
  Object.assign(APP, {
    wireHost: () => WIRE_HOST,
    applyInputs: () => WIRE_HOST.applyInputs(),
    rebuildSchedule: () => WIRE_HOST.rebuildSchedule(),
    drawWires: (t) => WIRE_HOST.draw(t),
    wireLayer: () => window.__wire || null,
    resolvedWires,
    portsOf, typeOfPort, pickPort,
  });
  window.__wire = {
    get layer() { return WIRE_HOST.layer(); },
    get ports() { return WIRE_HOST.ports(); },
    wires: resolvedWires,
    kinds: () => WIRE_HOST.kinds(),
    dyn: DYN_WIRES,
    get sched() { return GRAPH.scheduler(); },
    get order() { const s2 = GRAPH.scheduler(); return s2 ? s2.order : []; },
    get illegal() { const s2 = GRAPH.scheduler(); return s2 ? [...s2.illegal] : []; },
    rebuildSchedule: () => WIRE_HOST.rebuildSchedule(),
    graphTick,
  };

  /* ---------------- 4b. the wire that makes the corpus tile serve a project ---------------- */
  /* `armedHost` is declared up top, before step 1: the pair this function wires is built there */
  /** One wire: a project tile's `project` out-port into the corpus tile's `project` in-port.
   *  Exactly one — a workspace that already has one (the user's, snapped and re-made by hand)
   *  is left alone, and this only makes it when the pair has none. */
  function ensureProjectWire() {
    const tiles = APP.panes || [];
    const project = tiles.find((p) => p.kind === 'project');
    const corpus = tiles.find((p) => p.kind === 'corpus');
    if (!project || !corpus) return null;
    const already = DYN_WIRES.some((w) => w && w.to && w.to.p === corpus.id && w.to.port === 'project');
    if (already) return null;
    const wire = {
      id: 'd' + DYN_WIRES.length,
      from: { p: project.id, port: 'project' },
      to: { p: corpus.id, port: 'project' },
      kind: 'value',
      type: 'Project',
    };
    DYN_WIRES.push(wire);
    /* the layer may not be armed yet (the pair is built in step 1, the layer in step 4): the wire
       is on the list either way, and the arm replays the list */
    if (armedHost && armedHost.rebuildSchedule) armedHost.rebuildSchedule();
    updateWireCount();
    return wire;
  }

  /* ---------------- 5. the restore ---------------- */
  window.__boot = loadSettings();

  /* ---------------- 6. the loop ---------------- */
  CHROME.start();
  document.documentElement.dataset.ready = '1';
}
