/**
 * touch.js — Touch input handling (M2.19).
 *
 * Installs touchstart/touchmove/touchend listeners on the viewport.
 * Routes touch events to the pane under the pointer via hooks.hitTest.
 * Rejects touch events while pen is active (palm rejection).
 * Emits bus events for pane-specific touch input.
 */

/**
 * Install touch event handling.
 * @param {object} opts
 * @param {HTMLElement} opts.worldEl - DOM element to listen on
 * @param {object} opts.ws - Workspace API (camera, bus, hooks, lock)
 * @param {object} opts.camera - Camera state
 * @param {object} opts.store - Layout store
 * @param {object} opts.instances - Pane instance manager (for penState/isPalmRejected)
 * @returns {object} { destroy }
 */
export function installTouch({ worldEl, ws, camera, store, instances } = {}) {
  if (!worldEl) {
    throw new Error('installTouch: worldEl is required');
  }
  if (!ws) {
    throw new Error('installTouch: ws is required');
  }

  const { bus, hooks: inputHooks, lock } = ws;

  /**
   * Check if touch should be rejected due to active pen (palm rejection).
   */
  function isPalmRejected() {
    if (instances && instances.isPalmRejected) {
      return instances.isPalmRejected();
    }
    return false;
  }

  /**
   * Emit a pane-specific touch event on the bus.
   */
  function emitPaneEvent(paneId, type, detail) {
    if (!paneId || !bus) return;
    bus.emit('pane:touch:' + type, { paneId, ...detail });
  }

  /**
   * Get the primary touch point from a touch event.
   */
  function getPrimaryTouch(event) {
    const touches = event.changedTouches || event.touches;
    if (!touches || touches.length === 0) return null;
    return touches[0];
  }

  /**
   * Handle touchstart.
   */
  function onTouchStart(event) {
    /* SUPPRESSED IN DESIGN MODE, not outside it. `lock.get()` is true in the normal state (no
       design chrome, no dragging) and false while a person is moving panes around — where a
       press belongs to the drag, not to the pane under it. As written this read the other way
       round, so the module did nothing at all in the state the app ships in: no pen state, and
       nothing for palm rejection to arm on. */
    if (lock && lock.get && lock.get() === false) return;
    if (isPalmRejected()) return; // Reject touch while pen is active

    const touch = getPrimaryTouch(event);
    if (!touch) return;

    const { clientX, clientY } = touch;

    // Hit test to find target pane
    const paneId = inputHooks ? inputHooks.hitTest(clientX, clientY) : null;

    // Emit bus event
    emitPaneEvent(paneId, 'start', {
      clientX,
      clientY,
      touchIdentifier: touch.identifier,
      paneId
    });
  }

  /**
   * Handle touchmove.
   */
  function onTouchMove(event) {
    /* SUPPRESSED IN DESIGN MODE, not outside it. `lock.get()` is true in the normal state (no
       design chrome, no dragging) and false while a person is moving panes around — where a
       press belongs to the drag, not to the pane under it. As written this read the other way
       round, so the module did nothing at all in the state the app ships in: no pen state, and
       nothing for palm rejection to arm on. */
    if (lock && lock.get && lock.get() === false) return;
    if (isPalmRejected()) return; // Reject touch while pen is active

    const touch = getPrimaryTouch(event);
    if (!touch) return;

    const { clientX, clientY } = touch;

    // Hit test
    const paneId = inputHooks ? inputHooks.hitTest(clientX, clientY) : null;

    // Emit bus event
    emitPaneEvent(paneId, 'move', {
      clientX,
      clientY,
      touchIdentifier: touch.identifier,
      paneId
    });
  }

  /**
   * Handle touchend / touchcancel.
   */
  function onTouchEnd(event) {
    /* SUPPRESSED IN DESIGN MODE, not outside it. `lock.get()` is true in the normal state (no
       design chrome, no dragging) and false while a person is moving panes around — where a
       press belongs to the drag, not to the pane under it. As written this read the other way
       round, so the module did nothing at all in the state the app ships in: no pen state, and
       nothing for palm rejection to arm on. */
    if (lock && lock.get && lock.get() === false) return;
    if (isPalmRejected()) return;

    const touch = getPrimaryTouch(event);
    if (!touch) return;

    const { clientX, clientY } = touch;

    // Hit test
    const paneId = inputHooks ? inputHooks.hitTest(clientX, clientY) : null;

    // Emit bus event
    emitPaneEvent(paneId, 'end', {
      clientX,
      clientY,
      touchIdentifier: touch.identifier,
      paneId
    });
  }

  // Attach listeners
  worldEl.addEventListener('touchstart', onTouchStart, { passive: true });
  worldEl.addEventListener('touchmove', onTouchMove, { passive: true });
  worldEl.addEventListener('touchend', onTouchEnd, { passive: true });
  worldEl.addEventListener('touchcancel', onTouchEnd, { passive: true });

  return {
    destroy() {
      worldEl.removeEventListener('touchstart', onTouchStart);
      worldEl.removeEventListener('touchmove', onTouchMove);
      worldEl.removeEventListener('touchend', onTouchEnd);
      worldEl.removeEventListener('touchcancel', onTouchEnd);
    }
  };
}
