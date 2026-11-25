/*
 * world/canvas-input.js — what the pointer and the keyboard mean on the canvas.
 *
 * Extracted from the inline script in web-next/index.html. The POLICY is not here: it is
 * `js/input/*` (hooks.js decides from the pointer's region, wheel.js classifies the event,
 * scroll.js runs the easing), all pure and unit-tested. This is the host — the listeners,
 * and the three pieces of state a gesture needs between events:
 *
 *   the WHEEL      ctrl/meta zooms about the pointer (a trackpad pinch arrives as
 *                  ctrl+wheel); over a pane's body it scrolls THAT pane and nothing else —
 *                  no chaining to the camera at the edge, no falling through when the pane
 *                  has nowhere to scroll; anything else pans the camera. Which pane it
 *                  scrolls is the HOVERED one, which is also the one marked active, so it
 *                  is visible where a gesture will land before you make it.
 *
 *   the SCROLL     one easing per element. A notch eases over `notchEaseMs` instead of
 *                  jumping, so the animation is stepped from the frame loop, retargeted (not
 *                  restarted) by the next notch, and — for something laid out in lines —
 *                  snapped to a line when it settles. A file tree is not laid out in lines:
 *                  snapping it to `lineHeightPx` lands the list between rows.
 *
 *   the KEYS       Space grabs (drag the canvas), Tab frames the workspace and comes back,
 *                  0 frames, 1 is 100%, +/- nudge, w toggles the wires, Escape leaves
 *                  fullscreen. A key pressed while a text field has focus belongs to the
 *                  field: `js/input/keys.js::isTypingContext` owns that, and every shortcut
 *                  behind it. The editor is a CodeMirror `contenteditable` div, not a
 *                  `<textarea>`: a guard that only knew about form tags swallowed Space
 *                  before CodeMirror ever saw it.
 */

import { isTypingContext } from '../input/keys.js';

/**
 * @param {object} host
 * @param {Element} host.viewport
 * @param {Element} host.world
 * @param {{x:number,y:number,z:number}} host.cam
 * @param {() => void} host.applyCam
 * @param {() => void} host.stopZoom
 * @param {(px:number,x:number,y:number,pinch:boolean) => void} host.wheel
 * @param {() => {w:number,h:number}} host.viewportSize
 * @param {() => object|null} host.inputModules   js/input/* once loaded (null before)
 * @param {object} host.scrollCfg
 * @param {() => object|null} [host.hooks]        a pane/plugin wheel override, consulted first
 * @param {(f:number) => void} [host.zoomNudge]
 * @param {(t:object, ms:number) => void} [host.flyTo]
 * @param {() => object} [host.camFor]            the camera that frames the workspace
 * @param {() => void} [host.onToggleWires]
 * @param {() => object|null} [host.focus]        { solo, exit }
 * @param {(m:string) => void} [host.toast]
 * @param {object} [host.document]
 */
