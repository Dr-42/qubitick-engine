/*
 * tile-gestures.js — what happens while the pointer is down on a tile.
 *
 * Extracted from the inline script in web-next/index.html, where three gestures were
 * written out three times with the same twelve lines around each one: take the pointer
 * capture, remember the start, listen for move, count the move, drop the listeners, stop
 * saving sixty times a second. Every copy of that boilerplate is one place to forget
 * `releasePointerCapture` — or the `drag` class, which is what the cursor reads — and the
 * symptom of forgetting it is a tile that keeps resizing after the button is up.
 *
 * The gestures themselves are two rules, and both of them are about a boundary that must
 * not move when it was not grabbed:
 *
 *   RESIZE  the edge under the pointer moves; the opposite edge stays exactly put. So a
 *           drag on `w` writes BOTH `x` and `w`, and the clamp that stops a tile at its
 *           minimum has to put `x` back (at `x + w - min`) rather than leaving the far
 *           side to drift. The clamp runs TWICE — once on the raw drag, once after
 *           snapping — because snapping moves the edge onto a neighbour's line, and a
 *           neighbour's line can be nearer than the minimum. Without the second clamp a
 *           tile snaps itself shut.
 *
 *   MOVE    the tile follows the pointer, and what the drop MEANS depends on where it
 *           landed: on a cluster it docks (and the cluster re-tiles around it), anywhere
 *           else it is free. The host answers that question; the gesture only asks.
 *
 * An edge has three possible owners, and the host decides before calling in: a free tile
 * owns its rect (`free`), a tiled tile's edge belongs to the split tree's sash (the host
 * routes that one away), and an edge no sash owns is the cluster's own boundary
 * (`extent`). The vocabulary stays the same — `n`, `s`, `e`, `w`, and the corners — so
 * one grip table in js/world/grips.js feeds all three.
 */

/** A tile never collapses. */
export const RESIZE_MIN = { w: 220, h: 150 };
/** A cluster never collapses either, and it holds tiles inside it. */
export const CLUSTER_MIN = { w: 320, h: 240 };

/** Which sides of the box an edge string means. `ne` moves both, `e` moves one. */
export function edgeSides(edge) {
  const s = String(edge || '');
  return { e: s.includes('e'), w: s.includes('w'), s: s.includes('s'), n: s.includes('n') };
}

/**
 * Move a box's edges by a delta, whatever that means for the box.
 * @param {{x:number,y:number,w:number,h:number}} o   the box before the drag
 * @param {string} edge
 * @param {number} dx   in the box's own space (the host divides the pointer motion by the zoom)
 * @param {number} dy
 */
export function applyEdge(o, edge, dx, dy) {
  const s = edgeSides(edge);
  const r = { x: o.x, y: o.y, w: o.w, h: o.h };
  if (s.e) r.w = o.w + dx;
  if (s.w) { r.w = o.w - dx; r.x = o.x + dx; }
  if (s.s) r.h = o.h + dy;
  if (s.n) { r.h = o.h - dy; r.y = o.y + dy; }
  return r;
}

/**
 * Stop the box at its minimum WITHOUT moving the side that was not grabbed: the grabbed
 * side gives way, so the far side stays where the drag left it.
 * @param {{x:number,y:number,w:number,h:number}} r    mutated in place, returned
 * @param {{x:number,y:number,w:number,h:number}} o    the box before the drag
 */
export function clampResize(r, o, edge, min) {
  const m = min || RESIZE_MIN;
  const s = edgeSides(edge);
  if (r.w < m.w) { if (s.w) r.x = o.x + o.w - m.w; r.w = m.w; }
  if (r.h < m.h) { if (s.n) r.y = o.y + o.h - m.h; r.h = m.h; }
  return r;
}

/** Both steps at once, for the callers that do not snap. */
export function resizeBox(o, edge, dx, dy, min) {
  return clampResize(applyEdge(o, edge, dx, dy), o, edge, min);
}

/**
 * One pointer drag, from capture to release. `move(ev, s)` runs per move event and calls
 * `s.mark()` once the drag counts as a real move (the header drag needs a few px of
 * travel before a jitter counts as leaving its cluster; a resize counts immediately).
 * `end(s)` runs exactly once, after the listeners and the class are gone.
 */
export function captureDrag(el, e, { move, end }) {
  const state = { moved: false, mark() { state.moved = true; } };
  try { el.setPointerCapture(e.pointerId); }
  catch (err) { /* a synthetic pointer may have no capture to take; the drag still works */ }
  el.classList.add('drag');
  const mv = (ev) => move(ev, state);
  const up = () => {
    el.removeEventListener('pointermove', mv);
    el.removeEventListener('pointerup', up);
    el.removeEventListener('pointercancel', up);
    el.classList.remove('drag');
    try { el.releasePointerCapture(e.pointerId); } catch (err) { /* already gone */ }
    end(state);
  };
  el.addEventListener('pointermove', mv);
  el.addEventListener('pointerup', up);
  el.addEventListener('pointercancel', up);
  return state;
}

