/*
 * grips.js — the handles on a tile's edges and corners: how big they are, and where.
 *
 * Extracted from the inline script in web-next/index.html. A grip is measured in LAYOUT
 * space, inside the camera transform, so a fixed 7 layout px is 7 SCREEN px only at zoom
 * 1. Zoomed out, every grip becomes a hairline — a phone's stats tile measured an `.e`
 * grip 0.83 px wide, and a finger cannot hit that. Three constraints, and the smallest
 * wins:
 *
 *   on-screen  a band has a screen floor, and a coarse pointer gets a bigger one than a
 *              mouse;
 *   in-tile    a tile only a few screen px across is ALL grip once the floor is applied,
 *              and its grips then overlap its neighbours' — at z=0.12 on a phone an `.e`
 *              grab landed on another tile's `.se` corner and a horizontal drag resized
 *              on both axes. So a band is capped at a fraction of the tile's own screen
 *              size;
 *   the floor and the cap meet at about zoom 1: below it the grips shrink with the tile
 *              (correct — there is no room), above it they stop growing (or they would
 *              eat the tile).
 *
 * The sizes are cached per zoom because every caller runs per tile per frame (a camera
 * ease, a resize pass) and the answer only depends on z.
 *
 * And one more, added when a header stopped being pressable: a handle may not reach far
 * INTO the tile it belongs to. At the fitted zoom (~0.36 for the shipped workspace) the
 * north band is 16 SCREEN px tall, which is 44 LAYOUT px — it swallowed the whole 30 px
 * header, and the tile's own ⛶ and ✕ were visible but unclickable: `elementFromPoint` in
 * the middle of the button answered `.grip.n`. The band is still 16 screen px thick (a
 * finger needs that); it just sits OUTSIDE the top edge, reaching at most `GRIP_IN` layout
 * px in, so the header stays the header. The top corners get the same cap, since their
 * squares are even bigger — a corner square is centred on the corner, and half of it lands
 * inside.
 *
 * A grip does not resize anything itself: it reports the grab and the host decides what
 * that edge means (this tile's rect, its cluster's sash, or the cluster's own boundary).
 */

/** Exactly the eight handles, in the order they are created. */
export const GRIP_EDGES = ['n', 's', 'e', 'w', 'ne', 'nw', 'se', 'sw'];

/** The floor in layout px, and the in-tile cap as a fraction of the tile. */
export const GRIP = { band: 7, corner: 13 };
const GRIP_SCREEN = 16;                                  // on-screen floor, px, fine pointer
const GRIP_TOUCH = { band: 22, corner: 40 };             // on-screen floor, coarse pointer
const GRIP_CAP = { band: 0.34, corner: 0.30 };           // at most this much of the tile
/** How far a handle may reach into its own tile, in layout px: less than the header
 *  (30 layout px in canvas.css), so the header's buttons stay reachable at any zoom. */
export const GRIP_IN = 8;

/** Where the eight handles go, in layout px — pure, so the header rule can be tested. */
export function gripRects(r, { band: b, corner: c } = {}, reach = GRIP_IN) {
  const into = Math.min(reach, b / 2);        // the north band straddles the edge above 2*reach
  const ic = Math.min(reach, c / 2);
  const vw = Math.max(b, r.w - 2 * c), vh = Math.max(b, r.h - 2 * c);
  return {
    e:  [r.x + r.w - b / 2, r.y + c, b, vh],
    w:  [r.x - b / 2,       r.y + c, b, vh],
    s:  [r.x + c, r.y + r.h - b / 2, vw, b],
    n:  [r.x + c, r.y - b + into, vw, b],
    ne: [r.x + r.w - ic, r.y - c + ic, c, c],
    nw: [r.x - c + ic,   r.y - c + ic, c, c],
    se: [r.x + r.w - c / 2, r.y + r.h - c / 2, c, c],
    sw: [r.x - c / 2,       r.y + r.h - c / 2, c, c],
  };
}

