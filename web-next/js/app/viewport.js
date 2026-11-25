/*
 * viewport.js — the camera as arithmetic: where a world point lands, how a rect is fitted, how
 * a zoom about a point keeps that point still. ONE geometry for both apps.
 *
 * Extracted from the inline script in web-next/index.html; the engine's own copy of the same
 * arithmetic (js/engine/camera.js) was folded into it, because the two disagreed.
 *
 *   THE CAMERA IS {x, y, z}, AND THE DOM SAYS WHAT THOSE MEAN. The world element is written
 *   `transform: translate(x, y) scale(z)`, so a world point lands at
 *   `screen = world*z + (x, y)` — a SCREEN-space translation and a scale. Everything below is
 *   written against that line, and the engine's copy was not: it read x/y as a world-space
 *   origin (`screen = (world - x)*z`), which is the same camera only while x = y = 0. A fit
 *   computed there put the content off screen by exactly the pan it asked for.
 *
 * The animation frames, the transform write and the side effects of a camera move (grips, LOD,
 * saves) belong to the app that owns the loop; this module only answers where the camera goes.
 */

/** The zoom limits. 4% is "the whole corpus", 500% is "read one line". */
export const ZMIN = 0.04, ZMAX = 5;

/** Where a world rect lands on screen. */
export const screenRect = (r, cam) => ({ x: r.x * cam.z + cam.x, y: r.y * cam.z + cam.y, w: r.w * cam.z, h: r.h * cam.z });

/** The world point under a screen point. */
export const worldAt = (sx, sy, cam) => ({ x: (sx - cam.x) / cam.z, y: (sy - cam.y) / cam.z });

/**
 * Is any part of this SCREEN rect inside the viewport (grown by `m` px of margin)?
 * A tile nobody can see should cost nothing to paint, so this is asked every frame.
 *
 * The test is INCLUSIVE at both edges: a tile that begins exactly at the right edge, or
 * ends exactly at the left one, counts as visible. That is the boundary the frame loop
 * has always used, and it is the safer side of it — an exact-edge tile that flickers
 * between "drawn" and "not drawn" is a worse bug than one drawn at zero size.
 */
export const onScreen = (r, vw, vh, m = 0) =>
  !(r.x > vw + m || r.y > vh + m || r.x + r.w < -m || r.y + r.h < -m);

/** The union of a list of world rects. An empty list has no box — the caller decides
 *  what that means (the page frames nothing, rather than a zero-sized box at 0,0). */
