// induce.test.mjs — the induction planner: the case goals Mod-Enter writes under a `\mu` block.
//
//   node --test "tests/unit/*.test.mjs"
//
// The engine does not derive induction cases (there is no engine-side step for it: a carrier
// declares its constructors only sometimes, so the cases are the AUTHOR's). What is automatic is
// the part that follows from the author's own text: each case's goal is the statement above the
// block with the induction variable replaced by that case's pattern, substituted in the author's
// notation rather than rendered from the parsed expression — `\Delta a b` is what the engine renders
// where the corpus writes `a \Delta b`.
//
// The laws:
//
//   · the goal is the statement's LEFT side with the variable replaced, not the whole equation —
//     a case proves the left side down to the right one;
//   · a case that already has a body is left alone (re-inducing a proved block is not an edit);
//   · substitution is word-boundary on the author's writing: `\tau_0`, `\forall` and `\mathcal`
//     contain letters that are not the variable;
//   · every refusal says which of those it is, because the pane shows the reason and no toast.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { inductionPlan, caseEdits, substitute } from '../../web-next/js/live/induce.js';

/* the block the module's own doc comment uses: a carrier with a nullary and a recursive
   constructor, inducting on `a` */
const BLOCK = [
  'T.1. AddZero',
  '\\forall a : \\tau, \\tau_0 +_{\\tau} a = \\tau_0',
  'Proof:',
  '\\mu a',
  '  \\tau_0 \\mapsto',
  '  ^+ a \\mapsto',
];

const lineEnds = (lines) => {
  const ends = [];
  let acc = 0;
  /* the document offset where line n ENDS: its own newline is not counted, the ones before it are */
  for (const l of lines) { ends.push(acc + l.length); acc += l.length + 1; }
  return (n) => ends[n];
};

test('each case gets the statement left side with the variable replaced', () => {
  const plan = inductionPlan({ lines: BLOCK, index: 3 });
  assert.equal(plan.ready, true);
  assert.equal(plan.variable, 'a');
  assert.deepEqual(plan.cases.map((c) => c.pattern), ['\\tau_0', '^+ a']);
  assert.deepEqual(plan.cases.map((c) => c.goal), ['\\tau_0 +_{\\tau} \\tau_0', '\\tau_0 +_{\\tau} ^+ a']);
});

test('the caret inside a case body still finds the block header', () => {
  /* a body row under the LAST case only fills that case: the first is still bare */
  const lines = [...BLOCK, '      \\tau_0 +_{\\tau} ^+ a', '        = \\tau_0  [\\equiv]'];
  const plan = inductionPlan({ lines, index: 7 });
  assert.equal(plan.ready, true);
  assert.equal(plan.variable, 'a');
  assert.deepEqual(plan.cases.map((c) => c.pattern), ['\\tau_0']);
});

test('a block whose every case has a body says so instead of editing', () => {
  const lines = [
    'T.1. AddZero',
    '\\forall a : \\tau, \\tau_0 +_{\\tau} a = \\tau_0',
    'Proof:',
    '\\mu a',
    '  \\tau_0 \\mapsto',
    '    \\tau_0 +_{\\tau} \\tau_0',
    '  ^+ a \\mapsto',
    '    \\tau_0 +_{\\tau} ^+ a',
  ];
  const plan = inductionPlan({ lines, index: 3 });
  assert.equal(plan.ready, false);
  assert.match(plan.why, /already has a body/);
});

test('a proved case is not rewritten, and a bare one still is', () => {
  const lines = [
    'T.1. AddZero',
    '\\forall a : \\tau, \\tau_0 +_{\\tau} a = \\tau_0',
    'Proof:',
    '\\mu a',
    '  \\tau_0 \\mapsto',
    '    \\tau_0 +_{\\tau} \\tau_0',
    '      = \\tau_0  [\\equiv]',
    '  ^+ a \\mapsto',
  ];
  const plan = inductionPlan({ lines, index: 3 });
  assert.equal(plan.ready, true);
  assert.deepEqual(plan.cases.map((c) => c.pattern), ['^+ a']);
});

test('the three refusals name themselves', () => {
  assert.match(inductionPlan({ lines: ['T.1. X', 'a + b = b + a'], index: 1 }).why, /no \\mu header/);
  assert.match(inductionPlan({ lines: ['\\mu a', '  \\tau_0 \\mapsto'], index: 0 }).why, /nothing above the header/);
  assert.match(inductionPlan({ lines: BLOCK, index: 0 }).why, /no \\mu header/);
});

test('substitution is word-boundary on the author\'s writing', () => {
  /* a name continues with a letter, a digit, `_` and `'` — and a TeX command is not a variable */
  assert.equal(substitute('\\tau_0 +_{\\tau} a', 'a', '\\tau_0'), '\\tau_0 +_{\\tau} \\tau_0');
  assert.equal(substitute('a + a_0 + a\'', 'a', 'x'), 'x + a_0 + a\'');
  assert.equal(substitute('\\forall a : \\tau, \\mathcal{A} a', 'a', 'z'), '\\forall z : \\tau, \\mathcal{A} z');
  assert.equal(substitute('\\alpha a', 'alpha', 'z'), '\\alpha a');     // a command name, not a name
});

test('caseEdits inserts one goal line after each bare case, indented under the pattern', () => {
  const plan = inductionPlan({ lines: BLOCK, index: 3 });
  const edits = caseEdits({ plan, lineEnd: lineEnds(BLOCK) });
  assert.equal(edits.length, 2);
  assert.equal(edits[0].from, edits[0].to);            // pure insertions, nothing replaced
  assert.equal(edits[0].insert, '\n    \\tau_0 +_{\\tau} \\tau_0');
  assert.equal(edits[1].insert, '\n    \\tau_0 +_{\\tau} ^+ a');
  /* applying them back to front (the order CodeMirror needs) writes exactly the doc's example */
  const text = BLOCK.join('\n');
  const out = [...edits].sort((a, b) => b.from - a.from)
    .reduce((acc, e) => acc.slice(0, e.from) + e.insert + acc.slice(e.to), text);
  assert.equal(out.split('\n').slice(4).join('\n'),
    '  \\tau_0 \\mapsto\n    \\tau_0 +_{\\tau} \\tau_0\n  ^+ a \\mapsto\n    \\tau_0 +_{\\tau} ^+ a');
});

test('an unready plan edits nothing', () => {
  const plan = inductionPlan({ lines: ['T.1. X', 'a + b = b + a'], index: 1 });
  assert.deepEqual(caseEdits({ plan, lineEnd: () => 0 }), []);
});