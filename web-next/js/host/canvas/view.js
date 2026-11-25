/*
 * host/canvas/view.js — the camera, the grips, the guides and the rung pass, composed.
 *
 * This is the canvas's VIEW layer: the parts that are about where things are on screen and
 * what a gesture on an edge means. Each piece is already a module — the camera's motion
 * (js/app/camera.js), the handles and their sizes (js/world/grips.js), the snap arithmetic
 * (js/app/snap.js), the rungs (js/app/lod.js), the gestures (js/world/tile-gestures.js) —
 * and what was left in the page was the COMPOSITION: which of them is called on a camera move,
 * what a grab means depending on where the tile lives, and how a guide is drawn.
 *
 * Three rules that only exist in the composition:
 *
 *   A CAMERA MOVE IS ALSO A GRIP-SIZE CHANGE. The handles are measured in layout space and
 *   have to be hittable in screen space, so every camera move re-places them; a zoom that
 *   did not would leave its grips the size they were at the old zoom.
 *
 *   A SOLO TILE IS PINNED TO THE VIEWPORT. It is not where its rect says, so a camera move
 *   writes the viewport rect into it again rather than letting it drift with the world.
 *
 *   THE SAVE LANDS AFTER THE MOTION STOPS. Saving on every camera frame is sixty saves a
 *   second for a value that only matters at rest — the camera's own settle timer is what
 *   asks for it.
 */

import { createCameraHost } from '../../app/camera.js';
import { createGrips, GRIP_EDGES as GRIP_EDGE_LIST, GRIP as GRIP_BASE } from '../../world/grips.js';
import { createTileGestures } from '../../world/tile-gestures.js';
import { SNAP as SNAP_CFG, snapMove, snapEdge } from '../../app/snap.js';
import { ZMIN, ZMAX, screenRect as screenRectOf } from '../../app/viewport.js';

/**
 * @param {object} app   the canvas app (see web-next/js/README.md)
 *   cam, viewport(), world, panes, clusters,
 *   field(), chrome(), focus(), setRect(p), updateLOD(), relayout(c), detachFromCluster(p),
 *   clusterUnder(r), dockInto(c,p), clusterRect(c), sashFor(p,edge), moveSash(c,i,e,el),
 *   paneMin(p), snapper(p) [the snap targets], toast(m), save(), markDirty(),
 *   requestFrame(cb), apply(), render(p)
 */
