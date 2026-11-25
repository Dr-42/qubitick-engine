import { followUp } from '../../app/save-mode.js';

/**
 * host/canvas/doc-save.js — writing the open document back to the corpus.
 *
 * Nothing did. The canvas could OPEN a file (`app.openFile`, the tree, the editor's own file select) and
 * it could save the WORKSPACE (the console's `save`, which writes which tiles exist where) — and those
 * two are easy to confuse, which is why "how do I save the file I am editing" had no answer in the
 * interface at all. `POST /api/file` was reachable only through the source tile's editor and the node
 * form.
 *
 * What a save does, in order:
 *
 *   1. `POST /api/file` with the tile's document at its own path. The engine also injects that file's
 *      symbols into the DB on this route (crates/api/src/handlers_gap.rs::post_file ->
 *      qubitick_build::ingest_file_symbols), so the answer carries `symbols` and `inLedger` — the two
 *      things a person cannot otherwise see: whether the notation they just wrote is known yet.
 *   2. the document stops being dirty (`docs.docSaved`), which is what turns the rail's `● unsaved` and
 *      the editor bar's dot off.
 *   3. the SAVE MODE runs. It is one setting, three answers, and it exists because the honest answers
 *      cost very different amounts:
 *
 *        'none'     nothing more. The save is the save, and the symbol injection above is free.
 *        'eval'     check THIS file's proof rows (`POST /api/rows`, the same check the check strip
 *                   runs on a caret's row, but over the whole file) and say what it found.
 *        'rebuild'  re-ingest the corpus and build it (`POST /api/build`, the console's check). This is
 *                   the expensive one — a corpus ingest alone measured ~10 s for 330 files — and it is
 *                   the only mode that gives a brand-new file a node row, which is what an unledgered
 *                   save lacks (`inLedger: false`).
 *
 * The default is 'none': a save that silently starts a build is a save that hangs the interface for
 * seconds without being asked to.
 */

/**
 * @param {object} deps
 * @param {object} deps.docs            js/app/docs.js: `docPath`, `docText`, `docOf`, `docSaved`
 * @param {(name:string)=>string} deps.url
 * @param {(u:string, init?:object)=>Promise<Response>} deps.fetch
 * @param {() => string} deps.mode      the current save mode (read per save, not captured)
 * @param {(m:string)=>void} [deps.toast]
 * @param {() => void} [deps.runCheck]  the corpus build (the console's check)
 * @param {(content:string)=>Promise<object>} [deps.rows]  the per-file row check
 */

export function createDocSave({ docs, url, fetch, mode, toast, runCheck, rows, repaint }) {
  const say = toast || (() => {});
  let busy = false;

  /** One save at a time: a second Ctrl+S while the first is in flight must not post twice. */
  async function save(p) {
    if (busy) return { ok: false, why: 'busy' };
    const path = docs.docPath(p);
    if (!path) { say('nothing to save: this tile has no file'); return { ok: false, why: 'no path' }; }
    const text = docs.docText(p);
    busy = true;
    let answer = null;
    try {
      const r = await fetch(url('file.save'), {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path, content: text }),
      });
      answer = await r.json().catch(() => null);
      if (!r.ok || !answer || answer.ok === false) {
        say('save refused: ' + ((answer && (answer.message || answer.error)) || ('HTTP ' + r.status)));
        return { ok: false, answer };
      }
    } catch (e) {
      say('save failed: ' + (e && e.message));
      return { ok: false };
    } finally {
      busy = false;
    }

    const doc = docs.docOf(path, text);
    docs.docSaved(doc);
    // the tiles showing this file read `doc.dirty` in their own chrome: the mark is theirs to clear
    if (repaint) repaint(path);
    await followUp({ mode: mode(), content: text, answer, say, runCheck, rows });
    return { ok: true, answer };
  }

  return { save, get busy() { return busy; } };
}
