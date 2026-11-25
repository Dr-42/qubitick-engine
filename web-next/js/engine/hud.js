/**
 * hud.js — HUD manager (M2.14, rewired).
 *
 * A HUD root is a pane tree that does NOT live in the world: it is docked to an edge of the
 * screen, it stays where it is while the camera pans, and a HUD's height is what the camera
 * has to keep clear when it fits the workspace. Those three facts are this module: the layer,
 * the element per `layout.hud[]`, the dock arithmetic, and the INSETS those docks add up to.
 *
 * Two things it used to get wrong, both visible the moment something ran it:
 *
 *   THE LAYER WENT INSIDE THE WORLD. A `position:fixed` element is laid out against its
 *   nearest TRANSFORMED ancestor, so a layer appended to `.world` moves and scales with the
 *   camera — the opposite of "screen space". It belongs on the document.
 *
 *   THE LAYER SWALLOWED THE CLICKS. `pointer-events:none` on the layer is right (the empty
 *   space between HUD roots must not eat a drag), but the roots themselves are panes and need
 *   to receive the pointer: the ribbon is a row of buttons.
 *
 * The renderer owns WHAT is inside a HUD root (the pane tree, its instances, its LOD) and asks
 * this module for the element to render into — so the docking stays here and the tree stays
 * there, and neither one reaches into the other.
 */

/**
 * Dock → the CSS that pins a root to that edge, from ITS OWN rect.
 *
 * The rect is the decision (`computeHudRects` below places one root after another, and a top
 * dock moves the next one down); this is only how the decision is written. It used to be given
 * the FINAL insets instead — which, for the ribbon itself, are the insets AFTER the ribbon:
 * `top: 72px` on a 72px ribbon put it at 72, exactly where the content it had displaced begins.
 */
const DOCK_STYLES = {
  top: (rect) => ({ top: rect.y + 'px', left: '0', right: '0', bottom: '' }),
  bottom: (rect, vp) => ({ bottom: (vp.h - rect.y - rect.h) + 'px', left: '0', right: '0', top: '' }),
  left: (rect) => ({ left: rect.x + 'px', top: '0', bottom: '0', right: '' }),
  right: (rect, vp) => ({ right: (vp.w - rect.x - rect.w) + 'px', top: '0', bottom: '0', left: '' }),
};

/** The safe-area insets the browser reports, in px (0 where it does not). */
function safeAreaInsets() {
  const style = getComputedStyle(document.documentElement);
  const read = (name) => parseFloat(style.getPropertyValue(name) || '0') || 0;
  return {
    top: read('safe-area-inset-top'),
    right: read('safe-area-inset-right'),
    bottom: read('safe-area-inset-bottom'),
    left: read('safe-area-inset-left'),
  };
}

/**
 * Where each docked root goes, and how much of the screen the docks take.
 *
 * @param {Array} huds - layout.hud[]
 * @param {{w:number,h:number}} viewport
 * @returns {{rects: Map<string, object>, insets: {top:number,right:number,bottom:number,left:number}}}
 */
export function computeHudRects(huds, viewport) {
  const rects = new Map();
  const insets = { top: 0, right: 0, bottom: 0, left: 0 };
  if (!Array.isArray(huds) || !huds.length) return { rects, insets };

  const safe = safeAreaInsets();
  const { w, h } = viewport;

  huds.forEach((hud, i) => {
    const id = hud.id || 'hud-' + i;
    const size = hud.size || 48;                 // a HUD root states its size; 48 is the ribbon
    const dock = hud.dock || 'top';
    let rect;

    switch (dock) {
      case 'top':
        rect = { x: 0, y: insets.top + safe.top, w, h: size };
        insets.top += size;
        break;
      case 'bottom':
        rect = { x: 0, y: h - insets.bottom - size - safe.bottom, w, h: size };
        insets.bottom += size;
        break;
      case 'left':
        rect = { x: insets.left + safe.left, y: 0, w: size, h };
        insets.left += size;
        break;
      case 'right':
        rect = { x: w - insets.right - size - safe.right, y: 0, w: size, h };
        insets.right += size;
        break;
      default:
        rect = { x: 0, y: 0, w: 0, h: 0 };
    }

    rects.set(id, { rect, dock, size });
  });

  return { rects, insets };
}

