/*
 * tile-life.js — taking a tile apart.
 *
 * Extracted from the inline script in web-next/index.html. Closing a tile is the one path
 * where a mistake is INVISIBLE: a tile that leaves a subscription behind keeps receiving
 * every change made to its document, and a dead view that throws while receiving one used
 * to abort the whole fan-out — so the bug showed up as "editing stopped working", three
 * documents later, with nothing pointing at the tile that was closed.
 *
 * So the teardown is written down in one place, in a fixed order, and the order is the
 * point:
 *
 *   1. idempotent           disposing twice is not an error: the second call does nothing.
 *   2. grips first          they are separate world children holding a pointer capture;
 *                           a grip left behind keeps grabbing for a tile that is gone.
 *   3. the pane's unmount   a pane may have its own listeners (the editor does), so it is
 *                           asked to clean up — and a throw from it must not stop the rest.
 *   4. the subscriptions    the document, then the active-file watcher.
 *   5. the view list        leave the document's list, or the dead view hears everything.
 *   6. the back-references  patched to null so a stale handle cannot write to a dead tile.
 *
 * Removing it from the workspace is a separate concern and stays with the host: which list
 * it is in, which cluster was holding it, and whether the kind is allowed to be closed.
 */

/**
 * @param {object} p     the tile
 * @param {object} deps
 * @param {(p) => void} deps.dropGrips      remove the eight handles
 * @param {() => Iterable} deps.documents   every document, to leave any view list
 * @param {(msg, err) => void} [deps.warn]
 */
export function disposeTile(p, { dropGrips, documents, warn } = {}) {
  if (!p || p._disposed) return false;      // 1. idempotent
  p._disposed = true;

  if (dropGrips) { try { dropGrips(p); } catch (e) { warn && warn('[tile] grips', e); } }   // 2.

  if (typeof p.onUnmount === 'function') {                                                     // 3.
    try { p.onUnmount(); } catch (e) { warn && warn('[tile] unmount', e); }
  }

  for (const key of ['_docOff', '_actOff']) {                                                  // 4.
    const off = p[key];
    if (typeof off === 'function') {
      try { off(); } catch (e) { warn && warn('[tile] unsubscribe', e); }
    }
    p[key] = null;
  }

  if (p.view && documents) {                                                                   // 5.
    for (const d of documents()) {
      if (d && d.views && typeof d.views.delete === 'function') d.views.delete(p.view);
    }
  }

  p.retarget = null; p.chrome = null; p.pinPaint = null; p.ed = null; p.doc = null;            // 6.
  return true;
}

/**
 * Take a tile out of the workspace.
 *
 * The unclosable kinds are checked here rather than by hiding their ✕, because a saved
 * document or a keyboard path can reach this too: a workspace without its own control
 * surface cannot be repaired from inside itself.
 *
 * A cluster loses the tile ONLY if that cluster is the one holding it. Matching on kind
 * alone deleted a cluster's own tile whenever an unrelated free tile of the same kind was
 * closed, which quietly dropped it from the saved layout.
 *
 * @returns {'closed'|'refused'|'absent'}
 */
export function removeTile(p, {
  tiles, clusters = [], noClose, dispose, detachEl, onRemoved, onRefused, save, toast,
} = {}) {
  if (!p) return 'absent';
  const i = tiles ? tiles.indexOf(p) : -1;
  if (tiles && i < 0) return 'absent';
  if (noClose && noClose.has(p.kind)) {
    if (onRefused) onRefused(p);
    else if (toast) toast('the ' + p.kind + ' tile stays in the workspace');
    return 'refused';
  }
  if (dispose) dispose(p);
  if (detachEl) detachEl(p);
  if (tiles && i >= 0) tiles.splice(i, 1);
  if (typeof p._ro === 'object' && p._ro && typeof p._ro.disconnect === 'function') p._ro.disconnect();
  for (const c of clusters) {
    if (c.panes && c.panes[p.kind] === p) delete c.panes[p.kind];
  }
  if (onRemoved) onRemoved(p);
  if (save) save();
  return 'closed';
}