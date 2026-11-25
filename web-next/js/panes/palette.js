/**
 * panes/palette.js — Theme board.
 *
 * Every theme the workspace can wear, as a card of its own field at card scale: picking
 * is seeing. Dark themes and light ones are grouped, the themes you made sit in their own
 * group, and the palette underneath is still editable — a theme is data, and data with a
 * colour picker is data you can fix without editing a file.
 *
 * In:  {}   Out: `theme` (value theme module)   Call: `edit`
 *
 * The board owns no theme state. It asks `ctx.themes` what exists and what is on, and
 * every pick is a call back into an app that already knows how to apply and save a theme
 * (the same call the View tile's "themes…" button makes). That is deliberate: a board
 * that cached its own copy of the active theme would disagree with the canvas the moment
 * a workspace restored a different one.
 *
 * Editing a theme is a different job — it needs the module's full field, a draft, and a
 * save path — so the board offers `edit` and the app opens its editor in this same tile.
 */

import { valuePort, callPort } from '../core/ports.js';

export default {
  kind: 'palette',
  title: 'Theme',
  ports: { in: {}, out: { theme: 'value' }, call: { edit: 'call' } },
  create(ctx) {
    const esc = (ctx.ui && ctx.ui.esc) || ((s) => String(s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])));

    let host = null, wrap = null, now = null, board = null, grid = null;
    const outTheme = valuePort(null);

    const themes = () => (ctx.themes ? ctx.themes : null);

    /** One card: the theme's own field at card scale, so picking is seeing. */
    function card(mod, onPick) {
      const b = document.createElement('button');
      b.className = 'th-card';
      b.type = 'button';
      b.dataset.id = mod.id;
      b.dataset.mode = mod.mode || 'dark';
      b.setAttribute('aria-pressed', String(mod.id === (themes().currentId())));
      b.title = (mod.note ? mod.note + '\n' : '') + (mod.name || mod.id);
      const v = mod.vars || {};
      b.style.setProperty('--t-bg', v.bg || '#000');
      b.style.setProperty('--t-ink', v.ink || '#fff');
      b.style.setProperty('--t-line', v.line || '#333');
      b.style.setProperty('--t-accent', v.cyan || v.amber || v.ink || '#888');
      const thumb = document.createElement('span');
      thumb.className = 'th-thumb';
      const meta = document.createElement('span');
      meta.className = 'th-meta';
      const nm = document.createElement('b');
      nm.textContent = mod.name || mod.id;
      const md = document.createElement('em');
      md.textContent = mod.mode || 'dark';
      meta.append(nm, md);
      const dots = document.createElement('span');
      dots.className = 'th-dots';
      for (const k of ['cyan', 'violet', 'green', 'amber', 'ink']) {
        const i = document.createElement('i');
        i.style.setProperty('--c', v[k] || '#888');
        dots.appendChild(i);
      }
      b.append(thumb, meta, dots);
      b.addEventListener('click', () => onPick(mod));
      return b;
    }

    function paint() {
      const t = themes();
      if (!wrap || !t) return;
      const cur = t.current();
      const space = t.fieldSpace();
      now.innerHTML = '<span>theme</span><b>' + esc(t.currentName()) + '</b>'
        + '<span class="th-now-note">' + esc((cur && cur.note) || ('field · ' + space)) + '</span>';
      const edit = document.createElement('button');
      edit.id = 'b-theme-edit';
      edit.className = 'brun';
      edit.textContent = 'edit…';
      edit.title = 'open the theme editor, wired into this tile';
      edit.addEventListener('click', () => { if (ctx.themes.openEditor) ctx.themes.openEditor(); });
      now.appendChild(edit);

      board.textContent = '';
      const pick = (mod) => {
        Promise.resolve(t.pick(mod.id)).then(() => {
          outTheme.set(mod);
          if (ctx.ui && ctx.ui.toast) ctx.ui.toast('theme: ' + (mod.name || mod.id));
          paint();
        });
      };
      for (const mode of ['dark', 'light']) {
        const list = t.cards().filter((m) => (m.mode || 'dark') === mode && m.source !== 'custom');
        if (!list.length) continue;
        const head = document.createElement('div');
        head.className = 'th-mode-head';
        head.innerHTML = '<i>' + (mode === 'dark' ? '☾' : '☀') + '</i>' + mode + ' · ' + list.length;
        board.appendChild(head);
        for (const m of list) board.appendChild(card(m, pick));
      }
      const mine = t.cards().filter((m) => m.source === 'custom');
      if (mine.length) {
        const head = document.createElement('div');
        head.className = 'th-mode-head';
        head.innerHTML = '<i>◆</i>yours · ' + mine.length;
        board.appendChild(head);
        for (const m of mine) {
          const c = card(m, pick);
          const kill = document.createElement('button');
          kill.className = 'th-kill';
          kill.textContent = '✕';
          kill.title = 'forget this theme (the workspace keeps the rest)';
          kill.addEventListener('click', (e) => {
            e.stopPropagation();
            t.forget(m.id);
            if (ctx.ui && ctx.ui.toast) ctx.ui.toast('theme forgotten: ' + (m.name || m.id));
            paint();
          });
          c.appendChild(kill);
          board.appendChild(c);
        }
      }

      // the palette itself: still editable, still CSS out
      grid.textContent = '';
      for (const k of t.keys()) {
        const lab = document.createElement('label');
        lab.className = 'th-sw';
        const sw = document.createElement('input');
        sw.type = 'color';
        sw.className = 'th-c';
        sw.value = (t.vars() || {})[k] || '#000000';
        sw.addEventListener('input', () => {
          const next = Object.assign({}, t.vars(), { [k]: sw.value });
          t.applyVars(next);
        });
        const name = document.createElement('span');
        name.textContent = k;
        lab.append(sw, name);
        grid.appendChild(lab);
      }
    }

    const callEdit = callPort(() => {
      if (!ctx.themes || !ctx.themes.openEditor) return { ok: false, error: 'no editor' };
      ctx.themes.openEditor();
      return { ok: true };
    });

    return {
      mount(h) {
        host = h;
        wrap = document.createElement('div');
        wrap.className = 'bnode theme';
        now = document.createElement('div');
        now.className = 'th-now';
        board = document.createElement('div');
        board.className = 'th-board';
        grid = document.createElement('div');
        grid.className = 'th-grid';
        wrap.append(now, board, grid);
        host.appendChild(wrap);
        paint();
      },
      unmount() {
        if (wrap && wrap.parentNode) wrap.parentNode.removeChild(wrap);
        host = wrap = now = board = grid = null;
      },
      in: {}, out: { theme: outTheme },
      call: { edit: callEdit },
      get ports() { return { in: this.in, out: this.out, call: this.call }; },
    };
  },
};