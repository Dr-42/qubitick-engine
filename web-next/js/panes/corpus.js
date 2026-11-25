/**
 * panes/corpus.js — the corpus of whatever project is wired in.
 *
 * THIS IS THE SWAP. A corpus tile is handed a project on its `project` in-port and answers with
 * that project's corpus on four out-ports: `files`, `graph`, `stats`, `symbols`. Wire a project
 * tile in and the corpus arrives; wire a DIFFERENT project tile in and the whole corpus changes
 * over; take the wire out and there is no project, so it publishes nothing and the interface
 * empties. Nothing else in the app has to know a switch happened.
 *
 * In:  `project` ({ id, name })   Out: `files`, `graph`, `stats`, `symbols`   Call: `reload`
 *
 * Serving: the engine serves ONE project at a time (crates/api/src/handlers.rs::post_project), so
 * a corpus asked for a project that is not the active one first makes the engine serve it — and
 * only then, so a boot on the project that is already being served costs no switch. The previous
 * project stays active if the switch fails, and this tile then publishes nothing rather than
 * somebody else's corpus.
 *
 * The app is what reads these ports into the interface (`ctx.data`, via js/host/canvas/data.js):
 * a pane's job is to say what the corpus is, and the app's is to show it.
 */

import { valuePort, callPort } from '../core/ports.js';
import { ctlRow, ctlNote, ctlPill } from '../app/ctl.js';
import { nameHtml, whenKatex } from '../app/math.js';

/** What an unwired corpus publishes: nothing at all, which is what "no project" looks like. */
const EMPTY = { files: null, graph: null, stats: null, symbols: null };

/** A row's value: the count, or an en dash for a corpus that has not answered yet. */
const num = (v) => (v === null || v === undefined ? '–' : String(v));

