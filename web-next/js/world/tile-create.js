/*
 * tile-create.js — making a tile: its id, where it goes, and what its header does.
 *
 * Extracted from the inline script in web-next/index.html, where `mkPane` was both the
 * factory and the wiring, and every caller that wanted a tile had to remember which of the
 * five prefs-derived states the kind was supposed to start with.
 *
 * A tile is BUILT here, and only here, because a tile's construction order is load
 * bearing:
 *
 *   1. the shell        the head, the body and the card come from one module, so the
 *                       buttons a kind advertises are the buttons js/world/tile-shell.js
 *                       knows how to act on (a kind cannot offer an action that does
 *                       nothing);
 *   2. into the world   before the grips, because a grip measures against the tile;
 *   3. the grips        then the rect, because placing a gripped tile is one write;
 *   4. the kind's state the host's own (which file a corpus tile follows, whether its
 *                       selection is shared, whether it highlights syntax) — the factory
 *                       hands the tile over for this rather than guessing, because the
 *                       state lives with the app that owns it;
 *   5. the header       five buttons, a double-click that fits the view to the tile, and
 *                       a press anywhere else on it that drags the tile;
 *   6. the body         last, because a pane's mount reads its rect and its own state.
 *
 * An id is per kind and collision-checked (js/world/tile-factory.js), and a NEW tile lands
 * near the middle of what you are looking at, stepped so a burst fans out. The sequence
 * lives here with the factory: two counters would be two tiles with one id, and the wire
 * layer resolves a wire by id.
 */

import { freeTileId, placementFor } from './tile-factory.js';

/**
 * @param {object} host
 * @param {Element} host.world              where tiles are appended
 * @param {Array} host.panes                the workspace's tile list (the new tile is pushed)
 * @param {{view:object}} host.ui           the prefs a new tile is born with
 * @param {(kind:string) => string} host.titleOf
 * @param {object} host.kinds               { docs, links, syntax, noClose } — the kind sets
 * @param {(spec:object) => object} host.build   tile-shell.js buildShell
 * @param {(spec:object) => string[]} host.actionsFor
 * @param {(p:object) => void} host.mountGrips
 * @param {(p:object) => void} host.setRect
 * @param {(p:object, e:Event) => void} host.drag
 * @param {(p:object) => void} host.render
 * @param {(p:object, state:object) => void} host.boot       the host's own per-kind state
 * @param {object} host.actions            { fit, focus, full, src, del, pin, link }
 * @param {{x:number,y:number,z:number}} host.cam
 * @param {() => {w:number,h:number}} [host.viewport]   where "the middle of the view" is
 * @param {(m:string) => void} [host.toast]
 */
export function createTileFactory(host) {
  const { world, panes, ui, titleOf, kinds, build, actionsFor, mountGrips, setRect, drag, render, boot, actions, cam } = host;
  const viewport = host.viewport || (() => null);
  const toast = host.toast || (() => {});
  const seq = new Map();                       // kind → the counter its ids reached

  /** An id of this kind that nothing in the workspace already holds. */
  function next(kind) {
    const { id, counter } = freeTileId(kind, { taken: panes.map((p) => p.id), from: seq.get(kind) || 0 });
    seq.set(kind, counter);
    return id;
  }

  /** Build a tile at a rect. `state` is whatever the host saved for it. */
  function make(id, kind, wr, home, state) {
    const title = titleOf(kind);
    const docs = kinds.docs.has(kind);         // shows corpus text: it can be pinned to a file
    const links = kinds.links.has(kind);       // reads the corpus: its selection can be its own
    /* which buttons a kind gets is a table in js/world/tile-shell.js */
    const { el, head, body, card } = build({ id, kind, title,
      actions: actionsFor({ kind, docs, links, noClose: kinds.noClose }) });
    world.appendChild(el);
    const p = { id, kind, wr, el, head, body, card, lod: 'live', _summary: false,
                home: home || 'free' };
    mountGrips(p);
    setRect(p);
    if (boot) boot(p, state || null);          // 4. the host's own kind state
    /* 5. the header: a button does what its name says, and only if the shell made it */
    const on = (a, fn) => {
      const b = head.querySelector('[data-a="' + a + '"]');
      if (b) b.addEventListener('click', (e) => { e.stopPropagation(); fn(p); });
    };
    for (const a of ['fit', 'focus', 'full', 'src', 'del', 'pin', 'link']) {
      if (actions[a]) on(a, actions[a]);
    }
    head.addEventListener('dblclick', () => actions.fit && actions.fit(p));
    // drag a tile HEADER to reposition it freely; dropping detaches it from any cluster
    head.addEventListener('pointerdown', (e) => drag(p, e));
    panes.push(p);
    render(p);                                 // 6.
    return p;
  }

  /** A new tile of this kind, placed near the middle of what you are looking at. */
  function free(kind, state) {
    const id = next(kind);
    const wr = placementFor(cam, (seq.get(kind) || 1), { view: viewport() });
    const p = make(id, kind, wr, 'free', state);
    toast('+ ' + kind + ' tile');
    return p;
  }

  /** The workspace was reset: the sequences start over with it. */
  function reset() { seq.clear(); }

  return { make, free, next, reset, seq };
}
