/*
 * clusters.js — a tiled cluster: its frame, its sashes, and where its tiles go.
 *
 * Extracted from the inline script in web-next/index.html. A cluster is a box holding one
 * pane per kind, laid out as a split tree; the TREE is js/app/tiling.js (pure, tested) and
 * this file is everything around it: the frame that names the cluster, the sash elements a
 * drag grabs, and the operations that change what a cluster contains.
 *
 * Three rules that were learned by watching the canvas misbehave:
 *
 *   A FRAME IS NOT A TILE. The frame and its label are plain children of the world, drawn
 *     BEHIND the tiles, and they are the cluster's own (`x,y,w,h` plus 32×48 of furniture).
 *     Anything that walks the tiles must not walk the frames, and anything that rebuilds the
 *     world has to remove the frames it orphans — a stale frame sits ON TOP of the new panes.
 *
 *   A CLUSTER IS NOT ITS KINDS. `dockInto` refuses a kind the cluster already has, and
 *     removing a tile takes it out of the one cluster that actually holds it (`panes[kind]
 *     === p`), not every cluster with that kind. Matching on kind alone deleted a cluster's
 *     own tile whenever an unrelated free tile of the same kind was removed.
 *
 *   TIDY IS IDEMPOTENT AND SAYS SO. Re-tiling a cluster that is already tiled changes
 *     nothing, and the operation reports that rather than appearing to have done something.
 */

import { autoTree as buildTree, solveRects } from '../app/tiling.js';

/** The frame's furniture: a cluster's frame is 32 wider and 48 taller than its tiles. */
export const FRAME_PAD = { w: 32, h: 48 };
/** The margin the tiles keep inside their frame. */
export const INNER = { x: 16, y: 32 };
/** A cluster never collapses below this. */
export const CLUSTER_MIN = { w: 320, h: 240 };

/** The kinds a new cluster is made of when nobody says. */
export const DEFAULT_KINDS = ['tree', 'graph', 'stats', 'symbols', 'editor', 'render', 'goal'];

/**
 * @param {object} host
 * @param {Element} host.world        where frames and sashes live
 * @param {Document} [host.doc]
 * @param {Array} host.panes          the tile list (read and written)
 * @param {Array} host.clusters       the cluster list (pushed to)
 * @param {(p) => void} host.setRect  place a tile from its `wr`
 * @param {(id, kind, wr, home, state) => object} host.makeTile
 * @param {(msg: string) => void} host.toast
 * @param {() => void} host.save
 * @param {() => void} [host.markDirty]
 * @param {() => void} [host.updateLOD]
 * @param {{z:number}} host.cam       the sash drag needs the zoom
 * @param {(c: object, i: number, e: PointerEvent, el: Element) => void} host.dragSash
 */
