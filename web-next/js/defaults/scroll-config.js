// scroll-config.js — Scroll tuning configuration (M5.14)
export const DEFAULT_SCROLL_CONFIG = {
  notchPx: 96,
  notchEaseMs: 120,
  accel: { windowMs: 200, perEvent: 0.04, max: 1.8 },
  lineHeightPx: 18,
  touchFrictionPerMs: 0.004,
  touchStopPxPerMs: 0.02,
  snapToLine: true,
};
const CLAMP = {
  notchPx: { min: 24, max: 400 },
  notchEaseMs: { min: 0, max: 400 },
  'accel.max': { min: 1, max: 4 },
  lineHeightPx: { min: 8, max: 72 },
  touchFrictionPerMs: { min: 0, max: 0.1 },
  touchStopPxPerMs: { min: 0, max: 1 },
};
function clampNum(v, min, max) {
  return Math.min(Math.max(v, min), max);
}
function validateScrollValue(key, value) {
  var c = CLAMP[key];
  if (!c) return value;
  if (typeof value !== 'number' || !isFinite(value)) {
    return DEFAULT_SCROLL_CONFIG[key];
  }
  return clampNum(value, c.min, c.max);
}
export function clampScrollConfig(input) {
  if (!input || typeof input !== 'object') return { ...DEFAULT_SCROLL_CONFIG };
  var out = { ...DEFAULT_SCROLL_CONFIG };
  for (var key of Object.keys(DEFAULT_SCROLL_CONFIG)) {
    if (key in input) {
      out[key] = validateScrollValue(key, input[key]);
    }
  }
  if (input.accel && typeof input.accel === 'object') {
    out.accel = { ...DEFAULT_SCROLL_CONFIG.accel };
    if ('max' in input.accel) {
      out.accel.max = validateScrollValue('accel.max', input.accel.max);
    }
    if ('windowMs' in input.accel && typeof input.accel.windowMs === 'number') {
      out.accel.windowMs = input.accel.windowMs;
    }
    if ('perEvent' in input.accel && typeof input.accel.perEvent === 'number') {
      out.accel.perEvent = input.accel.perEvent;
    }
  }
  return out;
}
export function mergeScrollConfig(stored) {
  if (!stored || typeof stored !== 'object') return { ...DEFAULT_SCROLL_CONFIG };
  var merged = { ...DEFAULT_SCROLL_CONFIG };
  for (var key of Object.keys(stored)) {
    if (key in merged) {
      if (key === 'accel' && stored.accel && typeof stored.accel === 'object') {
        merged.accel = { ...merged.accel, ...stored.accel };
        merged.accel.max = validateScrollValue('accel.max', merged.accel.max);
      } else {
        merged[key] = validateScrollValue(key, stored[key]);
      }
    }
  }
  return merged;
}
export const SCROLL_CONFIG_KEY = 'scroll:config';
export function resetScrollConfig(storage) {
  if (!storage || typeof storage.put !== 'function') return;
  try { storage.put(SCROLL_CONFIG_KEY, null); } catch {}
}
