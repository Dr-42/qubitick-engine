// design/device.js — Device-class selection and persistence (M5.10)
import { screenClass } from '../layout/presets.js';

const DEVICE_KEY = 'device:selection';
const DISMISSED_KEY = 'device:dismissed';

export function suggestDevice(viewport, coarse) {
  const screen = screenClass({ width: viewport.width, height: viewport.height, coarse });
  return { mode: 'standard', screen: screen };
}

export function shouldSuggest(selection, now, observed, dismissed, debounceMs) {
  debounceMs = debounceMs || 2000;
  if (dismissed) return false;
  if (!selection) return false;
  if (selection.screen !== observed.screen) return true;
  return false;
}

export function createDeviceSelection(storage) {
  var cached = null;

  async function load() {
    if (cached) return cached;
    try {
      var raw = await storage.get(DEVICE_KEY);
      cached = raw || null;
    } catch { cached = null; }
    return cached;
  }

  async function save(sel) {
    cached = sel;
    try { await storage.put(DEVICE_KEY, sel); } catch {}
  }

  async function clear() {
    cached = null;
    try { await storage.put(DEVICE_KEY, null); } catch {}
  }

  async function getDismissed() {
    try {
      var raw = await storage.get(DISMISSED_KEY);
      return raw === true || raw === 'true';
    } catch { return false; }
  }

  async function setDismissed(val) {
    try { await storage.put(DISMISSED_KEY, val); } catch {}
  }

  return { load: load, save: save, clear: clear, getDismissed: getDismissed, setDismissed: setDismissed };
}
