/**
 * modal.js — the modal manager (M2.15, rewired).
 *
 * A modal is a root like any other: `layout.modals[]` declares it, its tree holds panes, and
 * those panes are built by their own modules. What is different is the visibility — a modal is
 * DECLARED at boot and shown only when something asks for it — and who does the asking. The
 * document says so itself:
 *
 *     { "id": "graph", "open": ["ui:graph"], "close": ["close:graph"], "tree": { "pane": "graph" } }
 *
 * so the declared names are what this subscribes to (on the workspace bus: a pane's button
 * emits `ui:graph` there — see js/panes/controls.js). `modal:<id>` still opens a modal, which
 * is the name the design bar and the palette use.
 *
 * Three things this module owns, and the reason it is not just a layer:
 *
 *   THE OVERLAY IS THE RENDERER'S TARGET. `js/layout/renderer.js` fills a modal by looking for
 *   `[data-root="modal"][data-modal="<id>"]`, so an overlay carries both attributes, exists
 *   from the first render (hidden) and is never rebuilt to be shown again. The overlays used to
 *   be created on OPEN, which meant the renderer had nothing to fill and put the modal's panes
 *   in the world instead.
 *
 *   `display` IS THE VISIBILITY. An overlay is `display:none` until it is opened. The module
 *   used to write "Modal content for <id>" into its own body as a placeholder.
 *
 *   THE KEYBOARD IS THE MODAL'S. Esc closes the top-most (unless `persistent`), Tab stays
 *   inside it, and focus returns to whatever had it before.
 */

const OVERLAY_CSS =
  'position:fixed; top:0; left:0; right:0; bottom:0; z-index:2000; display:none;'
  + 'align-items:center; justify-content:center; background:rgba(0,0,0,0.6); pointer-events:auto;';

/**
 * Create the modal manager.
 *
 * @param {object} opts
 * @param {object} opts.store - layout store
 * @param {object} opts.bus - workspace bus (createBus)
 * @returns {object} {open, close, toggle, isOpen, get, modal, list, destroy}
 */
export function createModals({ store, bus } = {}) {
  const modals = new Map();          // modalId -> {el, modalDef, prevFocus, keydown}
  const openStack = [];              // open modal ids, top-most last
  const offs = [];                   // bus unsubscribers
  let destroyed = false;

  const declaration = (id) => {
    const layout = store && store.get ? store.get() : null;
    return ((layout && layout.modals) || []).find((m) => m && m.id === id) || null;
  };

  /** Create the overlay for a declared modal and subscribe the events it declares. */
  function ensure(id) {
    let entry = modals.get(id);
    if (entry) return entry;
    const def = declaration(id);
    if (!def) return null;

    const el = document.createElement('div');
    el.className = 'qb-modal-' + id;
    el.dataset.modal = id;
    el.dataset.root = 'modal';
    el.style.cssText = OVERLAY_CSS;
    el.addEventListener('click', (e) => { if (e.target === el) close(id); });
    document.body.appendChild(el);

    entry = { el, modalDef: def, prevFocus: null, keydown: null };
    modals.set(id, entry);

    for (const name of (def.open || [])) offs.push(bus.on(name, () => open(id)));
    for (const name of (def.close || [])) offs.push(bus.on(name, () => close(id)));
    offs.push(bus.on('modal:' + id, () => open(id)));
    return entry;
  }

  const isOpen = (id) => openStack.includes(id);
  const isPersistent = (entry) => !!(entry && entry.modalDef && entry.modalDef.persistent === true);

  function focusablesIn(el) {
    const sel = 'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])';
    return [...el.querySelectorAll(sel)].filter((n) => n.offsetWidth > 0 && n.offsetHeight > 0 && !n.disabled);
  }

  function onKeydown(e) {
    if (e.key !== 'Escape' || !openStack.length) return;
    const id = openStack[openStack.length - 1];
    if (isPersistent(modals.get(id))) return;
    close(id);
  }

  function onTab(e) {
    if (e.key !== 'Tab' || !openStack.length) return;
    const entry = modals.get(openStack[openStack.length - 1]);
    if (!entry) return;
    const list = focusablesIn(entry.el);
    if (!list.length) return;
    const at = list.indexOf(document.activeElement);
    if (e.shiftKey) { if (at <= 0) { e.preventDefault(); list[list.length - 1].focus(); } }
    else if (at >= list.length - 1) { e.preventDefault(); list[0].focus(); }
  }

  function open(id) {
    if (destroyed || isOpen(id)) return;
    const entry = ensure(id);
    if (!entry) return;

    entry.prevFocus = document.activeElement;
    entry.el.style.display = 'flex';
    openStack.push(id);

    const first = focusablesIn(entry.el)[0];
    if (first) first.focus();

    entry.keydown = (e) => { if (entry.el.contains(e.target)) onTab(e); };
    document.addEventListener('keydown', entry.keydown);
    document.body.style.overflow = 'hidden';

    if (bus) bus.emit('modal.open', { id });
  }

  function close(id) {
    if (destroyed || !isOpen(id)) return;
    const entry = modals.get(id);
    if (!entry) return;

    entry.el.style.display = 'none';
    const at = openStack.indexOf(id);
    if (at >= 0) openStack.splice(at, 1);
    if (entry.keydown) document.removeEventListener('keydown', entry.keydown);
    entry.keydown = null;
    if (!openStack.length) document.body.style.overflow = '';
    if (entry.prevFocus && typeof entry.prevFocus.focus === 'function') entry.prevFocus.focus();

    if (bus) bus.emit('modal.close', { id });
  }

  const toggle = (id) => (isOpen(id) ? close(id) : open(id));

  document.addEventListener('keydown', onKeydown);
  document.addEventListener('ui:modal-close', (e) => {
    const id = e && e.detail && e.detail.id;
    if (id) close(id);
  });
  if (bus) offs.push(bus.on('modal.open:external', (d) => { if (d && d.id) open(d.id); }));

  /* The overlays exist for as long as the document declares their modals: created here, before
     the renderer's next pass, and removed with the declaration. A modal declared `open: true`
     opens when the layout says so. */
  function syncDeclared(layout) {
    const declared = ((layout && layout.modals) || []).filter((m) => m && m.id);
    const ids = new Set(declared.map((m) => m.id));
    for (const id of [...modals.keys()]) {
      if (ids.has(id)) continue;
      close(id);
      modals.get(id).el.remove();
      modals.delete(id);
    }
    for (const m of declared) {
      const entry = ensure(m.id);
      if (m.open === true && !isOpen(m.id)) open(m.id);
      if (entry) entry.modalDef = m;
    }
  }

  const unsubscribe = store && store.subscribe ? store.subscribe(syncDeclared) : () => {};
  if (store && store.get) syncDeclared(store.get());

  return {
    open,
    close,
    toggle,
    isOpen,
    /** The overlay element (what the renderer fills, and what older callers asked for). */
    get: (id) => (modals.get(id) ? modals.get(id).el : null),
    /** The declared modal, with the definition the document last gave. */
    modal: (id) => modals.get(id) || null,
    list: () => openStack.slice(),
    destroy() {
      destroyed = true;
      for (const id of [...openStack]) close(id);
      document.removeEventListener('keydown', onKeydown);
      for (const off of offs) { try { off(); } catch (e) {} }
      unsubscribe();
      for (const entry of modals.values()) entry.el.remove();
      modals.clear();
    },
  };
}
