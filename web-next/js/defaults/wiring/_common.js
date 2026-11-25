/**
 * defaults/wiring/_common.js — Shared wiring infrastructure (M4.31).
 *
 * Provides wireDesignPanes(ws) and onceReady(ws, fn) used by all wiring
 * presets (standard, minimal, power). No DOM dependencies.
 */

export function wireDesignPanes(ws) {
  const { wire, pane } = ws;
  const layoutJson = pane('layout-json');
  const layoutOutline = pane('layout-outline');
  if (!layoutJson || !layoutOutline || !wire) return;
  if (layoutJson.out && layoutJson.out.json && layoutOutline.in && layoutOutline.in.layout) {
    wire(layoutJson.out.json, layoutOutline.in.layout, { map: (json) => json });
  }
  if (layoutOutline.out && layoutOutline.out.outline && layoutJson.in && layoutJson.in.layout) {
    wire(layoutOutline.out.outline, layoutJson.in.layout, { map: (outline) => outline });
  }
}

export function onceReady(ws, fn, maxAttempts, interval) {
  maxAttempts = maxAttempts || 50;
  interval = interval || 100;
  let attempts = 0;
  const check = () => {
    attempts++;
    try {
      const editor = ws.pane ? ws.pane('editor') : null;
      if (editor || attempts >= maxAttempts) {
        fn();
        return;
      }
    } catch {}
    setTimeout(check, interval);
  };
  check();
}

export function maybeWire(ws, srcKind, srcPort, dstKind, dstPort, opts) {
  const { wire, pane } = ws;
  if (!wire || !pane) return false;
  const src = pane(srcKind);
  const dst = pane(dstKind);
  if (!src || !src.out || !src.out[srcPort]) return false;
  if (!dst || !dst.in || !dst.in[dstPort]) return false;
  wire(src.out[srcPort], dst.in[dstPort], opts || {});
  return true;
}

export function maybeCall(ws, kind, callName, payload) {
  const { pane } = ws;
  if (!pane) return undefined;
  const p = pane(kind);
  if (!p || !p.call || !p.call[callName]) return undefined;
  return p.call[callName].invoke(payload);
}
