/**
 * engine/index.js — the engine, assembled (M2.17, rewired).
 *
 * This is what `js/main.js` boots for a SHELL page (a `#root` and nothing else): the engine's
 * camera, world, renderer, LOD, HUD, modals, input and the design layer, wired to each other
 * and to the workspace API (`js/core/ws.js`) the panes talk to.
 *
 * It had never run. As written it called `createWorld(root)` (the module takes
 * `{root, governor, camera}`), imported `createCamera` from a module that exports pure
 * functions rather than a controller, used a `lock` before declaring it, read
 * `instances.bus`/`instances.penState` off a manager that has neither, and assembled its own
 * `createWorkspace` that referenced a `ws` it never had. None of that was visible while the
 * page assembled the pieces itself and this module sat unimported — which is exactly what
 * "wire the engine's own assembly" means: the bugs come out with the wiring.
 *
 * What main.js still owns: the query string, the storage adapter, the endpoints, the
 * governor, the bus, the pane registry, the preset, and the wiring script. What is here is
 * everything that is the ENGINE's, so that `shell.html` and (eventually) the canvas boot the
 * same way.
 */

import { createCameraHost } from '../app/camera.js';
import { createWorld } from './world.js';
import { createInstanceManager } from '../panes/instances.js';
import { createRenderer } from '../layout/renderer.js';
import { createLodManager } from './lod-apply.js';
import { createHudManager } from './hud.js';
import { createModals } from './modal.js';
import { createRootHandles } from './root-handles.js';
import { installDrag } from './drag.js';
import { createHooks, installHooks } from '../input/hooks.js';
import { installPointer } from '../input/pointer.js';
import { installTouch } from '../input/touch.js';
import { createKeyHandler } from '../input/keys.js';
import { installKeyboardGuard } from '../input/keyboard-guard.js';
import { createLock } from './lock.js';
import { createDesignBar } from '../design/design-bar.js';
import { createPalette } from '../design/palette.js';
import { createDeclareDialog } from '../design/declare-dialog.js';
import { installPaletteDrag } from '../design/palette-drag.js';
import { createAnchorUI } from '../design/anchor-ui.js';
import { createPanic } from '../design/panic.js';
import { createPersist } from '../design/persist.js';
import { createWorkspace } from '../core/ws.js';
import { parse as parseLayout } from '../layout/serialize.js';

/** Every root's rect as the document states it: the design layer's own coordinates. */
function rootRectsOf(store) {
  const layout = (store && store.get ? store.get() : null) || {};
  const out = new Map();
  for (const r of (layout.roots || [])) out.set(r.id || 'main', { x: r.x || 0, y: r.y || 0, w: r.w || 0, h: r.h || 0 });
  return out;
}

/**
 * Create the engine.
 *
 * @param {object} deps
 * @param {HTMLElement} deps.root - the element to build in (#root)
 * @param {object} deps.store - layout store
 * @param {object} deps.governor - frame governor
 * @param {object} deps.bus - event bus
 * @param {object} deps.endpoints - endpoint registry (js/net/endpoint-node.js)
 * @param {object} [deps.services] - the host's services for panes (projects, ui): what a pane asks
 *   its ctx for, supplied by the page that boots the engine
 * @param {object} deps.storage - storage adapter {get, put, ...}
 * @param {object} deps.scope - root scope (js/core/scope.js)
 * @param {function} deps.wirer - the wirer (js/core/wire.js)
 * @param {object} deps.registry - pane registry (js/panes/registry.js)
 * @param {boolean} [deps.safe] - safe boot
 * @param {boolean} [deps.debug] - ?debug=1
 * @returns {{app: object, parts: object, destroy: function}}
 */
