/**
 * panes/projects.js — Project pane: which corpus the engine is serving.
 *
 * The engine serves ONE project at a time, and a project is any directory whose charter
 * (`Canon.qbit`) names it. This pane lists what was found, says which one is current, and
 * offers the three things that change that: switch to another, wire in a path the scan did
 * not find, or FORGET a path that was wired in (a discovered directory is found again, so only
 * the wired ones are offered — the server says which they are).
 *
 * In:  {}   Out: {}   Call: `rescan`
 *
 * Switching a project re-ingests the corpus, so it is not an undoable UI change and the
 * pane does not pretend otherwise: the current project's button says "current" and is
 * inert, and every other row says what it will do. The app owns the list, the scan and the
 * switch (ctx.projects), so nothing here holds state that could disagree with the server.
 */

import { callPort } from '../core/ports.js';
import { ctlRow, ctlNote, ctlBtn, ctlGroup } from '../app/ctl.js';
import { nameHtml, whenKatex } from '../app/math.js';

export default {
  kind: 'projects',
  title: 'Project',
  ports: { in: {}, out: {}, call: { rescan: 'call' } },
  create(ctx) {
    const esc = (ctx.ui && ctx.ui.esc) || ((s) => String(s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])));
    let el = null, list = null, now = null, summary = null;

    const P = () => (ctx.projects ? ctx.projects() : { list: [], active: {} });

    function paint(p) {
      const { list: rows, active } = P();
      if (now) {
        now.innerHTML = '<span>project</span><b>' + nameHtml(active.name || '—') + '</b>'
          + '<span class="th-now-note">' + esc(active.id || '') + '</span>';
      }
      if (list) {
        list.textContent = '';
        for (const pr of rows) {
          const row = document.createElement('div');
          row.className = 'pj-row' + (pr.active ? ' is-active' : '');
          const name = document.createElement('b');
          /* a row's name too: this list is where the corpus's own names are read, and the
             charter's is TeX (see nameHtml) */
          name.innerHTML = nameHtml(pr.name || pr.id);
          const path = document.createElement('span');
          path.className = 'pj-path';
          path.textContent = pr.id;
          row.append(name, path);
          const key = String(pr.id).replace(/[^a-z0-9]+/gi, '-');
          const go = ctlBtn('b-pj-' + key,
            pr.active ? 'current' : 'switch',
            pr.active ? 'this is the project being served' : 'serve this project (the corpus is re-ingested)');
          if (!pr.active && ctx.useProject) go.addEventListener('click', () => ctx.useProject(pr.id));
          row.appendChild(go);
          /* A WIRED-IN path can be taken back. A DISCOVERED one cannot: the server finds it again
             on the next read, so offering to forget it would offer something the server cannot do
             — which is why the LIST says which rows are `wired` (`project_rows` in
             crates/api/src/handlers.rs) instead of the pane deciding from the path's shape. The
             served project is not offered either: the server refuses that one, and a button that
             is always refused is a button that should not be drawn. */
          if (pr.wired && !pr.active && ctx.forgetProject) {
            const forget = ctlBtn('b-pj-forget-' + key, 'forget',
              'stop offering this path as a project (nothing is deleted from disk)');
            forget.addEventListener('click', async () => {
              /* REPAINT WHATEVER THE ANSWER SAID, and read the list from the app when it does: the
                 number this pane draws is the SERVER's, and an answer that was lost on the way back
                 (a refused path, a dropped response) must not leave the row standing next to a
                 server that already forgot it. `ctx.forgetProject` refreshes the app's list on
                 success, so a repaint is right in both cases — and the toast, not this pane, is
                 where a refusal is said. */
              await ctx.forgetProject(pr.id);
              paint(p);
            });
            row.appendChild(forget);
          }
          list.appendChild(row);
        }
        if (!rows.length) {
          const empty = document.createElement('div');
          empty.className = 'ctl-note';
          empty.textContent = 'no projects found — wire one in below, or add Canon.qbit to a corpus';
          list.appendChild(empty);
        }
      }
      if (p && summary) summary(p);
    }

    return {
      mount(host, p) {
        el = document.createElement('div');
        el.className = 'ctl-host';
        const wrap = document.createElement('div');
        wrap.className = 'ctl-form';
        now = document.createElement('div');
        now.className = 'th-now';
        list = document.createElement('div');
        list.className = 'pj-list';

        const pathIn = document.createElement('input');
        pathIn.type = 'text';
        pathIn.className = 'te-in';
        pathIn.placeholder = '/path/to/corpus  (a directory with Canon.qbit)';
        const add = ctlBtn('b-pj-add', 'wire in', 'remember this path as a project');
        add.addEventListener('click', async () => {
          const path = pathIn.value.trim();
          if (!path) { if (ctx.ui && ctx.ui.toast) ctx.ui.toast('give a path'); return; }
          if (ctx.addProject && await ctx.addProject(path)) { pathIn.value = ''; paint(p); }
        });
        const rescan = ctlBtn('b-pj-rescan', 'rescan', 'look again at the projects directory');
        rescan.addEventListener('click', async () => {
          if (ctx.loadProjects) await ctx.loadProjects();
          paint(p);
        });

        wrap.append(
          now,
          ctlNote('a project is any directory whose charter (Canon.qbit) names it'),
          list,
          ctlRow('add', ctlGroup(pathIn, add)),
          ctlRow('', ctlGroup(rescan)),
        );
        el.appendChild(wrap);
        host.appendChild(el);
        if (p) summary = () => { p.summary = () => 'project · ' + (P().active.name || '—'); if (p._summary && p.card && p.card.lastChild) p.card.lastChild.textContent = p.summary(); };
        paint(p);
        /* the names in this list are the corpus's own — TeX, in the charter's case — and the
           typesetter arrives on first use, so the first frame comes from source and this repaints
           it once the library is here (js/app/math.js::whenKatex) */
        whenKatex(() => paint(p));
      },
      unmount() {
        if (el && el.parentNode) el.parentNode.removeChild(el);
        el = list = now = summary = null;
      },
      refresh: () => paint(),
      in: {}, out: {},
      call: { rescan: callPort(() => { if (ctx.loadProjects) ctx.loadProjects(); return { ok: true }; }) },
      get ports() { return { in: this.in, out: this.out, call: this.call }; },
    };
  },
};