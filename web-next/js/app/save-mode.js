/**
 * app/save-mode.js — what follows a write, as policy rather than as a screen.
 *
 * TWO CALLERS WRITE TO THE CORPUS and they must not describe it differently: the editor's save
 * (js/host/canvas/doc-save.js) and the node form (js/app/node-services.js, which creates a file). Both
 * hand the server's answer here and get the same sentence and the same follow-up.
 *
 * The mode is the workspace's setting (js/app/prefs.js owns its vocabulary); the ACTIONS are injected,
 * because "rebuild the project" and "check this file's rows" belong to the app, not to this module.
 */

import { saveModeOf } from './prefs.js';

/**
 * WHAT FOLLOWS A WRITE — the one implementation, because a document save and the node form are two
 * callers of the same act and would otherwise say different things about it.
 *
 * `answer` is the server's, so the note is about what the ENGINE did rather than what the client hoped:
 * `symbols` rows written for that file, and `inLedger` — a newly created file has no node row yet, so it
 * has nowhere to hang a symbol, and saying so is the difference between "created" and "created and the
 * engine knows the notation".
 */
export function symbolNote({ symbols = 0, inLedger } = {}) {
  if (inLedger === false) return ' (its symbol lands on the next build)';
  const n = Number(symbols || 0);
  return n ? ' · ' + n + ' symbol' + (n === 1 ? '' : 's') : '';
}

/**
 * @param {string} [o.head]  what the ACT was, for the person reading it: 'saved', or 'created <path>'.
 *   ONE message per act — a caller that also announced its own result would have that sentence
 *   overwritten a line later (measured: the node form said "created …" and the follow-up said "saved").
 */
export async function followUp({ mode, content, answer, say, runCheck, rows, head = 'saved' }) {
  const note = symbolNote(answer || {});
  /* A FUNCTION IS ASKED, A VALUE IS TAKEN AS IT IS: the mode is a live setting, so both callers hold a
     getter — and `fresh` is read at the moment of the write, not when the pane was built. Passing the
     getter where a value was expected made every node-create read the default ('none') and follow
     nothing, which looked exactly like a save mode that does not apply to created files. */
  const chosen = saveModeOf(typeof mode === 'function' ? mode() : mode);
  if (chosen === 'rebuild' && runCheck) {
    say(head + ' · rebuilt' + note);
    runCheck();
    return;
  }
  if (chosen === 'eval' && rows) {
    try {
      const rep = await rows(content);
      const list = (rep && rep.rows) || [];
      const bad = list.filter((r) => r.verdict !== 'confirmed' && r.verdict !== 'local').length;
      say(head + note + ' · ' + list.length + ' row' + (list.length === 1 ? '' : 's')
        + (bad ? ', ' + bad + ' unconfirmed' : ''));
    } catch (e) {
      say(head + note + ' · check failed: ' + (e && e.message));
    }
    return;
  }
  say(head + note);
}
