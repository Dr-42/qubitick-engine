/**
 * dropzone.js — Pure dropzone utilities (M2.16).
 *
 * Computes drop zones and target panes for drag-and-drop operations.
 */

/**
 * Determines which drop zone a point falls into within a rectangle.
 * @param {object} rect - {x, y, w, h} in screen pixels
 * @param {object} pt - {x, y} point in screen pixels
 * @param {number} frac - Edge fraction (default: CONFIG.layout.dropEdgeFraction = 0.25)
 * @returns {'left'|'right'|'top'|'bottom'|'center'|null}
 */
export function zoneAt(rect, pt, frac = 0.25) {
  if (!rect || rect.w <= 0 || rect.h <= 0) return null;

  const { x, y, w, h } = rect;
  const { x: px, y: py } = pt;

  // Check if point is inside rect
  if (px < x || px > x + w || py < y || py > y + h) return null;

  // Calculate distances to edges
  const distLeft = px - x;
  const distRight = x + w - px;
  const distTop = py - y;
  const distBottom = y + h - py;

  const edgeThreshold = Math.min(w, h) * frac;

  // Check if we're in the edge zones
  const isLeft = distLeft <= edgeThreshold;
  const isRight = distRight <= edgeThreshold;
  const isTop = distTop <= edgeThreshold;
  const isBottom = distBottom <= edgeThreshold;

  // Corners: nearest edge wins
  if (isLeft && isTop) {
    return distLeft < distTop ? 'left' : 'top';
  }
  if (isRight && isTop) {
    return distRight < distTop ? 'right' : 'top';
  }
  if (isLeft && isBottom) {
    return distLeft < distBottom ? 'left' : 'bottom';
  }
  if (isRight && isBottom) {
    return distRight < distBottom ? 'right' : 'bottom';
  }

  // Single edge
  if (isLeft) return 'left';
  if (isRight) return 'right';
  if (isTop) return 'top';
  if (isBottom) return 'bottom';

  // Center
  return 'center';
}

/**
 * Finds the target pane and drop zone for a point.
 * @param {Array} layoutRects - Array of {paneId, rect} in world coordinates
 * @param {object} pt - {x, y} point in world coordinates
 * @param {number} frac - Edge fraction (default: CONFIG.layout.dropEdgeFraction)
 * @returns {object|null} {paneId, zone} or null
 */
export function targetAt(layoutRects, pt, frac = 0.25) {
  if (!layoutRects || !layoutRects.length) return null;

  // Find the pane that contains the point
  for (const { paneId, rect } of layoutRects) {
    const zone = zoneAt(rect, pt, frac);
    if (zone) {
      return { paneId, zone };
    }
  }

  return null;
}
