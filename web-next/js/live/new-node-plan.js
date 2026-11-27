/**
 * live/new-node-plan.js — where a new node goes, what it is called, and what it says.
 *
 * THE PLAN FOR A NEW NODE, kept apart from the pane's form: the pane builds the fields and renders
 * the preview, this decides what the preview says and what the new file contains. It lived in the
 * pane until the pane crossed the file-size rule, and the split is the right one anyway — this half
 * is pure, so each stub below can be built and checked against a corpus without a browser.
 *
 * EVERYTHING IT INVENTS COMES FROM THE CORPUS IT IS CREATING IN. It used to decide all of it
 * itself, out of the harness's own corpus: the era list was `Era0`, the strata list `Strata0|1|2`,
 * the file-name token `BT`, the number always `1` (so the second new theorem overwrote the first).
 * A project is the user's: so the lists are the directories its files are in, the token is the one
 * its files already use (or, when they use none, the project's own name), the number is the next
 * free one in that directory, and the body is a STUB that says what to write rather than a specimen
 * of the harness's algebra.
 *
 * THREE CORPUS SHAPES, and the difference matters:
 *
 *   KNOWN, TREE   `Era0/Strata0/Theorems/...` — the era, strata and directory selects are these.
 *   KNOWN, FLAT   `notes.qbit` — no era level anywhere, so a node belongs at the ROOT: the engine's
 *                 own scope for a path with no era is `Era0/Strata0` (`pipeline::scope_of_path`), but
 *                 a corpus that never had the tree does not get one invented for it.
 *   UNKNOWN       no corpus behind the pane at all — then the engine's default tree, not a guess.
 */

/** The file-name token a corpus already uses in `dir` — `T_BT_01_X.qbit` → `BT`. */
export function tokenIn(dir, files) {
  const seen = new Map();
  for (const f of files) {
    if (dir && !String(f).includes('/' + dir + '/')) continue;
    const name = String(f).split('/').pop() || '';
    const m = name.match(/^[TDA]_([A-Za-z0-9]+)_(\d+)_/);
    if (m) seen.set(m[1], (seen.get(m[1]) || 0) + 1);
  }
  const best = [...seen.entries()].sort((a, b) => b[1] - a[1])[0];
  return best ? best[0] : null;
}

/** The token a project with no convention gets: `.../corpus-b` → `CORPUS_B`. */
export function tokenFromId(projectId) {
  const base = String(projectId || '').split('/').filter(Boolean).pop() || '';
  return base.replace(/[^A-Za-z0-9]+/g, '_').replace(/^_+|_+$/g, '').toUpperCase();
}

/** The next free number in `dir`, from the names already there. */
export function nextNumber(dir, files) {
  let max = 0;
  for (const f of files) {
    if (dir && !String(f).includes('/' + dir + '/')) continue;
    const name = String(f).split('/').pop() || '';
    const m = name.match(/^[TDA]_[A-Za-z0-9]+_(\d+)_/);
    if (m) max = Math.max(max, parseInt(m[1], 10) || 0);
  }
  return max + 1;
}

/** Is this corpus FLAT — no `Era/Strata/` level anywhere — so a node sits at its root? */
export const isFlat = (files) => !files.some((f) => String(f).split('/').length >= 3);

/**
 * The eras and strata a set of paths offers.
 *
 * @param {string[]} files
 * @returns {{eras: string[], strataOf: (era: string) => string[]}}
 */
export function layout(files) {
  const eras = new Set();
  const strata = new Map();
  for (const f of files) {
    const parts = String(f).split('/');
    if (parts.length >= 3) {
      const era = parts[0];
      eras.add(era);
      if (!strata.has(era)) strata.set(era, new Set());
      strata.get(era).add(parts[1]);
    }
  }
  /* a flat corpus has no era/strata to offer, and it does not need one: the engine's own scope for
     a file with no era in its path is `Era0/Strata0`, which is what the selects then say */
  if (!eras.size) {
    eras.add('Era0');
    strata.set('Era0', new Set(['Strata0']));
  }
  return {
    eras: [...eras].sort(),
    strataOf: (era) => (strata.has(era) ? [...strata.get(era)].sort() : []),
  };
}

