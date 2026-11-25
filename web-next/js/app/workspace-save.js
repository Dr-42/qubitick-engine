/*
 * workspace-save.js — saving the workspace, and what happens when that fails.
 *
 * Extracted from the inline script in web-next/index.html. The rules here are about not
 * losing work, so each one is deliberate:
 *
 *   DEBOUNCED      a drag changes the layout sixty times a second; a save per change would
 *                  be a request per frame. The delay is the caller's, and a second change
 *                  during it restarts the clock.
 *   SINGLE FLIGHT  one save at a time. A save that arrives while one is running does not
 *                  queue behind it — it RE-SCHEDULES, so what finally lands is the newest
 *                  state rather than a backlog of old ones.
 *   MIRRORED FIRST the local copy is written BEFORE the request, so a page that is closed
 *                  while the request is in flight still has its layout. The server copy is
 *                  the durable one; the mirror is what makes a reload look like nothing
 *                  happened.
 *   NEVER LOUD     a failed save is not an exception: the workspace is still on screen and
 *                  the mirror still has it. The caller decides whether to say anything, and
 *                  a quiet save (the debounced one) says nothing at all.
 *   SAFE MODE      a workspace in safe mode saves nothing, on purpose — that is what safe
 *                  mode MEANS, and it is the one case where the answer is "no" rather than
 *                  "try".
 *
 * Everything the saver touches is injected: the document source, the local mirror, the
 * request, the clock, and the way it speaks. So the rules above are testable without a
 * server, a browser or a wait.
 */

/**
 * @param {object} o
 * @param {() => object} o.document        the workspace document (collectWorkspace)
 * @param {() => string} o.projectKey      which project this save belongs to
 * @param {() => boolean} o.safe           safe mode: never write
 * @param {(key: string, doc: object) => void} o.mirror   write the local copy
 * @param {() => object} o.layouts         the in-memory table of every project's document
 * @param {(url: string, init: object) => Promise<Response>} o.fetch
 * @param {(name: string) => string} o.url route lookup
 * @param {(msg: string) => void} [o.toast]
 * @param {number} [o.delay]               debounce, ms (900 in the app)
 * @param {object} [o.timers]              { setTimeout, clearTimeout }
 * @param {() => number} [o.now]
 */
export function createSaver({
  document: docOf, projectKey, safe, mirror, layouts, fetch, url, toast,
  delay = 900, timers = globalThis, now = () => Date.now(),
}) {
  let timer = 0;
  let saving = false;

  function say(msg) { if (toast) toast(msg); }

  async function save({ loud = false } = {}) {
    if (safe && safe()) {
      // safe mode: nothing is written, and a loud caller is told so
      if (loud) say('safe mode: nothing saved');
      return { ok: false, reason: 'safe' };
    }
    if (saving) {
      /* One at a time. Re-scheduling (rather than queueing) means the save that finally
         lands carries the NEWEST state: a queue would write a sequence of stale layouts
         and the last of them would be as old as the backlog. */
      schedule();
      return { ok: false, reason: 'busy' };
    }
    saving = true;
    try {
      const doc = docOf();
      const table = typeof layouts === 'function' ? layouts() : layouts;
      if (table) table[projectKey()] = doc;
      // the mirror first: a page closed mid-request still has its layout
      try { mirror(projectKey(), doc); } catch (e) { /* a full disk is not a lost workspace */ }
      try {
        const r = await fetch(url('settings.save'), {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ workspace: { byProject: table, savedAt: now() } }),
        });
        if (loud) say(r && r.ok ? 'workspace saved' : 'save failed');
        return { ok: !!(r && r.ok), status: r && r.status };
      } catch (e) {
        // the mirror stands, and a quiet save says nothing: this is the debounced path
        if (loud) say('save unavailable (local copy kept)');
        return { ok: false, reason: 'offline', error: e };
      }
    } finally {
      saving = false;
    }
  }

  function schedule() {
    timers.clearTimeout(timer);
    timer = timers.setTimeout(() => { save(); }, delay);
  }

  /** Forget the workspace: drop the mirror, tell the server, then reload. */
  async function reset({ reload } = {}) {
    timers.clearTimeout(timer);
    try { mirror(projectKey(), null); } catch (e) { /* nothing to lose */ }
    try {
      await fetch(url('settings.save'), {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ workspace: null, layout: null }),
      });
    } catch (e) { /* the reload is what the caller asked for */ }
    if (reload) reload();
    return { ok: true };
  }

  return {
    save, schedule, reset,
    get saving() { return saving; },
    get pending() { return !!timer; },
    cancel() { timers.clearTimeout(timer); timer = 0; },
  };
}