/**
 * @param {object} host
 * @param {{z:number}} host.cam            read for its zoom
 * @param {Element} host.world             where the grip elements live
 * @param {() => object|null} host.solo    the fullscreen tile: no edges while it is up
 * @param {(e, p, edge) => void} host.grab what a grab means
 * @param {Document} [host.doc]
 */
export function createGrips({ cam, world, solo, grab, doc = document }) {
  const coarse = (() => {
    try { return doc.defaultView.matchMedia('(pointer: coarse)').matches; }
    catch (e) { return false; }
  })();

  /** The band sizes at the CURRENT zoom, recomputed only when the zoom actually moves. */
  let cache = { z: -1, g: null };
  function at() {
    const z = Math.max(cam.z, 0.005);
    if (cache.g && Math.abs(Math.log(z / cache.z)) < 0.002) return cache.g;
    const band = Math.max(GRIP.band, (coarse ? GRIP_TOUCH.band : GRIP_SCREEN) / z);
    const corner = Math.max(GRIP.corner, (coarse ? GRIP_TOUCH.corner : GRIP_SCREEN * 1.6) / z);
    cache = { z, g: { band, corner } };
    return cache.g;
  }

  /** The band for one rect: the zoom's sizes, capped by the tile's own screen size. */
  function band(r) {
    const { band: b, corner } = at();
    const z = Math.max(cam.z, 0.005);
    const w = (r && r.w ? r.w : 1e9) * z, h = (r && r.h ? r.h : 1e9) * z;   // screen px
    return {
      band: Math.max(GRIP.band, Math.min(b, Math.min(w / 3, h / 3) / z)),
      corner: Math.max(4, Math.min(corner, Math.min(w, h) * GRIP_CAP.corner / z)),
    };
  }

  /** Place a tile's grips on its current rect. Called from setRect, which every path that
   *  changes a tile's box goes through — so they cannot drift. */
  function sync(p) {
    if (!p || !p.grips) return;
    const r = p.wr;
    if (!r) return;
    const { band: b, corner: c } = band(r);
    const hidden = solo() === p;                        // fullscreen has no edges
    const put = (k, x, y, w, h) => {
      const g = p.grips[k];
      if (!g) return;
      Object.assign(g.style, { left: x + 'px', top: y + 'px', width: w + 'px', height: h + 'px', display: hidden ? 'none' : '' });
    };
    const where = gripRects(r, { band: b, corner: c });
    for (const edge of GRIP_EDGES) put(edge, ...where[edge]);
  }

  /** Build the eight handles for a tile, and light them up when the tile is hovered: the
   *  band is the affordance, and nothing shows when the pointer is somewhere else. */
  function mount(p) {
    p.grips = {};
    for (const edge of GRIP_EDGES) {
      const g = doc.createElement('div');
      g.className = 'grip ' + edge;
      g.dataset.pane = p.id;
      g.dataset.grip = edge;                 // so a probe can name a handle
      g.addEventListener('pointerdown', (e) => grab(e, p, edge));
      world.appendChild(g);
      p.grips[edge] = g;
    }
    const lit = (on) => {
      for (const edge of GRIP_EDGES) {
        const g = p.grips && p.grips[edge];
        if (g) g.classList.toggle('hot', on);
      }
    };
    p.el.addEventListener('pointerenter', () => lit(true));
    p.el.addEventListener('pointerleave', () => lit(false));
    sync(p);
  }

  function drop(p) {
    if (!p || !p.grips) return;
    for (const edge of GRIP_EDGES) {
      const g = p.grips[edge];
      if (g) g.remove();
    }
    p.grips = null;
  }

  /** A tile's box changed: the handles are rebuilt only if the cache is stale. */
  function invalidate() { cache = { z: -1, g: null }; }

  return { EDGES: GRIP_EDGES, at, band, sync, mount, drop, invalidate, coarse, rects: gripRects };
}