export function createClusters(host) {
  const { world, panes, clusters } = host;
  const doc = host.doc || document;

  /** A cluster's outer frame: its tiles plus the furniture. */
  const rect = (c) => ({ x: c.x, y: c.y, w: c.w + FRAME_PAD.w, h: c.h + FRAME_PAD.h });

  /** The frame and its label, as world children BEHIND the tiles. */
  function frameBox(x, y, w, h, label) {
    const f = doc.createElement('div');
    f.className = 'frame';
    Object.assign(f.style, { left: x + 'px', top: y + 'px', width: w + 'px', height: h + 'px' });
    world.prepend(f);
    const l = doc.createElement('div');
    l.className = 'frame-label';
    l.textContent = label;
    Object.assign(l.style, { left: x + 6 + 'px', top: y - 4 + 'px', transform: 'translateY(-100%)' });
    world.prepend(l);
    return { frame: f, label: l };
  }

  /** Put a cluster's frame in step with its tiles, making it the first time. */
  function syncFrame(c) {
    if (!c) return;
    const box = rect(c);
    const text = 'root ' + c.id + ' · tiled cluster (' + Object.keys(c.panes || {}).length + ' tiles)';
    if (!c.frameEl) {
      const made = frameBox(box.x, box.y, box.w, box.h, text);
      c.frameEl = made.frame;
      c.labelEl = made.label;
      return;
    }
    Object.assign(c.frameEl.style, { left: box.x + 'px', top: box.y + 'px', width: box.w + 'px', height: box.h + 'px' });
    if (c.labelEl) {
      Object.assign(c.labelEl.style, { left: (box.x + 6) + 'px', top: (box.y - 4) + 'px' });
      if (c.labelEl.textContent !== text) c.labelEl.textContent = text;
    }
  }

  /**
   * Apply the split tree: every tile gets its box, and the sashes are placed on the cuts.
   * The sash elements are created once per position and reused, because a relayout runs on
   * every frame of a drag.
   */
  function relayout(c) {
    syncFrame(c);
    const { rects, sashes } = solveRects(c.tree, { x: c.x + INNER.x, y: c.y + INNER.y, w: c.w, h: c.h });
    for (const k in c.panes) {
      c.panes[k].wr = rects[k];
      host.setRect(c.panes[k]);
    }
    c.sd = sashes;
    sashes.forEach((s, i) => {
      let el = c.sashEls[i];
      if (!el) {
        el = c.sashEls[i] = doc.createElement('div');
        world.appendChild(el);
        el.addEventListener('pointerdown', (e) => host.dragSash && host.dragSash(c, i, e, el));
      }
      el.className = 'sash ' + s.dir;
      Object.assign(el.style, { left: s.x + 'px', top: s.y + 'px', width: s.w + 'px', height: s.h + 'px' });
    });
  }

  /**
   * Make a cluster. `reuse` adopts a tile the workspace already has (that is how "cluster
   * these" keeps the panes you arranged) instead of building a new one for that kind.
   */
  function add(ox, oy, kinds, box, reuse, stateByKind) {
    const list = (kinds && kinds.length) ? kinds.slice() : DEFAULT_KINDS.slice();
    const w = (box && box.w) || (list.length > 6 ? 1776 : 1400);
    const h = (box && box.h) || (list.length > 6 ? 1000 : 820);
    const c = { id: 'c' + clusters.length, x: ox, y: oy, w, h, tree: buildTree(list, w, h), panes: {}, sashEls: [], sd: [] };
    const made = frameBox(ox, oy, c.w + FRAME_PAD.w, c.h + FRAME_PAD.h,
      'root ' + c.id + ' · tiled cluster (' + list.length + ' tiles)');
    c.frameEl = made.frame;
    c.labelEl = made.label;
    for (const k of list) {
      const taken = reuse && reuse[k];
      if (taken && !String(taken.home || '').startsWith('cluster')) {
        taken.home = 'cluster:' + c.id;
        c.panes[k] = taken;
      } else {
        c.panes[k] = host.makeTile(c.id + ':' + k, k, { x: 0, y: 0, w: 10, h: 10 }, 'cluster:' + c.id, stateByKind && stateByKind[k]);
      }
    }
    relayout(c);
    clusters.push(c);
    if (host.markDirty) host.markDirty();
    return c;
  }

  /** Put a loose tile into a cluster, re-tiling the cluster around it. */
  function dockInto(c, p) {
    if (!c || !p || c.panes[p.kind]) return false;      // a kind appears once per cluster
    c.panes[p.kind] = p;
    p.home = 'cluster:' + c.id;
    c.tree = buildTree(Object.keys(c.panes), c.w, c.h);
    relayout(c);
    return true;
  }

  /** Take a tile out of whatever cluster holds it, without removing the tile. */
  function detach(p) {
    p.home = 'free';
    for (const c of clusters) {
      if (c.panes && c.panes[p.kind] === p) { delete c.panes[p.kind]; return c; }
    }
    return null;
  }

  /** The cluster whose frame overlaps this rect the most (or null). */
  const under = (r) => host.tiling
    ? host.tiling.under(r)
    : null;

  /** The cluster whose centre is nearest a tile, for gathering a stray. */
  const nearest = (p) => host.tiling
    ? host.tiling.nearest(p.wr)
    : null;

  /** Gather every loose tile into one cluster, tiled by kind. */
  function freeTiles() {
    const free = panes.filter((p) => p && !String(p.home || '').startsWith('cluster') && p.el.isConnected);
    if (free.length < 2) { host.toast('need at least two loose tiles'); return null; }
    const reuse = {};
    for (const p of free) if (reuse[p.kind] === undefined) reuse[p.kind] = p;
    const kinds = Object.keys(reuse);
    if (kinds.length < 2) { host.toast('those tiles are all the same kind'); return null; }
    const x0 = Math.min(...free.map((p) => p.wr.x)), y0 = Math.min(...free.map((p) => p.wr.y));
    const cols = Math.ceil(Math.sqrt(kinds.length)), rows = Math.ceil(kinds.length / cols);
    return add(Math.round(x0), Math.round(y0), kinds, { w: 460 * cols, h: 340 * rows }, reuse);
  }

  /**
   * Re-tile every cluster from the kinds it holds, take in every loose tile, and put every
   * frame back in step. It reports what it did, including "nothing" — a control that
   * silently does nothing reads as broken.
   */
  function tidy() {
    let retiled = 0, docked = 0;
    const touched = new Set();
    for (const c of clusters) {
      const kinds = Object.keys(c.panes);
      if (kinds.length < 2) continue;
      const tree = buildTree(kinds, c.w, c.h);
      if (JSON.stringify(tree) !== JSON.stringify(c.tree)) { c.tree = tree; touched.add(c); }
      relayout(c);                                   // also syncs the frame
    }
    for (const p of panes.filter((q) => !String(q.home || '').startsWith('cluster'))) {
      const c = under(p.wr) || nearest(p);
      if (c && dockInto(c, p)) { docked++; touched.add(c); }
    }
    for (const c of clusters) syncFrame(c);
    retiled = touched.size;
    if (host.markDirty) host.markDirty();
    if (retiled || docked) host.save();
    const said = retiled ? ('re-tiled ' + retiled + ' cluster' + (retiled === 1 ? '' : 's')) : '';
    const dockedSaid = docked ? ((said ? ', ' : '') + 'docked ' + docked + ' loose tile' + (docked === 1 ? '' : 's')) : '';
    host.toast(!retiled && !docked ? 'already tidy' : ('tidy: ' + said + dockedSaid));
    return retiled;
  }

  /**
   * Move the ratio a sash owns. Only that one ratio changes — which is the whole point of a
   * tree — and the move is clamped so neither side can be squashed flat.
   */
  function moveSash(c, i, e, el = null) {
    const sw = c.sd[i];
    if (!sw || !sw.node) return;
    const target = el || c.sashEls[i];
    if (!target) return;
    e.preventDefault();
    e.stopPropagation();
    target.setPointerCapture(e.pointerId);
    target.classList.add('drag');
    const n = sw.node, start = n.r, sx = e.clientX, sy = e.clientY;
    const size = sw.dir === 'h' ? sw.pw : sw.ph;
    const z = (host.cam && host.cam.z) || 1;
    const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
    const mv = (ev) => {
      const d = (sw.dir === 'h' ? ev.clientX - sx : ev.clientY - sy) / z;
      n.r = clamp(start + d / size, 0.06, 0.94);
      relayout(c);
      if (host.markDirty) host.markDirty();
    };
    const up = () => {
      target.classList.remove('drag');
      target.removeEventListener('pointermove', mv);
      target.removeEventListener('pointerup', up);
      if (host.updateLOD) host.updateLOD();
      host.save();
    };
    target.addEventListener('pointermove', mv);
    target.addEventListener('pointerup', up);
  }

  /** A tile is gone: make sure no cluster still lists it. */
  function forget(p) {
    for (const c of clusters) if (c.panes && c.panes[p.kind] === p) delete c.panes[p.kind];
  }

  /** Remove every frame and sash from the world (a restore or a default view does this). */
  function clearFrames() {
    world.querySelectorAll('.frame,.frame-label,.sash').forEach((el) => el.remove());
  }

  /** The sash of this tile's cluster that lies along `edge`, if there is one. */
  function sashFor(p, edge, tol = 4) {
    const c = clusters.find((k) => String(p.home || '') === 'cluster:' + k.id);
    if (!c) return null;
    const axis = (edge.includes('e') || edge.includes('w')) ? 'h' : 'v';
    const r = p.wr;
    const at = edge.includes('e') ? r.x + r.w : edge.includes('w') ? r.x : edge.includes('s') ? r.y + r.h : r.y;
    let best = null;
    for (let i = 0; i < c.sd.length; i++) {
      const s = c.sd[i];
      if (s.dir !== axis) continue;
      const line = s.dir === 'h' ? s.x + s.w / 2 : s.y + s.h / 2;
      const along = s.dir === 'h' ? [r.y, r.y + r.h] : [r.x, r.x + r.w];
      const span = s.dir === 'h' ? [s.y, s.y + s.h] : [s.x, s.x + s.w];
      if (Math.abs(line - at) > tol / ((host.cam && host.cam.z) || 1)) continue;   // not this edge
      const ov = Math.min(along[1], span[1]) - Math.max(along[0], span[0]);
      if (ov <= 0) continue;                                                      // not along it
      if (!best || ov > best.ov) best = { c, i, ov };
    }
    return best;
  }

  return {
    FRAME_PAD, INNER, CLUSTER_MIN,
    frameBox, rect, syncFrame, relayout, add, dockInto, detach, freeTiles, tidy,
    moveSash, forget, clearFrames, sashFor, under, nearest,
  };
}