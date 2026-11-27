/**
 * dock/keys.js — the shell's keyboard.
 *
 * The binding table is DATA, so the palette lists the same chords it honours and a test can
 * read it without dispatching events:
 *
 *   Ctrl+1..9          focus the Nth visible panel (the tab order across the docks)
 *   Ctrl+Alt+1/2/3     Minimal / Standard / Power
 *   Ctrl+Alt+0         Custom (what the user last arranged)
 *   Ctrl+J             toggle the bottom dock
 *   Ctrl+B             toggle the left dock
 *   Ctrl+\             toggle the right dock
 *   Ctrl+K             the command palette
 *   Ctrl+Enter         run the corpus check
 *
 * WHY A CHORD AND NOT A KEY. `Ctrl+B` and `Ctrl+J` are chords a code editor already uses
 * (bold, join line); the shell takes them on the DOCUMENT and lets a focused text field win,
 * because a key handler that steals a keystroke mid-word is worse than a missing shortcut.
 * `inTextField` below is that rule, and it is the first thing every binding checks.
 */

import { PRESET_ORDER, visiblePanels, DOCKS } from './model.js';

/** Where the shortcuts are declared: label, chords, action id. The palette reads this. */
export const BINDINGS = [
  { id: 'preset.minimal', label: 'Preset: Minimal', chord: 'Ctrl+Alt+1' },
  { id: 'preset.standard', label: 'Preset: Standard', chord: 'Ctrl+Alt+2' },
  { id: 'preset.power', label: 'Preset: Power', chord: 'Ctrl+Alt+3' },
  { id: 'preset.custom', label: 'Preset: Custom', chord: 'Ctrl+Alt+0' },
  { id: 'dock.bottom', label: 'Toggle bottom dock', chord: 'Ctrl+J' },
  { id: 'dock.left', label: 'Toggle left dock', chord: 'Ctrl+B' },
  { id: 'dock.right', label: 'Toggle right dock', chord: 'Ctrl+\\' },
  { id: 'palette', label: 'Command palette', chord: 'Ctrl+K' },
  { id: 'build', label: 'Run corpus check', chord: 'Ctrl+Enter' },
  { id: 'reset', label: 'Reset layout', chord: 'Ctrl+Alt+R' },
];

/** Whether the event came from a text field, where a chord belongs to the field. */
export function inTextField(target) {
  if (!target || typeof target.closest !== 'function') return false;
  return !!target.closest('input, textarea, select, [contenteditable="true"], .cm-editor');
}

/**
 * Install the shell's keys.
 *
 * @param {object} opts
 * @param {object} opts.view - the dock view (`focusPanel`)
 * @param {object} opts.doc - the live document (the handler mutates it through `apply`)
 * @param {function} opts.apply - `(mutator, source) => void`: run a change and report it
 * @param {function} opts.setPreset - `(key) => void`
 * @param {function} opts.toggleDock - `(dock) => void`
 * @param {function} [opts.palette] - open the command palette
 * @param {function} [opts.build] - run the corpus check
 * @param {function} [opts.reset] - reset the layout to the active preset
 * @returns {function} uninstall
 */
export function installDockKeys({ view, doc, apply, setPreset, toggleDock, palette, build, reset }) {
  const onKey = (ev) => {
    const mod = ev.ctrlKey || ev.metaKey;
    if (!mod) return;
    if (inTextField(ev.target) && ev.key !== 'Enter') return;

    // Ctrl+Alt+<n>: presets
    if (ev.altKey && !ev.shiftKey) {
      const map = { '1': 'minimal', '2': 'standard', '3': 'power', '0': 'custom' };
      if (map[ev.key]) {
        ev.preventDefault();
        setPreset(map[ev.key]);
        return;
      }
      if (ev.key === 'r' || ev.key === 'R') {
        ev.preventDefault();
        if (reset) reset();
        return;
      }
    }

    // Ctrl+1..9: focus the Nth panel
    if (!ev.altKey && !ev.shiftKey && /^[1-9]$/.test(ev.key)) {
      const panels = visiblePanels(doc);
      const id = panels[Number(ev.key) - 1];
      if (id) { ev.preventDefault(); view.focusPanel(id); }
      return;
    }

    // dock toggles
    if (ev.key === 'j' || ev.key === 'J') { ev.preventDefault(); toggleDock('bottom'); return; }
    if (ev.key === 'b' || ev.key === 'B') { ev.preventDefault(); toggleDock('left'); return; }
    if (ev.key === '\\') { ev.preventDefault(); toggleDock('right'); return; }
    if (ev.key === 'k' || ev.key === 'K') { ev.preventDefault(); if (palette) palette(); return; }
    if (ev.key === 'Enter') { ev.preventDefault(); if (build) build(); return; }
  };

  document.addEventListener('keydown', onKey);
  return () => document.removeEventListener('keydown', onKey);
}

/** The preset a chord names, for the palette's own listing. */
export function presetKeys() {
  return PRESET_ORDER;
}

export { DOCKS };