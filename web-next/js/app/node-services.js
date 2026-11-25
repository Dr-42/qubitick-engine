import { followUp } from './save-mode.js';

/**
 * app/node-services.js — creating a corpus node, for whichever host is running.
 *
 * ONE OWNER, TWO HOSTS — the lesson `js/app/project-services.js` records, learned again here: a pane
 * asks its ctx to make the node (`ctx.createNode({ path, content })`) and never a route, a method or
 * a body. A pane that assembled the URL itself would be a second place a route string lives, and the
 * second place is the one that goes stale.
 *
 * WHAT IT DOES NOT DECIDE: the path or the body. `js/panes/new-node.js` reads the corpus it is
 * creating IN — the directories its files live in, the division token those files already use, the
 * next free number in that directory — and hands over a finished path and a stub body that parses.
 * This writes them and answers what the server said; a refusal (an illegal path, a write that
 * failed) is the server's own sentence, not a rule re-derived here.
 *
 * WHY IT EXISTS: that pane wrote through the ENGINE host's websocket (`ws.endpoint('file')`), which
 * the canvas has not got, so in the canvas the New Node tile could not be mounted at all. The feature
 * lived in one host and was absent from the other — which is what a route owned by a pane looks like
 * from the outside.
 */

/**
 * @param {object} o
 * @param {(name: string) => string} o.url   route name → URL (js/net/routes.js)
 * @param {Function} o.fetch                 fetch-compatible
 * @param {(msg: string) => void} [o.toast]  where a refusal or a confirmation goes
 * @param {() => string} [o.mode]            the save mode (`js/app/prefs.js`), asked per write
 * @param {() => void} [o.runCheck]          the corpus build, for the 'rebuild' mode
 * @param {(content: string) => Promise<object>} [o.rows]  the per-file row check, for 'eval'
 */
export function createNodeServices({ url, fetch, toast = () => {}, mode, runCheck, rows } = {}) {
  if (typeof url !== 'function') throw new Error('createNodeServices: url is required');
  if (typeof fetch !== 'function') throw new Error('createNodeServices: fetch is required');

  /**
   * Write one corpus file.
   *
   * @param {{path: string, content: string}} node
   * @returns {Promise<string|null>} the path the server acknowledged, or null when it refused
   */
  async function create({ path, content } = {}) {
    if (!path) { toast('a node needs a path'); return null; }
    let r;
    try {
      r = await fetch(url('file.save'), {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path, content: content == null ? '' : String(content) }),
      });
    } catch (e) {
      toast('could not create ' + path + ': ' + ((e && e.message) || e));
      return null;
    }
    /* A REFUSAL IS THE SERVER'S WORDS. `post_file` answers a phase/code/message body for a path it
       will not write (`server.illegal-path`), and an internal error for a write that failed; both
       are more useful to the author than "failed". */
    const j = r && r.json ? await r.json().catch(() => ({})) : {};
    if (!r || !r.ok) {
      toast((j && (j.error || j.message)) || ('could not create ' + path));
      return null;
    }
    /* A NEW FILE HAS NO NODE ROW, so the engine has nowhere to hang its symbol yet (`inLedger: false`
       from `POST /api/file`): the confirmation says so instead of letting a person believe the notation
       is known, and WHAT FOLLOWS is the same follow-up a document save runs — so 'rebuild project on
       save' is what makes a node and its symbol land together, which is the only mode that can. ONE
       message: the lead word is the act. */
    await followUp({ head: 'created ' + (j.path || path), mode, content, answer: j, say: toast, runCheck, rows });
    return j.path || path;
  }

  return { create };
}
