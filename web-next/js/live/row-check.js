/*
 * live/row-check.js — the editor's check strip: what the corpus makes of the row you are on.
 *
 * Two things are pure here, and they are the two the pane must not get wrong:
 *
 *   WHICH ROW A LINE IS. The engine numbers rows the way the parser reads them: the block's
 *   initial expression is row 1 and every `= ` line is one more. The verdicts come back keyed by
 *   that number, because it is the only position the parser carries (it works on field text, not
 *   spans — `qubitick_model::ChainRow::line` says so), so the mapping lives here once.
 *
 *   WHAT "TRY THIS" WRITES. A citation tag, never an expression. The engine renders a state in its
 *   own curried notation (`\Delta (\Delta a \varnothing) b`) which is NOT a `.qbit` line — measured:
 *   it does not parse back. A tag is the one part of a row the engine can hand over verbatim, and
 *   it is the part an author most often wants filled in.
 */

/** The step lines of a proof: `= …`, `, …` (a continuation) — the parser's own reading. */
import { tagOf, tagColumn } from './proof-row.js';

const STEP = /^\s*[=,]\s/;

/**
 * Which row each line of a buffer is, by the parser's numbering.
 *
 * EVERY line of the block gets a number, because the parser numbers the lines it reads (the
 * block's initial expression is row 1 and each `= ` line is one more) while only the step lines
 * become `ChainRow`s. The strip needs both facts: a verdict is keyed by row number, and a caret on
 * a line that is not a step (`Proof:` block's first line, a case pattern) has nothing checked.
 *
 * @param {string[]} lines
 * @param {number} proofAt the index of the `Proof:` line (the block starts below it)
 * @returns {Map<number, number>} line index → row number
 */
export function rowsOf(lines, proofAt) {
  const out = new Map();
  if (proofAt < 0) return out;
  let row = 0;
  for (let i = proofAt + 1; i < lines.length; i++) {
    const t = String(lines[i] == null ? '' : lines[i]).trim();
    if (!t) break;                                  // the block ends at a blank line
    if (/^(?:\\mu|μ)\s/.test(t)) { row = 0; continue; }   // an induction's cases restart at 1
    row += 1;                                        // the initial expression, then each step
    out.set(i, row);
  }
  return out;
}

/** Is this line one the corpus writes a row for? (`= …` or `, …`) */
export const isStep = (line) => STEP.test(String(line == null ? '' : line));

/** The block's rows: the lines under `proofAt`, up to the blank line that ends the block — the same
 *  rule `rowsOf` uses to stop numbering. */
function blockRange(lines, proofAt) {
  if (proofAt < 0) return { start: 0, end: 0 };
  let end = proofAt + 1;
  while (end < lines.length && String(lines[end] == null ? '' : lines[end]).trim()) end += 1;
  return { start: proofAt + 1, end };
}

/** A line without its trailing tag. A bracket that is not a citation is left alone. */
function withoutTag(line) {
  const s = String(line == null ? '' : line);
  return (tagOf(s) ? tagOf(s).before : s).replace(/\s+$/, '');
}

/**
 * THE WHOLE BLOCK, IN ONE COLUMN.
 *
 * `withTag` moves one line into the column the block already uses. If the rows above were written
 * at different columns — pasted, typed by hand, or grown a character at a time — aligning only the
 * new row leaves the block ragged: the citation is written neatly under rows that are not, which is
 * the thing an author notices. So every row of the block that HAS a tag is brought to the same
 * column, and the caller applies all of it as one change set (one undo, and CodeMirror maps the
 * caret through it).
 *
 * @param {string[]} lines the whole buffer
 * @param {number} proofAt index of the `Proof:` line, or -1 when the caret is not in a block
 * @param {number} at the row being written
 * @param {string} cite the tag to write there
 * @returns {{column:number, changes:Array<{index:number, text:string}>}} only rows that CHANGE
 */
export function alignTags(lines, proofAt, at, cite) {
  const all = Array.isArray(lines) ? lines : [];
  const bare = (i) => withoutTag(all[i]);
  const { start, end } = blockRange(all, proofAt);
  const rows = [];
  if (proofAt >= 0) {
    for (let i = start; i < end; i++) if (i !== at && tagOf(all[i])) rows.push(i);
  }
  /* The column is the block's own mode, never left of the longest row that must fit: a tag cannot
     be pushed into the text it follows. */
  const scope = proofAt >= 0 ? all.slice(start, end) : all;
  const longest = Math.max(bare(at).length, ...scope.map((_, k) => bare(proofAt >= 0 ? start + k : k).length), 0);
  const column = Math.max(tagColumn(scope, bare(at).length), longest + 1);

  const changes = [];
  for (const i of [...rows, at]) {
    const b = bare(i);
    const tag = i === at ? cite : tagOf(all[i]).cite;
    const text = b + ' '.repeat(Math.max(1, column - b.length)) + '[' + tag + ']';
    if (text !== String(all[i] == null ? '' : all[i])) changes.push({ index: i, text });
  }
  return { column, changes };
}

/**
 * What the strip should say about the caret's line.
 *
 * @param {{lines:string[], index:number, verdicts:Array<{row:number, verdict:string, cite:?string,
 *          reaches?:string[]}>, offer:?{cite:string}}} opts
 * @returns {{kind:string, text:string, detail:string, offer:?string}}
 */
export function strip({ lines, index, verdicts, offer }) {
  const proofAt = String(lines[index] || '').trim() === 'Proof:'
    ? index
    : lines.findIndex((l, i) => i <= index && String(l).trim() === 'Proof:');
  /* the nearest `Proof:` above the caret, so a caret inside a block finds that block */
  let blockAt = -1;
  for (let i = index; i >= 0; i--) {
    if (String(lines[i] || '').trim() === 'Proof:') { blockAt = i; break; }
    if (i < index && !String(lines[i] || '').trim()) break;
  }
  const rows = rowsOf(lines, blockAt >= 0 ? blockAt : proofAt);
  const row = rows.get(index);
  if (row == null) return { kind: 'none', text: '', detail: '', offer: offer && offer.cite ? offer.cite : null };
  const found = (verdicts || []).find((v) => v.row === row);
  if (!found) {
    /* a line with no verdict either has not been checked yet or is not a step at all
       (the block's own initial expression carries no row) */
    return isStep(lines[index])
      ? { kind: 'checking', text: 'checking…', detail: '', offer: offer && offer.cite ? offer.cite : null }
      : { kind: 'none', text: '', detail: '', offer: offer && offer.cite ? offer.cite : null };
  }
  return {
    kind: found.verdict,
    text: textFor(found),
    detail: detailFor(found),
    offer: offer && offer.cite ? offer.cite : null,
  };
}

function textFor(v) {
  switch (v.verdict) {
    case 'confirmed': return 'confirmed';
    case 'unconfirmed': return 'not confirmed';
    case 'local': return 'local law';
    case 'unknown-cite': return 'unknown cite';
    default: return String(v.verdict || '');
  }
}

function detailFor(v) {
  switch (v.verdict) {
    case 'confirmed':
      return v.cite ? 'this row is one step of ' + v.cite : 'the two sides are one expression';
    case 'unconfirmed':
      return v.cite
        ? v.cite + ' does not get there' + (v.reaches && v.reaches.length ? ' (it reaches ' + v.reaches[0] + ')' : '')
        : 'the two sides differ';
    case 'local':
      return (v.cite || '') + ' is a local law: the definitional step or the induction hypothesis';
    case 'unknown-cite':
      return (v.cite || '') + ' names nothing this corpus has';
    default:
      return '';
  }
}
