/*
 * world/chrome.js — the frame loop, the HUD and the status rail.
 *
 * Extracted from the inline script in web-next/index.html. This is the only place in the
 * client that runs every frame, so the rules are about what may run there:
 *
 *   THE LOOP DOES NOTHING IT DOES NOT HAVE TO. The layout pass, the wire-dot placement and
 *   the mount pass run only when something is DIRTY — the camera moved, a tile changed, the
 *   workspace was edited. Everything else here is per-frame by nature: the scroll easings, the
 *   tiles that draw themselves, the wire layer.
 *
 *   THE HUD IS WRITTEN ON A TIMER, NOT EVERY FRAME. Text nodes are layout: writing an fps
 *   readout sixty times a second is a layout pass sixty times a second, for a number nobody can
 *   read that fast. Once every ~400ms is the same information, one hundredth of the cost.
 *
 *   THE RAIL IS THE EDITOR'S, IN SCREEN SPACE. Line/column, file name and the unsaved mark
 *   follow the editor's own ports, so a tile in another corner of the world cannot make the
 *   rail disagree with the editor that has focus.
 *
 * The frame loop also NEVER THROWS: one broken tile drawing itself must not stop the loop —
 * a frame loop that throws stops being a frame loop, and the canvas freezes with no error the
 * user can see.
 */

/** How often the HUD's numbers are rewritten, in ms. */
const HUD_MS = 400;

/**
 * @param {object} host
 * @param {() => Element|null} host.el            by id
 * @param {Array} host.panes
 * @param {() => object} host.cam
 * @param {() => boolean} host.dirty              has anything changed since the last frame?
 * @param {() => void} host.clean                 the frame has dealt with it
 * @param {() => void} host.updateLOD
 * @param {() => void} host.mountVisible
 * @param {() => void} host.recount
 * @param {() => void} host.stepScrollers
 * @param {(t:number) => void} host.drawWires
 * @param {(visibleOnlyCheck:(id:string)=>boolean) => void} [host.placePorts]
 * @param {() => object} host.stats               { counts, drawn, built, total }
 * @param {() => number} host.nodes               corpus size, for the rail
 * @param {() => number} host.symbols
 * @param {() => void} [host.updateWireCount]
 * @param {() => void} [host.applyInputs]
 * @param {() => boolean} [host.wiresArmed]
 * @param {object} [host.rail]                    { cursor, file, dirty } ports (`.on(fn)`)
 * @param {() => object} [host.chip]              the check chip, as data
 * @param {() => void} [host.runCheck]
 * @param {(fn:Function) => any} [host.requestFrame]
 * @param {() => number} [host.now]
 */
export function createChrome(host) {
  const el = host.el || (() => null);
  const cam = host.cam;
  /* exactly one module in the client owns the frame scheduler: the page injects it */
  const requestFrame = host.requestFrame;
  const now = host.now || (() => performance.now());
  const offs = [];

  const RAIL = {
    mode: el('r-mode'), pos: el('r-pos'), file: el('r-file'), dirty: el('r-dirty'),
    nodes: el('r-nodes'), sym: el('r-sym'), zoom: el('r-zoom'), check: el('r-check'),
  };
  const HUD = {
    zoom: el('h-zoom'), perf: el('h-perf'), lod: el('h-lod'),
    zoom2: el('h-zoom2'), perf2: el('h-perf2'), lod2: el('h-lod2'),
    ops: el('h-ops'), wires: el('h-wires'), data: el('h-data'),
  };

  let last = now(), lastHud = 0, deltas = [];

  /** One frame. Everything the loop is allowed to do, in the order it has to happen. */
  function frame(t) {
    try {
      const d = t - last;
      last = t;
      deltas.push(d);
      if (deltas.length > 90) deltas.shift();

      if (host.dirty()) {
        host.updateLOD();
        host.clean();
        // only the visible tiles' ports are re-measured: a bounding rect costs layout
        if (host.placePorts) host.placePorts((id) => { const p = host.panes.find((x) => x.id === id); return !p || p.visible !== false; });
        host.mountVisible();
        host.recount();
      }
      host.stepScrollers();

      for (const p of host.panes) {
        if (p.visible && p.lod === 'live' && p.implDraw) {
          try { p.implDraw(t); } catch (e) { /* one tile must not stop the loop */ }
        }
      }
      host.drawWires(t);

      if (t - lastHud > HUD_MS) {
        lastHud = t;
        paintHud();
        if (host.updateWireCount) host.updateWireCount();
        if (host.applyInputs && (!host.wiresArmed || host.wiresArmed())) host.applyInputs();
      }
    } finally {
      if (requestFrame) requestFrame(frame);
    }
  }

  /** The numbers, once every HUD_MS. */
  function paintHud() {
    const s = host.stats();
    const counts = s.counts || {};
    const avg = deltas.length ? deltas.reduce((a, b) => a + b, 0) / deltas.length : 0;
    const fps = avg ? (1000 / avg).toFixed(0) : '–';
    const z = Math.round(cam().z * 100) + '%';
    const line = (s.drawn || 0) + ' drawn · ' + (counts.live || 0) + ' live · ' + (counts.frozen || 0) +
      ' frozen · ' + (counts.card || 0) + ' card · ' + (counts.culled || 0) + ' culled';
    if (HUD.zoom) HUD.zoom.textContent = z;
    if (HUD.perf) HUD.perf.textContent = fps + ' fps';
    if (HUD.lod) HUD.lod.textContent = line;
    if (HUD.zoom2) HUD.zoom2.textContent = z;
    if (HUD.perf2) HUD.perf2.textContent = fps + 'fps';
    if (HUD.lod2) HUD.lod2.textContent = counts.live + ' live';
    if (RAIL.zoom) RAIL.zoom.textContent = z;
    if (RAIL.nodes) RAIL.nodes.textContent = host.nodes ? (host.nodes() || '–') : '–';
    if (RAIL.sym) RAIL.sym.textContent = host.symbols ? (host.symbols() || '–') : '–';
  }

  /** The rail: the editor's vitals, in screen space, following the editor's own ports. */
  function bindRail() {
    const rail = host.rail;
    if (rail) {
      if (rail.cursor && rail.cursor.on) offs.push(rail.cursor.on((c) => { if (RAIL.pos) RAIL.pos.textContent = 'Ln ' + c.line + ', Col ' + c.col; }));
      if (rail.file && rail.file.on) offs.push(rail.file.on((f) => { if (RAIL.file) RAIL.file.textContent = String(f || '—').replace(/^\.\/project\//, ''); }));
      if (rail.dirty && rail.dirty.on) offs.push(rail.dirty.on((d) => { if (RAIL.dirty) RAIL.dirty.classList.toggle('hidden', !d); }));
    }
    if (RAIL.check) {
      if (host.runCheck) RAIL.check.addEventListener('click', () => host.runCheck());
      offs.push(() => { if (host.runCheck) RAIL.check.removeEventListener('click', host.runCheck); });
      paintChip();
    }
  }

  /** The chip is words and a tone out of the diagnostics module; the element is this. */
  function paintChip() {
    if (!RAIL.check || !host.chip) return;
    const c = host.chip();
    RAIL.check.textContent = c.text;
    RAIL.check.className = 'r-seg r-check ' + c.tone;
    RAIL.check.title = c.title;
  }

  function start() { if (requestFrame) requestFrame(frame); else frame(now()); }
  function stop() { while (offs.length) { const off = offs.pop(); try { if (typeof off === 'function') off(); } catch (e) { /* gone */ } } }

  return { RAIL, HUD, frame, start, stop, bindRail, paintHud, paintChip };}
