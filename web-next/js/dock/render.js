/**
 * dock/render.js — the docking shell's DOM.
 *
 * One element per region, one element per tab group, one sash between neighbours. The layout
 * is CSS flex, sizes are inline px, and the only thing this module decides is WHERE a thing
 * goes — what a panel draws is its own module, mounted into the group's body.
 *
 * THE SASHES ARE THE POINT of a docking shell, so they get the care:
 *
 *   · a drag clamps against the dock's own min and max (`model.js::clampSize`) and against
 *     the viewport, so a dock can never swallow the editor;
 *   · a DOUBLE-CLICK resets that one split to the preset's size — the escape hatch A0 asks
 *     for, and the reason a user can experiment without losing the layout they liked;
 *   · every change is reported through `onChange(doc, {source})`, which is how the document
 *     becomes `Custom` and reaches localStorage. The renderer never writes storage itself.
 *
 * PANEL ELEMENTS SURVIVE. A tab switch hides a body rather than destroying it: an editor
 * that loses its DOM loses its cursor, its undo stack and its file. `_unmount` is called only
 * when a panel leaves the document entirely.
 */

import {
  DOCKS, PANELS, RAIL, clampSize, isRailed,
} from './model.js';

const el = (tag, cls, parent) => {
  const n = document.createElement(tag);
  if (cls) n.className = cls;
  if (parent) parent.appendChild(n);
  return n;
};

/** The label a rail shows: the dock's own name, vertically. */
function railLabel(dock, doc) {
  const d = doc.docks[dock];
  const first = d && d.groups[0] ? PANELS[d.groups[0].tabs[0]] : null;
  return first ? first.title : dock;
}

/**
 * Create the dock view.
 *
 * @param {object} opts
 * @param {HTMLElement} opts.root - the page's `#dock` element
 * @param {object} opts.doc - a normalised dock document (see `model.js`)
 * @param {object} opts.instances - the pane instance manager (`panes/instances.js`)
 * @param {function} opts.onChange - `(doc, {source}) => void`, called after any user change
 * @param {function} [opts.onFocus] - `(panelId) => void`, when a panel is focused
 * @returns {object} { render, destroy, focusPanel, elementFor, setDoc }
 */
