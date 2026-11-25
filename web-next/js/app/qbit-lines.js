/**
 * app/qbit-lines.js — a `.qbit` line has a ROLE, and the role decides how it is read.
 *
 * This is the first module extracted from the live app's inline script, and it is
 * pure on purpose: it takes text and returns roles, with no DOM and no KaTeX. The
 * renderer builds elements from what this says; a unit test can read it without a
 * browser.
 *
 * Why it exists at all: the renderer used to hand EVERY line to KaTeX as display
 * math unless it was a comment or a "short title" (`/^[A-Z][\w .-]*$/` and at most
 * four words, a guess). So `Math: Choice is commutative` — a sentence about the
 * node — was typeset as a formula, `Phys: Events either happen…` became a failed
 * formula, and `Sema:` alone (whose formula is on the NEXT line) became an empty
 * one. Titles were recognised by luck: `T.BT.70. TickLeTrans` matched the regex,
 * `D.BT.32. Godel` did not.
 *
 * The label table is the corpus's own vocabulary, counted across all 329 files:
 * 329 `Math:`, 172 `Proof:`, 85 `Type:`, 65 `Sema:`, 10 `Note:`, 7 `Phys:`,
 * 5 `Symbol:`/`Rate:`/`QUnits:`/`Dims:`, 1 each of `Module:`/`Kind:`/`Id:`/
 * `Falsify:`. A label nobody taught it falls back to looking at the value's text,
 * so a new label degrades to "decide from content" rather than to "assume formula".
 */

/** Labels whose value is a sentence about the node. */
export const PROSE_LABELS = new Set([
  'math', 'phys', 'note', 'kind', 'module', 'id', 'name', 'purpose', 'require',
  'leandir', 'status', 'why', 'because',
]);

/** Labels whose value is mathematics, or whose body runs on below them. */
export const MATH_LABELS = new Set([
  'sema', 'proof', 'type', 'rate', 'qunits', 'dims', 'symbol', 'falsify',
  'statements', 'axioms', 'goal', 'def', 'axiom',
]);

/**
 * `A1. Realm`, `D.BT.32. Godel`, `T.BT.01. DeltaComm`, `S.3. P3-NamedCombinations`
 *
 * The first segment may carry a digit (`A1`), which the regex this replaced did
 * not allow — so all six axioms, the roots of the class chain, had no header and
 * rendered their title as body text. The corpus's other 324 headers happened to
 * be dotted names that matched.
 */
const HEAD = /^([A-Z][A-Za-z0-9]*(?:\.[A-Za-z0-9]+)*)\.\s+(.+)$/;
/** `Sema:`, `Type: \tau \to …`, `Math: …` */
const LABEL = /^([A-Z][A-Za-z]{1,14}):\s*(.*)$/;
/** what a proof row looks like: `  = a \Delta (b \Delta a)   [A1.1]` */
const CITE_TAIL = /^(.*?)\s*\[([^\]]*)\]\s*$/;
/** characters that mean "this is mathematics" when nothing labels the line */
const MATHY = /\\[a-zA-Z]+|[=≤≥⊆⊂⊇∈∉→↦↔⟷⟹∧∨¬∀∃∅∩∪∘⊗⊕≠≈′τℤ𝔈𝔮𝔠ΩΣΔΛ]/;

/** Does this text look like mathematics rather than a sentence? */
export function isMathy(t) {
  if (!t) return false;
  if (/^[=,]/.test(t)) return true;          // a proof row continues an equation
  if (MATHY.test(t)) return true;
  if (/^\[/.test(t)) return true;            // a bracketed side condition
  return false;
}

/**
 * Classify ONE line.
 *
 * Roles:
 *   blank    nothing to draw
 *   comment  `% …` — prose about the corpus
 *   head     the node's own line: `label` + `value` (the name)
 *   label    a label with a value; `math` says which way the value reads, and an
 *            empty value means the body runs on below it (`Sema:`, `Proof:`)
 *   math     mathematics, possibly with a citation: `body` + `cite`
 *   prose    a sentence
 *
 * @param {string} raw
 * @param {{first?: boolean}} [opts] `first` — may this line be the node header?
 */
export function classifyLine(raw, opts = {}) {
  const line = String(raw ?? '').replace(/\s+$/, '');
  const t = line.trim();
  if (!t) return { kind: 'blank', line };
  if (/^(%|#|\/\/)/.test(t)) return { kind: 'comment', line, text: t };

  if (opts.first) {
    const m = HEAD.exec(t);
    if (m) return { kind: 'head', line, label: m[1], value: m[2] };
  }

  const lm = LABEL.exec(t);
  if (lm) {
    const label = lm[1];
    const value = lm[2].trim();
    const key = label.toLowerCase();
    if (PROSE_LABELS.has(key)) return { kind: 'prose', line, label, text: value };
    if (MATH_LABELS.has(key) || value) {
      return { kind: 'label', line, label, value, math: isMathy(value) };
    }
  }

  if (isMathy(t)) {
    const m = CITE_TAIL.exec(t);
    return m
      ? { kind: 'math', line, body: m[1], cite: m[2] }
      : { kind: 'math', line, body: t, cite: '' };
  }
  return { kind: 'prose', line, text: t };
}

/**
 * Classify a whole file. Only the first non-blank, non-comment line may be the
 * node header — a `A1. Realm` further down is prose, not a second title.
 */
export function classifyLines(text) {
  const out = [];
  let seenHead = false;
  for (const raw of String(text ?? '').split('\n')) {
    const blank = !raw.trim();
    const comment = !blank && /^(%|#|\/\/)/.test(raw.trim());
    const r = classifyLine(raw, { first: !seenHead });
    if (!blank && !comment) seenHead = true;
    out.push(r);
  }
  return out;
}