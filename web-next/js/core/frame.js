import { CONFIG } from './config.js';
/* the frame timings report p95s, and `percentile` is js/lib/math.js's (it had a private copy
   of the same four lines). */
import { percentile } from '../lib/math.js';

export function realDeps() {
  return {
    raf: (cb) => globalThis.requestAnimationFrame(cb),
    caf: (id) => globalThis.cancelAnimationFrame(id),
    now: () => globalThis.performance.now(),
    setTimeout: globalThis.setTimeout.bind(globalThis),
    clearTimeout: globalThis.clearTimeout.bind(globalThis),
    isHidden: () => globalThis.document.hidden,
    hasFocus: () => globalThis.document.hasFocus(),
    onVisibility: (cb) => {
      if (typeof globalThis.document !== 'undefined') {
        globalThis.document.addEventListener('visibilitychange', cb);
      }
      return () => {
        if (typeof globalThis.document !== 'undefined') {
          globalThis.document.removeEventListener('visibilitychange', cb);
        }
      };
    },
  };
}

export function createGovernor(deps = realDeps()) {
  const {
    raf, caf, now, setTimeout, clearTimeout,
    isHidden, hasFocus, onVisibility,
  } = deps;

  const pending = new Map();
  let rafId = null;
  let timeoutId = null;
  let scheduled = false;
  let lastFrame = 0;
  let hasRun = false;
  let frames = 0;
  const last = [];

  function frameInterval() {
    if (isHidden()) return Infinity;
    return hasFocus() ? 1000 / CONFIG.frame.maxFps : 1000 / CONFIG.frame.unfocusedFps;
  }

  function maybeSchedule() {
    if (pending.size === 0 || scheduled) return;
    const iv = frameInterval();
    if (iv === Infinity) return;
    const elapsed = !hasRun ? Infinity : now() - lastFrame;
    if (elapsed >= iv) {
      rafId = raf(runFrame);
      scheduled = true;
    } else {
      timeoutId = setTimeout(() => {
        timeoutId = null;
        rafId = raf(runFrame);
        scheduled = true;
      }, iv - elapsed);
      scheduled = true;
    }
  }

  function runFrame(t) {
    rafId = null;
    scheduled = false;

    const start = now();
    const dt = lastFrame === 0 ? 0 : start - lastFrame;
    lastFrame = start;
    hasRun = true;
    frames++;

    // Capture callbacks queued before frame started
    const cbs = [...pending.values()];
    pending.clear();

    const frameStart = now();
    for (const cb of cbs) {
      try {
        cb();
      } catch (err) {
        console.error(err);
      }
    }
    const cost = now() - frameStart;

    last.push({ t: start, dt, cost });
    if (last.length > 300) last.shift();

    maybeSchedule();
  }

  function requestFrame(owner, cb) {
    const key = owner + '\0' + String(cb);
    /* THE LATEST REQUEST WINS. A key is an owner plus the SOURCE of its callback, so every
       renderer request has the same key — and this used to keep the FIRST one and drop the
       rest. Two changes inside one frame therefore drew the FIRST of them and nothing drew the
       second: a pane added to a layout appeared only after some later change, because the frame
       that ran was rendering the layout from before it. Coalescing is one frame per owner, not
       one request per owner. */
    pending.set(key, cb);
    maybeSchedule();
    return () => {
      pending.delete(key);
      if (pending.size === 0 && scheduled) {
        if (rafId) caf(rafId);
        if (timeoutId) clearTimeout(timeoutId);
        rafId = null;
        timeoutId = null;
        scheduled = false;
      }
    };
  }

  function stats() {
    const intervals = last.map((e) => e.dt);
    const costs = last.map((e) => e.cost);
    return {
      frames,
      p95Interval: intervals.length > 0 ? percentile(intervals, 95) : 0,
      p95Cost: costs.length > 0 ? percentile(costs, 95) : 0,
      last: last.slice(),
    };
  }

  function idle() {
    return pending.size === 0 && !scheduled;
  }

  function destroy() {
    if (rafId) caf(rafId);
    if (timeoutId) clearTimeout(timeoutId);
    rafId = null;
    timeoutId = null;
    scheduled = false;
    pending.clear();
  }

  let offVisibility = () => {};
  if (onVisibility) {
    offVisibility = onVisibility(() => {
      if (!isHidden()) maybeSchedule();
    });
  }

  return { requestFrame, stats, idle, destroy };
}