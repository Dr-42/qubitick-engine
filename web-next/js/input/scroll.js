/**
 * scroll.js — Scroll engine (M2.09).
 *
 * createScrollEngine({config, now}) → {
 *   feed(evt, {z, snapPx}) → void,
 *   step(nowMs) → {dyLayout, dxLayout, active:boolean},
 *   cancel(),
 *   runLength()
 * }
 *
 * Rules:
 * - notch: target screen delta = sign * notchPx * notches, eased over notchEaseMs with easeOutCubic
 * - pixel: immediate 1:1 screen px
 * - layout delta = screen delta / z, at the zoom the gesture started at
 * - acceleration: consecutive events within accel.windowMs multiply by min(accel.max, 1 + perEvent * (run - 1))
 * - when ease finishes and snapToLine: snapTarget(scrollTop, lineHeightPx) → nearest multiple
 * - step(now) returns incremental layout delta since previous step (sum of all steps == total layout delta exactly)
 */

import { CONFIG } from '../core/config.js';

const FALLBACK = { notchPx: 96, notchEaseMs: 120, accel: { windowMs: 200, perEvent: 0.04, max: 1.8 }, lineHeightPx: 18, snapToLine: true };

/** The accel config has no maxRun in defaults/scroll-config.js, and reading a missing
 *  one straight into Math.min yields NaN -- which poisons every delta downstream. */
function accelFactor(run, accel = FALLBACK.accel) {
  const capped = accel.maxRun ? Math.min(accel.maxRun, run) : run;
  return Math.min(accel.max, 1 + accel.perEvent * (capped - 1));
}

/**
 * @param {object} opts
 * @param {object} opts.config - CONFIG.scroll
 * @param {function} opts.now - timestamp function
 * @returns {object} scroll engine API
 */
// wrap it: an unbound performance.now loses its `this` and throws in Node
export function createScrollEngine({ config = CONFIG.scroll, now = () => performance.now() } = {}) {
  const cfg = { ...FALLBACK, ...(config || {}), accel: { ...FALLBACK.accel, ...((config && config.accel) || {}) } };
  let current = null;
  let accelRun = 0;
  let lastEventTime = 0;

  function feed(evt, { z = 1, snapPx } = {}) {
    if (!evt) return;
    const t = now();
    accelRun = (t - lastEventTime <= cfg.accel.windowMs) ? accelRun + 1 : 1;
    lastEventTime = t;
    const factor = accelFactor(accelRun, cfg.accel);
    const zoom = z > 0 ? z : 1;

    if (evt.kind === 'notch') {
      const notches = evt.notches || 0;
      current = {
        kind: 'notch', startTime: t, duration: cfg.notchEaseMs, z: zoom, snapPx,
        targetScreenY: Math.sign(notches) * Math.abs(notches) * cfg.notchPx * factor,
        targetScreenX: 0, doneScreenY: 0, doneScreenX: 0,
      };
    } else {
      // pixels (a trackpad) are already screen px: hand them back on the next step
      current = { kind: 'pixel', z: zoom, snapPx, targetScreenY: evt.pxY || 0, targetScreenX: evt.pxX || 0, doneScreenY: 0, doneScreenX: 0, duration: 0 };
    }
  }

  function step(nowMs) {
    if (!current) return { dyLayout: 0, dxLayout: 0, active: false };
    const t = nowMs === undefined ? now() : nowMs;
    let easedY, easedX, active;
    if (current.kind === 'notch') {
      const progress = current.duration > 0 ? Math.min((t - current.startTime) / current.duration, 1) : 1;
      const eased = 1 - Math.pow(1 - progress, 3);
      easedY = current.targetScreenY * eased;
      easedX = current.targetScreenX * eased;
      active = progress < 1;                       // still easing
    } else {
      // a pixel delta is delivered whole, and this step is the one that moved
      easedY = current.targetScreenY;
      easedX = current.targetScreenX;
      active = true;
    }
    // the increment in screen px, converted to layout px once, at the gesture's zoom
    const dyLayout = (easedY - current.doneScreenY) / current.z;
    const dxLayout = (easedX - current.doneScreenX) / current.z;
    current.doneScreenY = easedY;
    current.doneScreenX = easedX;
    const snapPx = current.snapPx;
    // a pixel delta is spent the moment it is delivered; a notch is dropped once
    // its ease is over. `active` above reports whether THIS step moved anything.
    if (current.kind !== 'notch' || !active) current = null;
    return { dyLayout, dxLayout, active, snapPx };
  }

  function cancel() { current = null; }
  // the length of the ease still owed, which is what a caller needs to know to keep
  // stepping; the acceleration run length is not a duration and is not this
  function runLength() { return current ? (current.duration || 0) : 0; }
  function active() { return !!current; }
  return { feed, step, cancel, runLength, active };
}

export function snapTarget(scrollTop, lineHeightPx) {
  if (!lineHeightPx) return scrollTop;
  const result = Math.round(scrollTop / lineHeightPx) * lineHeightPx;
  return result === 0 ? 0 : result; // normalize -0 to 0
}