export function createDockView({ root, doc, instances, onChange, onFocus }) {
  let current = doc;
  const paneBodies = new Map();   // panelId -> body element (kept across tab switches)
  const mounted = new Set();      // panelIds currently mounted
  let frame = null;

  const app = el('div', 'dock-app', root);
  /* THE TWO ROWS ARE REBUILT BY EVERY PAINT. `app.innerHTML = ''` is how a repaint drops the
     old dock, and it drops these children with it — writing into a detached `main` is a tree
     nothing ever sees, which is exactly what happened: a status strip and an empty page. */
  let main = null;
  let bottomRow = null;

  /** Schedule one paint for this frame: a drag is many events and one layout. */
  const schedule = () => {
    if (frame !== null) return;
    frame = requestAnimationFrame(() => { frame = null; paint(); });
  };

  const report = (source) => { if (onChange) onChange(current, { source }); };

  /**
   * Mount every pane the document shows, into the body element that belongs to it.
   *
   * The body is created once per panel and reused: `_mount(id, body)` is the instance
   * manager's entry, and calling it twice for the same id would re-run a pane's mount.
   */
  function syncInstances() {
    const wanted = new Set();
    for (const d of DOCKS) {
      const dock = current.docks[d];
      if (!dock || dock.hidden) continue;
      for (const g of dock.groups) {
        for (const id of g.tabs) wanted.add(id);
      }
    }
    for (const id of wanted) {
      if (mounted.has(id)) continue;
      const body = paneBodies.get(id);
      if (!body) continue;
      try {
        if (instances && typeof instances._mount === 'function') instances._mount(id, body);
        mounted.add(id);
      } catch (err) {
        console.error('dock: mount failed for', id, err);
        body.textContent = 'panel failed to mount: ' + id;
      }
    }
    for (const id of [...mounted]) {
      if (wanted.has(id)) continue;
      try { if (instances && typeof instances._unmount === 'function') instances._unmount(id); } catch (err) {
        console.error('dock: unmount failed for', id, err);
      }
      mounted.delete(id);
    }
  }

  /** A splitter that drags one number and resets it on double-click. */
  function sash(orient, { getSize, setSize, onEnd }) {
    const s = el('div', orient === 'v' ? 'dock-sash dock-sash-v' : 'dock-sash dock-sash-h');
    s.setAttribute('role', 'separator');
    s.setAttribute('aria-orientation', orient === 'v' ? 'vertical' : 'horizontal');
    s.tabIndex = 0;

    let startPx = 0;
    let startPos = 0;
    let dragging = false;

    const posOf = (ev) => (orient === 'v' ? ev.clientX : ev.clientY);

    const onMove = (ev) => {
      if (!dragging) return;
      const delta = posOf(ev) - startPos;
      // A left/top dock grows with the pointer; a right/bottom dock grows against it.
      setSize(startPx + (s.dataset.grows === '-1' ? -delta : delta));
    };
    const onUp = () => {
      if (!dragging) return;
      dragging = false;
      s.classList.remove('dragging');
      document.removeEventListener('pointermove', onMove);
      document.removeEventListener('pointerup', onUp);
      if (onEnd) onEnd();
    };

    s.addEventListener('pointerdown', (ev) => {
      ev.preventDefault();
      dragging = true;
      startPx = getSize();
      startPos = posOf(ev);
      s.classList.add('dragging');
      document.addEventListener('pointermove', onMove);
      document.addEventListener('pointerup', onUp);
    });
    // A keyboard can move a splitter too: arrows are the obvious pair for one axis.
    s.addEventListener('keydown', (ev) => {
      const step = ev.shiftKey ? 40 : 8;
      if (ev.key === 'ArrowLeft' || ev.key === 'ArrowUp') { ev.preventDefault(); setSize(getSize() - step); }
      else if (ev.key === 'ArrowRight' || ev.key === 'ArrowDown') { ev.preventDefault(); setSize(getSize() + step); }
      else if (ev.key === 'Enter') { ev.preventDefault(); s.dispatchEvent(new Event('dock:reset')); }
    });
    s.addEventListener('dblclick', () => s.dispatchEvent(new Event('dock:reset')));
    return s;
  }

  /** The tab strip of one group. */
  function tabsBar(dockName, gi, g) {
    const bar = el('div', 'dock-tabs');
    g.tabs.forEach((id, i) => {
      const def = PANELS[id];
      const tab = el('button', 'dock-tab' + (i === g.active ? ' active' : ''), bar);
      tab.type = 'button';
      tab.textContent = def ? def.title : id;
      tab.dataset.panel = id;
      tab.title = `${def ? def.title : id}  ·  Ctrl+click closes`;
      tab.addEventListener('click', (ev) => {
        if (ev.ctrlKey || ev.metaKey) { closePanel(id); return; }
        g.active = i;
        schedule();
        report('tab');
        if (onFocus) onFocus(id);
      });
    });
    // The collapse handle lives on the strip, so every dock collapses the same way.
    const collapse = el('button', 'dock-collapse', bar);
    collapse.type = 'button';
    collapse.textContent = dockName === 'bottom' ? '⌄' : (dockName === 'left' ? '«' : '»');
    collapse.title = 'Collapse this dock';
    collapse.addEventListener('click', () => {
      current.docks[dockName].collapsed = true;
      schedule();
      report('collapse');
    });
    return bar;
  }

  /** The body of one group: the ACTIVE panel's element, and the others kept but hidden. */
  function groupBody(g) {
    const body = el('div', 'dock-group-body');
    for (const id of g.tabs) {
      let paneBody = paneBodies.get(id);
      if (!paneBody) {
        paneBody = el('div', 'dock-pane');
        paneBody.dataset.panel = id;
        const def = PANELS[id];
        if (def) paneBody.dataset.kind = def.kind;
        paneBodies.set(id, paneBody);
      }
      const active = g.tabs[g.active] === id;
      paneBody.hidden = !active;
      paneBody.classList.toggle('active', active);
      body.appendChild(paneBody);
    }
    return body;
  }

  /** One dock: its groups, and the sashes between them. */
  function dockElement(dockName, parent) {
    const d = current.docks[dockName];
    const wrap = el('section', 'dock dock-' + dockName, parent);
    wrap.dataset.dock = dockName;
    if (d.hidden) { wrap.hidden = true; return wrap; }

    if (d.collapsed) {
      wrap.classList.add('railed');
      const rail = el('button', 'dock-rail', wrap);
      rail.type = 'button';
      rail.textContent = railLabel(dockName, current);
      rail.title = 'Expand the ' + dockName + ' dock';
      rail.addEventListener('click', () => {
        d.collapsed = false;
        schedule();
        report('expand');
      });
      wrap.style.flex = '0 0 ' + RAIL + 'px';
      return wrap;
    }

    // left/right stack their groups vertically; centre and bottom lay them side by side
    const stacked = dockName === 'left' || dockName === 'right';
    wrap.classList.add(stacked ? 'stack' : 'span');
    wrap.style.flex = dockName === 'center' ? '1 1 auto' : '0 0 ' + d.size + 'px';
    if (dockName === 'center') wrap.style.minWidth = '0';

    d.groups.forEach((g, gi) => {
      if (gi > 0) {
        const between = sash(stacked ? 'h' : 'v', {
          getSize: () => Math.round((d.sizes[gi - 1] || 0) * 1000),
          setSize: (px) => {
            // Sizes are fractions of the dock's extent along the split axis; a drag moves
            // the boundary between group gi-1 and gi, keeping every other fraction put.
            const extent = dockExtent(dockName, wrap);
            if (!extent) return;
            const a = d.sizes[gi - 1] || 0;
            const b = d.sizes[gi] || 0;
            const delta = px / 1000 - a;         // the sash speaks in thousandths
            const minFrac = 80 / extent;
            const na = Math.max(minFrac, Math.min(a + b - minFrac, a + delta));
            d.sizes[gi - 1] = na;
            d.sizes[gi] = a + b - na;
            schedule();
          },
          onEnd: () => report('split'),
        });
        between.dataset.for = dockName + ':' + gi;
        wrap.appendChild(between);
      }
      const gEl = el('div', 'dock-group', wrap);
      gEl.dataset.dock = dockName;
      gEl.dataset.group = String(gi);
      gEl.style.flex = stacked
        ? '0 0 ' + Math.round((d.sizes[gi] || 0) * 100) + '%'
        : '1 1 0';
      if (!stacked && d.sizes.length === d.groups.length) {
        gEl.style.flex = '0 0 ' + Math.round((d.sizes[gi] || 0) * 100) + '%';
      }
      if (g.tabs.length > 1) gEl.appendChild(tabsBar(dockName, gi, g));
      else {
        // a single tab still needs a handle: its title, and the collapse control
        const bar = el('div', 'dock-tabs single', gEl);
        const def = PANELS[g.tabs[0]];
        const label = el('span', 'dock-tab active static', bar);
        label.textContent = def ? def.title : g.tabs[0];
        label.title = (def ? def.title : g.tabs[0]) + '  ·  Ctrl+click closes';
        label.dataset.panel = g.tabs[0];
        label.addEventListener('click', (ev) => {
          if (ev.ctrlKey || ev.metaKey) closePanel(g.tabs[0]);
        });
        const collapse = el('button', 'dock-collapse', bar);
        collapse.type = 'button';
        collapse.textContent = dockName === 'bottom' ? '⌄' : (dockName === 'left' ? '«' : '»');
        collapse.title = 'Collapse this dock';
        collapse.addEventListener('click', () => {
          current.docks[dockName].collapsed = true;
          schedule();
          report('collapse');
        });
      }
      gEl.appendChild(groupBody(g));
    });
    return wrap;
  }

  /** The dock's extent along its split axis, measured once per drag step. */
  function dockExtent(dockName, wrap) {
    const r = wrap.getBoundingClientRect();
    return (dockName === 'left' || dockName === 'right') ? r.height : r.width;
  }

  /** Close a panel: take it out of its group, and the group out of the dock when it empties. */
  function closePanel(id) {
    for (const d of DOCKS) {
      const dock = current.docks[d];
      if (!dock || !dock.groups) continue;
      for (let gi = 0; gi < dock.groups.length; gi++) {
        const g = dock.groups[gi];
        const at = g.tabs.indexOf(id);
        if (at < 0) continue;
        g.tabs.splice(at, 1);
        if (g.active >= g.tabs.length) g.active = Math.max(0, g.tabs.length - 1);
        if (!g.tabs.length) {
          dock.groups.splice(gi, 1);
          dock.sizes = dock.groups.length ? dock.sizes.slice(0, dock.groups.length) : [];
          if (dock.groups.length && dock.sizes.length !== dock.groups.length) {
            dock.sizes = dock.groups.map(() => 1 / dock.groups.length);
          }
        }
        if (!dock.groups.length) dock.hidden = true;
        schedule();
        report('close');
        return;
      }
    }
  }

  /** Paint: rebuild the tree and keep the mounted bodies. */
  function paint() {
    app.innerHTML = '';
    main = el('div', 'dock-main', app);
    bottomRow = el('div', 'dock-bottom-row', app);

    const viewport = { width: root.clientWidth || window.innerWidth || 1400 };

    // The top row: left | centre | right, with a sash between each pair.
    const left = current.docks.left;
    const right = current.docks.right;
    if (left && !left.hidden) {
      main.appendChild(dockElement('left', main));
      main.appendChild(sash('v', {
        getSize: () => (left.collapsed ? RAIL : left.size),
        setSize: (px) => {
          if (left.collapsed) return;
          left.size = clampSize('left', px, viewport);
          schedule();
        },
        onEnd: () => report('resize'),
      }));
    }
    main.appendChild(dockElement('center', main));
    if (right && !right.hidden) {
      const between = sash('v', {
        getSize: () => (right.collapsed ? RAIL : right.size),
        setSize: (px) => {
          if (right.collapsed) return;
          right.size = clampSize('right', px, viewport);
          schedule();
        },
        onEnd: () => report('resize'),
      });
      between.dataset.grows = '-1';
      main.appendChild(between);
      main.appendChild(dockElement('right', main));
    }

    // The bottom dock, under everything, with its own sash.
    const bottom = current.docks.bottom;
    if (bottom && !bottom.hidden) {
      const between = sash('h', {
        getSize: () => bottom.size,
        setSize: (px) => {
          bottom.size = clampSize('bottom', px, viewport);
          schedule();
        },
        onEnd: () => report('resize'),
      });
      between.dataset.grows = '-1';
      bottomRow.appendChild(between);
      bottomRow.appendChild(dockElement('bottom', bottomRow));
    }

    syncInstances();

    /* DOUBLE-CLICK RESETS THIS SPLIT, and to what it was: the document carries `defaults`
       (the sizes the layout came up with, see `model.js::snapshot`), so the reset survives a
       reload and a custom layout resets to the arrangement it was first made from. */
    app.querySelectorAll('.dock-sash').forEach((s) => {
      s.addEventListener('dock:reset', (ev) => {
        ev.stopPropagation();
        const docks = current.docks || {};
        const def = current.defaults || {};
        const groupOf = s.dataset.for ? s.dataset.for.split(':') : null;

        if (groupOf) {
          // a sash BETWEEN TWO GROUPS of one dock: restore that dock's group sizes
          const name = groupOf[0];
          const d = docks[name];
          const want = def[name] && def[name].sizes;
          if (d && want && want.length === d.groups.length) d.sizes = want.slice();
          else if (d && d.groups.length) d.sizes = d.groups.map(() => 1 / d.groups.length);
          schedule();
          report('reset-sash');
          return;
        }

        // a sash on the shell: reset the dock it borders to the size it started at
        const prev = s.previousElementSibling;
        const next = s.nextElementSibling;
        const which = prev && prev.dataset && prev.dataset.dock && prev.dataset.dock !== 'center'
          ? prev.dataset.dock
          : (next && next.dataset ? next.dataset.dock : null);
        if (which && docks[which]) {
          const want = def[which];
          const d = docks[which];
          if (want) { d.size = want.size; d.collapsed = want.collapsed; }
          else if (d.groups.length) d.size = d.groups.length ? 240 : d.size;
          schedule();
          report('reset-sash');
        }
      });
    });
  }

  /** Focus a panel by id: click its tab if it has one, else its body. */
  function focusPanel(id) {
    const tab = app.querySelector(`.dock-tab[data-panel="${id}"]`);
    if (tab) { tab.click(); return true; }
    const body = paneBodies.get(id);
    if (body) { body.tabIndex = -1; body.focus(); return true; }
    return false;
  }

  function elementFor(id) { return paneBodies.get(id) || null; }

  function destroy() {
    if (frame !== null) cancelAnimationFrame(frame);
    for (const id of [...mounted]) {
      try { if (instances && typeof instances._unmount === 'function') instances._unmount(id); } catch { /* gone */ }
    }
    mounted.clear();
    paneBodies.clear();
    app.innerHTML = '';
  }

  paint();

  return {
    render: paint,
    setDoc: (next) => { current = next; paint(); },
    getDoc: () => current,
    focusPanel,
    elementFor,
    destroy,
    /** Every panel id the shell has a body for — what a test walks to check the inventory. */
    panels: () => [...paneBodies.keys()],
  };
}