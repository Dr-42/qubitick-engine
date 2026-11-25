/**
 * panes/project.js — one corpus project, as a node.
 *
 * A PROJECT IS A PATH ON THE SERVER, and nothing else. The corpus is the user's: a directory
 * carrying a charter, `Canon.qbit`, which names the project and says which `HL.lean` is that
 * project's own ground (crates/build/src/canon.rs). qubitick ships `core` and the `.qbit`
 * transpiler; the project ships its own `HL.lean`. So the one thing this tile has to know is
 * WHERE that directory is — and the server agrees, because a project's id IS its path
 * (crates/api/src/state.rs: `Project { id: path.display(), .. }`). That is what makes a wire
 * carrying `{ id, name }` a wire a corpus tile can be told to serve.
 *
 * The tile's subject is therefore a PATH: typed in it, or restored with the workspace. It is
 * wired in first — `POST /api/projects` is "remember this path as a project", and the engine
 * serves what it knows — and WHAT COMES BACK is what gets published. The client never derives
 * the id from the path itself: the server expands what it stores (`~/`), and a path that is not
 * a project is refused in the server's own words. A refusal publishes NOTHING, because a wire is
 * not a place to put a guess.
 *
 * With no path the tile stands for the project the server is SERVING, and it FOLLOWS it: the engine
 * can be switched by the projects pane, by another project tile, or by this tile's own `use` call, and
 * a tile that read the active project once would be a label that stops being true the moment somebody
 * else switches. `ctx.subscribeProjects` is that subscription (js/app/project-services.js), and every
 * tile with no path re-publishes what the engine reports — so a wire out of it carries the CURRENT
 * project rather than the one that was active when the tile was built.
 *
 * In:  {}   Out: `project` ({ id, name })   Call: `use`
 *
 * `use` asks the app to serve THIS project (the same switch the Project console pane offers).
 * The interesting use is the other one: wire this tile into a corpus tile and the whole
 * interface becomes this project (js/panes/corpus.js), then wire a DIFFERENT project tile in
 * instead and the interface is that one. Take the wire out and the interface is nothing.
 */

import { valuePort, callPort } from '../core/ports.js';
import { ctlRow, ctlNote, ctlBtn, ctlGroup } from '../app/ctl.js';
import { nameHtml, whenKatex } from '../app/math.js';

