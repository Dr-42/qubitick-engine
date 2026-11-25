/*
 * live/check-strip.js — the strip under an editor tile's bar: what the corpus makes of the row the
 * caret is on, and the citation that would complete it.
 *
 * THE CHECK CONFIRMS; IT CANNOT REFUTE. `POST /api/rows` answers per row and the answers are
 * `confirmed` / `unconfirmed` (with the states it did reach) / `local` / `unknown-cite` — a search
 * that fails is not a row that is wrong, which is why the strip prints a sentence and not a
 * squiggle (`crates/build/src/row_check.rs` measures the case that forced the wording).
 *
 * "TRY THIS" WRITES A CITATION TAG, never an expression: the engine renders a state in its own
 * curried notation (`\Delta (\Delta a \varnothing) b`) and that is not a `.qbit` line — measured,
 * it does not parse back. The tag is the part of a row the engine can hand over verbatim, and the
 * part an author most often wants filled in (`js/live/row-check.js` aligns it with the block).
 *
 * With the `suggest rewrites` preference on, the strip also lists what the corpus can reach from
 * where the caret is (`POST /api/steps`), each with its citation. Those are INFORMATION, not
 * source: they are shown in the engine's notation and are not insertable, for the reason above.
 */

import { strip, alignTags, rowsOf } from './row-check.js';
import { splitRelation } from './proof-row.js';

/**
 * @param {object} host
 * @param {object} host.app     the canvas app's capabilities (`rows`, `steps`, `autocite`,
 *                              `editorPrefs`) — a pane asks for answers, never for routes
 * @param {object} host.p       the pane instance (`p.ed` is the editor once mounted)
 * @param {Element} host.el     the strip's element
 * @param {(m:string) => void} host.toast
 * @param {() => string[]} host.lines the buffer, as lines
 */
