// check.mjs — everything this repo can check about itself, in one command.
//
//   node tools/check.mjs [baseUrl]      # baseUrl defaults to http://127.0.0.1:3123
//
// What runs, in order, and why each one is here:
//
//   1. `tools/lint.mjs` — the client's rules (one endpoint table, one frame governor, one storage
//      adapter, ctx over the document, no frameworks, a file-size ceiling, tools stay inside the repo).
//   2. `node --test tests/unit` — the arithmetic and the contracts: camera, tiling, layout trees,
//      wires, and the client's routes against the engine's router. No browser, no server, no network.
//   3. the browser gates — `tools/verify-default-view.mjs` (the shipped arrangement, the first view and
//      "default view") and `tools/shot.mjs` (the engine host still paints). These need a server, so
//      they run only when one answers; when it does not, that is REPORTED and the run fails, because a
//      gate that silently skips is a gate that has never failed.
//
// Rust is not here: `cargo test --workspace` is its own command and its own toolchain, and folding it
// in would make the fast loop slower than the thing it is guarding.
import { spawn } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const base = (process.argv[2] || 'http://127.0.0.1:3123').replace(/\/$/, '');

function run(cmd, args) {
  return new Promise((resolve) => {
    const started = Date.now();
    const child = spawn(cmd, args, { cwd: ROOT, stdio: ['ignore', 'pipe', 'pipe'] });
    let out = '';
    child.stdout.on('data', (d) => { out += d; });
    child.stderr.on('data', (d) => { out += d; });
    child.on('close', (code) => resolve({ code, out, ms: Date.now() - started }));
  });
}

const results = [];
const step = async (name, cmd, args, { quiet } = {}) => {
  const r = await run(cmd, args);
  results.push({ name, ok: r.code === 0, ms: r.ms, out: r.out });
  const mark = r.code === 0 ? 'ok  ' : 'FAIL';
  console.log(`${mark} ${name} (${(r.ms / 1000).toFixed(1)} s)`);
  if (r.code !== 0 || !quiet) {
    /* Only the tail: a failing test run is long, and the last lines are the ones that say why. */
    const lines = r.out.trim().split('\n');
    const tail = r.code === 0 ? lines.slice(-1) : lines.slice(-25);
    for (const line of tail) console.log('     ' + line);
  }
  return r.code === 0;
};

/** A server that answers, or null. Asked once, before the gates, so the report can say which. */
async function serverThere() {
  try {
    const r = await fetch(base + '/', { signal: AbortSignal.timeout(2000) });
    return r.ok;
  } catch { return false; }
}

await step('lint — the client keeps its rules', process.execPath, ['tools/lint.mjs'], { quiet: true });
await step('unit — arithmetic, contracts, routes', process.execPath, ['--test', 'tests/unit/*.test.mjs'], { quiet: true });

const gates = [
  ['gate — the shipped arrangement and "default view"', 'tools/verify-default-view.mjs', ['/tmp/check-default-view']],
  ['gate — the engine host still paints', 'tools/shot.mjs', ['/shell.html', '/tmp/check-shell.png']],
];
let gatesOk = true;
if (await serverThere()) {
  for (const [name, script, args] of gates) {
    const cmd = script === 'tools/shot.mjs' ? [base + args[0], ...args.slice(1)] : [base, ...args];
    gatesOk = (await step(name, process.execPath, [script, ...cmd], { quiet: true })) && gatesOk;
  }
} else {
  gatesOk = false;
  results.push({ name: 'gates — no server', ok: false, ms: 0, out: '' });
  console.log(`FAIL gates — nothing answered at ${base}; start qubitick-server and run again`);
}

const failed = results.filter((r) => !r.ok);
console.log('');
if (failed.length) {
  console.log(`${failed.length} of ${results.length} steps failed: ${failed.map((f) => f.name).join('; ')}`);
  process.exit(1);
}
console.log(`all ${results.length} steps passed (${(results.reduce((a, r) => a + r.ms, 0) / 1000).toFixed(1)} s)`);