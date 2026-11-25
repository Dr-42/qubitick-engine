/**
 * lod-apply.js — the LOD manager: who is worth drawing, and at what level (M2.13).
 *
 * `js/engine/lod.js` is the POLICY (the hysteresis table, the metric, the cull test) and this
 * is the part that RUNS it: it walks the layout, works out each pane's rect, asks the policy
 * for a level, and tells the two things that care — the element (`data-lod`, which the
 * stylesheets read) and the pane instance (`onLOD`, for a pane that does less when it is
 * small).
 *
 * It was rewritten to do that, because it did not:
 *
 *   THE RECTS WERE A PLACEHOLDER. `computeNodeRect` returned the parent's rect for every
 *   node ("for now, return the parent rect as placeholder"), so every pane got the same
 *   full-viewport rect and every level came out the same. The rects are
 *   js/layout/geometry.js's, which is what the renderer places panes with — one geometry, so
 *   a pane the renderer draws at 120 px cannot be a pane this calls 1440 px.
 *
 *   THE HYSTERESIS READ TWO VARIABLES IT NEVER DEFINED (`frozenBelow`, `cardBelow`), so the
 *   second time a pane's level changed it threw. The thresholds are lod.js's, by import.
 *
 *   IT SET THE LEVELS UNDER THE ROOT'S ID. `processNode(rootId, root.tree, …)` keyed the map
 *   by `main` for the whole tree, so `levelOf(paneId)` answered undefined for every pane.
 *
 * A level is a fact about the pane and the camera, so this recomputes the whole table on a
 * store change, a camera move or a resize, and reports only what CHANGED.
 */

import { computeRects } from '../layout/geometry.js';
import { rectToScreen } from '../app/viewport.js';
import { levelFor } from './lod.js';

/** Every pane instance id the document declares, in document order. */
function paneIdsOf(layout) {
  return Object.keys((layout && layout.panes) || {});
}

/**
 * Create the LOD manager.
 *
 * @param {object} opts
 * @param {object} opts.governor - frame governor
 * @param {object} opts.camera - camera state or controller (js/app/camera.js)
 * @param {object} opts.store - layout store
 * @param {object} opts.instances - instance manager (get(id), _updateLOD(id, level))
 * @param {function} [opts.viewport] - () => {w,h}; defaults to the window
 * @param {function} [opts.onChange] - (paneId, level, prev) => void, after each change
 * @returns {object} {update, levelOf, counts, destroy}
 */
export function createLodManager({ governor, camera, store, instances, viewport, onChange }) {
  const levels = new Map();          // paneId -> level
  let destroyed = false;

  const camState = () => {
    if (!camera) return { x: 0, y: 0, z: 1 };
    if (typeof camera.get === 'function') return camera.get() || { x: 0, y: 0, z: 1 };
    return camera;
  };
  const viewportSize = () => (typeof viewport === 'function'
    ? viewport()
    : { w: (typeof window !== 'undefined' ? window.innerWidth : 1440), h: (typeof window !== 'undefined' ? window.innerHeight : 900) });

  /** Every pane's rect in LAYOUT px, from the document — the renderer's own geometry. */
  function rectsOf(layout) {
    const out = new Map();
    for (const root of (layout.roots || [])) {
      if (!root || !root.tree) continue;
      let rects;
      try {
        rects = computeRects(root.tree, { x: root.x || 0, y: root.y || 0, w: root.w || 0, h: root.h || 0 },
                             { tabBarPx: 24, sashHit: 8 });
      } catch (e) { continue; }
      for (const node of (rects.nodes || [])) {
        if (node.kind !== 'leaf' || !node.paneId) continue;   // a leaf names its pane instance
        out.set(node.paneId, node.rect);
      }
    }
    return out;
  }

  function update() {
    if (destroyed) return counts();
    const layout = store.get();
    if (!layout) return counts();

    const cam = camState();
    const vp = viewportSize();
    const rects = rectsOf(layout);
    const ids = paneIdsOf(layout);
    const next = new Map();

    for (const id of ids) {
      const r = rects.get(id);
      const prev = levels.get(id);
      if (!r) { next.set(id, prev || 'live'); continue; }
      /* the policy wants a SCREEN rect: the pane is placed in layout px, and js/app/viewport.js's
         `rectToScreen` is the one place that mapping lives (screen = world*z + x) */
      const screen = rectToScreen(cam, r);
      const { level } = levelFor({ prev, rect: screen, cam, viewport: vp });
      next.set(id, level);
    }

    for (const id of ids) {
      const level = next.get(id);
      const prev = levels.get(id);
      if (level === prev) continue;

      const el = document.querySelector('[data-pane="' + id + '"]');
      if (el) el.dataset.lod = level;

      const inst = instances && instances.get ? instances.get(id) : null;
      if (inst) {
        const pane = inst.paneInstance || inst.instance || inst;
        if (pane && typeof pane.onLOD === 'function') pane.onLOD(level);
        else if (instances._updateLOD) instances._updateLOD(id, level);
      }
      if (typeof onChange === 'function') onChange(id, level, prev);
    }

    levels.clear();
    next.forEach((level, id) => levels.set(id, level));
    return counts();
  }

  function counts() {
    const out = { live: 0, frozen: 0, card: 0, culled: 0, total: levels.size };
    levels.forEach((level) => { if (out[level] !== undefined) out[level]++; });
    return out;
  }

  const offs = [];
  const schedule = () => governor.requestFrame('lod', update);
  if (store && store.subscribe) offs.push(store.subscribe(schedule));
  if (camera && camera.state && typeof camera.state.on === 'function') offs.push(camera.state.on(schedule));
  const onResize = () => schedule();
  if (typeof window !== 'undefined') window.addEventListener('resize', onResize);

  return {
    update,
    levelOf: (paneId) => levels.get(paneId),
    counts,
    destroy() {
      destroyed = true;
      for (const off of offs) { try { if (typeof off === 'function') off(); } catch (e) {} }
      if (typeof window !== 'undefined') window.removeEventListener('resize', onResize);
      levels.clear();
    },
  };
}