/**
 * The path, the node id and the kind of the node about to be made.
 *
 * @param {object} o
 * @param {string[]} o.files the evidence: the corpus's paths, plus the path the tile was opened on
 * @param {string} o.era
 * @param {string} o.strata
 * @param {string} o.dir  `Theorems`, `Definitions` or `Axioms`
 * @param {string} o.projectId the served project's id (only read when the corpus has no token)
 * @param {string} o.name what the author typed
 * @returns {{fullPath: string, nodeId: string, cleanName: string, typeLabel: string}}
 */
export function planNode({ files = [], era = 'Era0', strata = 'Strata0', dir = '', projectId = '', name = '' } = {}) {
  const known = files.length > 0;
  const flat = known && isFlat(files);
  const cleanName = String(name).trim().replace(/\s+/g, '_') || 'New';
  const dirPath = flat ? '' : `${era}/${strata}/${dir}`;

  const token = tokenIn(dir, files) || tokenFromId(projectId) || 'NODE';
  let prefix = '';
  let qbitPrefix = '';
  let typeLabel = '';
  if (dir === 'Theorems') {
    typeLabel = 'theorem'; prefix = 'T_' + token + '_'; qbitPrefix = 'T.' + token + '.';
  } else if (dir === 'Definitions') {
    typeLabel = 'definition'; prefix = 'D_' + token + '_'; qbitPrefix = 'D.' + token + '.';
  } else if (dir === 'Axioms') {
    /* an axiom is named by number alone (`A1. Base`) — the shipped corpus does it, and the parser
       takes no division token there */
    typeLabel = 'axiom'; prefix = 'A'; qbitPrefix = 'A';
  }

  const numStr = String(nextNumber(dir, files)).padStart(2, '0');
  const nodeId = `${qbitPrefix}${numStr}`;
  const fileName = `${prefix}${numStr}_${cleanName}.qbit`;
  const fullPath = dirPath ? `${dirPath}/${fileName}` : fileName;
  return { fullPath, nodeId, cleanName, typeLabel };
}

/**
 * The body of a new node: A STUB THAT PARSES, AND NOTHING INVENTED.
 *
 * The header grammar is real (`T.<t>.<n>`, `D.<t>.<n>`, `A<n>`) and the statement is the author's,
 * so each kind gets the smallest body the engine accepts and the TODO goes in PROSE — prose is kept,
 * printed in the PDF and carried into the Lean as a doc comment (crates/emit/src/module.rs), so the
 * note travels with the node instead of sitting in a field nothing reads. Each body was built and
 * run through the corpus gate (`qubitick build`) before it was written here:
 *
 *   axiom       `\top` — a vacuous statement, unmistakably a placeholder, and it verifies
 *   definition  `\mathrm{<Name>}` — the node's own name as its glyph, which is not a claim
 *   theorem     no statement yet, and `\sorry` — the build reports it as ADMITTED, which is exactly
 *               what an unfinished node is
 *
 * `req:` IS NOT IN ANY OF THEM, and that is a trap rather than tidying: `req` stopped being a field
 * (crates/parse/src/node.rs::split_fields), so a `req: None` line is read as a STATEMENT — every
 * node the pane made while it wrote one carried `req: None` as its first statement.
 */
export function stubFor({ typeLabel = 'theorem', nodeId = '', cleanName = '' } = {}) {
  if (typeLabel === 'axiom') {
    return `${nodeId}. ${cleanName}\n\\top\nMath: TODO — the statement goes here\n`;
  }
  if (typeLabel === 'definition') {
    return `${nodeId}. ${cleanName}\n\\mathrm{${cleanName}}\nMath: TODO — the symbol, and what it means\n`;
  }
  return `${nodeId}. ${cleanName}\nProof:\n\\sorry\nMath: TODO — the statement goes here\n`;
}