export function bboxOf(rects){
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for(const r of rects){
    if(r.x < x0) x0 = r.x;
    if(r.y < y0) y0 = r.y;
    if(r.x + r.w > x1) x1 = r.x + r.w;
    if(r.y + r.h > y1) y1 = r.y + r.h;
  }
  if(!Number.isFinite(x0)) return null;
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

/** The zoom at which a rect fits a viewport with `pad` px of space all round. */
export function zoomToFit(rect, vw, vh, pad = 40, maxZ = 3, minZ = ZMIN){
  if(!rect || !(rect.w > 0) || !(rect.h > 0)) return minZ;
  const z = Math.min((vw - 2 * pad) / rect.w, (vh - 2 * pad) / rect.h);
  return Math.min(Math.max(z, minZ), maxZ);
}

/** The zoom below which a workspace is a MAP rather than a workspace: at 31.5% a 10.5px label
 *  is 3.3px, and the shipped arrangement used to open there. */
export const Z_READABLE = 0.5;

/**
 * The camera a FIRST RENDER gets: the whole arrangement when it can be read, and otherwise its
 * START at a legible zoom.
 *
 * "Fit everything" is the wrong default for a canvas holding more than a viewport's worth, and the
 * shipped arrangement is one: 13 tiles over 2464x2474 px opened at 31.5% on a 1440x900 screen.
 * GitLab's graph-canvas pattern has the rule for exactly this case: "On first render, fit the graph
 * to view if that results in a zoom of 50% or more. Otherwise render at 50% anchored to the start
 * of the graph, so the first nodes are legible" (design.gitlab.com/patterns/graph-canvas) — and
 * "fit to view never zooms above 100%". So: fit if it is legible, else 50% with the top-left of the
 * arrangement at the inset, the rest a pan away. A workspace smaller than the viewport keeps its
 * own scale instead of being blown up.
 *
 * `camForRect` is deliberately left alone: a fit the USER asks for obeys the hand that asked, and
 * this is only about what a first render chooses.
 */
export function firstViewCam(rect, vw, vh, { pad = 60, maxZ = 1 } = {}){
  const fit = camForRect(rect, vw, vh, { pad, maxZ, minZ: ZMIN });
  if (!rect || !(rect.w > 0) || !(rect.h > 0)) return fit;
  if (fit.z >= Z_READABLE) return fit;
  return { z: Z_READABLE, x: pad - rect.x * Z_READABLE, y: pad - rect.y * Z_READABLE };
}

/**
 * A camera that frames `rect`, centred, with `pad` px of space all round it and no more
 * zoom than `maxZ`. The pad is not scaled: it is the breathing room on screen, which is
 * what the eye is judging.
 */
export function camForRect(rect, vw, vh, { pad = 40, maxZ = 3, minZ = ZMIN } = {}){
  const z = zoomToFit(rect, vw, vh, pad, maxZ, minZ);
  return { z, x: (vw - rect.w * z) / 2 - rect.x * z, y: (vh - rect.h * z) / 2 - rect.y * z };
}

/**
 * Zoom by `f` while keeping the world point under (mx,my) exactly there.
 *
 * This is the one camera rule that is easy to get subtly wrong: the offset has to be
 * rescaled by the ratio of the NEW zoom to the old, not applied to the old one. Asking
 * for 5% and getting 5.02% at the edge of the viewport is what that mistake looks like.
 */
export function zoomAbout(cam, mx, my, f, minZ = ZMIN, maxZ = ZMAX){
  const z = Math.min(maxZ, Math.max(minZ, cam.z * f));
  return { z, x: mx - (mx - cam.x) * z / cam.z, y: my - (my - cam.y) * z / cam.z };
}

/**
 * The camera that keeps the viewport centre on the same world point at a new zoom — the
 * "+"/"-" keys, which zoom about the middle rather than about the pointer.
 */
export function zoomAtCentre(cam, f, vw, vh, minZ = ZMIN, maxZ = ZMAX){
  return zoomAbout(cam, vw / 2, vh / 2, f, minZ, maxZ);
}

/** The world rect the viewport covers right now. */
export const viewportWorld = (cam, vw, vh) => ({
  x: -cam.x / cam.z, y: -cam.y / cam.z, w: vw / cam.z, h: vh / cam.z,
});
/* ---------- the camera as a screen-space translation (js/app/camera.js, js/engine/lod-apply.js) ---------- */

/** The zoom, inside the limits. */
export const clampZoom = (z) => Math.min(ZMAX, Math.max(ZMIN, z));

/** A world POINT on screen. */
export const toScreen = (cam, p) => ({ x: p.x * cam.z + cam.x, y: p.y * cam.z + cam.y });

/** A screen POINT in the world. */
export const toWorld = (cam, s) => ({ x: (s.x - cam.x) / cam.z, y: (s.y - cam.y) / cam.z });

/**
 * Move the VIEWPORT by (dx, dy) world units — the content moves the other way, which is what a
 * pan means from the camera's side.
 */
export const panBy = (cam, dx, dy) => ({ x: cam.x - dx * cam.z, y: cam.y - dy * cam.z, z: cam.z });

/** A world rect on screen. */
export const rectToScreen = (cam, rect) => screenRect(rect, cam);

/** Zoom to an ABSOLUTE z about a screen point (the wheel's form of `zoomAbout`). */
export function zoomAt(cam, screenPoint, newZ) {
  const z = clampZoom(newZ);
  if (z === cam.z) return { ...cam };
  return zoomAbout(cam, screenPoint.x, screenPoint.y, z / cam.z, ZMIN, ZMAX);
}

/** The world point at the middle of the viewport. */
export const viewCenter = (cam, { w, h }) => toWorld(cam, { x: w / 2, y: h / 2 });

/** The camera that puts a world point at the middle of the viewport, at zoom `z`. */
export function fromCenter(center, z, { w, h }) {
  const zz = clampZoom(z);
  return { x: w / 2 - center.x * zz, y: h / 2 - center.y * zz, z: zz };
}

/**
 * The camera that fits a world rect into the viewport, with `padding` px all round and the
 * insets (a docked HUD, say) kept clear — the rect is centred in what is LEFT of the viewport.
 */
export function fitRect(rect, viewport, padding = 24, insets = {}) {
  if (!rect || !(rect.w > 0) || !(rect.h > 0)) return { x: 0, y: 0, z: 1 };
  /* TWO SPELLINGS, ONE MEANING. The camera's own callers write `{t, r, b, l}`; the HUD manager
     (js/engine/hud.js) writes `{top, right, bottom, left}`, the way `env(safe-area-inset-*)` does
     — and it was handing that straight to this function, whose `insets.t` was therefore
     `undefined` and the fit silently ignored the docks it had just reported. */
  const ins = {
    t: insets.t ?? insets.top ?? 0,
    r: insets.r ?? insets.right ?? 0,
    b: insets.b ?? insets.bottom ?? 0,
    l: insets.l ?? insets.left ?? 0,
  };
  const availW = viewport.w - ins.l - ins.r - padding * 2;
  const availH = viewport.h - ins.t - ins.b - padding * 2;
  if (availW <= 0 || availH <= 0) return { x: 0, y: 0, z: 1 };

  const z = clampZoom(Math.min(availW / rect.w, availH / rect.h));
  // the centre of the area that is actually available, in screen px
  const cx = ins.l + padding + availW / 2;
  const cy = ins.t + padding + availH / 2;
  return { z, x: cx - (rect.x + rect.w / 2) * z, y: cy - (rect.y + rect.h / 2) * z };
}

/**
 * The camera that fits every rect in a list (`bboxOf`, then `fitRect`).
 *
 * A DEGENERATE RECT IS NOT A CORNER OF THE BOX. A pane that has not been laid out yet has w = 0
 * and sits at the origin, and letting it into the bounding box drags the whole fit to the corner
 * — the view then frames the origin and the thing you asked to look at is off screen. The
 * canvas's `bboxOf` is asked for the box of rects the caller has already decided are visible;
 * this filters first.
 */
export function fitRects(rects, viewport, padding = 24, insets) {
  const real = (rects || []).filter((r) => r && r.w > 0 && r.h > 0);
  const box = bboxOf(real);
  if (!box) return { x: 0, y: 0, z: 1 };
  return fitRect(box, viewport, padding, insets);
}

/**
 * Between two cameras, interpolating the zoom in LOG space (a zoom is exponential: halfway
 * between 100% and 400% looks like 200%, not 250%) and the view centre linearly in the world.
 */
export function lerpCamera(a, b, t, viewport) {
  if (t <= 0) return { ...a };
  if (t >= 1) return { ...b };
  const z = Math.exp(Math.log(a.z) + (Math.log(b.z) - Math.log(a.z)) * t);
  const ca = viewCenter(a, viewport);
  const cb = viewCenter(b, viewport);
  return fromCenter({ x: ca.x + (cb.x - ca.x) * t, y: ca.y + (cb.y - ca.y) * t }, z, viewport);
}
