/*
 * host/canvas/tiles.js — building tiles, and the actions their headers carry.
 *
 * Extracted from the inline script in web-next/index.html. Everything it calls is already a
 * module (the tile factory, the shell, the grips, the gestures, the focus, the home
 * arrangement, the mount dispatcher); what was in the page was the WIRING — which function
 * each header button runs, what state a tile is born with, and what "close" means for a kind
 * that may not be closed.
 *
 * The wiring is the interesting part, because a header button is a promise about a KIND:
 *
 *   fit     frames the camera on this tile, whatever it is;
 *   focus   centres the camera on it and marks it hot, changing nothing about the workspace;
 *   full    the solo (js/world/focus.js), which remembers where the tile came from;
 *   src     asks the source tile for this kind's module (js/panes/source-of.js says which);
 *   del     closes it — unless the kind is in `noClose`, and the refusal is a toast rather
 *           than a hidden button, because a button that is there and does nothing is worse;
 *   pin     a corpus tile follows the active file or not (js/host/canvas/tile-state.js);
 *   link    a picking tile shares the selection or keeps its own.
 *
 * THE BIRTH STATE is the other half: a NEW tile follows the active file unless the preference
 * says pin, while a RESTORED tile keeps the pin and the subject it was saved with. Getting
 * that backwards makes a workspace rearrange itself on reload.
 */

import { createTileFactory } from '../../world/tile-create.js';
import { buildShell, actionsFor } from '../../world/tile-shell.js';
import { createHome } from '../../world/home.js';
import { firstViewCam } from '../../app/viewport.js';
import { createFocus } from '../../world/focus.js';
import { createTileMount } from '../../world/tile-mount.js';