export function createCheckStrip({ app, p, el, toast, lines }) {
    let verdicts = [];
    let offer = null;
    let steps = [];
    let checking = 0;

    const localLines = () => String((p.ed && p.ed.getDoc && p.ed.getDoc()) || (p.doc && p.doc.text) || '').split('\n');
    const caretLine = () => {
      const v = p.ed && p.ed.view;
      if (!v) return -1;
      return v.state.doc.lineAt(v.state.selection.main.head).number - 1;
    };

    /** The block the caret is in: the nearest `Proof:` at or above it. */
    function blockAt(all, index) {
      for (let i = index; i >= 0; i--) {
        if (String(all[i] || '').trim() === 'Proof:') return i;
        if (i < index && !String(all[i] || '').trim()) break;
      }
      return -1;
    }

    /** Is the `suggest rewrites` preference on? Read at every look, so turning it off takes the
     *  chips away on the next paint rather than on the next thing that happens to repaint. */
    const suggestOn = () => !!(app.editorPrefs && app.editorPrefs().suggest);

    /** Ask the corpus what it makes of every row: debounced, because this runs as you type. */
    function scheduleCheck() {
      if (!p.doc) return;
      clearTimeout(checking);
      checking = setTimeout(async () => {
        const text = localLines().join('\n');
        if (!text.trim() || !app.rows) return;
        try {
          const answer = await app.rows(text);
          verdicts = Array.isArray(answer && answer.rows) ? answer.rows : [];
        } catch (e) { verdicts = []; }
        paint();
      }, 400);
    }

    /** What the caret's row needs: its verdict, a citation worth offering, and — if the setting
     *  is on — the rewrites the corpus can reach from here. */
    function scheduleLook() {
      if (!p.doc || !app.autocite) return;
      const all = localLines();
      const at = caretLine();
      if (at < 0) { offer = null; steps = []; paint(); return; }
      const { from, to } = rowPair(all, at);
      if (!from || !to) { offer = null; steps = []; paint(); return; }
      const on = suggestOn();
      (async () => {
        /* TRY THIS: a row with no citation whose step autocite can bridge in one, offers the tag.
           The tag is the one part of a row the engine can hand over verbatim — its rendering of a
           state is curried (`\Delta (\Delta a \varnothing) b`) and does not parse back. */
        try {
          const answer = await app.autocite(from, to);
          const rewrites = ((answer && answer.citations) || []).slice(1).filter((s) => s && s.cite);
          const written = rowCite(all, at);
          offer = answer && answer.reached && rewrites.length === 1 && !written ? { cite: rewrites[0].cite } : null;
        } catch (e) { offer = null; }
        steps = [];
        if (on && app.steps) {
          try {
            const answer = await app.steps(from, 3);
            steps = ((answer && answer.steps) || []).map((s) => ({ cite: s.cite, expr: s.expr }));
          } catch (e) { steps = []; }
        }
        paint();
      })();
    }

    /** The two sides of the caret's row: the expression the row above ends at, and the row's own. */
    function rowPair(all, at) {
      const trimmed = (i) => String(all[i] == null ? '' : all[i]).trim();
      if (!/^[=,]\s/.test(trimmed(at))) return { from: null, to: null };
      const body = trimmed(at).replace(/^[=,]\s*/, '').replace(/\s*\[[^\]]*\]\s*$/, '').trim();
      let above = -1;
      for (let i = at - 1; i >= 0; i--) {
        const t = trimmed(i);
        if (!t || t === 'Proof:' || /^(?:\\mu|μ)\s/.test(t) || /\\mapsto|↦/.test(t)) continue;
        above = i;
        break;
      }
      if (above < 0) return { from: null, to: body };
      /* the row above ends at its own right-hand side; the statement above a first row is the
         whole sentence, and its left-hand side is what the chain starts from */
      const line = trimmed(above).replace(/\s*\[[^\]]*\]\s*$/, '').trim();
      const m = /^[=,]\s*(.*)$/.exec(line);
      if (m) {
        const rel = splitRelation(m[1]);
        return { from: (rel.rhs || rel.lhs).trim(), to: body };
      }
      const stmt = line.replace(/^\\forall[^,]*,\s*/, '');
      const rel = splitRelation(stmt);
      return { from: (rel.lhs || stmt).trim(), to: body };
    }

    /** The citation the caret's row already carries, if any. */
    function rowCite(all, at) {
      const m = /\[([^\]]*)\]\s*$/.exec(String(all[at] == null ? '' : all[at]).trim());
      return m ? m[1].trim() : null;
    }

    /** Paint the strip: the verdict, the offer, and the suggestions the setting asks for. */
    function paint() {
      const all = localLines();
      const at = caretLine();
      /* THE SETTING IS ASKED HERE, not only where the list is fetched: `steps` can be in hand from a
         look taken while it was on, and a paint triggered by anything else (a verdict arriving) must
         not put chips back on a row after the author turned them off. */
      const shown = suggestOn() ? steps : [];
      el.textContent = '';
      if (at < 0) return;
      const s = strip({ lines: all, index: at, verdicts, offer });
      if (s.kind === 'none' && !s.offer && !shown.length) return;
      if (s.kind !== 'none') {
        const chip = document.createElement('span');
        chip.className = 'ed-verdict ' + s.kind;
        chip.textContent = s.text;
        chip.title = s.detail || s.text;
        el.append(chip);
      }
      if (s.offer) {
        const b = document.createElement('button');
        b.className = 'ed-try';
        b.type = 'button';
        b.textContent = 'try this: [' + s.offer + ']';
        b.title = 'write this citation into the row (it is the one step the corpus needs)';
        b.addEventListener('click', (e) => {
          e.stopPropagation();
          writeTag(view(), s.offer);
        });
        el.append(b);
      }
      for (const st of shown) {
        const b = document.createElement('button');
        b.className = 'ed-sug';
        b.type = 'button';
        b.textContent = '→ ' + st.expr + ' · [' + st.cite + ']';
        b.title = 'a rewrite the corpus can reach from here; the state is shown in the engine’s own notation, which is not .qbit source';
        el.append(b);
      }
    }

    const view = () => p.ed && p.ed.view;

    /** Write a citation tag into the caret's row, in the block's own column. */
    function writeTag(v, cite) {
      if (!v) return;
      const all = localLines();
      const at = caretLine();
      const blockLine = blockAt(all, at);
      const row = rowsOf(all, blockLine).get(at);
      /* THE WHOLE BLOCK MOVES, NOT JUST THIS ROW. Rows written at other columns are brought to the
         same one — a block that is ragged above the row you just cited is the thing the author
         notices — and it all goes in as ONE dispatch, so one undo takes it back. */
      const { changes } = alignTags(all, blockLine, at, cite);
      if (!changes.length) return;
      const doc = v.state.doc;
      const edits = changes.map((c) => {
        const line = doc.line(c.index + 1);            // CodeMirror is 1-based
        return { from: line.from, to: line.to, insert: c.text };
      });
      /* the caret stays where the author left it, mapped through the changes */
      v.dispatch({ changes: edits });
      const moved = changes.length - 1;
      toast('cited [' + cite + '] on row ' + (row == null ? '?' : row)
        + (moved > 0 ? ' — and aligned ' + moved + ' row' + (moved === 1 ? '' : 's') + ' above' : ''));
      scheduleCheck();
      scheduleLook();
    }

    return { paint, scheduleCheck, scheduleLook, writeTag };
}
