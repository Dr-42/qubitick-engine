/*
 * ctl.js — the small widgets the Controls ribbon and the settings panes are built from.
 *
 * Extracted from the inline script in web-next/index.html. These are the markup
 * primitives only: they build an element and hand it back. What goes IN a section,
 * which button is enabled, and what happens on click all stay in the page, so this
 * file has no state and no callbacks.
 *
 * The names are the contract — `ctlRow('mode', select)` is one labelled line of a
 * settings pane — and the call sites in the page did not change when these moved.
 */
export function ctlRow(label, node, cls) {
  const row = document.createElement('div'); row.className = 'row' + (cls ? ' ' + cls : '');
  const k = document.createElement('span'); k.className = 'k'; k.textContent = label;
  row.append(k, node); return row;
}

export function ctlSec(title, rows) {
  const sec = document.createElement('div'); sec.className = 'hud-sec';
  const lab = document.createElement('span'); lab.className = 'hud-lab'; lab.textContent = title;
  sec.append(lab, ...rows); return sec;
}

export function ctlNote(text) {
  const d = document.createElement('div'); d.className = 'ctl-note'; d.textContent = text; return d;
}

export function ctlBtn(id, text, title) {
  const b = document.createElement('button'); if (id) b.id = id; b.textContent = text; if (title) b.title = title; return b;
}

export function ctlPill(id, text) {
  const s = document.createElement('span'); s.className = 'pill'; if (id) s.id = id; if (text != null) s.textContent = text; return s;
}

export function ctlCheck(id, label, checked) {
  const l = document.createElement('label'); const i = document.createElement('input');
  i.type = 'checkbox'; if (id) i.id = id; i.checked = !!checked;
  l.append(i, document.createTextNode(' ' + label));
  /* THE BOX IS THE INPUT. A caller puts the LABEL in its row, so the label is what it holds — and
     every caller wrote `box.checked = v` (js/panes/view.js five times, js/panes/prefs.js), which
     set a property on the label and moved nothing on screen: a control that never followed its
     value, only the value it was built with. One accessor here instead of `.input.checked` at ten
     call sites, and `box.checked` reads back what is on screen. */
  Object.defineProperty(l, 'checked', { get: () => i.checked, set: (v) => { i.checked = !!v; }, configurable: true });
  return l;
}

export function ctlGroup(...nodes) {
  const s = document.createElement('span'); s.className = 'ctl-group';
  s.style.cssText = 'display:flex;gap:6px;align-items:center'; s.append(...nodes); return s;
}
