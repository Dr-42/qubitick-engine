/**
 * app/project-services.js — the project list, and the four things that change it.
 *
 * ONE OWNER, TWO HOSTS. A projects pane asks its ctx for the list and for the calls that change it
 * (`ctx.projects()`, `ctx.addProject(path)`, …), and both hosts give it the same answers — that is
 * what makes a pane a module rather than a piece of one app. The canvas used to own all of this in
 * `host/canvas/net-app.js`; the ENGINE host had none of it at all, so a projects tile opened in
 * `shell.html` drew "no projects found" over a server with projects in it and every one of its
 * controls was inert. This module is that table, and each host supplies only the four things it
 * alone can: how to name a route, how to fetch, where a message goes, and what to do after the
 * engine has been switched to another project.
 *
 * THREE RULES THAT ARE NOT OBVIOUS, and each of them was a bug in the canvas's copy:
 *
 *   the LIST IS THE SERVER'S. Nothing here invents an entry: `load` replaces the whole document
 *   with what the last request was answered, and every mutation re-reads rather than patching —
 *   a client that patches can disagree with the settings file that the NEXT boot reads.
 *
 *   A LATE ANSWER MUST NOT WIN. `load` is called on the boot, on a rescan, and after every add and
 *   forget, and the requests are not queued. Whichever answer came back LAST used to be the list,
 *   whatever order they were asked in, so a slow fetch from before a `forget` could land after the
 *   one that followed it and put the forgotten path back — which the pane then drew, over a server
 *   that no longer had it (one run in six of a five-worker stress). The counter is the fix: only the
 *   last request issued may write.
 *
 *   THE SERVED PROJECT IS NOT THIS MODULE'S TO REFUSE. `forget` sends the path and shows whatever
 *   the server answers: whether a project is being served, and whether a path can be forgotten at
 *   all, are the server's rules (`crates/api/src/state.rs`), and a client that re-derives them is a
 *   client that will one day disagree.
 *
 *   AND IT IS SUBSCRIBABLE. A tile whose subject is "the project being served" has to FOLLOW the
 *   engine, not read it once: the projects pane can switch, a project tile's own `use` call can, and
 *   a switch made anywhere else is not something the other tiles hear about. `subscribe` is how they
 *   do — every write that changes the document notifies, and the pane re-reads what it needs
 *   (js/panes/project.js). It is a notification, not a value: a subscriber that draws from
 *   something else would still be drawing from a snapshot.
 */

/**
 * @param {object} o
 * @param {(name: string) => string} o.url      route name → URL (js/net/routes.js, or the endpoint registry)
 * @param {Function} o.fetch                    fetch-compatible
 * @param {(msg: string) => void} [o.toast]     where a refusal or a confirmation goes
 * @param {() => void} [o.onSwitched]           after the engine has been switched to another project
 * @param {object} [o.initial]                  the document to start with (tests; a boot that fetched early)
 */
export function createProjectServices({ url, fetch, toast = () => {}, onSwitched = () => {}, initial = null } = {}) {
  if (typeof url !== 'function') throw new Error('createProjectServices: url is required');
  if (typeof fetch !== 'function') throw new Error('createProjectServices: fetch is required');

  let PROJECTS = initial || { active: { id: '', name: '' }, list: [] };
  let SEQ = 0;
  const listeners = new Set();

  /** Tell every subscriber the document changed. One bad listener must not stop the rest. */
  function emit(why) {
    for (const fn of [...listeners]) {
      try { fn(PROJECTS, why); } catch (e) { console.warn('[projects] subscriber', e); }
    }
  }

  /**
   * Follow the list. Returns the unsubscribe, which the pane MUST call on unmount — a tile that
   * subscribes per render and never lets go is a tile that gets slower the longer you use it.
   * @param {(doc: object, why: string) => void} fn
   */
  function subscribe(fn) {
    if (typeof fn !== 'function') return () => {};
    listeners.add(fn);
    return () => { listeners.delete(fn); };
  }

  const json = async (name, init) => {
    const res = await fetch(url(name), init);
    try {
      return await res.json();
    } catch {
      return { ok: false, error: 'the server answered something that is not JSON' };
    }
  };

  const post = (name, body) => json(name, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });

  /** The document as it stands: `{ active: {id, name}, list: [{id, name, active, wired}] }`. */
  const get = () => PROJECTS;

  /** The key a workspace is saved under: the served project, by path. */
  const key = () => PROJECTS.active.id || PROJECTS.active.name || 'default';

  /** Read the list again. Only the last request issued may write (see the header). */
  async function load() {
    const mine = ++SEQ;
    try {
      const j = await json('projects.list');
      if (mine === SEQ) {
        const next = { active: j.active || { id: '', name: '' }, list: j.projects || [] };
        /* NOTIFY ON A CHANGE, not on every read: `load` runs on the boot, on a rescan and after
           every add and forget, and a subscriber that re-read the engine each time would turn one
           keystroke into a conversation. The comparison is by value, because a fresh parse of the
           same answer is never `===`. */
        const changed = JSON.stringify(next) !== JSON.stringify(PROJECTS);
        PROJECTS = next;
        if (changed) emit('load');
      }
    } catch (e) { console.warn('[projects]', e); }   // an older server simply has none
    return PROJECTS;
  }

  /** Serve another project. The engine re-ingests its corpus, so the page reloads. */
  async function use(id) {
    try {
      const j = await post('project.use', { id });
      if (j && j.ok) {
        const active = j.active || { id, name: '' };
        /* the answer carries the project that took over, so the document is updated from it rather
           than from a second round trip — and subscribers are told BEFORE the host is handed the
           switch, so a host that reloads still has an interface that was right for a moment, and a
           host that does not reload (a tile's own `use` call does not) has one that stays right */
        PROJECTS = { active, list: PROJECTS.list };
        emit('use');
        toast('project: ' + (active.name || id));
        onSwitched(active);
        return true;
      }
      toast('project: ' + ((j && j.error) || 'could not switch'));
    } catch (e) { toast('project: ' + e.message); }
    return false;
  }

  /** Wire a PATH in. The answer carries the project the path turned out to be, not the string sent. */
  async function add(path) {
    try {
      const j = await post('projects.add', { path });
      if (j && j.ok) {
        await load();
        toast('project wired in');
        return true;
      }
      toast('project: ' + ((j && j.error) || 'could not add'));
    } catch (e) { toast('project: ' + e.message); }
    return false;
  }

  /** Take a wired-in path back out. The server says what it will not do (see the header). */
  async function forget(path) {
    try {
      const j = await post('projects.forget', { path });
      if (j && j.ok) {
        await load();
        toast('project forgotten');
        return true;
      }
      toast('project: ' + ((j && j.error) || 'could not forget'));
    } catch (e) { toast('project: ' + e.message); }
    return false;
  }

  return { get, key, load, use, add, forget, subscribe };
}