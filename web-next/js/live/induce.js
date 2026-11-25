/*
 * live/induce.js — the induction block you declared, with each case's goal written in it.
 *
 * The corpus writes an induction as a header, one group per case, and the goal of each case in the
 * author's own notation:
 *
 *   \mu a
 *     \tau_0 \mapsto
 *       \tau_0 +_{\tau} \tau_0
 *         = \tau_0    [\equiv]
 *     ^+ a \mapsto
 *       \tau_0 +_{\tau} ^+ a
 *
 * The goal under a pattern is the statement above the block with the induction variable replaced
 * by that case's pattern — and it is done by SUBSTITUTING IN THE AUTHOR'S OWN TEXT rather than by
 * instantiating the parsed expression and rendering it back, because the engine has no renderer for
 * the corpus's surface: an `Expr` renders to Lean, curried (`\Delta a b`), where the corpus writes
 * `a \Delta b`. Substituting the text keeps the row readable, and the two lines it produces for a
 * real theorem are the two lines that theorem already has (see the unit test, which compares them
 * against T.BT.11).
 *
 * WHAT IS NOT HERE: the case patterns themselves. A carrier declares its constructors only
 * sometimes (`Sema: \mathcal{W} = \mathbf{p} \vee \mathbf{q}` is a `Formation`; `\tau` has no such
 * declaration), so the cases are the author's, and deriving them from the corpus — from a
 * recursive definition's constructor equations, or from how the corpus's other proofs induct on the
 * same carrier — is a separate, engine-side step. Pressing the chord on a block whose header has no
 * `\mapsto` group writes nothing and says why.
 */

/* The two line shapes of an induction block are the corpus's grammar, and they live in
   `proof-row.js` beside the walk that has to skip them; this module reads them. */
import { stated, splitRelation, CASE_PATTERN as PATTERN, INDUCTION_HEADER as HEADER } from './proof-row.js';

/**
 * The statement a line states, with its leading binders split off: `\forall a : \tau, body`.
 *
 * @param {string} line
 * @returns {{body:string, binders:string}}
 */
function splitBinders(line) {
  const t = stated(line);
  /* a `\forall`/`\exists` line is `binders, body` — the comma is at bracket depth zero, and the
     binder itself never reaches the substitution (substituting there would rewrite the sentence
     into `\forall \tau_0 : \tau`) */
  if (!/^\\(forall|exists)\b/.test(t)) return { body: t, binders: '' };
  let depth = 0;
  for (let i = 0; i < t.length; i++) {
    const c = t[i];
    if (c === '(' || c === '[' || c === '{') depth++;
    else if (c === ')' || c === ']' || c === '}') depth = Math.max(0, depth - 1);
    else if (c === ',' && depth === 0) return { binders: t.slice(0, i + 1), body: t.slice(i + 1).trim() };
  }
  return { body: t, binders: '' };
}

/**
 * `text` with every free occurrence of `name` replaced by `by`.
 *
 * Word-boundary substitution on the author's own writing: `a` is replaced in `a +_{\tau} a`, and
 * left alone in `\tau_0` (where it is a letter inside a command), in `\mathcal` and in `\forall` —
 * a TeX command name is not a variable.
 *
 * @param {string} text
 * @param {string} name
 * @param {string} by
 * @returns {string}
 */
export function substitute(text, name, by) {
  if (!name) return text;
  /* a name character is a letter, a digit and the two things a name continues with: `a_0` is
     another name, and so is `a'` */
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return String(text).replace(new RegExp(`(^|[^A-Za-z0-9_\\\\'])${escaped}(?![A-Za-z0-9_'])`, 'g'), (m, lead) => lead + by);
}

/**
 * What pressing the chord on the caret's line would do.
 *
 * @param {{lines:string[], index:number}} opts `index` is 0-based: the line the caret is on
 * @returns {{ready:boolean, why:string|null, variable:string|null, statement:string|null,
 *            cases:Array<{lineNumber:number, pattern:string, indent:string, goal:string}>}}
 */
export function inductionPlan({ lines, index }) {
  const all = lines || [];
  const none = (why) => ({ ready: false, why, variable: null, statement: null, cases: [] });

  /* the header: on the caret's line, or the nearest one above it inside the same block */
  let headerAt = -1;
  for (let i = Math.min(index, all.length - 1); i >= 0; i--) {
    const t = String(all[i] == null ? '' : all[i]).trim();
    if (!t) break;
    if (HEADER.test(t)) { headerAt = i; break; }
    if (PATTERN.test(t)) continue;                       // a case group of this block
  }
  if (headerAt < 0) return none('no \\mu header on or above this line');
  const variable = HEADER.exec(String(all[headerAt]).trim())[1];

  /* the statement it inducts on: the nearest one above the header */
  let statementAt = -1;
  for (let i = headerAt - 1; i >= 0; i--) {
    const t = String(all[i] == null ? '' : all[i]).trim();
    if (!t) continue;
    const s = stated(all[i]);
    if (!s || /^[A-Za-z][A-Za-z ]{0,14}:\s*/.test(s)) continue;
    statementAt = i;
    break;
  }
  if (statementAt < 0) return none('nothing above the header states an expression');
  const { body } = splitBinders(all[statementAt]);
  if (!body) return none('the statement above the header states nothing');
  /* THE CASE'S GOAL IS THE SIDE THE PROOF WORKS ON. A case shows the statement's left-hand side
     with the variable replaced — `\tau_0 +_{\tau} \tau_0` — and the rows under it walk to the
     right-hand side. Substituting the whole equation would write a line the author then has to
     edit back down. */
  const { lhs } = splitRelation(body);
  if (!lhs) return none('the statement above the header states nothing');

  /* the case groups, and which of them have no body yet */
  const cases = [];
  for (let i = headerAt + 1; i < all.length; i++) {
    const raw = String(all[i] == null ? '' : all[i]);
    const t = raw.trim();
    if (!t) break;
    if (HEADER.test(t)) break;                            // a second block, not this one
    const m = PATTERN.exec(t);
    if (!m) continue;                                     // a body row: part of the case above
    let hasBody = false;
    for (let j = i + 1; j < all.length; j++) {
      const next = String(all[j] == null ? '' : all[j]).trim();
      if (!next) break;
      hasBody = !PATTERN.test(next) && !HEADER.test(next);
      break;
    }
    if (hasBody) continue;                                // already proved: leave it alone
    cases.push({
      lineNumber: i,
      pattern: m[1].trim(),
      indent: (/^\s*/.exec(raw) || [''])[0],
      goal: substitute(lhs, variable, m[1].trim()),
    });
  }
  if (!cases.length) return none('every case of this block already has a body');

  return { ready: true, why: null, variable, statement: stated(all[statementAt]), cases };
}

/**
 * The edits the plan asks for: a goal line under each case that has none.
 *
 * Returned as `{from, to, insert}` ranges, so the caller applies ONE transaction — the block is
 * written or it is not.
 *
 * @param {{plan:object, lineEnd:(n:number) => number}} opts `lineEnd(n)` is the offset of the end
 *   of the 0-based line `n` (the caret's document owns its own offsets)
 * @returns {Array<{from:number, to:number, insert:string}>}
 */
export function caseEdits({ plan: p, lineEnd }) {
  if (!p || !p.ready) return [];
  return p.cases.map((c) => ({
    from: lineEnd(c.lineNumber),
    to: lineEnd(c.lineNumber),
    insert: '\n' + c.indent + '  ' + c.goal,
  }));
}