export default {
  kind: 'project',
  title: 'Project',
  ports: { in: {}, out: { project: 'value' }, call: { use: 'call' } },
  create(ctx) {
    const esc = (ctx.ui && ctx.ui.esc) || ((s) => String(s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])));

    /* WHERE THE PATH COMES FROM. The canvas keeps a tile's own state on the tile (`ctx.tile`) and
       writes it into the workspace document, so a project tile comes back naming the same corpus
       it was saved with (js/world/restore.js). The engine host has no tile: there a node's params
       come from the layout document (js/panes/instances.js), and `params.project` is the older
       spelling of the same thing — one id, which was always this path. */
    const savedPath = () => ((ctx.tile && ctx.tile.path) || (ctx.params && (ctx.params.path || ctx.params.project)) || null);
    const given = () => { const p = savedPath(); return p ? String(p) : null; };

    const outProject = valuePort(null);
    let el = null;
    let now = null;
    let rows = null;
    let note = null;
    let input = null;
    let summary = null;
    let offSource = null;
    let offFollow = null;
    let path = null;             // the path this tile names (null: whatever the server serves)
    let project = null;          // { id, name } — what that path IS, as the server says
    let refused = null;          // the server's own words for a path it would not wire in

    /** The route table is the app's (`ctx.url`), never a literal path — and it arrives
     *  asynchronously, so a mount that has to fetch waits for it (js/net/routes.js). */
    const url = (name) => (ctx.url ? ctx.url(name) : null);
    const routesReady = () => (ctx.routesReady ? Promise.resolve(ctx.routesReady()).catch(() => {}) : Promise.resolve());
    const getJSON = async (name) => {
      await routesReady();
      const u = url(name);
      if (!u) return null;
      try { const r = await fetch(u); return r.ok ? await r.json() : null; } catch (e) { return null; }
    };
    /** A POST that keeps the server's own refusal: a 400 carries `{ ok:false, error }`, and that
     *  sentence is the only honest thing to show for a path that is not a project. */
    const postJSON = async (name, body) => {
      await routesReady();
      const u = url(name);
      if (!u) return { ok: false, error: 'no route' };
      try {
        const r = await fetch(u, {
          method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body),
        });
        const j = await r.json().catch(() => null);
        return j || { ok: false, error: 'HTTP ' + r.status };
      } catch (e) { return { ok: false, error: e.message }; }
    };

    const listOf = async () => {
      const j = await getJSON('projects.list');
      return (j && j.projects) || [];
    };

    function paint() {
      if (!el) return;
      const label = project ? (project.name || project.id) : (path ? path : '—');
      const idnote = project ? project.id : (path || '');
      /* THE NAME IS NOT ALWAYS PROSE: the charter writes it as TeX, and escaping it printed
         `\text{The} \mathfrak{Q}` in the readout of every tile that named the project. The
         id/path note stays escaped text — it is a directory, not a formula (and a UI test reads
         the tile through it). */
      if (now) now.innerHTML = '<span>project</span><b>' + nameHtml(label) + '</b>'
        + '<span class="th-now-note">' + esc(idnote) + '</span>';
      if (note) {
        note.textContent = refused ? refused
          : project ? 'on the wire: everything wired to it follows this project'
            : path ? 'wiring in ' + path + ' ...'
              : 'no project (the engine reported none)';
      }
      if (input && document.activeElement !== input) input.value = path || '';
    }

    function publish() {
      /* THE PORT CARRIES THE PROJECT, not the id: a corpus tile needs the name for its own
         readout, and a wire that carries an object is a wire you can read. A path the server
         refused publishes null — "nothing to serve", which the corpus tile waits on rather than
         loading somebody else's corpus. */
      outProject.set(project ? { id: project.id, name: project.name || project.id } : null);
      paint();
      if (summary) summary();
    }

    async function learn() {
      path = given();
      refused = null;
      if (!path) {
        // no path: the project the server is serving. That is a first visit, not a mistake.
        const j = await getJSON('projects.list');
        const active = (j && j.active) || {};
        project = active.id ? { id: active.id, name: active.name || active.id } : null;
        publish();
        return;
      }
      const list = await listOf();
      let known = list.find((p) => p && p.id === path) || null;
      if (!known) {
        /* WIRE THE PATH IN FIRST, AND LET THE SERVER SPEAK. A path outside the engine's projects
           root is not a project until it has been registered, and the answer says which project
           the path turned out to be — an expanded `~/`, a canonical directory — so the id that
           goes on the wire is the server's, never a string we assembled. */
        paint();                                   // "wiring in ..." while the server answers
        const r = await postJSON('projects.add', { path });
        if (!r || !r.ok) {
          project = null;
          refused = (r && r.error) || 'the server did not answer';
          publish();
          return;
        }
        if (r.project && r.project.id) { project = { id: r.project.id, name: r.project.name || r.project.id }; publish(); return; }
        // an older server answered with the list only: find it there, or take the path itself
        const after = await listOf();
        known = after.find((p) => p && p.id === path) || null;
      }
      project = known ? { id: known.id, name: known.name || known.id } : { id: path, name: path };
      publish();
    }

    /** Follow the engine's active project — on a NOTIFICATION, so never a surprise write.
     *
     *  `learn()` is the full reading: it may wire a path in (`POST /api/projects`) and it is what a
     *  mount, a rescan and the tile's own field do. A notification must not do that: a tile whose
     *  path the server refused would re-send the refusal on every unrelated change, and a tile whose
     *  path is fine would still re-POST it. So this reads WITHOUT writing — the active project for a
     *  tile with no path, and its own entry's name for a tile that names one. */
    async function follow() {
      if (!path) {
        const j = await getJSON('projects.list');
        const active = (j && j.active) || {};
        const next = active.id ? { id: active.id, name: active.name || active.id } : null;
        if ((next && project && next.id === project.id && next.name === project.name) || (!next && !project)) {
          return;                               // unchanged: a publish here would be a re-render for nothing
        }
        project = next;
        publish();
        return;
      }
      if (refused) return;                      // a refused path stays refused until someone edits it
      const list = await listOf();
      const known = list.find((x) => x && x.id === project?.id) || list.find((x) => x && x.id === path);
      if (!known) return;
      const next = { id: known.id, name: known.name || known.id };
      if (project && next.id === project.id && next.name === project.name) return;
      project = next;
      publish();
    }

    /** Name a different path — the tile's own gesture, and the workspace remembers it. */
    async function setName(next) {
      if (!ctx.tile) { paint(); return { ok: false, error: 'the document names this node\'s path' }; }
      ctx.tile.path = next || null;
      if (ctx.saveWorkspace) ctx.saveWorkspace();
      await learn();
      if (ctx.ui && ctx.ui.toast) {
        ctx.ui.toast(refused ? 'project: ' + refused : 'project: ' + ((project && (project.name || project.id)) || '—'));
      }
      return { ok: !refused };
    }

    const callUse = callPort(async () => {
      if (!project) return { ok: false, error: 'no project' };
      const r = await postJSON('project.use', { id: project.id });
      if (r && r.ok && ctx.ui && ctx.ui.toast) ctx.ui.toast('project: ' + ((r.active && r.active.name) || project.id));
      else if (ctx.ui && ctx.ui.toast) ctx.ui.toast('project: ' + ((r && r.error) || 'could not switch'));
      /* THIS TILE'S OWN SWITCH IS NOT HEARD FROM THE OTHER SIDE: the host's `useProject` reloads, and
         this call posts to the route directly, so the tile that asked has to say what happened. A
         tile WITH a path keeps its subject (the path is the subject); a tile with none follows. */
      if (r && r.ok) await follow();
      return { ok: !!(r && r.ok) };
    });

    return {
      mount(host, p) {
        el = document.createElement('div');
        el.className = 'ctl-host';
        now = document.createElement('div');
        now.className = 'th-now';
        note = document.createElement('div');
        note.className = 'ctl-note';
        rows = document.createElement('div');
        rows.className = 'ctl-form';
        el.append(now, note, rows);
        host.appendChild(el);

        /* THE PATH, AS A ROW THE USER CAN TYPE IN. A tile whose subject is a directory on the
           server is a tile that has to offer the directory: this is where a corpus outside the
           engine's projects root enters the interface. It is the tile's OWN state (not a global,
           not the server's active project), so two of these tiles are two projects you can swap
           between with one wire. */
        if (ctx.tile) {
          input = document.createElement('input');
          input.type = 'text';
          input.className = 'te-in';
          input.placeholder = '/path/to/corpus  (a directory with Canon.qbit)';
          input.addEventListener('keydown', (e) => {
            if (e.key === 'Enter') { e.preventDefault(); setName(input.value.trim()); }
          });
          const add = ctlBtn('b-pj-path', 'wire in', 'serve this directory as the project this tile names');
          add.addEventListener('click', () => setName(input.value.trim()));
          rows.appendChild(ctlRow('path', ctlGroup(input, add)));
        } else {
          rows.appendChild(ctlNote('this node\'s path comes from the layout document'));
        }

        // the app owns the named-source table; the pane names the port and gets a remover
        if (ctx.registerSource && p) offSource = ctx.registerSource(p.id + '_project', outProject);
        /* THE ENGINE CAN BE SWITCHED BY SOMEBODY ELSE, so this tile listens: a switch, a path wired
           in or a path forgotten all come back through the same notification, and each of them can
           change what this tile stands for. The remover is kept for `unmount` — a tile re-renders on
           every change of subject, and a subscription per render that is never released is the leak
           this host's own mount warns about. */
        if (ctx.subscribeProjects) {
          /* the promise is RETURNED, not dropped: a host that awaits its subscribers (a test, a
             reload that wants the interface settled first) can then wait for this tile to have
             caught up, and a caller that does not await loses nothing */
          offFollow = ctx.subscribeProjects(() => follow());
        }
        if (p) summary = () => {
          p.summary = () => 'project · ' + ((project && (project.name || project.id)) || '—');
          if (p._summary && p.card && p.card.lastChild) p.card.lastChild.textContent = p.summary();
        };
        paint();
        /* KaTeX arrives on first use and a tile that only PRINTS A NAME has not asked for it yet
           (js/app/math.js::whenKatex): the first frame shows the name as source and this repaints
           it once the library is here. Without it, a workspace with no render tile never typesets
           anything — which is exactly the shape of the bug this fixes. */
        whenKatex(paint);
        learn();
      },
      unmount() {
        if (typeof offFollow === 'function') { try { offFollow(); } catch (e) {} offFollow = null; }
        if (typeof offSource === 'function') { try { offSource(); } catch (e) {} offSource = null; }
        if (el && el.parentNode) el.parentNode.removeChild(el);
        el = now = note = rows = input = summary = null;
      },
      refresh: () => { paint(); return learn(); },
      in: {},
      out: { project: outProject },
      call: { use: callUse },
      get ports() { return { in: this.in, out: this.out, call: this.call }; },
    };
  },
};
