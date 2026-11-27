// routes.test.mjs — the client's route table against the engine's router, in both directions.
//
//   node --test tests/unit
//
// The client may only name routes the engine mounts (`js/defaults/endpoints.js` is the one place a
// route string is allowed to live — tools/lint.mjs rule (a)), and the engine may only mount routes the
// client knows about OR that are declared server-only here with a reason. Both directions matter and
// only one of them fails loudly at runtime:
//
//   · a client route the server does not mount answers 404 — the pane shows nothing and the console
//     says nothing, so a test is the only place this shows up;
//   · a server route the client does not know is dead weight, and is allowed only when it is named
//     here — which is how a route that the interface stopped calling gets noticed rather than
//     lingering.
//
// The parser reads `crates/api/src/router.rs` as text. That is deliberate: a test that compiles Rust
// to ask about route strings would put cargo in the path of a node test. It is kept honest by
// counting `.route(` occurrences — if the parser misses one, that count disagrees and the test fails.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { ENDPOINTS } from '../../web-next/js/defaults/endpoints.js';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const ROUTER = readFileSync(path.join(ROOT, 'crates/api/src/router.rs'), 'utf8');

/**
 * Every route the axum router mounts, as `"METHOD /path"`.
 *
 * The argument list is found by counting parentheses, not by a regular expression that guesses where
 * it ends: `.route("/api/file", get(a).post(b))` closes on its own line, `.route("/api/build", post(c))`
 * is spread over four, and a lazy `[\s\S]*?` reads the first as the second and credits the route with
 * the next route's method.
 */
const routeEntries = () => {
  const out = [];
  let i = 0;
  while ((i = ROUTER.indexOf('.route(', i)) >= 0) {
    const start = i + '.route('.length;
    let depth = 1;
    let j = start;
    while (j < ROUTER.length && depth > 0) {
      const c = ROUTER[j];
      if (c === '(') depth += 1;
      else if (c === ')') depth -= 1;
      j += 1;
    }
    const args = ROUTER.slice(start, j - 1);
    const pathMatch = /^\s*"([^"]+)"/.exec(args);
    if (pathMatch) {
      const methods = [...args.matchAll(/\b(get|post|put|delete|patch)\s*\(/g)].map((m) => m[1].toUpperCase());
      out.push({ path: pathMatch[1], methods });
    }
    i = j;
  }
  return out;
};

const serverRoutes = () => {
  const out = new Set();
  for (const entry of routeEntries()) for (const meth of entry.methods) out.add(meth + ' ' + entry.path);
  return out;
};

/** Every route the client may send, as `"METHOD /path"`. A websocket is a GET upgrade. */
const clientRoutes = () => {
  const out = new Set();
  for (const e of ENDPOINTS) {
    const method = String(e.method || 'GET').toUpperCase();
    out.add((method === 'WS' ? 'GET' : method) + ' ' + e.path);
  }
  return out;
};

/**
 * Routes the engine mounts that no pane calls. Each needs a reason, because "nobody uses it" is how a
 * route rots: the reason is either a real second caller or a decision to keep it.
 */
const SERVER_ONLY = {
  'POST /api/rewrite': 'the rewrite engine is called by the CLI and by autocite; no pane sends to it',
};

test('the route table has one entry per route, and every entry is complete', () => {
  const seen = new Set();
  for (const e of ENDPOINTS) {
    assert.ok(e.name && e.path, `incomplete entry: ${JSON.stringify(e)}`);
    assert.match(e.path, /^\/(api|ws)\//, `${e.name} names a path outside the API: ${e.path}`);
    const key = String(e.method).toUpperCase() + ' ' + e.path;
    assert.equal(seen.has(key), false, `duplicate route ${key}`);
    seen.add(key);
    if (String(e.method).toUpperCase() === 'WS') assert.match(e.path, /^\/ws\//, `${e.name} is a ws route with a non-ws path`);
    else assert.match(e.path, /^\/api\//, `${e.name} is not a ws route, so its path should not be /ws/`);
  }
});

test('the parser found every .route( in the router', () => {
  const declared = (ROUTER.match(/\.route\(/g) || []).length;
  const entries = routeEntries();
  assert.equal(entries.length, declared, `router.rs declares ${declared} routes, the parser read ${entries.length}`);
  for (const e of entries) assert.ok(e.methods.length > 0, `${e.path} has no method the parser recognises`);
  assert.ok(serverRoutes().size >= declared, 'a route with two methods counts twice');
});

test('every route the client can name is mounted by the engine', () => {
  const server = serverRoutes();
  const missing = [...clientRoutes()].filter((r) => !server.has(r)).sort();
  assert.deepEqual(missing, [], `these would answer 404:\n  ${missing.join('\n  ')}`);
});

test('every route the engine mounts is named by the client, or declared server-only', () => {
  const client = clientRoutes();
  const unknown = [...serverRoutes()]
    .filter((r) => !client.has(r) && !SERVER_ONLY[r])
    .sort();
  assert.deepEqual(unknown, [], `the engine mounts routes nobody declared here:\n  ${unknown.join('\n  ')}`);
});

test('the server-only list carries a reason, and none of it is stale', () => {
  const server = serverRoutes();
  const client = clientRoutes();
  for (const [route, reason] of Object.entries(SERVER_ONLY)) {
    assert.ok(reason && reason.length > 10, `${route} needs a reason`);
    assert.equal(server.has(route), true, `${route} is listed as server-only but the engine does not mount it`);
    assert.equal(client.has(route), false, `${route} IS in the client table — drop the exemption`);
  }
});

test('the websocket routes are the three the engine upgrades', () => {
  const ws = ENDPOINTS.filter((e) => String(e.method).toUpperCase() === 'WS').map((e) => e.path).sort();
  assert.deepEqual(ws, ['/ws/build', '/ws/check', '/ws/slate']);
  assert.match(ROUTER, /"\/ws\/check"/);
  assert.match(ROUTER, /"\/ws\/slate"/);
  assert.match(ROUTER, /"\/ws\/build"/);
});