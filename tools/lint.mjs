// lint.mjs — the rules this client keeps, checked in one pass, with no dependencies.
//
//   node tools/lint.mjs            # check; exit 1 if a rule fires
//   node tools/lint.mjs --list     # print every rule and every allowance, and what is wrong today
//
// EACH RULE EXISTS BECAUSE ALLOWING IT CAUSED A BUG, and the point of writing them down in code
// instead of in prose is that prose cannot fail a build. The rules are deliberately grep-shaped:
// no parser, no type checker, no plugin protocol — a rule that needs a toolchain to express is a
// rule this project does not get to have.
//
// THERE IS NO BASELINE FILE, AND THAT IS THE DESIGN. A count-keyed baseline ("12 violations when we
// started, never go above") lets debt grow silently and needs its own --write-baseline ceremony.
// What is here instead is `ALLOW`: one entry per (file, rule) with a REASON, and the check fails in
// both directions — a violation that is not listed fails, and a listed entry that no longer fires
// fails too, so the list can only shrink or be corrected, never rot.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const CLIENT = 'web-next';

/* ---------------- the rules ---------------- */

/**
 * A rule is `{ id, why, scope, test(file, text) }` where `test` returns nothing when the file is
 * clean and a string naming the offence when it is not. `file` is repo-relative with forward slashes.
 *
 * `scope` says whose convention the rule is. The client's conventions — the endpoint table, the one
 * frame governor, the storage adapter, `ctx` over the document — are about the client's code, so they
 * do not police `tools/`; a tool is allowed to name a route in a comment, and this file itself is
 * full of the identifiers the client may not use. Framework imports and page references are about the
 * repo, and a tool reaching outside the repo is about the tools.
 */
