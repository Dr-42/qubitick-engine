/*
 * live/proof-row.js — the sentence you typed, as the next row of the proof above it.
 *
 * A `.qbit` proof is a chain of rows, each ONE rewrite with the citation that justifies it, the
 * tags lined up in a column:
 *
 *   (a \Delta b) \Delta (b \Delta a)
 *     = a \Delta (b \Delta (b \Delta a))                [A1.1]
 *     = a \Delta ((b \Delta b) \Delta a)                [\ulcornerA1.1 b b a\urcorner]
 *
 * So when an author types the statement they want next and presses Enter, three questions have to
 * be answered, and they are answered here rather than in the editor's key handler:
 *
 *   WHAT IS BEING PROVED, AND FROM WHAT. `plan` reads the caret's line as the new row's
 *   expression and walks up to the nearest line that states one — the row above inside a proof,
 *   or the statement when the `Proof:` label is all that stands between them.
 *
 *   WHICH SIGN COMES. `=` for the corpus's equation chains: the sign the proof already uses if
 *   it has one, otherwise the relation the statement above is about. A proof that steps with
 *   `\leftrightarrow` keeps stepping with `\leftrightarrow`.
 *
 *   WHERE THE TAG GOES. Beside the row, in the column the block already lines its tags up in —
 *   the mode of the tag columns of the same block, or two spaces past its longest row when the
 *   block has none yet. A row that runs past that column gets one space, which is what the
 *   corpus does with its long `\ulcorner…\urcorner` instances.
 *
 * A ROW IS ONLY WRITTEN WHEN ONE CITATION JUSTIFIES IT. The engine answers with a chain, and a
 * chain of more than one step is several rows: writing the first citation beside the typed
 * statement would be a proof that does not follow. So a multi-step answer is reported, with the
 * citations it needs, and nothing is written — the author keeps their sentence and writes the
 * intermediate row. The other half of the reason is notation: the engine renders an intermediate
 * state in its own curried form (`\Delta a b`) where the corpus writes `a \Delta b`, and the
 * states between the two the author wrote have no source text yet.
 */

import { isMathy } from '../app/qbit-lines.js';

/** The relations a row or a statement can be written with, longest first so `\leftrightarrow`
 *  is not read as a `\to`, and matched only at bracket depth zero. */
const RELATIONS = [
  '\\leftrightarrow', '\\longrightarrow', '\\Longleftrightarrow', '\\iff',
  '\\Rightarrow', '\\rightarrow', '\\implies', '\\to', '=',
];

/** `indent`, then the row's own sign, then the expression, then the tag. */
const SIGN = /^(\\leftrightarrow|\\iff|\\Rightarrow|\\rightarrow|\\implies|\\to|↔|→|⇒|=)\s+/;
/** A trailing citation tag, with the whitespace in front of it kept apart: `(prefix)(gap)[tag]`. */
const TAG = /^(.*?)(\s*)\[([^\[\]]*)\]\s*$/;

/** One entry of a citation tag: an id the corpus writes (`A1.1`, `D.BT.02`, `T.BT.44`) or an
 *  instance of one (`\ulcornerA1.1 b b a\urcorner`). */
const CITE_ITEM = /^(?:[ADT][A-Za-z0-9.]*\d|\\ulcorner[\s\S]*\\urcorner)$/;

/**
 * True when a trailing bracket is a CITATION rather than part of the expression.
 *
 * A proof row ends with a bracket for exactly one reason in this corpus, but an expression may
 * end with one too — `[a, b]` is a side condition, `f[x]` is an application, `[\Delta a b]` is a
 * citation of nothing — so the tag has to READ like a citation: every comma-separated entry names
 * an id, and nothing else does.
 */
function looksLikeCite(inner) {
  const items = String(inner).split(',').map((s) => s.trim()).filter(Boolean);
  return items.length > 0 && items.every((it) => CITE_ITEM.test(it));
}

/** The tag of a line, as `{ cite, column }` — `column` is where the `[` starts.
 *
 *  SHARED, not mirrored: js/live/row-check.js had its own copy of this, of `tagColumn` and of
 *  `CITE_ITEM`, with a comment admitting they were the same thing. One reader of a tag means the
 *  strip's "the block's own column" and the plan's are the same column. */
export function tagOf(line) {
  const raw = String(line == null ? '' : line).replace(/\s+$/, '');
  const m = TAG.exec(raw);
  if (!m || !looksLikeCite(m[3])) return null;
  return { cite: m[3], column: m[1].length + m[2].length, before: m[1], gap: m[2] };
}

/**
 * The citation tag of one line, with the offsets of the brackets themselves.
 *
 * PUBLIC because a tag is not only read by the row that carries it: the editor marks these ranges
 * as links (`js/live/editor.js`), and a second reading of "what looks like a citation" would be a
 * second answer to which text is clickable.
 *
 * @param {string} line
 * @returns {{cite:string, from:number, to:number}|null} offsets within the line
 */
