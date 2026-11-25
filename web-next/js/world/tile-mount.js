/*
 * world/tile-mount.js — building a tile's body, and building nothing else.
 *
 * Extracted from the inline script in web-next/index.html. Three rules, and the first one is
 * the reason the other two exist:
 *
 *   NOTHING OFF SCREEN IS BUILT. A tile nobody can see should cost nothing: its body is
 *   replaced with a placeholder and it is marked `pendingMount`, and the frame loop builds it
 *   when (and only when) it comes into view. A workspace restored from a document can be ten
 *   screens wide, and a view that builds all of it is a view that stalls for a second on
 *   arrival and then draws a canvas nobody is looking at.
 *
 *   A BEHAVIOUR NODE IS MOUNTED FROM ITS PROGRAM. The three behaviours (`js-node`, `clock`,
 *   `track`) declare a program rather than a render, so they are mounted by the code path that
 *   compiles user code — never by the pane table — or a program node would get a render that
 *   does not exist.
 *
 *   EVERYTHING ELSE IS THE REGISTRY'S. `kind → mount` is the pane registry's own answer
 *   (`js/panes/app-mounts.js` for the canvas kinds, a manifest's mount for a user pane), and
 *   a kind with no handler says so in its body rather than throwing in the frame loop — a
 *   frame loop that throws stops being a frame loop.
 */

/**
 * @param {object} host
 * @param {Array} host.panes
 * @param {object} host.registry            kind → pane record ({ category, mount, render })
 * @param {object} host.lod                 js/app/lod.js (screenRect / onScreen)
 * @param {() => {x:number,y:number,z:number}} host.cam
 * @param {() => {w:number,h:number}} host.viewport
 * @param {() => boolean} [host.visibleOnly]  build only what is on screen (the default)
 * @param {(rec:object) => Function} host.behaviourMount   mount a program node
 * @param {(render:object) => Function} host.userMount     mount a user pane's descriptor
 * @param {(s:string) => string} [host.esc]
 */
export function createTileMount(host) {
  const { panes, registry, lod } = host;
  const esc = host.esc || ((s) => String(s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])));
  const cam = host.cam;
  const size = host.viewport;
  const visibleOnly = host.visibleOnly || (() => true);

  /** Is this tile on screen at all? An unmeasured tile is measured here, once. */
  function onScreen(p) {
    const { w, h } = size();
    return lod.onScreen(p.sr || lod.screenRect(p, cam()), w, h, 0);
  }

  /** Build one tile's body, or say why it is not being built. */
  function render(p) {
    const rec = registry[p.kind];
    if (!rec) return false;
    // nothing off screen is built: the frame loop calls back when it comes into view
    if (visibleOnly() && p.el && p.el.isConnected && !onScreen(p)) {
      p.pendingMount = true;
      p.body.innerHTML = '<div class="empty">' + esc(p.kind) + ' builds when it is in view</div>';
      return false;
    }
    p.pendingMount = false;
    let mount;
    if (rec.category === 'behaviour') mount = host.behaviourMount(rec);
    else mount = rec.mount || host.userMount(rec.render);
    if (mount) mount(p);
    else p.body.innerHTML = '<div class="empty">no handler for ' + esc(p.kind) + '</div>';
    p._summary = true;
    // a card says what the tile is, so it has to be filled the moment the tile is built
    if (p.lod === 'card' && p.summary && p.card && p.card.lastChild) p.card.lastChild.textContent = p.summary();
    return true;
  }

  /** Build whichever tiles have come into view since the last frame. */
  function mountVisible() {
    for (const p of panes) {
      if (!p.pendingMount) continue;
      if (onScreen(p)) render(p);
    }
  }

  /** The kinds a picker can offer, in the registry's own order (unknown kinds last). */
  function kindsFor(order = []) {
    const rank = (k) => { const i = order.indexOf(k); return i < 0 ? 999 : i; };
    return Object.keys(registry).sort((a, b) => (rank(a) - rank(b)) || a.localeCompare(b));
  }

  /** Fill a `<select>` with every kind, in that order, defaulting to the registry's default. */
  function populate(select, { order = [], fallback = null, doc = (typeof document !== 'undefined' ? document : null) } = {}) {
    if (!select || !doc) return false;
    select.innerHTML = '';
    for (const id of kindsFor(order)) {
      const o = doc.createElement('option');
      o.value = id;
      o.textContent = (registry[id] && registry[id].title) || id;
      select.appendChild(o);
    }
    if (fallback && [...select.options].some((o) => o.value === fallback)) select.value = fallback;
    return true;
  }

  return { render, mountVisible, onScreen, kindsFor, populate };}