export function createEngine({ root, store, governor, bus, endpoints, storage, scope, wirer, registry, services = {}, safe = false, debug = false }) {
  if (!root) throw new Error('createEngine: root is required');

  /* ---------------- the world and the camera ---------------- */
  let hud = null;
  const world = createWorld({ root, governor, camera: null });   // the camera lands below
  /* THE ONE CAMERA (js/app/camera.js): the canvas's, with the engine's three additions. Its
     frame comes from the governor — the engine's one loop — and its transform goes through the
     world module, so a camera move is one style write and one announcement (`state`), which is
     what the world writer and the LOD pass listen to. */
  const cam = { x: 0, y: 0, z: 1, lock: 'free' };
  const camera = createCameraHost({
    cam,
    viewport: () => world.viewportSize(),
    applyTransform: (c) => world.setTransform(c),
    requestFrame: (cb) => governor.requestFrame('camera', () => cb(typeof performance !== 'undefined' ? performance.now() : Date.now())),
    insets: () => (hud ? hud.getInsets() : { t: 0, r: 0, b: 0, l: 0 }),
    /* `fit` needs to know WHAT to fit: without a provider it fitted nothing, silently, behind
       three call sites (the design bar, the panic reset, the keyboard shortcut). */
    rectsProvider: () => [...rootRectsOf(store).values()],
  });
  /* world.js reads the camera it is given; give it the real one now that it exists (the two
     are created in this order because the camera's viewport IS the world's size) */
  world.follow(camera);

  /* ---------------- the panes ---------------- */
  let modals = null;
  const modalPort = {
    get: (id) => (modals ? modals.get(id) : null),
    add: () => {},
    open: (id) => modals && modals.open(id),
    close: (id) => modals && modals.close(id),
    toggle: (id) => modals && modals.toggle(id),
  };
  const clocks = {
    clock: () => ({}),
    delay: () => () => {},
    pauseAll: () => {}, resumeAll: () => {}, stepAll: () => {},
    isPaused: () => false,
  };

  const ws = createWorkspace({
    bus, storage, endpoints, governor,
    layoutStore: store,
    camera,
    modals: modalPort,
    clocks, wirer,
    paneRegistry: registry,
    scope, safe, debug,
  });
  /* `services` is the host's table for the panes (see js/app/project-services.js): this engine
     builds the whole view and knows nothing about projects, so whoever boots it says how a pane
     asks the server. */
  const instances = createInstanceManager({ ws, registry, governor, scope, services });
  ws._setInstances(instances);
  /* Created BEFORE the renderer, so on a layout change the instances exist before it mounts
     them (subscriptions run in the order they were made). */
  const offSync = store.subscribe((layout) => instances.sync(layout));
  instances.sync(store.get());

  /* ---------------- the chrome the renderer needs ---------------- */
  const lock = createLock();
  let lod = null;
  let renderer = null;

  hud = createHudManager({
    store, camera,
    viewport: () => world.viewportSize(),
    renderer: { setHudInsets: (ins) => { if (renderer && renderer.setHudInsets) renderer.setHudInsets(ins); } },
  });
  lod = createLodManager({ governor, camera, store, instances, viewport: () => world.viewportSize() });

  const anchorUI = createAnchorUI({
    root: world.worldEl,                      // the overlay lives with the roots it joins
    store,
    getRootRects: () => rootRectsOf(store),
  });

  /* The modal manager first: its overlays are what the renderer fills, so they have to exist
     before it draws anything. */
  const modalsManager = createModals({ store, bus });
  modals = modalsManager;

  const onFit = () => { camera.set({ x: 0, y: 0, z: 1 }); camera.fit(); };
  const onDetach = (paneId) => store.dispatch({ type: 'tree.move', id: paneId, to: { newRoot: { x: 0, y: 0, w: 300, h: 200 } } });
  const onRemove = (paneId) => {
    store.dispatch({ type: 'tree.remove', id: paneId });
    store.dispatch({ type: 'pane.remove', id: paneId });
  };

  renderer = createRenderer({
    worldEl: world.worldEl,
    store, instances, registry, governor, camera, lock,
    onFit, onDetach, onRemove,
    lod,
    huds: hud,          // the HUD manager: it owns the layer and the elements (see hud.js)
  });

  /* the anchors menu is the renderer's own context menu, and its two handlers are
     js/design/anchor-ui.js's (the submenu it opens writes the action it reads back) */
  if (renderer.contextMenu && anchorUI.setContextMenu) anchorUI.setContextMenu(renderer.contextMenu);

  const drag = installDrag({ ws: { lock, store }, store, instances, camera, worldEl: world.worldEl });

  /* ---------------- input ---------------- */
  const hooks = createHooks();
  const inputHooks = installHooks({ viewportEl: world.viewportEl, ws: { camera, store, bus }, hooks });
  const inputWs = { camera, bus, hooks: inputHooks, lock, store };
  const pointer = installPointer({ worldEl: world.viewportEl, ws: inputWs, camera, store, instances });
  const touch = installTouch({ worldEl: world.viewportEl, ws: inputWs, camera, store, instances });
  createKeyHandler({ ws });
  installKeyboardGuard({ ws, rootEl: root, viewportEl: world.viewportEl });

  /* ---------------- the design layer ---------------- */
  const designBar = createDesignBar({ root, ws: { lock, store } });
  const palette = createPalette({ root, registry, store, lock });
  const declareDialog = createDeclareDialog({ root, store });
  root.addEventListener('ui:palette-toggle', () => palette.toggle());
  root.addEventListener('ui:add-hud', () => declareDialog.open());
  root.addEventListener('ui:add-modal', () => declareDialog.open());

  /* A palette item can be dragged onto a pane's edge to place the new pane there, instead of
     being clicked (which puts it in a default spot). The module installs its own
     document-level pointerdown, scoped to `.palette-item`; it had never been installed —
     main.js imported it and never called it. */
  installPaletteDrag({ ws, store, instances, camera, worldEl: world.worldEl });

  const rootHandles = createRootHandles({
    root: world.worldEl,                       // in the world: the handles track the roots
    store, lock,
    getZoom: () => camera.get().z || 1,
    onRootRects: () => rootRectsOf(store),
  });

  const panic = createPanic({
    root, store, camera, storage,
    presetId: 'standard-desktop', debug, noConfirm: false,
  });
  const persist = createPersist({
    store, storage,
    serialize: (layout) => JSON.stringify(layout),
    deserialize: parseLayout,
    applyLayout: (layout) => store.dispatch({ type: 'layout.replace', layout }),
    reset: false,
  });

  const app = {
    root, store, governor, bus, endpoints, ws, camera, world, instances, renderer, lod, hud,
    modals: modalsManager, lock, drag, input: { hooks: inputHooks, pointer, touch },
    designBar, palette, declareDialog, anchorUI, rootHandles, panic, persist,
    safe, debug,
  };

  function destroy() {
    try { offSync(); } catch (e) {}
    for (const part of [pointer, touch, persist, panic, rootHandles, anchorUI, declareDialog,
                        designBar, drag, modalsManager, hud, lod, world]) {
      try { if (part && typeof part.destroy === 'function') part.destroy(); } catch (e) { console.warn('[engine] destroy', e); }
    }
    instances.destroyAll();
    root.innerHTML = '';
  }

  return { app, destroy };
}
