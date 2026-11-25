/**
 * app/util.js — the small pure helpers the rest of the app needs.
 *
 * Extracted first because almost every other module wants them: `esc` is used 31
 * times in the app, `clamp` 8. They are imported BY NAME, so the call sites in
 * index.html did not change at all when they moved — which is the rule for this
 * refactor: extract the definition, not the callers.
 */

/** Escape for interpolation into HTML. One table, used everywhere. */
export const esc = s =>
  String(s ?? '').replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

/** Clamp `v` into `[a, b]`. */
export const clamp = (v, a, b) => Math.max(a, Math.min(b, v));

/** A debounced call: the last one wins after `ms` of quiet. */
export const debounce = (fn, ms) => {
  let t;
  return (...a) => {
    clearTimeout(t);
    t = setTimeout(() => fn(...a), ms);
  };
};

/**
 * A duration a person reads: a whole-corpus check is tens of seconds, so
 * "73371ms" is the wrong unit long before it is the wrong number.
 */
export function durText(ms) {
  if (!ms) return '';
  return ms < 3000 ? Math.round(ms) + 'ms' : (ms / 1000).toFixed(1) + 's';
}