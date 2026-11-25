/**
 * sash.js — Pure sash drag logic + Pointer Events attachment (M2.08).
 */

/**
 * Compute new split sizes after a sash drag.
 * @param {object} opts
 * @param {number[]} opts.sizes - Current sizes array (sums to 1)
 * @param {number} opts.index - Index of sash (between sizes[index] and sizes[index+1])
 * @param {number} opts.deltaScreen - Drag delta in screen pixels
 * @param {number} opts.z - Current camera zoom
 * @param {number} opts.extentLayout - Total layout extent in the drag direction (layout px)
 * @param {number} opts.minPx - Minimum pane size in layout px
 * @returns {number[]} New sizes array (sums to 1)
 */
export function dragSizes({ sizes, index, deltaScreen, z, extentLayout, minPx }) {
  if (index < 0 || index >= sizes.length - 1) {
    return sizes; // Invalid index
  }

  // Convert screen delta to layout delta
  const deltaLayout = deltaScreen / z;

  // Current sizes in layout px
  const leftSize = sizes[index] * extentLayout;
  const rightSize = sizes[index + 1] * extentLayout;

  // If both panes are already at minimum, nothing can move
  if (leftSize <= minPx && rightSize <= minPx) {
    return sizes;
  }

  // New sizes in layout px
  let newLeft = leftSize + deltaLayout;
  let newRight = rightSize - deltaLayout;

  // Clamp to minimum sizes, preserving sum = extentLayout
  if (newLeft < minPx) {
    newLeft = minPx;
    newRight = extentLayout - minPx;
  }
  if (newRight < minPx) {
    newRight = minPx;
    newLeft = extentLayout - minPx;
  }

  const newSizes = [...sizes];
  newSizes[index] = newLeft / extentLayout;
  newSizes[index + 1] = newRight / extentLayout;

  // Verify sum is preserved
  const sum = newSizes.reduce((a, b) => a + b, 0);
  if (Math.abs(sum - 1) > 1e-9) {
    const sumVal = sum;
    for (let i = 0; i < newSizes.length; i++) {
      newSizes[i] /= sumVal;
    }
  }

  return newSizes;
}

/**
 * Attach sash drag handler using Pointer Events.
 * @param {object} opts
 * @param {HTMLElement} opts.el - Sash hit element
 * @param {object} opts.store - Layout store
 * @param {string} opts.path - Path to the split node
 * @param {object} opts.container - Split container element
 * @param {object} opts.camera - Camera state {x, y, z, lock}
 * @param {'h'|'v'} opts.orient - Split orientation
 * @returns {function} Cleanup function to remove event listeners
 */
export function attachSash({ el, store, path, index, container, camera, orient }) {
  const camState = () => (camera && typeof camera.get === 'function' ? camera.get() : (camera || { x: 0, y: 0, z: 1 }));
  let startScreenPos = 0;
  let isDragging = false;
  let gestureStarted = false;
  const isHorizontal = orient === 'h';

  /* THE MOVE HANDLER CANNOT LIVE ON THE THING IT MOVES. Every `tree.sizes` re-renders the root,
     which replaces every element in it — including this sash — so a drag with its listeners on
     the element applied its first move and then went deaf (the canvas's console splitter learned
     the same lesson the same way). The drag listens on the document while it is in flight, and
     identifies itself by the path it was made for. */
  function onPointerDown(e) {
    if (e.button !== 0 && e.pointerType !== 'pen') return;
    isDragging = true;
    startScreenPos = isHorizontal ? e.clientX : e.clientY;
    try { el.setPointerCapture(e.pointerId); } catch {}
    try { store.beginGesture(); } catch {}
    gestureStarted = true;
    document.addEventListener('pointermove', onPointerMove);
    document.addEventListener('pointerup', onPointerUp);
    document.addEventListener('pointercancel', onPointerCancel);
    e.preventDefault();
  }

  function onPointerMove(e) {
    if (!isDragging) return;
    const layout = store.get();
    const node = resolvePath(layout, path);
    if (!node || !node.split || !node.sizes) return;

    const containerRect = container.getBoundingClientRect();
    const z = camState().z || 1;
    const extentLayout = isHorizontal ? containerRect.width / z : containerRect.height / z;
    const minPx = 80;

    const newSizes = dragSizes({
      sizes: node.sizes,
      index: Number.isInteger(index) ? index : getSashIndex(path),
      deltaScreen: isHorizontal ? e.clientX - startScreenPos : e.clientY - startScreenPos,
      z,
      extentLayout,
      minPx,
    });

    /* THE PATH IS THE SPLIT'S OWN. This used to drop the last step — a convention where the
       path ended at the split AS a child of something — which for a root's own tree left an
       empty path and the store refused the command ("path must point to a split node"), so a
       sash drag could never move anything. The index, which used to be inferred from that last
       step, is the `index` option now. */
    store.dispatch({
      type: 'tree.sizes',
      path: [...path],
      sizes: newSizes,
    }, { undo: false });
    e.preventDefault();
  }

  function onPointerUp(e) {
    if (!isDragging) return;
    isDragging = false;
    document.removeEventListener('pointermove', onPointerMove);
    document.removeEventListener('pointerup', onPointerUp);
    document.removeEventListener('pointercancel', onPointerCancel);
    try { el.releasePointerCapture(e.pointerId); } catch {}
    if (gestureStarted) {
      try { store.endGesture(); } catch {}
      gestureStarted = false;
    }
    if (e.preventDefault) e.preventDefault();
  }

  function onPointerCancel(e) { onPointerUp(e); }

  el.addEventListener('pointerdown', onPointerDown);

  return function cleanup() {
    el.removeEventListener('pointerdown', onPointerDown);
    document.removeEventListener('pointermove', onPointerMove);
    document.removeEventListener('pointerup', onPointerUp);
    document.removeEventListener('pointercancel', onPointerCancel);
  };
}

function resolvePath(layout, path) {
  let node = layout;
  for (const step of path) {
    if (node === null || node === undefined) return null;
    if (step.root !== undefined) {
      node = node.roots?.find(r => r.id === step.root);
    } else if (step.hud !== undefined) {
      node = node.hud?.find(h => h.id === step.hud);
    } else if (step.modal !== undefined) {
      node = node.modals?.find(m => m.id === step.modal);
    } else if (step.children !== undefined) {
      node = node.children?.[step.children] ?? null;
    } else if (step.tabs !== undefined) {
      node = node.tabs?.[step.tabs] ?? null;
    } else if (step.group !== undefined) {
      node = node.group ?? null;
    } else {
      return null;
    }
  }
  /* A path whose last step names a ROOT (or a HUD root, or a modal) names the CONTAINER: the
     tree that holds the split is its `tree`. Without this, the root's own split resolved to the
     root object, the drag found no `split` on it and returned — which is how a sash that was
     wired to a drag handler still did nothing. */
  if (node && !node.split && node.tree) return node.tree;
  return node;
}

function getSashIndex(path) {
  const lastStep = path[path.length - 1];
  return lastStep?.children ?? 0;
}