export function createView(app) {
  const { cam, world } = app;
  /* the world element's transform is js/engine/world.js's, in both apps (see runtime.js) */
  const worldHost = app.worldHost || null;
  const SNAP = SNAP_CFG;

  /* ---------------- guides ---------------- */
  let guideEl = null;
  function guideLayer() {
    if (!guideEl || !guideEl.isConnected) {
      guideEl = document.createElement('div');
      guideEl.id = 'snap-guides';
      world.appendChild(guideEl);
    }
    return guideEl;
  }
  function showGuides(list) {
    const layer = guideLayer();
    layer.textContent = '';
    for (const g of list) {
      const el = document.createElement('i');
      el.className = 'snap-guide ' + g.dir;
      if (g.dir === 'v') {
        el.style.left = g.at + 'px';
        el.style.top = g.from + 'px';
        el.style.height = Math.max(1, g.to - g.from) + 'px';
      } else {
        el.style.top = g.at + 'px';
        el.style.left = g.from + 'px';
        el.style.width = Math.max(1, g.to - g.from) + 'px';
      }
      layer.appendChild(el);
    }
  }
  const hideGuides = () => { if (guideEl) guideEl.textContent = ''; };

  /* ---------------- snapping ---------------- */
  /** Everything the snap rules may align to: the other tiles and the cluster frames. */
  function snapTargets(self) {
    const list = app.panes.filter((q) => q !== self && q.el && q.el.isConnected).map((q) => q.wr);
    for (const c of app.clusters) list.push(app.clusterRect(c));
    return list;
  }
  /** Nearest snap for a dragged tile: where it lands, and the guides to draw. */
  function snapPos(self, x, y, gridStep) {
    const r = snapMove({ x, y, w: self.wr.w, h: self.wr.h, others: snapTargets(self), tol: SNAP.tolPx / cam.z, gutter: SNAP.gutter, grid: gridStep || 0 });
    showGuides(r.guides);
    return { x: r.x, y: r.y, snapped: r.snapped };
  }
  /** Snap the moving edge onto a neighbour's edge, and say so with a guide. */
  function snapResize(p, r, edge, grid) {
    const out = snapEdge({ rect: r, edge, others: snapTargets(p), tol: SNAP.tolPx / cam.z, grid: grid || 0 });
    Object.assign(r, out.rect);
    if (out.guides.length) showGuides(out.guides); else hideGuides();
  }

  /* ---------------- grips and gestures ---------------- */
  const GRIPS = createGrips({ cam, world, solo: () => app.focus().solo(), grab: (e, p, edge) => onGripGrab(e, p, edge) });
  const syncGrips = (p) => GRIPS.sync(p);
  const mountGrips = (p) => GRIPS.mount(p);
  const dropGrips = (p) => GRIPS.drop(p);
  const gripAt = () => GRIPS.at();
  const gripBand = (r) => GRIPS.band(r);

  /** Drag a split ratio. One implementation, reached from the sash itself and from the grip
   *  of any tile that borders it. */
  const startSashDrag = (c, i, e, captureEl) => app.moveSash(c, i, e, captureEl);

  function routeToSash(e, p, edge) {
    const hit = app.sashFor(p, edge);
    if (!hit) return false;
    startSashDrag(hit.c, hit.i, e, e.currentTarget);
    return true;
  }

  /** What a grab MEANS depends on where the tile lives — and only on that. */
  function onGripGrab(e, p, edge) {
    if (!p || !p.el || !p.el.isConnected || app.focus().solo() === p) return;
    if (String(p.home || '').startsWith('cluster:')) {
      // a tiled tile has no rect of its own: its size belongs to the split tree
      if (routeToSash(e, p, edge)) return;
      GESTURES.extent(e, p, edge);
      return;
    }
    GESTURES.free(e, p, edge);
  }

  const GESTURES = createTileGestures({
    cam,
    setRect: (q) => app.setRect(q),
    markDirty: () => { if (app.markDirty) app.markDirty(); },
    settle: () => { app.updateLOD(); if (app.save) app.save(); },
    minOf: (q) => app.paneMin(q),
    snapRect: (q, r, edge, ev) => { if (SNAP.on && !ev.shiftKey) snapResize(q, r, edge, ev.altKey ? SNAP.grid : 0); else hideGuides(); },
    snapPos: (q, x, y, ev) => snapPos(q, x, y, ev.altKey ? SNAP.grid : 0),
    clearGuides: () => hideGuides(),
    clusterOf: (q) => app.clusters.find((k) => String(q.home || '') === 'cluster:' + k.id),
    relayout: (c) => app.relayout(c),
    detach: (q) => app.detachFromCluster(q),
    clusterUnder: (r) => app.clusterUnder(r),
    dockInto: (c, q) => app.dockInto(c, q),
    toast: (m) => app.toast(m),
  });

  /* ---------------- the camera ---------------- */
  let lastZoomLabel = '';
  function syncGrid() {
    const f = app.field();
    if (f) f.sync(cam);
  }
  /** The zoom readout is part of the camera: it moves with it, and the HUD's own 400ms tick
   *  is too slow to watch an eased zoom land. */
  function syncZoomLabel() {
    const s = Math.round(cam.z * 100) + '%';
    if (s === lastZoomLabel) return;
    lastZoomLabel = s;
    const rail = app.chrome() && app.chrome().RAIL;
    const el = app.el || (() => null);
    const a = el('h-zoom');
    if (a) a.textContent = s;
    const b = el('h-zoom2');
    if (b) b.textContent = s;
    if (rail && rail.zoom) rail.zoom.textContent = s;
  }

  const CAM = createCameraHost({
    cam,
    viewport: () => app.viewport(),
    applyTransform: (c) => {
      syncGrid();
      syncZoomLabel();
      /* ONE WRITER: js/engine/world.js's `setTransform` (it writes the transform, `--z` and
         `--inv`). This used to be its own three lines here, which is how the two apps ended up
         with two implementations of the same DOM write. */
      if (worldHost) worldHost.setTransform(c);
      else world.style.transform = 'translate(' + c.x + 'px,' + c.y + 'px) scale(' + c.z + ')';
      world.style.willChange = 'transform';
    },
    onMove: () => {
      /* a zoom is also a grip-size change: the same pass that moves the tiles */
      for (const p of app.panes) syncGrips(p);
      const solo = app.focus().solo();
      if (solo) { Object.assign(solo.wr, app.viewportWorld()); app.setRect(solo); }
      if (app.markDirty) app.markDirty();
    },
    onSettle: () => {
      world.style.willChange = 'auto';
      for (const p of app.panes) if (p.lod === 'live' && p.implDraw) p.implDraw();
      if (app.save) app.save();
    },
    rects: () => app.panes.map((p) => p.wr),
    /* the page owns its own loop; the camera asks it for a frame instead of starting
       a second one */
    frame: (cb) => app.requestFrame(cb),
  });

  const applyCam = () => CAM.apply();
  const zoomAt = (mx, my, f) => CAM.zoomAt(mx, my, f);
  const stopZoom = () => CAM.stopZoom();
  const zoomNudge = (dz) => CAM.nudge(dz);
  const zoom100 = () => CAM.zoom100();
  const camFor = (wr, pad, maxZ) => CAM.camFor(wr, pad || 0, maxZ || 3);
  const firstView = (wr, pad) => CAM.firstView(wr, pad === undefined ? 60 : pad);
  const bbox = () => CAM.frameBox();

  let flyStop = null;
  /** Fly the camera to a target over `ms`, easing in and out (a zoom is exponential, so the
   *  interpolation happens in log z and the view stays anchored on the same world point).
   *  The frame scheduler is the app's: one module owns it, and the app hands out frames. */
  function flyToCam(t, ms, done) {
    if (flyStop) { flyStop(); flyStop = null; }
    stopZoom();
    /* `ms = 0` means "put it there now", and it has to be said, not computed: `(now - t0) / 0` is
       NaN on the very first frame (0/0) and Infinity on any later one, so the eased path could
       write `cam.z = NaN` — a camera that is neither at the target nor at the start, never
       settles (every comparison against NaN is false) and leaves a caller waiting for it to stop
       waiting forever. Seen from a test that flew to a tile with a duration of 0. */
    if (!(ms > 0)) {
      cam.z = t.z; cam.x = t.x; cam.y = t.y;
      applyCam();
      if (done) done();
      return;
    }
    const s = { ...cam };
    const cs = { x: (app.viewport().w / 2 - s.x) / s.z, y: (app.viewport().h / 2 - s.y) / s.z };
    const ct = { x: (app.viewport().w / 2 - t.x) / t.z, y: (app.viewport().h / 2 - t.y) / t.z };
    const t0 = performance.now();
    const step = (now) => {
      let u = Math.min(1, (now - t0) / ms);
      u = u < 0.5 ? 2 * u * u : 1 - Math.pow(-2 * u + 2, 2) / 2;
      const z = Math.exp(Math.log(s.z) + (Math.log(t.z) - Math.log(s.z)) * u);
      const cx = cs.x + (ct.x - cs.x) * u, cy = cs.y + (ct.y - cs.y) * u;
      const vp = app.viewport();
      cam.z = z; cam.x = vp.w / 2 - cx * z; cam.y = vp.h / 2 - cy * z;
      applyCam();
      if (u < 1) flyStop = app.requestFrame(step); else { flyStop = null; if (done) done(); }
    };
    flyStop = app.requestFrame(step);
  }
  const fitRect = (wr, pad, ms) => flyToCam(camFor(wr, pad), ms || 400);

  /* ---------------- rungs ---------------- */
  let visibleOnly = true;
  let drawStats = { drawn: 0, built: 0, total: 0 };
  let lodCounts = { live: 0, frozen: 0, card: 0, culled: 0 };

  function updateLOD() {
    const vp = app.viewport();
    const r = app.lod.updateLOD({ panes: app.panes, cam, vw: vp.w, vh: vp.h, lodOn: app.lodOn(), visibleOnly });
    lodCounts = r.counts;
    drawStats = { drawn: r.drawn, built: r.built, total: r.total };
    if (app.setStats) app.setStats({ counts: lodCounts, drawn: r.drawn, built: r.built, total: r.total });
    return r;
  }
  function recountDrawStats() { drawStats = app.lod.recount({ panes: app.panes }); return drawStats; }

  return {
    SNAP, GRIPS, GESTURES, CAM,
    GRIP_EDGES: GRIP_EDGE_LIST, GRIP: GRIP_BASE, ZMIN, ZMAX,
    // camera
    applyCam, zoomAt, stopZoom, zoomNudge, zoom100, camFor, firstView, bbox, flyToCam, fitRect, syncGrid, syncZoomLabel,
    // grips and snapping
    gripAt, gripBand, syncGrips, mountGrips, dropGrips, onGripGrab,
    snapTargets, snapPos, snapResize, showGuides, hideGuides, routeToSash, startSashDrag,
    // rungs
    updateLOD, recountDrawStats,
    visibleOnly: () => visibleOnly,
    setVisibleOnly: (on) => { visibleOnly = (on !== false); return visibleOnly; },
    stats: () => ({ counts: lodCounts, drawn: drawStats.drawn, built: drawStats.built, total: drawStats.total }),
    screenRect: (p) => screenRectOf(p.wr, cam),
  };
}