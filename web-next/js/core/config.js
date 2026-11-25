export const isMac = navigator.platform.toUpperCase().includes("MAC");
export function deepFreeze(obj) {
  if (obj === null || typeof obj !== 'object') return obj;
  if (Object.isFrozen(obj)) return obj;
  Object.freeze(obj);
  for (const key of Object.keys(obj)) {
    deepFreeze(obj[key]);
  }
  return obj;
}

export const CONFIG = deepFreeze({
  zoom: { min: 0.04, max: 5, wheelFactor: 1.0015, pinchFactor: 1 },
  frame: { maxFps: 30, unfocusedFps: 10 },
  lod: { cardBelow: 110, frozenBelow: 190, zoomRef: 360, up: 1.12, down: 0.9, cullMarginPx: 300 },
  layout: {
    minPaneSize: 80,
    sashVisualPx: 2,
    sashHitPx: 8,
    sashHitPxCoarse: 48,
    tabBarPx: 28,
    tabBarPxCoarse: 44,
    defaultNewRoot: { w: 520, h: 380 },
    dropEdgeFraction: 0.25,
    undoLimit: 100,
    headerPx: 26,
    headerPxCoarse: 40,
  },
  scroll: {
    notchPx: 96,
    notchEaseMs: 120,
    accel: { windowMs: 200, perEvent: 0.04, max: 1.8 },
    lineHeightPx: 18,
    touchFrictionPerMs: 0.004,
    touchStopPxPerMs: 0.02,
    snapToLine: true,
  },
  touch: { hitPx: 48, penGraceMs: 300 },
  wire: { maxDepth: 32 },
  fly: { ms: 450 },
});