export default {
  kind: 'corpus',
  title: 'Corpus',
  ports: {
    in: { project: 'value' },
    out: { files: 'value', graph: 'value', stats: 'value', symbols: 'value' },
    call: { reload: 'call' },
  },
  create(ctx) {
    const esc = (ctx.ui && ctx.ui.esc) || ((s) => String(s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])));
    const params = (ctx.params && ctx.params.project) ? String(ctx.params.project) : null;

    /* THE PORTS START WITH NO VALUE (`valuePort()`, not `valuePort(null)`), and that is not a
       detail: a port that starts at null HAS a value, so it REPLAYS null to every new subscriber
       — and a remounted tile would tell the app "the corpus is empty" before it had asked the
       server for one. No value means "nothing said yet", which is the truth. "Unwired" is said
       explicitly instead, by publishing EMPTY. */
    const inProject = valuePort();
    const outFiles = valuePort();
    const outGraph = valuePort();
    const outStats = valuePort();
    const outSymbols = valuePort();

    let el = null;
    let now = null;
    let rows = null;
    let summary = null;
    let serving = null;          // the project the ENGINE is serving, as far as we know
    let seq = 0;                 // only the newest load may publish
    let current = null;          // the project we are showing
    let last_wanted = null;      // the project id last asked for, so a repeat delivery is not a reload
    let last = { ...EMPTY };

    /** The route table is the app's (`ctx.url`), never a literal path — and it arrives
     *  asynchronously, so asking for one has to wait for it (js/net/routes.js). A corpus tile
     *  fetches the moment it is handed a project, which on boot is before the table lands. */
    const url = (name) => (ctx.url ? ctx.url(name) : null);
    const routesReady = () => (ctx.routesReady ? Promise.resolve(ctx.routesReady()).catch(() => {}) : Promise.resolve());
    const getJSON = async (name) => {
      await routesReady();
      const u = url(name);
      if (!u) return null;
      try { const r = await fetch(u); return r.ok ? await r.json() : null; } catch (e) { return null; }
    };
    const postJSON = async (name, body) => {
      await routesReady();
      const u = url(name);
      if (!u) return null;
      try {
        const r = await fetch(u, {
          method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body),
        });
        return r.ok ? await r.json() : null;
      } catch (e) { return null; }
    };

    function publish(next) {
      last = { ...EMPTY, ...next };
      outFiles.set(last.files);
      outGraph.set(last.graph);
      outStats.set(last.stats);
      outSymbols.set(last.symbols);
      paint();
      if (summary) summary();
    }

    function paint() {
      if (!el) return;
      const p = current;
      if (now) {
        /* the project's name, which the charter writes as TeX (see nameHtml); the id below it is
           a directory and stays escaped text */
        now.innerHTML = '<span>corpus</span><b>' + nameHtml(p ? (p.name || p.id) : '— no project —') + '</b>'
          + '<span class="th-now-note">' + esc(p ? p.id : '') + '</span>';
      }
      if (rows) {
        const files = Array.isArray(last.files) ? last.files.length : null;
        const nodes = last.stats && last.stats.nodes !== undefined ? last.stats.nodes
          : (last.graph && Array.isArray(last.graph.nodes) ? last.graph.nodes.length : null);
        const symbols = Array.isArray(last.symbols) ? last.symbols.length
          : (last.stats && last.stats.symbols !== undefined ? last.stats.symbols : null);
        /* The three rows are the app's own control rows (js/app/ctl.js), which is also what this
           tile's `ctl-host` class is for: `.ctl-host .row` is the two-column grid and `.k` the
           muted label. Hand-rolled markup here once read `files330` — label and value with no gap
           — because it borrowed a class (`ctl-row`) that only ever styled a checkbox, and wrapped
           it in `pj-list`, which belongs to the projects picker. Build them; the look follows. */
        rows.replaceChildren(
          ctlNote(p
            ? 'serving ' + p.id + (serving && serving !== p.id ? ' · switching…' : '')
            : 'wire a project into this tile: nothing is served without one'),
          ctlRow('files', ctlPill(null, num(files))),
          ctlRow('nodes', ctlPill(null, num(nodes))),
          ctlRow('symbols', ctlPill(null, num(symbols))));
      }
    }

    /** Make the engine serve this project, once: a boot on the active project costs no switch. */
    async function serve(project) {
      if (!project || !project.id) return false;
      if (serving === null) {
        const j = await getJSON('projects.list');
        serving = (j && j.active && j.active.id) || '';
      }
      if (serving === project.id) return true;
      const r = await postJSON('project.use', { id: project.id });
      if (r && r.ok) {
        serving = project.id;
        /* TELL THE APP WHICH PROJECT IT IS NOW SERVING. A switch made from here is the same
           switch the Project pane offers, and the app keys everything per project: which
           workspace is saved, what a reload restores (js/host/canvas/net-app.js::projectKey).
           Skipping this leaves the arrangement saved under the OLD project's key while the
           engine serves the new one — the swap would work until the next boot, and then the
           workspace would be somebody else's. Not a reload: the running interface is the point. */
        if (ctx.loadProjects) await Promise.resolve(ctx.loadProjects()).catch(() => {});
        return true;
      }
      serving = null;
      return false;
    }

    /** Load one project's corpus. Only the newest call publishes. */
    async function load(project) {
      const mine = ++seq;
      current = project || null;
      if (!project || !project.id) { publish(EMPTY); return; }
      paint();                                   // "switching…" while the engine gets there
      const ok = await serve(project);
      if (mine !== seq) return;
      if (!ok) { publish(EMPTY); return; }

      const [files, graph, stats, symbols] = await Promise.all([
        getJSON('files.list'), getJSON('graph.get'), getJSON('stats.get'), getJSON('symbols.get'),
      ]);
      if (mine !== seq) return;                  // a newer project took over while we were fetching
      publish({
        files: files ? (files.files || []) : null,
        graph: graph ? (graph.graph && graph.graph.nodes ? graph.graph : { nodes: graph.nodes || [], edges: graph.edges || [] }) : null,
        stats: stats ? (stats.stats || stats) : null,
        symbols: symbols ? (symbols.symbols || symbols) : null,
      });
    }

    const callReload = callPort(() => load(current));

    /* WHAT ARRIVES ON THE WIRE IS WHAT THIS TILE SERVES: the in-port is the whole interface of
       the swap, so the pane listens to it rather than to whoever fills it. Both hosts deliver
       into it — the canvas hands it a wire's value (js/live/wire/host.js), the engine writes its
       node ports in by name (js/app/pane-ports.js) — and neither has to know that a switch is a
       reload.

       AND THE THREE CASES ARE NOT THE SAME (this is the difference between a workspace that
       works and one that flickers):

         `undefined`  the wire is GONE — the tile is unwired and serves nothing;
         `null`       the wire is there and its source has nothing YET — a project tile that has
                      not answered the server, or one that has just been remounted. Waiting is
                      the only correct answer: unloading here makes the corpus blink on every
                      HUD tick (js/world/chrome.js re-applies the inputs) and refetch itself;
         a project    serve it — and the same id twice is the same corpus, not a reload. */
    inProject.subscribe((v) => {
      if (v === undefined) { last_wanted = null; load(null); return; }
      if (!v || !v.id) return;
      const id = String(v.id);
      if (last_wanted === id) return;
      last_wanted = id;
      load({ id, name: v.name || id });
    });

    return {
      mount(host, p) {
        el = document.createElement('div');
        el.className = 'ctl-host';
        now = document.createElement('div');
        now.className = 'th-now';
        rows = document.createElement('div');
        el.append(now, rows);
        host.appendChild(el);
        if (p) summary = () => {
          p.summary = () => 'corpus · ' + ((current && (current.name || current.id)) || 'unwired');
          if (p._summary && p.card && p.card.lastChild) p.card.lastChild.textContent = p.summary();
        };
        paint();
        whenKatex(paint);          // the name may be TeX: repaint when the typesetter lands
        // a tile that arrives with its project already in params (a restored workspace)
        if (params) load({ id: params, name: params });
      },
      unmount() {
        if (el && el.parentNode) el.parentNode.removeChild(el);
        el = now = rows = summary = null;
        seq++;                                    // nothing in flight may publish into a dead tile
      },
      refresh: () => { paint(); return load(current); },
      in: { project: inProject },
      out: { files: outFiles, graph: outGraph, stats: outStats, symbols: outSymbols },
      call: { reload: callReload },
      get ports() { return { in: this.in, out: this.out, call: this.call }; },
    };
  },
};