/** The kinds the workspace's own control surface is made of, from the registry. */
export function createTiles(app, deps) {
  const { panes, clusters, cam, world, REG } = deps;
  const { DOC_KINDS, LINK_KINDS, SYNTAX_KINDS } = app.tileKinds;

  const titleOf = (k) => (app.registry()[k] || {}).title || k;

  /** Which panes may be closed: the core pane is the workspace's own control surface and has
   *  to survive. A kind opts out HERE rather than hiding its own ✕. */
  const NOCLOSE = new Set(['core']);

  const TILES = createTileFactory({
    world, panes,
    titleOf,
    kinds: { docs: DOC_KINDS, links: LINK_KINDS, noClose: NOCLOSE },
    build: (spec) => buildShell(spec),
    actionsFor: (spec) => actionsFor(spec),
    mountGrips: (p) => app.mountGrips(p),
    setRect: (q) => app.setRect(q),
    drag: (p, e) => app.gestures().move(p, e),
    render: (p) => app.render(p),
    boot: (p, state) => bootTile(p, state),
    actions: {
      fit: (p) => app.fitRect(p.wr, 70, 500),
      focus: (p) => app.paneFocus(p),
      full: (p) => app.paneFullscreen(p),
      src: (p) => app.paneEditSource(p),
      del: (p) => app.paneClose(p),
      pin: (p) => app.togglePin(p),
      link: (p) => app.toggleSelLink(p),
    },
    cam,
    viewport: () => app.viewport(),
    toast: (m) => app.toast(m),
  });

  const mkPane = (id, kind, wr, home, state) => TILES.make(id, kind, wr, home, state);

  /** The state a tile is born with. A new tile follows the active file unless the preference
   *  says pin; a restored one keeps what it was saved with. */
  function bootTile(p, state) {
    const kind = p.kind;
    const view = app.viewPrefs();
    if (DOC_KINDS.has(kind)) {
      p.pin = (state && state.pin !== undefined) ? !!state.pin : !!view.newTilePin;
      p.subject = (state && state.subject) || null;
      p.pinned = p.pin ? p.subject : null;
      app.wirePin(p);
    }
    if (LINK_KINDS.has(kind)) {
      p.selLink = (state && state.selLink !== undefined) ? !!state.selLink : (view.newTileSel !== false);
      p.selLocal = (state && state.selLocal) || null;
      app.wireSel(p);
    }
    if (kind === 'source') p.subjectPane = (state && state.subjectPane) || null;
    /* A project tile's subject is a corpus directory on the server, and it is the tile's own
       state — a path a user typed into the tile and the workspace remembers (js/panes/project.js).
       Nothing here derives it: with no path the tile stands for whatever the server serves. */
    if (kind === 'project') p.path = (state && state.path) || null;
    if (SYNTAX_KINDS.has(kind)) {
      p.syntax = (state && state.syntax !== undefined) ? !!state.syntax : (view.syntax !== false);
      p.syntaxWired = false;
    }
  }

  /* ---------------- the shipped arrangement ---------------- */
  let consoleKinds = REG.kinds.console;
  const HOME = createHome({
    panes, clusters, cam,
    /* `state` goes through: the factory is handed a tile's saved state and this forwarder is the
       last step before it. Dropping it here is invisible until a kind is born with state — and
       then a workspace comes back subtly not the one that was saved. */
    make: (id, kind, wr, home, state) => mkPane(id, kind, wr, home, state),
    addCluster: (ox, oy, kinds, box) => app.addCluster(ox, oy, kinds, box),
    frameBox: (x, y, w, h, label) => app.frameBox(x, y, w, h, label),
    clearFrames: () => app.clearFrames(),
    resetTiles: () => TILES.reset(),
    dispose: (p) => app.disposePane(p),
    detachEl: (p) => p.el.remove(),
    bbox: () => app.bbox(),
    camFor: (wr, pad) => app.camFor(wr, pad),
    /* THE SHIPPED CAMERA, asked for by name from js/app/viewport.js. `app.firstView` never
       existed on the object this factory is handed, so "default view" composed the arrangement
       and then threw before it could frame it — the tiles came back and the camera stayed where
       it was, which reads as a half-applied layout. `app.viewport()` IS on that object (this
       factory takes one argument, so a second `deps` name would be undefined and every framing
       would fall back to 1400x900 whatever the window is); this is the same call the boot makes
       (js/host/canvas/boot.js). */
    firstView: (wr, pad) => {
      const vp = app.viewport() || { w: 1400, h: 900 };
      return firstViewCam(wr, vp.w, vp.h, { pad: pad || 60 });
    },
    applyCam: () => app.applyCam(),
    updateLOD: () => app.updateLOD(),
    /* `app` has no `exitFullscreen`; the one that exists is `FOCUS.exit()` (this file builds it
       below). Calling through the missing name threw, so "default view" and closing a tile that
       was in solo focus both died with `app.exitFullscreen is not a function` — the arrangement
       the workspace ships was unreachable from the button that restores it. The arrow is only
       CALLED after `FOCUS` exists (a user action, not the boot), so closing over it is safe. */
    exitFullscreen: () => { if (FOCUS && typeof FOCUS.exit === 'function') FOCUS.exit(); },
    consoleKinds: () => consoleKinds,
    ensureProjectWire: () => app.ensureProjectWire(),
    ensureCorpus: () => HOME.ensureCorpus(),
    toast: (m) => app.toast(m),
    markDirty: () => app.markDirty(),
    save: () => app.save(),
  });

  /* ---------------- focus: one tile fills the viewport ---------------- */
  const FOCUS = createFocus({
    cam,
    viewport: () => app.viewport(),
    viewportWorld: () => app.viewportWorld(),
    flyTo: (target, ms) => app.flyToCam(target, ms),
    setRect: (q) => app.setRect(q),
    updateLOD: () => app.updateLOD(),
    render: (q) => app.render(q),
    measure: (q) => { q.sr = app.screenRect(q); },
    setHot: (el) => app.setHotPane(el),
    toast: (m) => app.toast(m),
    clusterOf: (q) => clusters.find((cl) => cl.panes && cl.panes[q.kind] === q) || null,
    detach: (q) => app.detachFromCluster(q),
    relayout: (c) => app.relayout(c),
    dirty: () => app.markDirty(),
    save: () => app.save(),
    body: document.body,
  });

  /* ---------------- building a tile's body ---------------- */
  const TILE_MOUNT = createTileMount({
    panes, registry: app.registry(), lod: app.lod,
    cam: () => cam,
    viewport: () => app.viewport(),
    visibleOnly: () => app.visibleOnly(),
    behaviourMount: (rec) => app.behaviourMount(rec),
    userMount: (render) => app.userMount(render),
    esc: (s) => app.esc(s),
  });

  /** The corpus file a tile is about, if it has one. */
  function paneSubject(p) {
    if (p.doc && p.doc.path) return p.doc.path;
    const sel = app.selOf(p) || app.tree().selection.get() || app.tree().file.get();
    if (sel && String(sel).indexOf('/') >= 0) return String(sel);
    const active = app.activeDoc();
    return (active && active.path) || app.editor().file.get() || null;
  }

  /** Close a tile — or refuse, out loud, for a kind that is the workspace's own. */
  function paneClose(p) {
    if (NOCLOSE.has(p.kind)) { app.toast('the ' + p.kind + ' tile stays in the workspace'); return; }
    if (FOCUS.solo() === p) FOCUS.exit();
    app.removePane(p);
    app.toast('closed ' + p.kind);
  }

  /** The add-tile picker's contents, in the registry's own order. */
  function populateKinds(into) {
    const sel = into || app.el('#s-kind');
    if (sel) TILE_MOUNT.populate(sel, { order: REG.kinds.order, fallback: REG.kinds.default });
  }

  return {
    NOCLOSE, TILES, mkPane, bootTile, HOME, FOCUS, TILE_MOUNT,
    consoleKinds: () => consoleKinds,
    ensureProjectWire: () => app.ensureProjectWire(),
    ensureCorpus: () => HOME.ensureCorpus(),
    setConsoleKinds: (k) => { consoleKinds = k; },
    buildDefaultWorkspace: () => HOME.buildDefault(),
    defaultCam: () => HOME.defaultCam(),
    defaultView: () => HOME.resetToDefault(),
    ensureConsole: () => HOME.ensureConsole().length > 0,
    paneFocus: (p) => FOCUS.focus(p),
    paneFullscreen: (p) => FOCUS.fullscreen(p),
    exitFullscreen: () => FOCUS.exit(),
    paneSubject, paneClose, populateKinds,
    renderPane: (p) => TILE_MOUNT.render(p),
    mountVisible: () => TILE_MOUNT.mountVisible(),
  };}
