/*
 * tile-factory.js — where a new tile goes, and what it is called.
 *
 * Extracted from the inline script in web-next/index.html, where this arithmetic was
 * written twice: once as `nextFreeId` and once again inside `addFreeTile`. Two copies of
 * "make me an id" is how two tiles end up sharing one, and an id that two panes share makes
 * every wire to it a coin toss — the wire layer resolves by id.
 *
 * The rules, both of them learned the hard way:
 *
 *   · an id is `<counter>:<kind>`, and the counter is PER KIND, so the hundredth graph tile
 *     does not carry the number the first stats tile has;
 *   · the counter restarts with the page, but a restored workspace may already own
 *     `t1:stats` — so an id is only taken if nothing holds it, and the search continues
 *     until one is free. (A restored layout is not a rare case: it is every reload.)
 *
 * A new tile appears NEAR the middle of what you are looking at, stepped a little for each
 * one so a burst of tiles does not stack into a single pile. The step is in world px and
 * scaled by the zoom, so the cascade is the same size on screen whatever the camera is
 * doing — the same space rule as everything else that has to be laid out and then looked at.
 */

/** The id format, in one place. */
export const tileId = (n, kind) => 't' + n + ':' + kind;

/**
 * An id of this kind that nothing already holds.
 * @param {string} kind
 * @param {object} o
 * @param {Iterable<string>} [o.taken]   every id in the workspace
 * @param {number} [o.from]              the counter to start from (the kind's own sequence)
 * @returns {{ id: string, counter: number }}
 */
export function freeTileId(kind, { taken = [], from = 0 } = {}) {
  const held = taken instanceof Set ? taken : new Set(taken);
  let n = from;
  let id;
  do { id = tileId(++n, kind); } while (held.has(id));
  return { id, counter: n };
}

/**
 * Where a new tile goes: in the middle of what you are looking at, stepping for each tile
 * so a burst fans out instead of stacking.
 *
 * The camera is a translation and a scale (`screen = world * z + cam`), so the world point
 * under the top-left of the viewport is `-cam.x / z` — NEGATIVE. This arithmetic shipped
 * with the sign the wrong way round for a long time: a tile asked for near the middle of
 * the workspace landed mirrored about the origin, which is to say off screen, and the
 * app answered "+ stats tile" while nothing appeared. The step is added in world px AFTER
 * the camera offset is divided by z, so a burst is the same size on screen at any zoom.
 *
 * @param {{x:number,y:number,z:number}} cam
 * @param {number} seq  how many of this kind already came this way (its counter)
 * @param {object} [o]  { step = 40, slots = 6, size = {w:380,h:280}, view = {w,h} | null }
 */
export function placementFor(cam, seq = 0, { step = 40, slots = 6, size = { w: 380, h: 280 }, view = null } = {}) {
  const z = cam.z || 1;
  const i = Math.max(1, seq);
  const off = step * (i % slots);              // 40 layout px: 40px on screen at any zoom
  // with a viewport, centre the tile in it; without one, fall back to the view's corner
  const cx = view ? (-cam.x + (view.w - size.w) / 2) / z : -cam.x / z;
  const cy = view ? (-cam.y + (view.h - size.h) / 2) / z : -cam.y / z;
  return {
    x: Math.round(cx) + off,
    y: Math.round(cy) + off,
    w: size.w,
    h: size.h,
  };
}