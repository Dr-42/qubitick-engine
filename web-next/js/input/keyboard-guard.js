// keyboard-guard.js — Soft-keyboard behaviour (M5.13)
function isCoarse() {
  if (typeof window === 'undefined' || !window.matchMedia) return false;
  return window.matchMedia('(pointer: coarse)').matches;
}
function isTypingTarget(target) {
  if (!target) return false;
  var tag = target.tagName ? target.tagName.toLowerCase() : '';
  if (tag === 'input' || tag === 'textarea' || tag === 'select') return true;
  if (target.isContentEditable) return true;
  if (target.closest && target.closest('.cm-editor')) return true;
  if (target.closest && target.closest('[data-pane]')) return true;
  return false;
}
function findPaneEl(target) {
  if (!target || !target.closest) return null;
  var pane = target.closest('[data-pane]');
  return pane || null;
}
function scrollCaretIntoView(paneEl) {
  if (!paneEl) return;
  var body = paneEl.querySelector('.pane-body') || paneEl;
  var active = document.activeElement;
  if (!active || !body.contains(active)) return;
  try {
    active.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  } catch {}
}
export function installKeyboardGuard(opts) {
  opts = opts || {};
  var ws = opts.ws;
  var rootEl = opts.rootEl;
  var viewportEl = opts.viewportEl;
  var coarse = isCoarse();
  if (!coarse) return { destroy: function() {} };
  var lastViewport = null;
  function captureViewport() {
    return { w: window.innerWidth, h: window.innerHeight };
  }
  lastViewport = captureViewport();
  function onFocusIn(event) {
    if (!isTypingTarget(event.target)) return;
    var paneEl = findPaneEl(event.target);
    scrollCaretIntoView(paneEl);
  }
  function onViewportResize() {
    if (!viewportEl || !window.visualViewport) return;
    var vv = window.visualViewport;
    var vw = window.innerWidth;
    var vh = window.innerHeight;
    try {
      document.documentElement.style.setProperty('--vvh', Math.max(vv.height, vh) + 'px');
    } catch {}
    var now = { w: vw, h: vh };
    var prev = lastViewport || { w: vw, h: vh };
    var widthChanged = now.w !== prev.w;
    var prevSign = Math.sign(prev.w - prev.h);
    var nowSign = Math.sign(now.w - now.h);
    var orientationChanged = widthChanged || prevSign !== nowSign;
    lastViewport = now;
    if (ws.camera && typeof ws.camera.get === 'function') {
      var cam = ws.camera.get();
      if (cam && cam.lock === 'fit' && orientationChanged) {
        if (typeof ws.camera.fit === 'function') {
          ws.camera.fit();
        }
      }
    }
  }
  if (rootEl) {
    rootEl.addEventListener('focusin', onFocusIn);
  }
  if (window.visualViewport) {
    window.visualViewport.addEventListener('resize', onViewportResize);
    window.visualViewport.addEventListener('scroll', onViewportResize);
  }
  return {
    destroy: function() {
      if (rootEl) {
        rootEl.removeEventListener('focusin', onFocusIn);
      }
      if (window.visualViewport) {
        window.visualViewport.removeEventListener('resize', onViewportResize);
        window.visualViewport.removeEventListener('scroll', onViewportResize);
      }
    },
  };
}
