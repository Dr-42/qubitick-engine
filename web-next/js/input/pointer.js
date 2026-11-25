/**
 * pointer.js — Pointer input handling (M2.19).
 *
 * Installs pointerdown/pointermove/pointerup listeners on the viewport.
 * Routes pointer events to the pane under the pointer via hooks.hitTest.
 * Tracks pen state (active/inactive, pointerType) for palm rejection.
 * Emits bus events for pane-specific pointer input.
 */

/**
 * Default pen state.
 */
const DEFAULT_PEN_STATE = {
  active: false,
  type: null,   // 'pen' | 'mouse' | 'touch' | null
  x: 0,
  y: 0,
  pressure: 0,
  timestamp: 0
};

/**
 * Install pointer event handling.
 * @param {object} opts
 * @param {HTMLElement} opts.worldEl - DOM element to listen on
 * @param {object} opts.ws - Workspace API (camera, bus, hooks, lock)
 * @param {object} opts.camera - Camera state
 * @param {object} opts.store - Layout store
 * @param {object} opts.instances - Pane instance manager
 * @returns {object} { destroy }
 */
export function installPointer({ worldEl, ws, camera, store, instances } = {}) {
  if (!worldEl) {
    throw new Error('installPointer: worldEl is required');
  }
  if (!ws) {
    throw new Error('installPointer: ws is required');
  }

  const { bus, hooks: inputHooks, lock } = ws;

  // Pen state - shared with touch module via instances
  const penState = {
    active: false,
    type: null,
    x: 0,
    y: 0,
    pressure: 0,
    timestamp: 0
  };

  // Expose penState and palm rejection on instances
  if (instances) {
    instances.penState = penState;
    instances.isPalmRejected = () => penState.active && penState.type === 'pen';
  }

  /**
   * Emit a pane-specific pointer event on the bus.
   */
  function emitPaneEvent(paneId, type, detail) {
    if (!paneId || !bus) return;
    bus.emit('pane:pointer:' + type, { paneId, ...detail });
  }

  /**
   * Handle pointerdown.
   */
  function onPointerDown(event) {
    /* SUPPRESSED IN DESIGN MODE, not outside it. `lock.get()` is true in the normal state (no
       design chrome, no dragging) and false while a person is moving panes around — where a
       press belongs to the drag, not to the pane under it. As written this read the other way
       round, so the module did nothing at all in the state the app ships in: no pen state, and
       nothing for palm rejection to arm on. */
    if (lock && lock.get && lock.get() === false) return;

    const { clientX, clientY, pointerType, pressure = 0 } = event;

    // Update pen state
    penState.active = true;
    penState.type = pointerType || 'mouse';
    penState.x = clientX;
    penState.y = clientY;
    penState.pressure = pressure;
    penState.timestamp = Date.now();

    // Hit test to find target pane
    const paneId = inputHooks ? inputHooks.hitTest(clientX, clientY) : null;

    // Emit bus event
    emitPaneEvent(paneId, 'down', {
      clientX,
      clientY,
      pointerType: penState.type,
      pressure,
      paneId
    });

    // Let pane-specific handlers deal with it (e.g. probe canvas)
    // We don't preventDefault here so panes can handle it
  }

  /**
   * Handle pointermove.
   */
  function onPointerMove(event) {
    /* SUPPRESSED IN DESIGN MODE, not outside it. `lock.get()` is true in the normal state (no
       design chrome, no dragging) and false while a person is moving panes around — where a
       press belongs to the drag, not to the pane under it. As written this read the other way
       round, so the module did nothing at all in the state the app ships in: no pen state, and
       nothing for palm rejection to arm on. */
    if (lock && lock.get && lock.get() === false) return;

    const { clientX, clientY, pointerType, pressure = 0 } = event;

    // Update pen state position
    penState.x = clientX;
    penState.y = clientY;
    penState.pressure = pressure;
    penState.timestamp = Date.now();

    // Hit test
    const paneId = inputHooks ? inputHooks.hitTest(clientX, clientY) : null;

    // Emit bus event
    emitPaneEvent(paneId, 'move', {
      clientX,
      clientY,
      pointerType: penState.type,
      pressure,
      paneId
    });
  }

  /**
   * Handle pointerup / pointerleave / pointercancel.
   */
  function onPointerEnd(event) {
    /* SUPPRESSED IN DESIGN MODE, not outside it. `lock.get()` is true in the normal state (no
       design chrome, no dragging) and false while a person is moving panes around — where a
       press belongs to the drag, not to the pane under it. As written this read the other way
       round, so the module did nothing at all in the state the app ships in: no pen state, and
       nothing for palm rejection to arm on. */
    if (lock && lock.get && lock.get() === false) return;

    const { clientX, clientY, pointerType } = event;

    // Update pen state
    penState.active = false;
    penState.x = clientX;
    penState.y = clientY;
    penState.timestamp = Date.now();

    // Hit test
    const paneId = inputHooks ? inputHooks.hitTest(clientX, clientY) : null;

    // Emit bus event
    emitPaneEvent(paneId, 'up', {
      clientX,
      clientY,
      pointerType: penState.type,
      paneId
    });
  }

  // Attach listeners
  worldEl.addEventListener('pointerdown', onPointerDown, { passive: true });
  worldEl.addEventListener('pointermove', onPointerMove, { passive: true });
  worldEl.addEventListener('pointerup', onPointerEnd, { passive: true });
  worldEl.addEventListener('pointerleave', onPointerEnd, { passive: true });
  worldEl.addEventListener('pointercancel', onPointerEnd, { passive: true });

  return {
    destroy() {
      worldEl.removeEventListener('pointerdown', onPointerDown);
      worldEl.removeEventListener('pointermove', onPointerMove);
      worldEl.removeEventListener('pointerup', onPointerEnd);
      worldEl.removeEventListener('pointerleave', onPointerEnd);
      worldEl.removeEventListener('pointercancel', onPointerEnd);
      penState.active = false;
      penState.type = null;
      if (instances) {
        instances.penState = undefined;
        instances.isPalmRejected = undefined;
      }
    },

    /** Get current pen state (read-only). */
    get penState() {
      return {
        active: penState.active,
        type: penState.type,
        x: penState.x,
        y: penState.y,
        pressure: penState.pressure,
        timestamp: penState.timestamp
      };
    },

    /** Check if touch should be palm-rejected while pen is active. */
    isPalmRejected() {
      return penState.active && penState.type === 'pen';
    }
  };
}
