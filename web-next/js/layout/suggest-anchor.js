/**
 * suggest-anchor.js — suggest anchor parameters to reproduce a position (M3.00).
 *
 * suggestAnchor(parentRect, childRect) → {edge, toEdge, dx, dy} per D-018.
 *
 * Chooses `toEdge` as the parent edge nearest to the child's centre direction,
 * `edge` as the opposite child edge, and `dx,dy` as the exact offsets so that
 * applying the D-018 anchor resolution reproduces childRect's position.
 *
 * D-018 reference point P on parent by toEdge:
 *   left/top → (px,py)   right → (px+pw,py)   bottom → (px,py+ph)
 * Child edge placed at P+(dx,dy):
 *   left/top:  child (x,y) = P+(dx,dy)
 *   right:     x = P.x+dx−w, y = P.y+dy
 *   bottom:    x = P.x+dx,   y = P.y+dy−h
 */

const OPPOSITE_EDGE = {
  left: 'right',
  right: 'left',
  top: 'bottom',
  bottom: 'top',
};

/**
 * Suggest anchor parameters so the child sits at childRect relative to parentRect.
 * @param {object} parentRect - {x,y,w,h}
 * @param {object} childRect - {x,y,w,h}
 * @returns {{edge, toEdge, dx, dy}}
 */
export function suggestAnchor(parentRect, childRect) {
  const px = parentRect.x + parentRect.w / 2;
  const py = parentRect.y + parentRect.h / 2;
  const cx = childRect.x + childRect.w / 2;
  const cy = childRect.y + childRect.h / 2;

  const dxDir = Math.abs(cx - px);
  const dyDir = Math.abs(cy - py);

  let toEdge;
  if (dxDir >= dyDir) {
    toEdge = cx >= px ? 'right' : 'left';
  } else {
    toEdge = cy >= py ? 'bottom' : 'top';
  }

  const edge = OPPOSITE_EDGE[toEdge];

  let dx;
  let dy;
  switch (toEdge) {
    case 'right':
      // P = (parent.x+parent.w, parent.y); edge left → (x,y) = P+(dx,dy)
      dx = childRect.x - parentRect.x - parentRect.w;
      dy = childRect.y - parentRect.y;
      break;
    case 'left':
      // P = (parent.x, parent.y); edge right → x = P.x+dx-w, y = P.y+dy
      dx = childRect.x - parentRect.x + childRect.w;
      dy = childRect.y - parentRect.y;
      break;
    case 'bottom':
      // P = (parent.x, parent.y+parent.h); edge top → (x,y) = P+(dx,dy)
      dx = childRect.x - parentRect.x;
      dy = childRect.y - parentRect.y - parentRect.h;
      break;
    case 'top':
      // P = (parent.x, parent.y); edge bottom → x = P.x+dx, y = P.y+dy-h
      dx = childRect.x - parentRect.x;
      dy = childRect.y - parentRect.y + childRect.h;
      break;
    default:
      dx = 0;
      dy = 0;
  }

  return { edge, toEdge, dx, dy };
}