/**
 * @param {object} host                    what the page knows and the gestures do not
 * @param {{z:number}} host.cam            pointer motion is divided by the zoom
 * @param {(p) => void} host.setRect       write a tile's box to the DOM
 * @param {() => void} host.markDirty
 * @param {() => void} host.settle         after a gesture: the LOD pass, then the save
 * @param {(p) => {w:number,h:number}} [host.minOf]      a tile's own floor (console panes differ)
 * @param {(p, r, edge, ev) => void} [host.snapRect]     snap a resizing edge, draw the guides
 * @param {(p, x, y, ev) => {x:number,y:number}} [host.snapPos]  snap a moved tile
 * @param {() => void} [host.clearGuides]
 * @param {(p) => void} [host.detach]                    leave whatever cluster holds it
 * @param {(r) => object|null} [host.clusterUnder]
 * @param {(c, p) => boolean} [host.dockInto]
 * @param {(m:string) => void} [host.toast]
 * @param {(p) => object|null} [host.clusterOf]          for a tiled tile's outer edge
 * @param {(c) => void} [host.relayout]
 * @param {{w:number,h:number}} [host.extentMin]
 */
export function createTileGestures(host) {
  const { cam, setRect, markDirty, settle } = host;
  const minOf = host.minOf || (() => RESIZE_MIN);
  const extentMin = host.extentMin || CLUSTER_MIN;
  const clearGuides = host.clearGuides || (() => {});
  const z = () => Math.max(cam.z || 1, 0.005);

  /** Drag a tile's header: it follows the pointer, and the drop decides where it lives. */
  function move(p, e) {
    if (e.target.closest('button')) return;      // a button in the head is a button
    e.preventDefault(); e.stopPropagation();
    const el = p.el, sx = e.clientX, sy = e.clientY, ox = p.wr.x, oy = p.wr.y;
    captureDrag(el, e, {
      move(ev, s) {
        const dx = (ev.clientX - sx) / z(), dy = (ev.clientY - sy) / z();
        let nx = ox + dx, ny = oy + dy;
        if (host.snapPos && !ev.shiftKey) { const r = host.snapPos(p, nx, ny, ev); nx = r.x; ny = r.y; }
        else clearGuides();
        p.wr.x = nx; p.wr.y = ny;
        setRect(p);
        if (Math.abs(dx) + Math.abs(dy) > 3) s.mark();
        markDirty();
      },
      end(s) {
        clearGuides();
        if (!s.moved) return;
        if (host.detach) host.detach(p);
        // dropped onto a cluster: take it in and re-tile around it
        const c = host.clusterUnder && host.clusterUnder(p.wr);
        if (c && host.dockInto && host.dockInto(c, p)) {
          host.toast && host.toast(p.kind + ' docked into ' + c.id);
          settle();
          return;
        }
        settle();
        host.toast && host.toast(p.kind + ' moved (free tile)');
      },
    });
  }

  /** Resize a free tile's own rect from one of its edges. */
  function free(e, p, edge) {
    e.preventDefault(); e.stopPropagation();
    const el = e.currentTarget, sx = e.clientX, sy = e.clientY, o = { x: p.wr.x, y: p.wr.y, w: p.wr.w, h: p.wr.h };
    const min = minOf(p);
    captureDrag(el, e, {
      move(ev, s) {
        const dx = (ev.clientX - sx) / z(), dy = (ev.clientY - sy) / z();
        const r = clampResize(applyEdge(o, edge, dx, dy), o, edge, min);
        if (host.snapRect) host.snapRect(p, r, edge, ev); else clearGuides();
        clampResize(r, o, edge, min);                 // the second clamp: snapping can shut a tile
        p.wr.x = Math.round(r.x); p.wr.y = Math.round(r.y);
        p.wr.w = Math.round(r.w); p.wr.h = Math.round(r.h);
        setRect(p); s.mark(); markDirty();
      },
      end(s) { clearGuides(); if (s.moved) settle(); },
    });
  }

  /** An edge of a tiled tile that no sash owns is the cluster's own boundary: the
   *  cluster grows and everything inside it re-tiles to the new box. */
  function extent(e, p, edge) {
    const c = host.clusterOf && host.clusterOf(p);
    if (!c) return false;
    e.preventDefault(); e.stopPropagation();
    const el = e.currentTarget, sx = e.clientX, sy = e.clientY, o = { x: c.x, y: c.y, w: c.w, h: c.h };
    captureDrag(el, e, {
      move(ev, s) {
        const dx = (ev.clientX - sx) / z(), dy = (ev.clientY - sy) / z();
        const r = resizeBox(o, edge, dx, dy, extentMin);
        c.x = Math.round(r.x); c.y = Math.round(r.y);
        c.w = Math.round(r.w); c.h = Math.round(r.h);
        if (host.relayout) host.relayout(c);
        markDirty(); s.mark();
      },
      end(s) { if (s.moved) settle(); },
    });
    return true;
  }

  return { move, free, extent };
}
