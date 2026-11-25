/*
 * host/canvas/world.js — the world element's own composition: one tile's box, one tile's
 * teardown, and the clusters.
 *
 * Extracted from the inline script in web-next/index.html. Everything here is already a module
 * (`js/world/clusters.js` for the frames and sashes, `js/world/tile-life.js` for the teardown,
 * `js/app/tiling.js` for the split arithmetic, `js/world/grips.js` for the handles); what was in
 * the page was the wiring, and three of those wires carry a rule:
 *
 *   A BOX CHANGE HAS THREE CONSEQUENCES, IN THIS ORDER. The style is written, the GRIPS are
 *   re-placed (their size depends on the tile's own size, and a stale cache makes a re-sized
 *   tile's handles hittable where it used to be), and the pane's two columns are re-fitted.
 *
 *   TEARDOWN IS NOT REMOVAL. A tile leaving the world must first let go of its documents and
 *   its grips, or a dead view keeps receiving edits — and the kind that may not be closed says
 *   so, out loud, before anything is taken apart.
 *
 *   A CLUSTER'S TILING IS PURE ARITHMETIC WITH A DOM SHELL AROUND IT. `und`er and `nearest` are
 *   asked through the tiling module with the cluster's own frame rect, so "which cluster is
 *   this tile over" and "which is nearest" cannot disagree about geometry.
 */

import { createClusters } from '../../world/clusters.js';
import { disposeTile, removeTile } from '../../world/tile-life.js';
import { clusterUnder as underCluster, nearestCluster as nearCluster } from '../../app/tiling.js';

/**
 * @param {object} app
 *   world, panes, clusters, cam, registry-ish makeTile, NOCLOSE,
 *   syncGrips(p), dropGrips(p), refitSplit(p), invalidateGrips(),
 *   documents(), updateLOD(), startSashDrag(c,i,e,el), toast(m), save(), markDirty()
 */
export function createWorldHost(app) {
  const { world, panes, clusters, cam } = app;

  /** Write one tile's box, and everything that follows from the box having changed. */
  function setRect(p) {
    const s = p.el.style;
    s.left = p.wr.x + 'px';
    s.top = p.wr.y + 'px';
    s.width = p.wr.w + 'px';
    s.height = p.wr.h + 'px';
    if (app.invalidateGrips) app.invalidateGrips();   // the band cap depends on the tile's size
    app.syncGrips(p);
    app.refitSplit(p);
  }

  /** Take a tile down: grips first, then its own teardown, then its document subscriptions. */
  function disposePane(p) {
    return disposeTile(p, {
      dropGrips: (q) => app.dropGrips(q),
      documents: () => app.documents(),
      warn: (msg, e) => console.warn(msg, e && e.message),
    });
  }

  /** Remove it from the workspace — or refuse, out loud, for a kind that may not be closed. */
  function removePane(p) {
    return removeTile(p, {
      tiles: panes, clusters, noClose: app.NOCLOSE,
      dispose: (q) => disposePane(q),
      detachEl: (q) => q.el.remove(),
      onRefused: (q) => app.toast('the ' + q.kind + ' tile stays in the workspace'),
      onRemoved: () => { if (app.markDirty) app.markDirty(); },
      save: () => app.save(),
    }) === 'closed';
  }

  /* ---------------- the clusters ---------------- */
  const CLUSTERS = createClusters({
    world, doc: document, panes, clusters,
    setRect: (q) => setRect(q),
    makeTile: (id, kind, wr, home, state) => app.makeTile(id, kind, wr, home, state),
    toast: (m) => app.toast(m),
    save: () => app.save(),
    markDirty: () => { if (app.markDirty) app.markDirty(); },
    updateLOD: () => app.updateLOD(),
    cam,
    dragSash: (c, i, e, el) => app.startSashDrag(c, i, e, el),
    tiling: {
      // asked through the tiling module with the cluster's own frame rect, so "under" and
      // "nearest" cannot disagree about the geometry
      under: (r) => underCluster(r, clusters, { frameOf: (c) => CLUSTERS.rect(c) }),
      nearest: (r) => nearCluster(r, clusters),
    },
  });

  return {
    setRect, disposePane, removePane, CLUSTERS,
    frameBox: (x, y, w, h, label) => CLUSTERS.frameBox(x, y, w, h, label),
    syncFrame: (c) => CLUSTERS.syncFrame(c),
    relayout: (c) => CLUSTERS.relayout(c),
    addCluster: (ox, oy, kinds, box, reuse, stateByKind) => CLUSTERS.add(ox, oy, kinds, box, reuse, stateByKind),
    dockInto: (c, p) => CLUSTERS.dockInto(c, p),
    detachFromCluster: (p) => CLUSTERS.detach(p),
    clusterFreeTiles: () => CLUSTERS.freeTiles(),
    tidyClusters: () => CLUSTERS.tidy(),
    sashFor: (p, edge) => CLUSTERS.sashFor(p, edge),
    clusterRect: (c) => CLUSTERS.rect(c),
    clusterUnder: (r) => CLUSTERS.under(r),
    nearestCluster: (p) => CLUSTERS.nearest(p),
    clearFrames: () => CLUSTERS.clearFrames(),
    moveSash: (c, i, e, el) => CLUSTERS.moveSash(c, i, e, el),
  };}
