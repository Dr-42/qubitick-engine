/**
 * hooks.js — Input hooks and the default wheel policy (M2.19 / M2.10).
 *
 * The wheel means whatever the thing under the pointer means. That is the whole
 * policy, and it is decided from the pointer's *region*, never from what happens
 * to be scrollable somewhere up the tree:
 *
 *   ctrl / meta + wheel   → zoom the camera about the pointer
 *                           (a trackpad pinch arrives as ctrl+wheel)
 *   over a pane's body    → scroll that pane's own content, and NOTHING else:
 *                           no chaining to the camera at the edge, and no
 *                           falling through when the pane has nothing to scroll.
 *                           The pane under the pointer owns the gesture.
 *   over a pane's head,
 *   a sash, a tab bar,
 *   or empty canvas       → pan the camera
 *
 * `hitRegion` answers "which region is this pointer in"; `decideWheel` turns that
 * plus the modifier into an action and is pure, so the policy is testable without
 * a DOM. `createHooks` is the override seam: a pane or a plugin can register a
 * wheel hook and take the gesture before the default policy runs.
 */

export function createHooks() {
  const hooks = new Map(); // name -> [{fn, priority}]

  return {
    /** Register a callback for a named hook; higher priority runs first. */
    register(name, fn, priority = 0) {
      if (!hooks.has(name)) hooks.set(name, []);
      hooks.get(name).push({ fn, priority });
      hooks.get(name).sort((a, b) => b.priority - a.priority);
      return () => this.unregister(name, fn);
    },

    /** Unregister a previously registered callback. */
    unregister(name, fn) {
      const list = hooks.get(name);
      if (!list) return;
      const idx = list.findIndex((h) => h.fn === fn);
      if (idx >= 0) list.splice(idx, 1);
    },

    /** Run every callback for a named hook; true as soon as one consumes the event. */
    run(name, ...args) {
      const list = hooks.get(name);
      if (!list) return false;
      for (const { fn } of [...list]) {
        try {
          if (fn(...args) === true) return true;
        } catch (err) {
          console.error('Hook ' + name + ' threw:', err);
        }
      }
      return false;
    },

    names() { return [...hooks.keys()]; },
  };
}

/** The regions a pointer can be in. Everything not listed is chrome. */
export const REGIONS = ['pane-body', 'pane-head', 'tabbar', 'sash', 'empty', 'chrome'];

/**
 * Which region is this event in? Walks up from the event target to the viewport,
 * naming the first structural thing it enters.
 *
 * Deliberately cheap: it runs on every pointer move to keep the hovered pane
 * marked, so it does no layout reads and never scans the pane's subtree.
 *
 * @param {Element|null} target
 * @param {Element} viewportEl
 * @returns {{region:string, pane:Element|null, body:Element|null}}
 */
export function hitRegion(target, viewportEl) {
  const el = target instanceof Element ? target : null;
  let pane = null, body = null, region = null;
  for (let n = el; n && n !== viewportEl; n = n.parentElement) {
    const cls = n.classList;
    if (!cls) continue;
    if (cls.contains('sash')) return { region: 'sash', pane: pane || (n.closest && n.closest('[data-pane]')), body: null };
    if (cls.contains('tabbar') || cls.contains('frame-label')) return { region: 'tabbar', pane, body: null };
    if (!pane && n.dataset && n.dataset.pane) pane = n;
    if (!body && cls.contains('pane-body')) { region = 'pane-body'; body = n; }
    if (!region && cls.contains('pane-head')) region = 'pane-head';
  }
  if (region === 'pane-body') return { region, pane, body };
  if (region === 'pane-head') return { region, pane, body: null };
  if (pane) return { region: 'pane-body', pane, body: null };
  const inside = !!el && !!viewportEl && viewportEl.contains(el);
  return { region: inside ? 'empty' : 'chrome', pane: null, body: null };
}

/**
 * The element a gesture inside a pane body should scroll: the nearest ancestor of
 * the event target (up to and including the body) that really scrolls. Walking up
 * from the target rather than scanning the subtree is both cheaper and truer —
 * the thing under the pointer is the thing that scrolls.
 */
export function scrollHostIn(target, body) {
  let n = target instanceof Element ? target : null;
  for (; n; n = n.parentElement) {
    const cs = getComputedStyle(n);
    const oy = cs.overflowY, ox = cs.overflowX;
    const canY = (oy === 'auto' || oy === 'scroll') && n.scrollHeight - n.clientHeight > 1;
    const canX = (ox === 'auto' || ox === 'scroll') && n.scrollWidth - n.clientWidth > 1;
    if (canY || canX) return n;
    if (n === body) break;
  }
  return null;
}

/** Can this element still move along the gesture's axis? */
export function canScrollAlong(el, cls) {
  if (!el || !cls) return false;
  const down = cls.kind === 'notch' ? cls.notches > 0 : cls.pxY > 0;
  const up = cls.kind === 'notch' ? cls.notches < 0 : cls.pxY < 0;
  const y = el.scrollHeight - el.clientHeight > 1 && (down ? el.scrollTop < el.scrollHeight - el.clientHeight - 1 : up ? el.scrollTop > 0 : false);
  const right = cls.pxX > 0, left = cls.pxX < 0;
  const x = el.scrollWidth - el.clientWidth > 1 && (right ? el.scrollLeft < el.scrollWidth - el.clientWidth - 1 : left ? el.scrollLeft > 0 : false);
  return y || x;
}

/**
 * The default wheel policy. Pure: give it the region and it answers.
 * @returns {{action:'zoom-camera'|'scroll-pane'|'pan-camera'|'none', because:string}}
 */
export function decideWheel({ region, modifier = false, canScroll = true } = {}) {
  if (modifier) return { action: 'zoom-camera', because: 'ctrl/meta always zooms the camera' };
  switch (region) {
    case 'pane-body':
      // the pane owns the wheel: no chaining to the camera, and nothing at all
      // when the pane has nowhere to scroll — the gesture is still the pane's
      return canScroll
        ? { action: 'scroll-pane', because: 'the pane under the pointer owns the wheel' }
        : { action: 'none', because: 'the pane under the pointer cannot scroll' };
    case 'empty':
    case 'pane-head':
    case 'tabbar':
    case 'sash':
      return { action: 'pan-camera', because: 'the pointer is not over a pane body' };
    default:
      return { action: 'none', because: 'not our input' };
  }
}

/**
 * Minimal DOM wiring kept for callers that just want the pane id.
 * @returns {{hitTest:function, hitRegion:function, hooks:object, destroy:function}}
 */
export function installHooks({ viewportEl, ws, hooks } = {}) {
  if (!viewportEl) throw new Error('installHooks: viewportEl is required');
  if (!hooks) throw new Error('installHooks: hooks is required');
  const hitTest = (x, y) => {
    const el = document.elementFromPoint(x, y);
    if (!el) return null;
    const pane = el.closest && el.closest('[data-pane]');
    return pane ? pane.getAttribute('data-pane') : null;
  };
  return { hitTest, hitRegion: (t) => hitRegion(t, viewportEl), hooks, ws, destroy() {} };
}