export function citeTag(line) {
  const tag = tagOf(line);
  if (!tag) return null;
  return { cite: tag.cite, from: tag.column, to: tag.column + tag.cite.length + 2 };
}

/** A case group's head: `<pattern> \mapsto`. The induction grammar is `js/live/induce.js`'s
 *  subject, and it is defined here because the walk to the statement above a row has to know that
 *  a pattern is not one — `induce.js` builds on this module, not the other way. */
export const CASE_PATTERN = /^(.*?)\s*(?:\\mapsto|↦)\s*$/;
/** The induction header: `\mu v` (the `μ` form the parser also accepts). */
export const INDUCTION_HEADER = /^(?:\\mu|μ)\s+(\S+)\s*$/;

/**
 * A labelled line: `Proof:`, `Sema:`, `Math: something`, `Type: \tau \to \mathfrak{E}`.
 *
 * A label is not a statement, whether or not it carries a value: `Type:` gives a node's type and
 * `Math:` carries a sentence, and neither is an expression a proof row could bridge from.
 */
const LABEL = /^[A-Za-z][A-Za-z ]{0,14}:\s*/;

/**
 * The expression a line states: no step sign in front, no citation tag behind.
 *
 * @param {string} line
 * @returns {string} '' when the line states nothing
 */
export function stated(line) {
  let t = String(line == null ? '' : line).trim();
  t = t.replace(SIGN, '');
  const tag = tagOf(t);
  if (tag) t = tag.before;
  return t.trim();
}

/**
 * A line read as a row: the sign it steps with, the expression it states, the citation it already
 * carries, and the indent it is written at.
 *
 * @param {string} line
 * @returns {{sign:string|null, expr:string, cite:string|null, indent:string}}
 */
export function row(line) {
  const raw = String(line == null ? '' : line);
  const m = raw.trim().match(SIGN);
  const tag = tagOf(raw);
  return {
    sign: m ? m[1] : null,
    expr: stated(raw),
    cite: tag ? tag.cite : null,
    indent: (/^\s*/.exec(raw) || [''])[0],
  };
}

/** The first relation at bracket depth zero, or null. */
function topLevelRelation(text) {
  let depth = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === '(' || c === '[' || c === '{') { depth++; continue; }
    if (c === ')' || c === ']' || c === '}') { depth = Math.max(0, depth - 1); continue; }
    if (depth !== 0) continue;
    for (const rel of RELATIONS) {
      if (!text.startsWith(rel, i)) continue;
      /* `\to` must not match inside `\toppled`; `=` is one character and needs no guard */
      const after = text[i + rel.length];
      if (rel !== '=' && after !== undefined && /[A-Za-z]/.test(after)) break;
      return { rel, at: i };
    }
  }
  return null;
}

/**
 * A statement split at its FIRST relation at bracket depth zero: `a \Delta b = b \Delta a` is
 * `{ lhs: 'a \Delta b', rel: '=', rhs: 'b \Delta a' }`.
 *
 * One documented limitation: a statement that is an equation on the left and a biconditional
 * around it (`… = … ↔ …`) splits at the equation, because that is the relation it opens with. The
 * engine's own parser is the authority on what a statement says; it is not asked about this yet.
 *
 * @param {string} text an expression, tag and step sign stripped
 * @returns {{lhs:string, rel:string|null, rhs:string|null}}
 */
export function splitRelation(text) {
  const t = String(text == null ? '' : text);
  const found = topLevelRelation(t);
  if (!found) return { lhs: t, rel: null, rhs: null };
  return {
    lhs: t.slice(0, found.at).trim(),
    rel: found.rel,
    rhs: t.slice(found.at + found.rel.length).trim(),
  };
}

/**
 * The sign the statement above is about: its first relation at bracket depth zero, else `=`.
 *
 * @param {string} statement the line above, tag and all
 * @returns {string} the sign as source text (`=`, `\leftrightarrow`, …)
 */
export function stepSign(statement) {
  const { rel } = splitRelation(stated(statement));
  return rel || '=';
}

/**
 * The sign a new row under `index` should step with: the proof's own if it has one, else the
 * sign the statement above is about.
 *
 * @param {string[]} lines
 * @param {number} index the line the row goes under
 * @returns {string}
 */
export function proofSign(lines, index) {
  for (let i = index - 1; i >= 0; i--) {
    const r = row(lines[i]);
    if (!r.expr) continue;
    return r.sign || stepSign(lines[i]);
  }
  return '=';
}

