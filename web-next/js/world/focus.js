/*
 * focus.js — flying to a tile, and giving one tile the whole viewport.
 *
 * Extracted from the inline script in web-next/index.html. Two gestures that look alike and
 * are not:
 *
 *   FOCUS       the camera moves; nothing about the workspace changes. You keep your
 *               place in the world, and the tile you asked for is in the middle of the
 *               screen.
 *
 *   FULLSCREEN  the tile LEAVES its cluster, covers the viewport, and follows the camera
 *               — so it is not the layout that changed, it is the tile's home. Which is
 *               why the tile's previous rect and its previous cluster are remembered
 *               before anything moves, and why "exit" is a restore rather than a resize:
 *               a tile that was fullscreen at 1200x800 in a two-column cluster has to come
 *               back to that column, at that size, in that cluster's tiling.
 *
 * Only one tile can be solo, so a second fullscreen exits the first — and the exit has to
 * happen BEFORE the new tile is measured, or the old solo's restored rect is written over
 * the new one's viewport rect and the two tiles end up on top of each other.
 *
 * The solo tile is the one thing in the workspace that is NOT where its rect says: it is
 * pinned to the viewport, so `wrOf(p)` answers with the rect the workspace knows, not the
 * rect on screen. Every caller that saves or serializes a tile must use it.
 */

/**
 * @param {object} host
 * @param {{x:number,y:number,z:number}} host.cam
 * @param {() => {w:number,h:number}} host.viewport
 * @param {() => object} host.viewportWorld     the world rect that covers the viewport
 * @param {(target:object, ms:number) => void} host.flyTo
 * @param {(p:object) => void} host.setRect
 * @param {(p:object) => void} host.updateLOD
 * @param {(p:object) => void} [host.render]    mount a tile that was built off screen
 * @param {(p:object) => void} [host.measure]   write the tile's screen rect (p.sr)
 * @param {(el:Element) => void} [host.setHot]  the tile under the pointer
 * @param {(m:string) => void} [host.toast]
 * @param {(p:object) => object|null} [host.clusterOf]
 * @param {(p:object) => void} [host.detach]
 * @param {(c:object) => void} [host.relayout]
 * @param {() => void} [host.dirty]
 * @param {() => void} [host.save]
 * @param {Element} [host.body]
 */
export function createFocus(host) {
  const { cam, viewport, viewportWorld, flyTo, setRect, updateLOD } = host;
  const toast = host.toast || (() => {});
  const body = host.body || null;
  let solo = null, saved = null;

  /** Put a tile in the middle of the screen without touching the workspace. */
  function focus(p) {
    if (!p) return false;
    const { w, h } = viewport();
    const cx = p.wr.x + p.wr.w / 2, cy = p.wr.y + p.wr.h / 2;
    flyTo({ z: cam.z, x: w / 2 - cx * cam.z, y: h / 2 - cy * cam.z }, 260);
    if (host.setHot) host.setHot(p.el);
    toast('focus · ' + p.kind);
    return true;
  }

  /** Give one tile the viewport: it leaves its cluster and follows the camera. */
  function fullscreen(p) {
    if (!p) return false;
    if (solo === p) return exit();
    if (solo) exit();                                  // one solo at a time, and first
    const c = (host.clusterOf && host.clusterOf(p)) || null;
    saved = { wr: { x: p.wr.x, y: p.wr.y, w: p.wr.w, h: p.wr.h }, cluster: c };
    if (c && host.detach) host.detach(p);
    Object.assign(p.wr, viewportWorld());
    p.el.classList.add('solo');
    solo = p;
    if (body) body.classList.add('solo');
    setRect(p);
    updateLOD();
    /* a tile that fills the viewport is on screen: a tile built away from it must not
       stay a placeholder in the middle of it */
    if (host.measure) host.measure(p);
    if (host.render && (p.pendingMount || p.lod !== 'live')) host.render(p);
    if (host.dirty) host.dirty();
    toast('fullscreen · ' + (c ? 'Esc or ⛶ puts it back' : 'Esc or ⛶ restores it'));
    return true;
  }

  /** Put the solo tile back where it came from — its rect, and its cluster. */
  function exit() {
    if (!solo) return false;
    const p = solo, was = saved;
    solo = null; saved = null;
    p.el.classList.remove('solo');
    if (body) body.classList.remove('solo');
    Object.assign(p.wr, was.wr);
    setRect(p);
    // put it back in the cluster it came from, and re-tile around it
    if (was.cluster && !was.cluster.panes[p.kind]) {
      was.cluster.panes[p.kind] = p;
      p.home = 'cluster:' + was.cluster.id;
      if (host.relayout) host.relayout(was.cluster);
    }
    updateLOD();
    if (host.dirty) host.dirty();
    if (host.save) host.save();
    return true;
  }

  /** The tile that is filling the viewport, if any. */
  const who = () => solo;

  /** The rect the WORKSPACE knows for a tile: a solo tile is on screen at another size. */
  const wrOf = (p) => (p === solo && saved) ? saved.wr : p.wr;

  return { focus, fullscreen, exit, solo: who, wrOf };
}
