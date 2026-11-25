/*
 * live/cite-link.js — following a citation tag to what it names.
 *
 * A `[A1.4]` in a proof row is a reference, and following one is an engine question with a pane's
 * answer: the engine resolves the label (`GET /api/cite?name=`, `crates/build/src/cite_target.rs`)
 * and hands back the node AND THE FILE, because the engine is the side that knows where the corpus
 * is; the pane opens it and says what it pointed at.
 *
 * IT LIVES HERE RATHER THAN IN THE PANE because two panes will want it — the canvas editor tile
 * marks the tags (`js/live/editor.js`) and the shell's citation-aware editor is the natural second
 * caller — and because `panes/app-live.js` is at the lint's file ceiling, which is the same reason
 * the check strip became its own module.
 *
 * A tag that names nothing, and a LOCAL law (`\equiv`, `^i`, which name no node at all), say so
 * rather than doing nothing when clicked.
 */

/**
 * @param {object} host
 * @param {object} host.app   the app's `cite(name)` and `openFile(path)`
 * @param {(m:string) => void} host.toast
 * @param {string} name the citation as written (`A1.4`, `T.BT.01`)
 * @returns {Promise<{ok:boolean, target:?object, said:string}>} what happened, for a caller that
 *   wants to report it itself
 */
export async function followCite({ app, toast, name }) {
  if (!app || !app.cite) return { ok: false, target: null, said: 'this host cannot follow citations' };
  try {
    const answer = await app.cite(name);
    const target = answer && answer.target;
    if (!target) {
      const said = (answer && answer.note) ? answer.note : '[' + name + '] names nothing here';
      if (toast) toast(said);
      return { ok: false, target: null, said };
    }
    if (target.path && app.openFile) app.openFile(target.path);
    const said = target.label + ' · ' + target.kind + ' ' + target.node
      + (target.statement ? ' · statement ' + target.statement : '');
    if (toast) toast(said);
    return { ok: true, target, said };
  } catch (e) {
    const said = 'cite: ' + (e && e.message ? e.message : e);
    if (toast) toast(said);
    return { ok: false, target: null, said };
  }
}
