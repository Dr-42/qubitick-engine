/**
 * persist.js — Layout persistence for design mode (M3.09).
 *
 * - Debounced 500 ms `storage.put('layout:current', serialize(layout))`
 *   on every store change (camera changes debounced 1 s).
 * - Boot: restores `layout:current` if valid, else falls back to preset.
 * - `?reset=1` ignores the stored layout.
 *
 * @param {object} opts
 * @param {object} opts.store - layout store {get(), subscribe(fn)→unsub, dispatch(cmd)}
 * @param {object} opts.storage - storage adapter {put(key, value), get(key)}
 * @param {function} opts.serialize - serialize(layout) → JSON string
 * @param {function} opts.deserialize - deserialize(text) → {ok, layout} | {ok:false, error}
 * @param {function} opts.applyLayout - (layout) => void, applies layout to store
 * @param {boolean} opts.reset - whether ?reset=1 is set (skip stored layout)
 * @param {number} opts.layoutDebounceMs - debounce for layout changes (default 500)
 * @param {number} opts.cameraDebounceMs - debounce for camera changes (default 1000)
 * @returns {{destroy:()=>void, restore:()=>Promise<boolean>}}
 */
import { debounce } from '../lib/debounce.js';

export function createPersist({ store, storage, serialize, deserialize, applyLayout, reset = false, layoutDebounceMs = 500, cameraDebounceMs = 1000 }) {
  /* Two debounced writes, and the debouncing is js/lib/debounce.js's rather than two
     hand-rolled timers: they were the same eight lines twice, with the difference between
     them (which key, which payload) buried in the middle. */
  const saveLayout = debounce((layout) => {
    const json = serialize(layout);
    if (storage && typeof storage.put === 'function') {
      storage.put('layout:current', json);
    }
  }, layoutDebounceMs);

  const saveCamera = debounce((cameraState) => {
    const json = JSON.stringify({ camera: cameraState });
    if (storage && typeof storage.put === 'function') {
      storage.put('layout:current', json);
    }
  }, cameraDebounceMs);

  const unsub = store.subscribe((layout, { cmd }) => {
    saveLayout(layout);

    // Camera changes: debounce separately at 1s
    if (cmd && cmd.type === 'camera.set') {
      saveCamera(layout.camera);
    }
  });

  /**
   * Restore layout from storage on boot.
   * Returns true if a stored layout was applied, false if fallback should be used.
   */
  async function restore() {
    if (reset) return false;
    if (!storage || !deserialize || !applyLayout) return false;

    const stored = storage.get('layout:current');
    if (!stored) return false;

    try {
      const result = deserialize(stored);
      if (result.ok && result.layout) {
        applyLayout(result.layout);
        return true;
      }
    } catch (e) {
      // Invalid stored layout, fall through to fallback
    }
    return false;
  }

  return {
    destroy() {
      if (unsub) unsub();
      saveLayout.cancel();
      saveCamera.cancel();
    },
    restore,
  };
}