/**
 * Create the HUD manager.
 *
 * @param {object} opts
 * @param {object} opts.store - layout store
 * @param {object} opts.camera - camera (js/app/camera.js)
 * @param {function} [opts.viewport] - () => {w,h}; defaults to the window
 * @param {HTMLElement} [opts.layerParent] - where the layer goes; defaults to document.body
 * @param {object} [opts.renderer] - anything with setHudInsets(insets)
 * @returns {object} {layer, elementFor, render, update, getInsets, counts, destroy}
 */
export function createHudManager({ store, camera, viewport, layerParent, renderer } = {}) {
  const layer = document.createElement('div');
  layer.className = 'qb-hud-layer';
  layer.style.cssText =
    'position:fixed; inset:0; pointer-events:none; z-index:1000;';
  (layerParent || document.body).appendChild(layer);

  const elements = new Map();      // hudId -> HTMLElement
  let currentInsets = { top: 0, right: 0, bottom: 0, left: 0 };
  let destroyed = false;

  const viewportSize = () => {
    if (typeof viewport === 'function') return viewport();
    return { w: window.innerWidth || 1, h: window.innerHeight || 1 };
  };

  /**
   * The element a HUD root's tree is rendered into. Created once and kept: the renderer
   * mounts pane instances into it, and a fresh element would throw those away.
   */
  function elementFor(hud, i = 0) {
    const id = hud.id || 'hud-' + i;
    let el = elements.get(id);
    if (!el) {
      el = document.createElement('div');
      el.className = 'qb-hud-' + id;
      el.setAttribute('data-root', id);
      /* the root is a pane: it takes the pointer. The layer around it does not. */
      /* `border-box`: the size a HUD declares is the size the camera has to keep clear, and
         the border of a content-box element made a 72px ribbon 74px on screen — 2px of the
         layout under it, for a number the insets said was free. */
      el.style.cssText =
        'position:absolute; display:flex; pointer-events:auto; overflow:hidden; box-sizing:border-box;'
        + ' border:1px solid #2a2f3e; border-radius:4px; background:#12141e;';
      elements.set(id, el);
      layer.appendChild(el);
    }
    return el;
  }

  /** Dock every declared HUD root: create the missing ones, drop the ones no longer declared. */
  function render(huds = []) {
    if (destroyed) return currentInsets;
    const vp = viewportSize();
    const { rects, insets } = computeHudRects(huds, vp);
    currentInsets = insets;

    const live = new Set();
    rects.forEach(({ rect, dock, size }, id) => {
      const hud = huds.find((h, i) => (h.id || 'hud-' + i) === id) || { id };
      const el = elementFor(hud);
      live.add(id);
      el.style.flexDirection = (dock === 'left' || dock === 'right') ? 'column' : 'row';
      el.style.width = (dock === 'left' || dock === 'right') ? size + 'px' : 'auto';
      el.style.height = (dock === 'top' || dock === 'bottom') ? size + 'px' : 'auto';
      Object.assign(el.style, DOCK_STYLES[dock] ? DOCK_STYLES[dock](rect, vp) : {});
      el.dataset.dock = dock;
      el.dataset.hud = String(rect.w) + 'x' + String(rect.h);
    });

    for (const [id, el] of [...elements]) {
      if (!live.has(id)) { el.remove(); elements.delete(id); }
    }

    if (renderer && typeof renderer.setHudInsets === 'function') renderer.setHudInsets(currentInsets);
    return currentInsets;
  }

  /** Publish the insets and re-fit a camera that is following the layout. */
  function update() {
    const layout = store && store.get ? store.get() : null;
    const insets = render((layout && layout.hud) || []);
    const cam = typeof camera.get === 'function' ? camera.get() : camera;
    if (cam && cam.lock === 'fit' && typeof camera.fit === 'function') camera.fit();
    return insets;
  }

  const unsubscribe = store && store.subscribe
    ? store.subscribe((layout) => render((layout && layout.hud) || []))
    : () => {};
  const onResize = () => update();
  if (typeof window !== 'undefined') window.addEventListener('resize', onResize);

  // first paint
  if (store && store.get) render((store.get() || {}).hud || []);

  function destroy() {
    destroyed = true;
    unsubscribe();
    if (typeof window !== 'undefined') window.removeEventListener('resize', onResize);
    elements.clear();
    layer.remove();
  }

  return {
    layer,
    elementFor,
    render,
    update,
    destroy,
    getInsets: () => currentInsets,
    counts: () => ({ roots: elements.size, layers: layer.children.length }),
  };
}
