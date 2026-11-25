/**
 * wheel.js — Wheel event classification (M2.09).
 *
 * D-022: classifyWheel({deltaX,deltaY,deltaMode}, {pageHeight, lineHeightPx})
 *   → {kind:'notch'|'pixel', notches?, pxY, pxX}
 *
 * deltaMode 0 (pixel): pxY = deltaY, pxX = deltaX, kind='pixel'
 *   … unless the delta is a whole number of legacy wheel clicks (≥ 40, a multiple
 *   of 120), which is a mouse notch wearing pixel clothes
 * deltaMode 1 (line):  notches = round(|dy|/3) || 1, pxY = notches * lineHeightPx * 3, kind='notch'
 * deltaMode 2 (page):  pxY = deltaY * pageHeight, pxX = deltaX * pageHeight, kind='pixel'
 *
 * The three shapes have to be told apart because they mean very different amounts:
 * a Chrome notch arrives as deltaY 100-120, Firefox sends 3 lines, some trackpads
 * send 0.5-3 px at 60 Hz. Callers that scroll a pane want `kind` (to ease a notch
 * and hand a pixel delta straight through); callers that move the camera want
 * screenDelta(), where every shape is worth the same 96 px.
 */

export const LEGACY_NOTCH_MIN = 40;    // deltaMode 0 px at or above this is a click, not a drag
export const LEGACY_NOTCH_UNIT = 120;  // …in units of this many px per click
export const LINES_PER_NOTCH = 3;      // deltaMode 1 lines in one click

export function classifyWheel({ deltaX = 0, deltaY = 0, deltaMode = 0 } = {}, { pageHeight = 0, lineHeightPx = 18, notchPx = 96 } = {}) {
  // line mode: whole lines, whose height the caller supplies
  if (deltaMode === 1) {
    const notches = Math.round(Math.abs(deltaY) / LINES_PER_NOTCH) || 1;
    const xNotches = Math.round(Math.abs(deltaX) / LINES_PER_NOTCH) || 0;
    const direction = deltaY >= 0 ? 1 : -1;
    return {
      kind: 'notch',
      notches: direction * notches,
      pxY: direction * notches * lineHeightPx * LINES_PER_NOTCH,
      pxX: Math.sign(deltaX) * xNotches * lineHeightPx * LINES_PER_NOTCH,
      notchPx,
    };
  }

  // page mode: whole viewports
  if (deltaMode === 2) {
    return { kind: 'pixel', pxY: deltaY * pageHeight, pxX: deltaX * pageHeight, notchPx };
  }

  // pixel mode, with the legacy wheel's round numbers recognised as clicks
  if (Math.abs(deltaY) >= LEGACY_NOTCH_MIN) {
    const notches = Math.round(Math.abs(deltaY) / LEGACY_NOTCH_UNIT) || 1;
    const xNotches = Math.round(Math.abs(deltaX) / LEGACY_NOTCH_UNIT) || 0;
    return {
      kind: 'notch',
      notches: (deltaY > 0 ? 1 : -1) * notches,
      pxY: (deltaY > 0 ? 1 : -1) * notches * lineHeightPx * LINES_PER_NOTCH,
      pxX: Math.sign(deltaX) * xNotches * lineHeightPx * LINES_PER_NOTCH,
      notchPx,
    };
  }

  return { kind: 'pixel', pxY: deltaY, pxX: deltaX, notchPx };
}

/**
 * screenDelta(cls, {notchPx}) → {sx, sy} in *screen* px.
 * Whatever shape the wheel arrived in, one notch is worth notchPx and a pixel
 * delta is worth itself, so panning the camera at any zoom feels the same.
 */
export function screenDelta(cls, { notchPx = 96 } = {}) {
  if (!cls) return { sx: 0, sy: 0 };
  if (cls.kind === 'notch') {
    const n = cls.notches || 0;
    // a wheel notch is vertical: no horizontal component, and not a -0 either
    return { sx: 0, sy: Math.sign(n) * Math.abs(n) * notchPx };
  }
  return { sx: cls.pxX || 0, sy: cls.pxY || 0 };
}