export function createCanvasInput(host) {
  const { viewport, world, cam, applyCam, stopZoom } = host;
  const doc = host.document || (typeof document !== 'undefined' ? document : null);
  const win = (doc && doc.defaultView) || (typeof window !== 'undefined' ? window : null);
  const cfg = host.scrollCfg || {};
  const textKinds = new Set(host.textKinds || ['editor', 'render', 'ir', 'lean']);
  const clamps = host.clampDelta || ((v) => Math.max(-240, Math.min(240, v)));

  const scrollers = new Map();        // element → { engine, snapPx }: one easing per element
  let hot = null;                     // the tile under the pointer
  let space = false;
  let saved = null;                   // the camera Tab took you away from
  const offs = [];

  const IN = () => (host.inputModules ? host.inputModules() : null);
  const size = () => host.viewportSize();
  const clamp = (v) => clamps(v);

  /** Snapping a finished scroll onto a line only makes sense for something laid out in
   *  lines: a tree of files is not, and snapping it lands the list between rows. */
  function isTextHost(el, paneEl) {
    if (!el) return false;
    if (/cm-scroller|cm-content/.test(el.className || '')) return true;
    return !!(paneEl && textKinds.has(paneEl.dataset.kind) && /cm-|ed-/.test(el.className || ''));
  }

  function scrollerFor(el, snapPx) {
    let st = scrollers.get(el);
    if (!st) {
      const mods = IN();
      st = { engine: mods.createScrollEngine({ config: cfg }), snapPx: snapPx || 0 };
      scrollers.set(el, st);
    }
    return st;
  }

  /** One step of every running scroll easing. Called from the frame loop. */
  function step() {
    if (!scrollers.size) return;
    const mods = IN();
    for (const [el, st] of [...scrollers]) {
      const s = st.engine.step();
      if (s.dyLayout) el.scrollTop += s.dyLayout;
      if (s.dxLayout) el.scrollLeft += s.dxLayout;
      if (!st.engine.active()) {
        scrollers.delete(el);
        if (cfg.snapToLine && st.snapPx > 0) {
          const snapped = mods.snapTarget(el.scrollTop, st.snapPx);
          if (snapped !== el.scrollTop && Math.abs(snapped - el.scrollTop) < st.snapPx) el.scrollTop = snapped;
        }
      }
    }
  }

  /** The tile under the pointer is the ACTIVE one: it is where the wheel goes, and it is
   *  marked so you can see that before you use it. */
  function setHotPane(el) {
    if (el === hot) return;
    if (hot) hot.removeAttribute('data-active');
    hot = el || null;
    if (hot) hot.setAttribute('data-active', '');
  }

  const hitRegion = (ev) => {
    const mods = IN();
    return mods
      ? mods.hitRegion(ev.target, viewport)
      : { region: (ev.target === viewport || ev.target === world) ? 'empty' : 'chrome', pane: null, body: null };
  };

  function onWheel(e) {
    const mods = IN();
    if (!mods) {                        // the policy module has not landed yet
      if (e.ctrlKey || e.metaKey) { e.preventDefault(); const { w, h } = size(); host.wheel(e.deltaY, e.clientX, e.clientY, true); }
      else if (e.target === viewport || e.target === world) { e.preventDefault(); stopZoom(); cam.x -= e.deltaX; cam.y -= e.deltaY; applyCam(); }
      return;
    }
    const hit = hitRegion(e);
    const cls = mods.classifyWheel(e, { pageHeight: size().h, lineHeightPx: cfg.lineHeightPx, notchPx: cfg.notchPx });
    const hooks = host.hooks ? host.hooks() : null;
    if (hooks && hooks.run('wheel', e, { hit, cls, camera: cam })) { e.preventDefault(); return; }
    const px = cls
      ? (cls.kind === 'notch' ? Math.sign(cls.notches || 0) * Math.abs(cls.notches || 0) * cfg.notchPx : (cls.pxY || 0))
      : e.deltaY;
    const el = (hit.region === 'pane-body') ? mods.scrollHostIn(e.target, hit.body) : null;
    const canScroll = el ? mods.canScrollAlong(el, cls) : false;
    const dec = mods.decideWheel({ region: hit.region, modifier: e.ctrlKey || e.metaKey, canScroll });
    if (dec.action === 'zoom-camera') {
      e.preventDefault();
      host.wheel(px, e.clientX, e.clientY, cls && cls.kind !== 'notch');
    } else if (dec.action === 'scroll-pane') {
      e.preventDefault();
      stopZoom();
      const snapPx = isTextHost(el, hit.pane) ? cfg.lineHeightPx : 0;
      scrollerFor(el, snapPx).engine.feed(cls, { z: cam.z, snapPx });
    } else if (dec.action === 'pan-camera') {
      e.preventDefault();
      stopZoom();
      const d = mods.screenDelta(cls, { notchPx: cfg.notchPx });
      const dy = clamp(d.sy), dx = clamp(d.sx);
      if (e.shiftKey) { cam.x -= dy; cam.y -= dx; } else { cam.x -= dx; cam.y -= dy; }
      applyCam();
    } else if (dec.action === 'none') {
      // the pane owns the gesture and cannot move: the camera must stay put
      e.preventDefault();
    }
  }

  function onMove(e) {
    const hit = hitRegion(e);
    setHotPane(hit.region === 'pane-body' || hit.region === 'pane-head' || hit.region === 'sash' ? hit.pane : null);
  }

  function onDown(e) {
    const empty = e.target === viewport || e.target === world;
    if (e.button === 1 || (e.button === 0 && (space || empty))) {
      e.preventDefault();
      stopZoom();
      viewport.setPointerCapture(e.pointerId);
      let lx = e.clientX, ly = e.clientY;
      const mv = (ev) => { cam.x += ev.clientX - lx; cam.y += ev.clientY - ly; lx = ev.clientX; ly = ev.clientY; applyCam(); };
      const up = () => { viewport.removeEventListener('pointermove', mv); viewport.removeEventListener('pointerup', up); };
      viewport.addEventListener('pointermove', mv);
      viewport.addEventListener('pointerup', up);
    }
  }

  /** A key pressed while a text field has focus belongs to the field — and an editor is a
   *  field even when it is a `contenteditable` div rather than a form tag. */
  const typing = (e) => isTypingContext(e.target);

  function onKeyDown(e) {
    /* SAVE THE OPEN DOCUMENT. The canvas had no save at all — the console's `save` writes the WORKSPACE
       (which tiles exist where), so a person looking for "save my file" found that button and the wrong
       meaning. Ctrl/Cmd+S is the shortcut everyone tries, and the browser's own would reload the page. */
    if ((e.ctrlKey || e.metaKey) && (e.key === 's' || e.key === 'S')) {
      e.preventDefault();
      if (host.saveDoc) host.saveDoc();
      return;
    }
    if (e.key === 'Escape') {
      const f = host.focus ? host.focus() : null;
      if (f && f.solo()) { f.exit(); return; }
      if (doc && doc.activeElement) doc.activeElement.blur();
      return;
    }
    if (typing(e)) return;
    const mods = IN();
    if (e.code === 'Space') {
      e.preventDefault();
      space = true;
      viewport.classList.add('grab');
      return;
    }
    if (e.key === 'Tab') {
      e.preventDefault();
      if (!e.repeat && !saved && host.flyTo) { saved = { ...cam }; host.flyTo(host.camFor(), 260); }
      return;
    }
    if (e.key === '0' && host.flyTo) host.flyTo(host.camFor(), 400);
    else if (e.key === '1' && host.flyTo) {
      const { w, h } = size();
      const c = { x: (w / 2 - cam.x) / cam.z, y: (h / 2 - cam.y) / cam.z };
      host.flyTo({ z: 1, x: w / 2 - c.x, y: h / 2 - c.y }, 300);
    }
    else if (e.key === '+' || e.key === '=') { if (host.zoomNudge) host.zoomNudge(1.25); }
    else if (e.key === '-' || e.key === '_') { if (host.zoomNudge) host.zoomNudge(1 / 1.25); }
    else if (e.key === 'w' || e.key === 'W') { if (host.onToggleWires) host.onToggleWires(); }
    void mods;
  }

  function onKeyUp(e) {
    if (e.code === 'Space') { space = false; viewport.classList.remove('grab'); }
    if (e.key === 'Tab' && saved && host.flyTo) { host.flyTo(saved, 260); saved = null; }
  }

  /** Attach the listeners. Returns a detacher (a test, or a re-boot).
   *  Idempotent: attaching twice would give every wheel two listeners, and a page that
   *  boots the canvas once must not pay for the possibility that it booted twice. */
  function attach() {
    if (offs.length) return detach;
    const add = (el, type, fn, opts) => { el.addEventListener(type, fn, opts); offs.push(() => el.removeEventListener(type, fn, opts)); };
    add(viewport, 'wheel', onWheel, { passive: false });
    add(viewport, 'pointermove', onMove, { passive: true });
    add(viewport, 'pointerleave', () => setHotPane(null), { passive: true });
    add(viewport, 'pointerdown', onDown);
    if (win) {
      add(win, 'keydown', onKeyDown);
      add(win, 'keyup', onKeyUp);
    }
    return detach;
  }

  function detach() {
    while (offs.length) { const off = offs.pop(); try { off(); } catch (e) { /* already gone */ } }
    setHotPane(null);
    scrollers.clear();
  }

  return {
    attach, detach, step,
    setHotPane,
    hotPane: () => hot,
    space: () => space,
    scrollers,
    isTextHost,
    wheelCfg: cfg,
  };
}