/** The contiguous non-blank lines around `index`, indents and tags included. */
export function block(lines, index) {
  const out = [];
  for (let i = index; i >= 0; i--) {
    const t = String(lines[i] == null ? '' : lines[i]);
    if (!t.trim() || /^(%|#|\/\/)/.test(t.trim())) break;
    out.unshift(t);
  }
  for (let i = index + 1; i < lines.length; i++) {
    const t = String(lines[i] == null ? '' : lines[i]);
    if (!t.trim() || /^(%|#|\/\/)/.test(t.trim())) break;
    out.push(t);
  }
  return out;
}

/**
 * Where a tag goes on a row of `rowLength` characters, given the block the row belongs to.
 *
 * @param {string[]} siblingLines every line of the block, the new row among them
 * @param {number} rowLength the length of the row the tag goes on
 * @returns {number} the column the `[` starts at
 */
export function tagColumn(siblingLines, rowLength) {
  const counts = new Map();
  for (const line of siblingLines) {
    const tag = tagOf(line);
    if (!tag) continue;
    counts.set(tag.column, (counts.get(tag.column) || 0) + 1);
  }
  let column = 0;
  let best = 0;
  for (const [col, n] of [...counts].sort((a, b) => a[0] - b[0])) {
    if (n > best) { best = n; column = col; }
  }
  if (!counts.size) {
    const longest = siblingLines.reduce((m, l) => Math.max(m, String(l).length), 0);
    column = Math.max(longest, rowLength) + 2;
  }
  return Math.max(column, rowLength + 1);
}

/**
 * What pressing Enter on `index` would do, decided WITHOUT the engine — so the key handler knows
 * whether Enter is a conversion or merely a newline before anything is fetched.
 *
 * @param {{lines:string[], index:number}} opts `index` is 0-based: the line the caret is on
 * @returns {{ready:boolean, why:string|null, lineNumber:number, typed:string, expr:string,
 *            lhs:string|null, sign:string, indent:string, siblingLines:string[]}}
 */
export function plan({ lines, index }) {
  const typed = String((lines || [])[index] == null ? '' : lines[index]);
  const expr = stated(typed);
  const none = (why) => ({
    ready: false, why, lineNumber: index, typed, expr, lhs: null, sign: '=', indent: '', siblingLines: [],
  });
  if (!expr) return none('the line states nothing');
  if (LABEL.test(expr)) return none('the line is a label');
  /* a row that already carries its citation is justified: Enter at the end of one is the newline
     it looks like, not an argument with the citation that is written there */
  if (row(typed).cite) return none('the line already carries its citation');
  /* a sentence, not mathematics: the corpus's own reader says what mathematics is
     (`js/app/qbit-lines.js::isMathy`), so prose typed under a statement stays prose */
  if (!isMathy(expr)) return none('the line is prose');

  let above = -1;
  for (let i = index - 1; i >= 0; i--) {
    const t = String(lines[i] == null ? '' : lines[i]).trim();
    if (!t || /^(%|#|\/\/)/.test(t)) continue;
    const s = stated(lines[i]);
    if (!s || LABEL.test(s)) continue;                   // `Proof:` sits between the two
    /* AN INDUCTION BLOCK'S OWN LINES STATE NOTHING. `\mu a` announces the induction and
       `\tau_0 \mapsto` announces a case: a row typed under an induced goal bridges from the goal,
       and a row typed straight under a pattern bridges from the statement the block inducts on —
       either way the pattern is not what it is proving against. */
    if (INDUCTION_HEADER.test(s) || CASE_PATTERN.test(s)) continue;
    /* a node's title (`A1. Realm`) states nothing to bridge from: a candidate has to be
       mathematics, which is the same question `js/app/qbit-lines.js` asks of every line */
    if (!isMathy(s)) continue;
    above = i;
    break;
  }
  if (above < 0) return none('nothing above it states an expression');
  const lhs = stated(lines[above]);
  if (lhs === expr) return none('the line states what the line above it states');

  return {
    ready: true,
    why: null,
    lineNumber: index,
    typed,
    expr,
    lhs,
    sign: proofSign(lines, index),
    indent: row(typed).indent || row(lines[above]).indent || '  ',
    siblingLines: block(lines, index),
  };
}

/**
 * The row to write, from the plan and the engine's answer.
 *
 * @param {{plan:object, answer?:object|null, error?:string|null}} opts
 * @returns {{ok:true, line:string, cite:string} | {ok:false, error:string}}
 */
export function rowFor({ plan: p, answer, error }) {
  const NOT_FOUND = 'no citation found to convert it';
  if (error) return { ok: false, error: 'autocite: ' + error };
  if (!answer || typeof answer !== 'object') return { ok: false, error: NOT_FOUND };
  if (answer.reached !== true) return { ok: false, error: NOT_FOUND };

  /* the chain starts where the search started, so the steps after the first are the rewrites */
  const steps = (Array.isArray(answer.citations) ? answer.citations : []).slice(1);
  const rewrites = steps.filter((s) => s && s.cite);
  if (!rewrites.length) return { ok: false, error: NOT_FOUND };
  if (rewrites.length > 1) {
    const cites = rewrites.map((s) => s.cite).join(', ');
    return {
      ok: false,
      error: 'no single citation converts it — the engine needs ' + rewrites.length
        + ' rows here (' + cites + ')',
    };
  }

  const cite = rewrites[0].cite;
  const head = p.indent + p.sign + ' ' + p.expr;
  /* the block's OWN tags set the column; the row being written is not one of them yet */
  const column = tagColumn(p.siblingLines, head.length);
  return { ok: true, line: head + ' '.repeat(column - head.length) + '[' + cite + ']', cite };
}
