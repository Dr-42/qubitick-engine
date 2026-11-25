/**
 * keys.js — Keyboard input handling (M2.12).
 *
 * Registers keydown/keyup on document. Handles named actions via key chords.
 * Emits named actions on the bus (ws.emit(name)) for engine/design code to subscribe.
 */

import { isMac } from '../core/config.js';

const MODIFIER_KEY = isMac ? 'metaKey' : 'ctrlKey';

/**
 * Check if the event target is in a typing context.
 * @param {EventTarget} target
 * @returns {boolean}
 */
export function isTypingContext(target) {
  if (!target) return false;
  const tag = target.tagName?.toLowerCase();
  if (tag === 'input' || tag === 'textarea' || tag === 'select') return true;
  if (target.isContentEditable) return true;
  /* `closest` is on an Element, and a key event can be dispatched at the document (a test, a
     script, an extension) — where the target has no such method and asking for one threw. */
  if (target.closest && target.closest('.cm-editor')) return true;
  return false;
}

/**
 * Create key handler.
 * @param {object} opts
 * @param {object} opts.ws - workspace API (bus, camera, etc.)
 * @returns {object} { destroy }
 */
export function createKeyHandler({ ws }) {
  let overviewCamera = null;
  let tabKeyDown = false;

  function emit(name, detail = {}) {
    ws.bus.emit(name, detail);
  }

  function onKeyDown(event) {
    const typing = isTypingContext(event.target);
    const key = event.key;
    const alt = event.altKey;
    const shift = event.shiftKey;
    const ctrl = event[MODIFIER_KEY];

    // Tab overview (hold Tab)
    if (key === 'Tab' && !alt && !ctrl && !shift) {
      if (!tabKeyDown) {
        tabKeyDown = true;
        if (!typing) {
          overviewCamera = ws.camera.get();
          ws.camera.set({ ...ws.camera.get(), lock: 'fit' });
          emit('ui:overview-start');
        }
      }
      // In typing context, allow normal Tab behavior
      return;
    }

    // Escape handling
    if (key === 'Escape') {
      if (typing) {
        event.target.blur();
        event.preventDefault();
      } else {
        // Close top modal - modal module also listens
        emit('ui:modal-close');
        event.preventDefault();
      }
      return;
    }

    // Ignore bare keys in typing context
    if (typing) return;

    // Bare keys
    switch (key) {
      case '0':
        ws.camera.set({ ...ws.camera.get(), lock: 'fit' });
        event.preventDefault();
        return;
      case '1':
        ws.camera.set({ ...ws.camera.get(), z: 1, lock: 'free' });
        event.preventDefault();
        return;
    }

    // Alt chords (work everywhere, preventDefault)
    if (alt && !ctrl && !shift) {
      switch (key) {
        case '.':
          emit('ui:tab-next');
          event.preventDefault();
          return;
        case ',':
          emit('ui:tab-prev');
          event.preventDefault();
          return;
        case 'd':
        case 'D':
          emit('ui:design-toggle');
          event.preventDefault();
          return;
        case 'z':
        case 'Z':
          emit('ui:undo');
          event.preventDefault();
          return;
        case '0':
          emit('ui:fit-all');
          event.preventDefault();
          return;
        case 'f':
        case 'F':
          emit('ui:fullscreen');
          event.preventDefault();
          return;
        case 'h':
        case 'H':
          emit('ui:handwrite');
          event.preventDefault();
          return;
        case 'ArrowUp':
          emit('ui:swipe-prev');
          event.preventDefault();
          return;
        case 'ArrowDown':
          emit('ui:swipe-next');
          event.preventDefault();
          return;
      }
    }

    // Alt+Shift chords
    if (alt && shift && !ctrl) {
      switch (key) {
        case 'z':
        case 'Z':
          emit('ui:redo');
          event.preventDefault();
          return;
        case 'r':
        case 'R':
          emit('ui:panic-reset');
          event.preventDefault();
          return;
        case 'f':
        case 'F':
          emit('ui:fit-all');
          event.preventDefault();
          return;
      }
    }
  }

  function onKeyUp(event) {
    const key = event.key;

    // Tab overview release
    if (key === 'Tab' && tabKeyDown) {
      tabKeyDown = false;
      if (overviewCamera) {
        emit('ui:overview-end');
        ws.camera.flyTo(overviewCamera, { durationMs: 200 });
        overviewCamera = null;
      }
    }
  }

  document.addEventListener('keydown', onKeyDown);
  document.addEventListener('keyup', onKeyUp);

  return {
    destroy() {
      document.removeEventListener('keydown', onKeyDown);
      document.removeEventListener('keyup', onKeyUp);
    }
  };
}
