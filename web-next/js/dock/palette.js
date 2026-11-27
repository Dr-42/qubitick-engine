/**
 * dock/palette.js — the command palette (Ctrl+K).
 *
 * One list, one filter, one Enter. The commands are the shell's own actions plus every panel
 * it can show, so "open the Build panel" and "switch to Minimal" are the same kind of act —
 * which is what makes the palette a real second way to drive the layout rather than a menu
 * of one feature.
 *
 * Keyboard: ↑/↓ move, Enter runs, Escape closes. The filter matches on any word of a label
 * and on the chord, so `ctrl+j` finds the dock toggle it names.
 */

/**
 * @param {object} opts
 * @param {Array<{id:string,label:string,hint?:string,run:Function}>} opts.commands
 * @param {HTMLElement} [opts.parent]
 * @returns {object} { open, close, toggle, isOpen }
 */
export function createPalette({ commands, parent }) {
  let backdrop = null;
  let input = null;
  let list = null;
  let filtered = [];
  let active = 0;

  function build() {
    backdrop = document.createElement('div');
    backdrop.className = 'dock-palette-backdrop';
    const box = document.createElement('div');
    box.className = 'dock-palette';
    input = document.createElement('input');
    input.type = 'text';
    input.placeholder = 'command or panel…';
    input.spellcheck = false;
    list = document.createElement('ul');
    box.appendChild(input);
    box.appendChild(list);
    backdrop.appendChild(box);
    (parent || document.body).appendChild(backdrop);

    backdrop.addEventListener('mousedown', (ev) => { if (ev.target === backdrop) close(); });
    input.addEventListener('input', () => { active = 0; paint(); });
    input.addEventListener('keydown', (ev) => {
      if (ev.key === 'Escape') { ev.preventDefault(); close(); }
      else if (ev.key === 'ArrowDown') { ev.preventDefault(); active = Math.min(filtered.length - 1, active + 1); paint(); }
      else if (ev.key === 'ArrowUp') { ev.preventDefault(); active = Math.max(0, active - 1); paint(); }
      else if (ev.key === 'Enter') {
        ev.preventDefault();
        const cmd = filtered[active];
        if (cmd) { close(); cmd.run(); }
      }
    });
  }

  function match(q) {
    const needle = q.trim().toLowerCase();
    if (!needle) return commands.slice();
    const words = needle.split(/\s+/);
    return commands.filter((c) => {
      const hay = (c.label + ' ' + (c.hint || '')).toLowerCase();
      return words.every((w) => hay.includes(w));
    });
  }

  function paint() {
    filtered = match(input.value);
    active = Math.min(active, Math.max(0, filtered.length - 1));
    list.innerHTML = '';
    filtered.forEach((c, i) => {
      const li = document.createElement('li');
      li.dataset.active = i === active ? '1' : '0';
      const name = document.createElement('b');
      name.textContent = c.label;
      li.appendChild(name);
      if (c.hint) {
        const hint = document.createElement('span');
        hint.textContent = c.hint;
        li.appendChild(hint);
      }
      li.addEventListener('mousedown', (ev) => { ev.preventDefault(); close(); c.run(); });
      li.addEventListener('mouseenter', () => { active = i; paint(); });
      list.appendChild(li);
    });
  }

  function open() {
    if (!backdrop) build();
    backdrop.style.display = 'flex';
    input.value = '';
    active = 0;
    paint();
    input.focus();
  }

  function close() {
    if (backdrop) backdrop.style.display = 'none';
  }

  function toggle() {
    if (backdrop && backdrop.style.display !== 'none') close();
    else open();
  }

  return { open, close, toggle, isOpen: () => !!(backdrop && backdrop.style.display !== 'none') };
}