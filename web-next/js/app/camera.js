/*
 * camera.js — THE camera: who owns the zoom, who moves it, and how it settles. Both apps use it.
 *
 * The MATHS is in js/app/viewport.js (fit, zoom about a point, union of rects, and the
 * screen/world conversions) and has its own node tests. What is here is the part that
 * cannot be a pure function: a camera that eases, a camera that must be told when it has
 * moved, and an animation loop that must not outlive the thing it is moving.
 *
 * Three motions, and they are different on purpose:
 *
 *   zoom   wheel, pinch and the +/- keys. Anchored at a SCREEN point that stays put, and
 *          eased by elapsed time rather than by frame count — a starved frame loop must
 *          not turn a 70ms settle into a second-long crawl. While the pointer stays put
 *          the world point under it is held for the whole gesture, so a long pinch cannot
 *          creep.
 *   fly    "go and look at that": the viewport centre travels to a target camera, with
 *          zoom interpolated in LOG space so equal feel at every scale.
 *   apply  one write of the transform per change, then a settle timer: grips are sized
 *          per zoom, the LOD pass and the save happen after the motion stops, not 60
 *          times a second during it.
 *
 * The host owns none of the consequences. `onMove` is called after every camera change
 * (the page sizes grips, redraws the field, marks dirty) and `onSettle` after the motion
 * stops (the page re-runs LOD, saves the workspace). That split is what lets the same
 * camera drive the canvas today and a layout mode of the engine tomorrow.
 *
 * Nor does it schedule a frame: `requestFrame(cb)` is injected, because exactly one thing
 * in this client owns the animation loop (js/core/frame.js in the module tree; the page's
 * own loop in the canvas). A module that schedules frames itself is a second loop, and two
 * loops is how a canvas ends up doing its layout twice a frame.
 *
 * THE ENGINE'S SIDE is the same object with three more things on it, and they are additions
 * rather than a second camera: `get()`/`set()` for code that thinks in states (a key binding, a
 * fit button), `fit()` with the insets a docked HUD costs, and a `state` port that announces a
 * change to whoever follows the camera (the world's transform writer and the LOD pass). The
 * engine used to have its own controller (`js/engine/camera-anim.js`); two cameras over one
 * shared arithmetic still meant two answers to "where is the camera", and only one of them was
 * ever exercised by the UI matrix.
 */

import { ZMIN, ZMAX, camForRect, firstViewCam, bboxOf, zoomAbout, viewportWorld, fitRects, fromCenter } from './viewport.js';

/** Wheel tuning, in one place: pixels -> log zoom, and how fast the view settles. */
export const ZOOM_TUNING = {
  k: 0.0018,        // ~100px notch is ~1.2x
  kTiny: 0.005,     // trackpad pinch deltas are 1–3px and need more gain
  maxDelta: 60,     // one event may never move more than ~11%
  tau: 70,          // ms
  settleMs: 180,    // after the last move: grips, LOD, save
};

/**
 * @param {object} o
 * @param {{x:number,y:number,z:number}} o.cam   the camera object (mutated in place)
 * @param {() => {w:number,h:number}} o.viewport  the viewport size, read live
 * @param {(cam) => void} o.applyTransform        write the transform (one style write)
 * @param {() => void} [o.onMove]                 after any change
 * @param {() => void} [o.onSettle]               after the motion stops
 * @param {() => Array} [o.rects]                 world rects, for "frame everything"
 */
