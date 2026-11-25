/*
 * lod.js — how much detail a tile is worth, and whether it is worth drawing at all.
 *
 * Extracted from the inline script in web-next/index.html. Nothing here touches the
 * DOM, so the thresholds and the hysteresis can be unit-tested without a browser.
 *
 *   screenRect / onScreen   where a tile's world rect lands, and whether that is on
 *                           screen at all (this is not a preference: a tile nobody
 *                           can see should cost nothing to paint)
 *   pick                    the rung a tile falls on at a given size
 *   setLOD                  apply a rung — the one place that writes the class
 *   updateLOD / recount     the frame's pass over every tile
 *
 * The rungs are size-based, and the numbers are ratios rather than thresholds with a
 * gap: a tile whose edge is 1.12x over a boundary goes UP a rung, and only falls back
 * below 0.9x. That gap is the hysteresis — without it, a tile sitting exactly on a
 * boundary flickers between two rungs as the camera eases.
 */
import { screenRect as rectOnScreen, onScreen as rectOnScreenCheck } from './viewport.js';

export const RK = { card: 0, frozen: 1, live: 2 };
export const TH = [110, 190];

/** `size` is the tile's smallest drawn edge in screen px (height feeds in as zoom*360). */
export function pick(size, cur) {
  let r = cur === 'culled' ? 0 : RK[cur];
  while (r < 2 && size > TH[r] * 1.12) r++;
  while (r > 0 && size < TH[r - 1] * .9) r--;
  return ['card', 'frozen', 'live'][r];
}

/* The two viewport questions are arithmetic and live in viewport.js, next to the rest of
   the camera maths. These are the pane-shaped ways of asking them, kept because LOD is
   what asks them on every frame and the call sites read better as `LOD.screenRect(p)`. */
export const screenRect = (p, cam) => rectOnScreen(p.wr, cam);
export const onScreen = (r, vw, vh, m = 0) => rectOnScreenCheck(r, vw, vh, m);

/** The class is the rung; `off` is the second, independent axis (seen or not).
 *
 * Only those two axes are THIS module's. `solo` belongs to js/world/focus.js, and writing
 * `className` whole dropped it: a tile that went fullscreen while its rung changed lost its
 * solo class on the next frame and stayed at the viewport rect, unstyled, until something
 * exited a state it could no longer see. So the classes are edited, not replaced. */
const RUNGS = ['lod-live', 'lod-frozen', 'lod-card', 'lod-culled'];
export function setLOD(p, l) {
  p.lod = l;
  p.el.classList.remove(...RUNGS);
  p.el.classList.add('lod-' + l);
  p.el.classList.toggle('off', p.visible === false);
  p.head.lastChild.textContent = l;
  if (l === 'card') p.card.lastChild.textContent = p.summary ? p.summary() : '';
  if (p.implDraw && l === 'live') p.implDraw();
}

/** One pass over the tiles: visibility, rung, and the counts the HUD shows. */
export function updateLOD({ panes, cam, vw, vh, lodOn, visibleOnly, margin = 300 }) {
  const cn = { live: 0, frozen: 0, card: 0, culled: 0 };
  let drawn = 0, built = 0;
  for (const p of panes) {
    const r = p.sr = screenRect(p, cam);
    // on screen at all? (margin 0: the tile's own rect against the viewport)
    const vis = !visibleOnly || onScreen(r, vw, vh, 0);
    if (vis !== p.visible) { p.visible = vis; p.el.classList.toggle('off', !vis); }
    let l = 'live';
    if (lodOn) l = onScreen(r, vw, vh, margin) ? pick(Math.min(r.w, r.h, cam.z * 360), p.lod) : 'culled';
    if (l !== p.lod) setLOD(p, l);
    cn[p.lod]++;
    if (p.visible) drawn++;
    if (!p.pendingMount) built++;
  }
  return { counts: cn, drawn, built, total: panes.length };
}

/* after the mounts, so "built" describes the frame the user is about to see */
export function recount({ panes }) {
  let drawn = 0, built = 0;
  for (const p of panes) { if (p.visible) drawn++; if (!p.pendingMount) built++; }
  return { drawn, built, total: panes.length };
}
