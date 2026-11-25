/**
 * store.js — layout store with undo/redo (M1.24 + M2.00).
 *
 * Wraps commands.reduce() with an undo/redo stack and subscriber bus.
 * Each successful dispatch pushes ONE snapshot on the undo stack and
 * clears the redo stack. Stack is capped at CONFIG.layout.undoLimit.
 * Failed commands push nothing and leave the store unchanged.
 * layout.replace with invalid layout leaves store unchanged and sets lastError().
 * get() returns a deeply frozen object. Subscribers receive (layout, {cmd, source}).
 *
 * D-034: store.dispatch(cmd, {undo:false}) applies without an undo snapshot;
 * store.beginGesture() / store.endGesture() make every dispatch in between
 * snapshot-free and push ONE snapshot (the pre-gesture layout) at endGesture()
 * if anything changed. Camera updates always use {undo:false}.
 */
import { reduce } from './commands.js';
import { CONFIG } from '../core/config.js';
import { deepFreeze } from '../core/config.js';

const DEFAULT_LIMIT = CONFIG.layout.undoLimit;

function deepClone(v) {
  return JSON.parse(JSON.stringify(v));
}

function notify(subs, layout, meta) {
  for (const fn of subs) {
    try {
      fn(layout, meta);
    } catch (err) {
      console.error('store subscriber error:', err);
    }
  }
}

/**
 * Create a layout store.
 * @param {object} initial - initial layout
 * @param {object} opts - { limit?: number }
 * @returns {object} store API
 */
export function createStore(initial, opts = {}) {
  return createLayoutStore(initial, opts);
}

export function createLayoutStore(initial, opts = {}) {
  const limit = typeof opts.limit === 'number' && opts.limit > 0 ? opts.limit : DEFAULT_LIMIT;
  let layout = deepClone(initial);
  const undoStack = [];
  const redoStack = [];
  let lastError = null;
  const subs = new Set();

  // Gesture state (D-034)
  let inGesture = false;
  let gestureSnapshot = null;
  let gestureChanged = false;

  function pushUndo(state) {
    undoStack.push(state);
    if (undoStack.length > limit) undoStack.shift();
  }

  function clearRedo() {
    redoStack.length = 0;
  }

  function notifySubs(meta) {
    notify(subs, layout, meta);
  }

  return {
    /**
     * Dispatch a command through the reducer.
     * @param {object} cmd - command object
     * @param {object} [opts] - { undo?: boolean }
     * @returns {object} {ok, layout?, error?}
     */
    dispatch(cmd, opts = {}) {
      const undo = opts.undo !== false; // default true
      
      const result = reduce(layout, cmd);
      if (!result.ok) {
        lastError = result.error;
        return result;
      }
      
      if (undo && !inGesture) {
        // Normal dispatch with undo
        pushUndo(deepClone(layout));
        clearRedo();
      } else if (undo && inGesture) {
        // Inside a gesture - track if anything changed
        if (!gestureChanged) {
          gestureSnapshot = deepClone(layout);
          gestureChanged = true;
        }
      }
      // If !undo, no snapshot at all
      
      layout = result.layout;
      lastError = null;
      notifySubs({ cmd, source: 'dispatch' });
      return { ok: true, layout };
    },

    /** Begin a gesture - dispatches until endGesture() are snapshot-free, one snapshot at end. */
    beginGesture() {
      if (inGesture) throw new Error('gesture already open');
      inGesture = true;
      gestureSnapshot = null;
      gestureChanged = false;
    },

    /** End a gesture - push one undo snapshot if anything changed. */
    endGesture() {
      if (!inGesture) throw new Error('no gesture open');
      if (gestureChanged && gestureSnapshot) {
        pushUndo(gestureSnapshot);
        clearRedo();
      }
      inGesture = false;
      gestureSnapshot = null;
      gestureChanged = false;
    },

    /** Whether a gesture is currently open. */
    inGesture() {
      return inGesture;
    },

    /** Undo the last command. Returns {ok: true} if undone, {ok: false, error} if not. */
    undo() {
      if (undoStack.length === 0) return { ok: false, error: 'nothing to undo' };
      redoStack.push(deepClone(layout));
      layout = undoStack.pop();
      notifySubs({ cmd: null, source: 'undo' });
      return { ok: true };
    },

    /** Redo the last undone command. Returns {ok: true} if redone, {ok: false, error} if not. */
    redo() {
      if (redoStack.length === 0) return { ok: false, error: 'nothing to redo' };
      undoStack.push(deepClone(layout));
      layout = redoStack.pop();
      notifySubs({ cmd: null, source: 'redo' });
      return { ok: true };
    },

    /** Whether undo is available. */
    canUndo() {
      return undoStack.length > 0;
    },

    /** Whether redo is available. */
    canRedo() {
      return redoStack.length > 0;
    },

    /**
     * Get the current layout (deeply frozen).
     * @returns {object} frozen layout
     */
    get() {
      return deepFreeze(deepClone(layout));
    },

    /**
     * Replace the entire layout (used by layout-json pane).
     * Validates the new layout; on failure, store is unchanged and lastError is set.
     * @param {object} newLayout
     * @returns {boolean} true if replaced
     */
    reset(newLayout) {
      const result = reduce(newLayout, { type: 'layout.replace', layout: newLayout });
      if (!result.ok) {
        lastError = result.error;
        return false;
      }
      pushUndo(deepClone(layout));
      clearRedo();
      layout = result.layout;
      lastError = null;
      notifySubs({ cmd: { type: 'layout.replace' }, source: 'reset' });
      return true;
    },

    /**
     * Get the last error from a failed dispatch/reset.
     * @returns {string|null}
     */
    lastError() {
      return lastError;
    },

    /**
     * Subscribe to layout changes.
     * @param {function} fn - called with (layout, meta) where meta = {cmd, source}
     * @returns {function} unsubscribe
     */
    subscribe(fn) {
      if (typeof fn !== 'function') throw new Error('subscribe requires a function');
      subs.add(fn);
      // Immediate replay with current layout
      fn(layout, { cmd: null, source: 'init' });
      return () => subs.delete(fn);
    },

    /**
     * Create a thin layout API wrapper for panes/scripts.
     * @returns {object} ws.layout-compatible API
     */
    layoutApi() {
      return createLayoutApi(this);
    },
  };
}

