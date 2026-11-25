/**
 * resize-root.js — pure root resize helper (M3.02).
 *
 * resizeRoot(rect, edge, dxLayout, dyLayout, minW=160, minH=120)
 *   → new {x, y, w, h}
 *
 * Edges: n, s, e, w, ne, nw, se, sw.
 * West/north grips also move x/y. Delta ÷ z is applied by the caller
 * (layout px are pre-divided by zoom).
 *
 * Clamps to minW/minH. When a clamp is hit, the opposite side stays
 * fixed and the moving side stops at the minimum.
 */

/**
 * Resize a rect from a given edge/corner.
 *
 * @param {{x:number,y:number,w:number,h:number}} rect
 * @param {'n'|'s'|'e'|'w'|'ne'|'nw'|'se'|'sw'} edge
 * @param {number} dxLayout - delta x in layout px (pre zoom-division)
 * @param {number} dyLayout - delta y in layout px (pre zoom-division)
 * @param {number} [minW=160]
 * @param {number} [minH=120]
 * @returns {{x:number,y:number,w:number,h:number}}
 */
export function resizeRoot(rect, edge, dxLayout, dyLayout, minW = 160, minH = 120) {
  let { x, y, w, h } = rect;

  switch (edge) {
    case 'e':
      w = Math.max(minW, w + dxLayout);
      break;
    case 'w':
      {
        const newW = Math.max(minW, w - dxLayout);
        x += w - newW;
        w = newW;
      }
      break;
    case 's':
      h = Math.max(minH, h + dyLayout);
      break;
    case 'n':
      {
        const newH = Math.max(minH, h - dyLayout);
        y += h - newH;
        h = newH;
      }
      break;
    case 'ne':
      {
        const newH = Math.max(minH, h - dyLayout);
        y += h - newH;
        h = newH;
        w = Math.max(minW, w + dxLayout);
      }
      break;
    case 'nw':
      {
        const newH = Math.max(minH, h - dyLayout);
        y += h - newH;
        h = newH;
        const newW = Math.max(minW, w - dxLayout);
        x += w - newW;
        w = newW;
      }
      break;
    case 'se':
      h = Math.max(minH, h + dyLayout);
      w = Math.max(minW, w + dxLayout);
      break;
    case 'sw':
      {
        h = Math.max(minH, h + dyLayout);
        const newW = Math.max(minW, w - dxLayout);
        x += w - newW;
        w = newW;
      }
      break;
    default:
      break;
  }

  return { x, y, w, h };
}