export function createCameraHost({ cam, viewport, applyTransform, onMove, onSettle, rects, insets, rectsProvider,
  /** (cb) => cancel, or a governor's requestFrame. Defaults to a timer: correct, if not
   *  smooth, so a caller that forgets to inject one still animates instead of throwing. */
  requestFrame = (cb) => { const id = setTimeout(() => cb(typeof performance !== 'undefined' ? performance.now() : Date.now()), 16); return () => clearTimeout(id); } }) {
  const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
  const size = () => viewport();

  let zoomTarget = null, zoomCancel = null, zoomLast = 0;
  let flyCancel = null, settleT = 0, dirty = false;

  /* The camera, announced: who is following it (the world's transform writer, the LOD pass). */
  const statePort = {
    value: cam,
    subs: new Set(),
    on(fn) { this.subs.add(fn); return () => this.subs.delete(fn); },
    notify() { this.value = cam; for (const fn of [...this.subs]) { try { fn(cam); } catch (e) { console.error(e); } } },
  };

  /** One camera change: transform, then whoever asked to know. */
  function apply() {
    applyTransform(cam);
    statePort.notify();
    if (onMove) onMove(cam);
    dirty = true;
    clearTimeout(settleT);
    settleT = setTimeout(() => {
      dirty = false;
      if (onSettle) onSettle();
    }, ZOOM_TUNING.settleMs);
  }

  /* ---------------------------------------------------------------- zoom */
  function zoomTo(z, sx, sy) {
    // pin the world point under the pointer for the whole gesture, unless it moved
    const held = zoomTarget && Math.abs(sx - zoomTarget.sx) < 1.5 && Math.abs(sy - zoomTarget.sy) < 1.5;
    zoomTarget = {
      z: clamp(z, ZMIN, ZMAX), sx, sy,
      wx: held ? zoomTarget.wx : (sx - cam.x) / cam.z,
      wy: held ? zoomTarget.wy : (sy - cam.y) / cam.z,
    };
    if (!zoomCancel) zoomCancel = requestFrame(zoomStep);
  }

  function zoomStep(now) {
    const dt = zoomLast ? Math.min(250, now - zoomLast) : 16;
    zoomLast = now;
    let z = cam.z + (zoomTarget.z - cam.z) * (1 - Math.exp(-dt / ZOOM_TUNING.tau));
    if (Math.abs(zoomTarget.z - z) < zoomTarget.z * 0.002) z = zoomTarget.z;
    cam.z = z;
    cam.x = zoomTarget.sx - zoomTarget.wx * z;
    cam.y = zoomTarget.sy - zoomTarget.wy * z;
    const done = z === zoomTarget.z;
    if (done) zoomTarget = null;
    apply();
    zoomCancel = done ? null : requestFrame(zoomStep);
    if (done) zoomLast = 0;
  }

  function stopZoom() {
    if (zoomCancel) { zoomCancel(); zoomCancel = null; }
    zoomTarget = null; zoomLast = 0;
  }

  /**
   * Wheel/pinch. `delta` is already normalised to pixels by the page (a notch, a Firefox
   * line scroll and a page scroll all arrive as three different units); `tiny` says the
   * event looked like a trackpad pinch, which needs more gain for the same feel.
   */
  function wheel(delta, sx, sy, tiny = Math.abs(delta) < 4) {
    const k = tiny ? ZOOM_TUNING.kTiny : ZOOM_TUNING.k;
    const capped = Math.sign(delta) * Math.min(Math.abs(delta), ZOOM_TUNING.maxDelta);
    zoomTo((zoomTarget ? zoomTarget.z : cam.z) * Math.exp(-capped * k), sx, sy);
  }

  /** A step about the viewport centre (the console's +/- buttons). */
  function nudge(factor) {
    const { w, h } = size();
    zoomTo(clamp((zoomTarget ? zoomTarget.z : cam.z) * factor, ZMIN, ZMAX), w / 2, h / 2);
  }

  /** Back to 100%, still looking at the same world point. */
  function zoom100() {
    const { w, h } = size();
    const cx = (w / 2 - cam.x) / cam.z, cy = (h / 2 - cam.y) / cam.z;
    fly({ z: 1, x: w / 2 - cx, y: h / 2 - cy }, 300);
  }

  /* ---------------------------------------------------------------- fly */
  function fly(target, ms = 400, done) {
    if (flyCancel) { flyCancel(); flyCancel = null; }
    stopZoom();
    const { w, h } = size();
    const start = { ...cam };
    const sc = { x: (w / 2 - start.x) / start.z, y: (h / 2 - start.y) / start.z };
    const tc = { x: (w / 2 - target.x) / target.z, y: (h / 2 - target.y) / target.z };
    const t0 = (typeof performance !== 'undefined' ? performance : Date).now();
    const step = (now) => {
      let u = Math.min(1, (now - t0) / ms);
      u = u < 0.5 ? 2 * u * u : 1 - Math.pow(-2 * u + 2, 2) / 2;      // ease in-out
      const z = Math.exp(Math.log(start.z) + (Math.log(target.z) - Math.log(start.z)) * u);
      const cx = sc.x + (tc.x - sc.x) * u, cy = sc.y + (tc.y - sc.y) * u;
      cam.z = z;
      cam.x = w / 2 - cx * z;
      cam.y = h / 2 - cy * z;
      apply();
      if (u < 1) flyCancel = requestFrame(step);
      else { flyCancel = null; if (done) done(); }
    };
    flyCancel = requestFrame(step);
  }

  /* ---------------------------------------------------------------- compose */
  /** The camera that frames one rect, with `pad` px of breathing room on screen. */
  function camFor(rect, pad = 40, maxZ = 3) {
    const { w, h } = size();
    if (!rect || !(rect.w > 0) || !(rect.h > 0)) return { z: 1, x: w / 2, y: h / 2 };
    return camForRect(rect, w, h, { pad, maxZ, minZ: ZMIN });
  }

  /** The camera a first render gets — js/app/viewport.js::firstViewCam owns the rule and says why.
   *  A fit the user asks for is `camFor`; this is what the workspace chooses when nobody has. */
  function firstView(rect, pad = 60) {
    const { w, h } = size();
    return firstViewCam(rect, w, h, { pad });
  }

  /** The box around everything the caller says is there (or the viewport, if nothing is). */
  function frameBox() {
    const { w, h } = size();
    const list = rects ? rects() : [];
    return bboxOf(list) || { x: 0, y: 0, w, h };
  }

  /** Go and look at everything. */
  function frameAll(pad = 40, ms = 400) {
    fly(camFor(frameBox(), pad), ms);
  }

  /** The world rect the viewport covers right now. */
  function world() {
    const { w, h } = size();
    return viewportWorld(cam, w, h);
  }

  /** Zoom by a factor about a screen point, immediately (used by keyboard shortcuts). */
  function zoomAt(sx, sy, factor) {
    Object.assign(cam, zoomAbout(cam, sx, sy, factor, ZMIN, ZMAX));
    apply();
  }

  /* ---------------------------------------------------------------- the engine's side */
  /** The state, as a copy: a caller that wants to change it says so with `set`. */
  const get = () => ({ ...cam });

  /**
   * Set what a state-thinker names, and nothing else. `lock: 'fit'` puts the camera back into
   * following the layout (a resize re-fits; manual zooms are the layout's business then).
   */
  function set(partial = {}) {
    stopFly();                       // a manual change is a decision: the fly it interrupted is over
    if (partial.lock !== undefined) cam.lock = partial.lock;
    if (cam.lock === 'fit' && partial.lock === 'fit') { fit(); return; }
    let changed = false;
    if (partial.z !== undefined) { cam.z = clamp(partial.z, ZMIN, ZMAX); changed = true; }
    if (partial.x !== undefined) { cam.x = partial.x; changed = true; }
    if (partial.y !== undefined) { cam.y = partial.y; changed = true; }
    if (changed) apply();
  }

  /**
   * Frame the layout: the rects the caller names, or everything the `rectsProvider` reports, with
   * `padding` px around it and the docked insets kept clear (a HUD's height is not the camera's
   * to draw under).
   */
  function fit({ rects: list, padding = 24 } = {}) {
    const { w, h } = size();
    const boxes = list || (rectsProvider ? rectsProvider() : (rects ? rects() : []));
    const next = fitRects(boxes, { w, h }, padding, insets ? insets() : {});
    Object.assign(cam, { x: next.x, y: next.y, z: next.z });
    apply();
  }

  /** The engine's fly: a target may name a world CENTRE and a zoom instead of a raw camera. */
  function flyTo(target, opts = {}) {
    const ms = opts.ms ?? opts.durationMs ?? 400;
    if (!target) return;
    if (target.center !== undefined) {
      const next = fromCenter(target.center, target.z ?? cam.z, size());
      fly({ x: next.x, y: next.y, z: next.z }, ms);
      return;
    }
    fly({ x: target.x ?? cam.x, y: target.y ?? cam.y, z: target.z ?? cam.z }, ms);
  }

  const isFlying = () => !!(flyCancel || zoomCancel);
  function stopFly() {
    stopZoom();
    if (flyCancel) { flyCancel(); flyCancel = null; }
  }

  return {
    apply, fly, flyTo, camFor, firstView, frameBox, frameAll, world, zoomAt, zoom100, nudge, wheel,
    stopZoom, stopFly, isFlying, get, set, fit, state: statePort,
    get moving() { return dirty; },
    destroy() {
      stopZoom();
      if (flyCancel) { flyCancel(); flyCancel = null; }
      clearTimeout(settleT);
    },
  };
}