/**
 * Create the ws.layout wrapper API used by panes and wiring scripts.
 * Thin wrappers around store.dispatch with ergonomic names.
 * @param {object} store - store returned by createLayoutStore
 * @returns {object} ws.layout API
 */
export function createLayoutApi(store) {
  const { dispatch, get, subscribe } = store;

  function toPath(container) {
    if (!container) return [];
    if (container.root) return [{ root: container.root }];
    if (container.hud) return [{ hud: container.hud }];
    if (container.modal) return [{ modal: container.modal }];
    return [];
  }

  return {
    /** Get current layout (frozen). */
    get: store.get,
    /** Subscribe to layout changes. */
    subscribe: store.subscribe,
    /** Dispatch a raw command. */
    dispatch: store.dispatch,

    /** Insert a pane. */
    insert(pane, to) {
      return dispatch({ type: 'tree.insert', pane, to });
    },

    /** Move a pane. */
    move(pane, to) {
      return dispatch({ type: 'tree.move', id: pane, to });
    },

    /** Split a pane beside another. */
    split(pane, beside, side, ratio = 0.5) {
      return dispatch({ type: 'tree.insert', pane, to: { beside, side, ratio } });
    },

    /** Add pane as tab into another pane. */
    tab(pane, into) {
      return dispatch({ type: 'tree.insert', pane, to: { tabInto: into } });
    },

    /** Detach a pane into a new root. */
    detach(pane, rect = {}) {
      return dispatch({ type: 'tree.insert', pane, to: { newRoot: rect } });
    },

    /** Resize a split container at path. */
    resize(container, path, sizes) {
      return dispatch({ type: 'tree.sizes', path: toPath(container).concat(path), sizes });
    },

    /** Set anchor on a root. */
    anchor(spec) {
      return dispatch({ type: 'anchor.set', ...spec });
    },

    /** Delete a pane (tree.remove). */
    delete(pane) {
      return dispatch({ type: 'tree.remove', id: pane });
    },

    /** Undo last command. */
    undo() {
      return store.undo();
    },

    /** Redo last undone command. */
    redo() {
      return store.redo();
    },

    /** Get current layout. */
    get: store.get,
    /** Subscribe to changes. */
    subscribe: store.subscribe,
  };
}
