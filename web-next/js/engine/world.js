/**
 * world.js — the viewport, the world element, and the camera's one write per frame (M2.06).
 *
 * Two elements: `.viewport` (clips, takes the gestures) and `.world` inside it (the whole
 * canvas, transformed). The camera's transform is applied in ONE place — a frame callback
 * through the governor — so a pan or a zoom is a style write and not a re-layout, and
 * `will-change` is on only while the camera is actually moving.
 *
 * The camera is read through `get()` when it has one (js/app/camera.js) and treated
 * as a plain `{x,y,z}` otherwise, because both shapes are in the tree. And when the camera
 * can ANNOUNCE its state (a value port), this follows it: the module used to read
 * `camera.x`/`.y`/`.z` off the controller object itself, where those properties do not
 * exist, so the world would have been pinned at 0,0,1 forever.
 *
 * Exposes: viewportEl, worldEl, viewportSize(), screenToWorld(clientX, clientY), applyCamera(),
 * destroy().
 */

/**
 * Create the world element and viewport — or ADOPT the ones the page already has.
 *
 * The shell has no markup for them (it is a `#root` and a boot call), so this makes them. The
 * canvas page does have them (`index.html`'s `#viewport > #world`, styled by `css/canvas.css`),
 * and hands them over: one module owns the viewport/world pair and the camera's transform in
 * BOTH apps, which is the first column of "the canvas is a layout mode of the engine".
 *
 * @param {object} opts
 * @param {HTMLElement} opts.root - element to attach to / measure (#root, or the page's viewport)
 * @param {object} opts.governor - frame governor (one callback per frame)
 * @param {object} opts.camera - camera state, or a controller with get()/state
 * @param {HTMLElement} [opts.viewportEl] - the page's own viewport, adopted when given
 * @param {HTMLElement} [opts.worldEl] - the page's own world, adopted when given
 * @returns {object} the world API
 */
export function createWorld({ root, governor, camera, viewportEl: existingViewport, worldEl: existingWorld }) {
  const viewportEl = existingViewport || document.createElement('div');
  if (!existingViewport) {
    viewportEl.className = 'viewport';
    viewportEl.style.cssText =
      'position:absolute; inset:0; overflow:hidden; touch-action:none; contain:strict;';
  }

  const worldEl = existingWorld || document.createElement('div');
  if (!existingWorld) {
    worldEl.className = 'world';
    worldEl.style.cssText =
      'position:absolute; left:0; top:0; transform-origin:0 0; will-change:transform;';
  }

  if (!existingViewport) viewportEl.appendChild(worldEl);
  if (!existingViewport) root.appendChild(viewportEl);

  /** The camera's numbers, however the camera offers them. */
  const camState = () => {
    if (!camera) return { x: 0, y: 0, z: 1 };
    if (typeof camera.get === 'function') return camera.get() || { x: 0, y: 0, z: 1 };
    return camera;
  };

  let viewportWidth = root.clientWidth || window.innerWidth || 0;
  let viewportHeight = root.clientHeight || window.innerHeight || 0;

  let resizeObserver = null;
  if (typeof ResizeObserver !== 'undefined') {
    resizeObserver = new ResizeObserver((entries) => {
      for (const entry of entries) {
        if (entry.target !== root) continue;
        viewportWidth = entry.contentRect.width;
        viewportHeight = entry.contentRect.height;
      }
    });
    resizeObserver.observe(root);
  }
  const onWindowResize = () => {
    viewportWidth = root.clientWidth || window.innerWidth || 0;
    viewportHeight = root.clientHeight || window.innerHeight || 0;
  };
  if (typeof window !== 'undefined') window.addEventListener('resize', onWindowResize);

  let pendingCameraUpdate = false;
  let lastCamera = { x: 0, y: 0, z: 1 };
  let lastMovedAt = 0;

  /**
   * Write a camera state into the world element — THE one writer, for both apps.
   *
   * The shell reaches it through `applyCamera` (once per frame, below). The canvas calls it
   * synchronously from its own camera: `CAM.apply()` runs on every camera change there, and its
   * tests move the camera and read the DOM in the same tick, so the write cannot wait for a
   * frame. Both write the same three things: the transform, `--z` (the scale, which
   * `css/panes.css` sizes a pane body with) and `--inv` (its reciprocal, which `css/canvas.css`
   * and `css/skin.css` size grips and labels with).
   */
  function setTransform(cam) {
    const { x = 0, y = 0, z = 1 } = cam || {};
    worldEl.style.transform = 'translate(' + x + 'px, ' + y + 'px) scale(' + z + ')';
    worldEl.style.setProperty('--z', String(z));
    worldEl.style.setProperty('--inv', (1 / (z || 1)).toFixed(4));
    return { x, y, z };
  }

  /** One style write per frame, through the governor. */
  function applyCamera() {
    if (pendingCameraUpdate) return;
    pendingCameraUpdate = true;

    governor.requestFrame('world', () => {
      pendingCameraUpdate = false;
      const { x = 0, y = 0, z = 1 } = camState();

      if (x !== lastCamera.x || y !== lastCamera.y || z !== lastCamera.z) {
        setTransform({ x, y, z });
        lastCamera = { x, y, z };
        lastMovedAt = (typeof performance !== 'undefined' ? performance.now() : Date.now());
      }
      /* `will-change` keeps a layer alive, which is worth it while the camera moves and
         wasteful when it does not: it comes off shortly after the last movement. */
      const now = (typeof performance !== 'undefined' ? performance.now() : Date.now());
      worldEl.style.willChange = (now - lastMovedAt < 200) ? 'transform' : '';
    });
  }

  /* A camera that can announce its own state is followed: this is the subscription the
     module was missing, and without it the world only ever moved when something else
     remembered to call applyCamera(). */
  let unsubscribeCamera = null;
  function follow(next) {
    if (unsubscribeCamera) { unsubscribeCamera(); unsubscribeCamera = null; }
    camera = next;
    if (camera && camera.state && typeof camera.state.on === 'function') {
      unsubscribeCamera = camera.state.on(() => applyCamera());
    }
    applyCamera();
    return api;
  }

  applyCamera();

  const api = {
    viewportEl,
    worldEl,
    setTransform,
    follow,
    viewportSize() {
      return { w: viewportWidth || root.clientWidth || window.innerWidth || 0,
               h: viewportHeight || root.clientHeight || window.innerHeight || 0 };
    },
    screenToWorld(clientX, clientY) {
      const rect = viewportEl.getBoundingClientRect();
      const cam = camState();
      const z = cam.z || 1;
      return { x: (clientX - rect.left) / z + (cam.x || 0), y: (clientY - rect.top) / z + (cam.y || 0) };
    },
    applyCamera,
    destroy() {
      if (resizeObserver) resizeObserver.disconnect();
      if (typeof window !== 'undefined') window.removeEventListener('resize', onWindowResize);
      if (unsubscribeCamera) unsubscribeCamera();
      viewportEl.remove();
    },
  };
  return api;
}
