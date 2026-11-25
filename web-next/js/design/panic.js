/**
 * panic.js — Panic reset for design mode (M3.09, updated M5.11).
 *
 * Handles `ui:panic-reset` events dispatched by the design bar.
 * Confirms with window.confirm (skipped when ?debug=1&noconfirm=1),
 * saves current layout JSON to storage key `layout:backup`,
 * then `layout.replace` with the shipped preset layout (presetUrl(layout.id))
 * and `camera.fit()`. Clears `layout:<id>` from storage.
 *
 * @param {object} opts
 * @param {HTMLElement} opts.root - root element to listen on
 * @param {object} opts.store - layout store {dispatch(cmd), get()}
 * @param {object} opts.camera - camera {fit?(), set?(x,y,z)}
 * @param {object} opts.storage - storage adapter {put(key, value), get(key)}
 * @param {string} opts.presetId - preset id, e.g. 'standard-desktop'
 * @param {boolean} opts.debug - whether ?debug=1 is set
 * @param {boolean} opts.noConfirm - whether ?noconfirm=1 is set
 * @returns {{destroy:()=>void}}
 */
import { presetUrl } from '../layout/presets.js';

export function createPanic({ root, store, camera, storage, presetId = 'standard-desktop', debug = false, noConfirm = false }) {
  function onPanicReset(e) {
    e.preventDefault();
    e.stopPropagation();

    // Skip confirmation when ?debug=1&noconfirm=1
    const shouldConfirm = !(debug && noConfirm);
    if (shouldConfirm && !window.confirm('Reset layout to default? This cannot be undone.')) {
      return;
    }

    // Save current layout as backup
    const current = store.get();
    const backupJson = JSON.stringify(current, null, 2);
    if (storage && typeof storage.put === 'function') {
      storage.put('layout:backup', backupJson);
    }

    // Load and apply the preset layout
    const url = presetUrl(presetId);
    fetch(url)
      .then(r => {
        if (!r.ok) throw new Error(`preset ${url} not found (${r.status})`);
        return r.json();
      })
      .then(presetLayout => {
        const result = store.dispatch({ type: 'layout.replace', layout: presetLayout });
        if (!result.ok) {
          console.error('Panic reset failed:', result.error);
          return;
        }
        // Fit camera after reset
        if (camera && typeof camera.fit === 'function') {
          camera.fit();
        } else if (camera && typeof camera.set === 'function') {
          camera.set({ x: 0, y: 0, z: 1 }, { undo: false });
        }
        // Clear per-preset stored layout
        if (storage && typeof storage.put === 'function') {
          storage.put('layout:' + presetId, null);
        }
      })
      .catch(err => {
        console.error('Panic reset failed to load preset:', err);
      });
  }

  root.addEventListener('ui:panic-reset', onPanicReset);

  return {
    destroy() {
      root.removeEventListener('ui:panic-reset', onPanicReset);
    },
  };
}