const RULES = [
  {
    id: 'a',
    scope: 'client',
    why: 'a route written anywhere but the endpoint table is a route nobody can rename',
    test(file, text) {
      if (file === `${CLIENT}/js/defaults/endpoints.js`) return null;
      const hit = /(['"`])(\/api\/|\/ws\/)/.exec(text);
      return hit ? `route literal ${hit[1]}${hit[2]}… outside js/defaults/endpoints.js` : null;
    },
  },
  {
    id: 'b',
    scope: 'client',
    why: 'one frame governor, or the LOD and the HUD fight over the same frame',
    test(file, text) {
      if (file === `${CLIENT}/js/core/frame.js`) return null;
      const line = text.split('\n').findIndex((l) => /\brequestAnimationFrame\b/.test(l));
      return line < 0 ? null : `requestAnimationFrame outside js/core/frame.js (line ${line + 1})`;
    },
  },
  {
    id: 'c',
    scope: 'repo',
    why: 'the client ships as plain modules; a framework would arrive unbuilt',
    test(file, text) {
      const m = /from\s+['"](react|react-dom|vue|svelte|alpinejs|lit|htmx\.org|preact|jquery)['"/]/.exec(text)
        || /<script[^>]+src=["'][^"']*(react|vue|svelte|alpine|lit|htmx|jquery)[^"']*["']/i.exec(text);
      return m ? `framework dependency "${m[1]}"` : null;
    },
  },
  {
    id: 'd',
    scope: 'client',
    why: 'a file that grows past this has the app back inside it',
    limit: 450,
    test(file, text) {
      const lines = text.split('\n').length - (text.endsWith('\n') ? 1 : 0);
      return lines > 450 ? `${lines} lines, over the 450 limit` : null;
    },
  },
  {
    id: 'e',
    scope: 'client',
    why: 'one storage adapter, with the server copy and the local mirror in one place',
    test(file, text) {
      if (file === `${CLIENT}/js/core/storage.js`) return null;
      const line = text.split('\n').findIndex((l) => /\blocalStorage\b/.test(l));
      return line < 0 ? null : `localStorage outside js/core/storage.js (line ${line + 1})`;
    },
  },
  {
    id: 'f',
    scope: 'client',
    why: 'a pane reaches its own elements through ctx, not through the document',
    test(file, text) {
      if (file === `${CLIENT}/js/main.js`) return null;
      const line = text.split('\n').findIndex((l) => /\bgetElementById\b/.test(l));
      return line < 0 ? null : `getElementById outside js/main.js (line ${line + 1})`;
    },
  },
  {
    id: 'g',
    scope: 'tools',
    why: 'a tool in this repo may only reach into this repo',
    test(file, text) {
      if (!file.startsWith('tools/')) return null;
      const m = /from\s+['"](\/(?!\/)[^'"]*|node_modules[^'"]*)['"]/.exec(text);
      return m ? `imports "${m[1]}" — an absolute path or a package outside the repo` : null;
    },
  },
];

/**
 * Rule (h) is about the pages, not the modules: every file a page names has to exist, because a
 * missing script or stylesheet is a request that 404s while the page still paints, which is the
 * failure mode the gates cannot see as an error unless the page says so.
 */
const PAGES = [`${CLIENT}/index.html`, `${CLIENT}/shell.html`];

/* ---------------- the allowances ---------------- */

/**
 * `file|rule` -> why the offence is acceptable HERE. Rewrite the entry when the reason stops being
 * true; the check refuses an entry that no longer fires, so a stale reason cannot outlive its bug.
 */
const ALLOW = {
  [`${CLIENT}/js/main.js|b`]:
    'the entry yields one frame while it swaps hosts — `await new Promise(r => requestAnimationFrame(r))` is a scheduling primitive, not a loop',
  [`${CLIENT}/js/panes/graph.js|b`]:
    'the graph pane animates its own canvas; folding it into the governor is a pane change, not a lint fix (tracked in the rules table as the one real violation of (b))',
  [`${CLIENT}/js/live/graph.js|d`]:
    'the force layout, its pointer handling and its paint in one file; splitting it is code motion over the pane that just stabilised',
  [`${CLIENT}/js/panes/editor-ctxmenu.js|d`]:
    'the editor menu is a table of items and their actions; the table is the file',
  [`${CLIENT}/js/panes/editor.js|d`]: 'the editor pane: mount, keys, selection, save — splitting is planned, not done',
  [`${CLIENT}/js/panes/editor-vim.js|d`]: 'a key map is a table and a table is long',
  [`${CLIENT}/js/panes/editor-symbols.js|d`]: 'symbol resolution and its decorations share one walk over the document',
  [`${CLIENT}/js/panes/editor-citations.js|d`]: 'citation decorations and their alignment are one pass over the same ranges',
  [`${CLIENT}/js/panes/graph.js|d`]: 'the graph pane drew its own animation loop, so it also got long; both go together when (b) is fixed',
};

/* ---------------- the run ---------------- */

function walk(dir, out = []) {
  for (const name of readdirSync(path.join(ROOT, dir))) {
    const rel = dir ? `${dir}/${name}` : name;
    const abs = path.join(ROOT, rel);
    if (statSync(abs).isDirectory()) {
      if (name === 'node_modules' || name === 'vendor' || name === 'target') continue;
      walk(rel, out);
    } else if (/\.(js|mjs|html)$/.test(name)) {
      out.push(rel);
    }
  }
  return out;
}

const files = [...walk(CLIENT), ...walk('tools')].sort();
const problems = [];
const fired = new Set();
const inScope = (rule, file) =>
  rule.scope === 'repo' || (rule.scope === 'client' ? file.startsWith(CLIENT + '/') : file.startsWith('tools/'));

for (const file of files) {
  const text = readFileSync(path.join(ROOT, file), 'utf8');
  for (const rule of RULES) {
    if (!inScope(rule, file)) continue;
    const offence = rule.test(file, text);
    if (!offence) continue;
    const key = `${file}|${rule.id}`;
    fired.add(key);
    if (ALLOW[key]) continue;
    problems.push({ file, rule: rule.id, offence });
  }
}

/* a page naming a file that is not there */
for (const page of PAGES) {
  const text = readFileSync(path.join(ROOT, page), 'utf8');
  for (const m of text.matchAll(/(?:src|href)=["']([^"'#?]+)["']/g)) {
    const ref = m[1];
    if (/^(https?:)?\/\//.test(ref) || ref.startsWith('data:')) continue;
    const target = ref.startsWith('/') ? path.join(CLIENT, ref.slice(1)) : path.join(path.dirname(page), ref);
    try { statSync(path.join(ROOT, target)); } catch {
      problems.push({ file: page, rule: 'h', offence: `names ${ref}, which does not exist` });
    }
  }
}

/* an allowance whose offence has been fixed is a lie the next reader would believe */
const stale = Object.keys(ALLOW).filter((k) => !fired.has(k)).map((k) => {
  const [file, rule] = k.split('|');
  return { file, rule, why: ALLOW[k] };
});

if (process.argv.includes('--list')) {
  console.log(JSON.stringify({
    files: files.length,
    rules: RULES.map((r) => ({ id: r.id, scope: r.scope, why: r.why })),
    allow: Object.fromEntries(Object.entries(ALLOW).map(([k, v]) => [k, v])),
    problems,
    staleAllow: stale,
  }, null, 2));
} else if (problems.length || stale.length) {
  for (const p of problems) console.log(`FAIL ${p.file} [${p.rule}] ${p.offence}`);
  for (const s of stale) console.log(`STALE ${s.file} [${s.rule}] the offence is gone — delete the allowance: "${s.why}"`);
  console.log(`\n${problems.length} violation(s), ${stale.length} stale allowance(s), ${files.length} files checked`);
} else {
  console.log(`ok — ${RULES.length + 1} rules, ${files.length} files, ${Object.keys(ALLOW).length} allowances with reasons`);
}

process.exit(problems.length || stale.length ? 1 : 0);