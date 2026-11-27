

/**
 * ENDPOINTS — the single source of truth for all route strings.
 *
 * Every HTTP route (/api/…) and WebSocket route (/ws/…) used by the UI
 * must be declared here. No other file under web-next/ may contain
 * a route string (enforced by tools/lint.mjs).
 *
 * Each entry:
 *   { name, method, path, ws?, request?(args), response?(raw), binary? }
 *
 * Every route here is MOUNTED by the engine (crates/api/src/router.rs). SIX of them answer with a
 * placeholder rather than an implementation, and a pane that calls one shows the server's own words:
 *
 *   handwrite.upload   `POST /api/handwrite` → `{ ok: false, error: "handwriting OCR not implemented
 *                      yet" }` — the route is real, the OCR is not
 *   ir.post            `POST /api/ir` echoes the buffer (`IR→HL emitter not yet implemented`)
 *   latex.post         `POST /api/latex` → `LaTeX typesetting not implemented yet`
 *   expand.post        `POST /api/expand` → `goal expansion not implemented yet`
 *   induce.post        `POST /api/induce` → `theorem induction not implemented yet`
 *   slate.post         `POST /api/slate` parses `.qbit`, so the Slate pane's Lean input is refused
 *                      with `unknown node header`; the pane's label promises Lean
 *
 * `POST /api/rewrite` is mounted and answers `{"lean_term": "by sorry", "ok": <goal parsed>}`; it is
 * not in this table because no pane calls it.
 *
 * `symbols.add` is NOT one of those: `POST /api/symbols` inserts a symbol row
 * (handlers_gap.rs::post_symbols, mounted at router.rs:21). It was marked a "backend gap" here long
 * after it had been implemented, and the label was believed and repeated. What that endpoint adds is a
 * DB-level symbol — tex, display, lean name, a fixity, scoped to the file's own path — while a symbol
 * AUTHORED in the corpus is a definition node whose `Type:` (primitive) or `Sema:` (composite) the
 * parser reads (crates/parse/src/node.rs, crates/build/src/axioms.rs). Two different things.
 *
 * A pane still shows the server's own words and keeps working when a route refuses
 * (tests/unit/routes.test.mjs holds the table against the engine's router, both directions).
 */
export const ENDPOINTS = [
  // Files
  { name: 'files.list', method: 'GET', path: '/api/files', response: (raw) => JSON.parse(raw) },
  { name: 'file.get', method: 'GET', path: '/api/file', response: (raw) => JSON.parse(raw) },
  { name: 'file.save', method: 'POST', path: '/api/file' },
  // Lean
  { name: 'lean.get', method: 'GET', path: '/api/lean', response: (raw) => JSON.parse(raw) },
  // A whole-corpus check. The report carries `verified`, `failures[]` and
  // `warnings[]` in the same diagnostic shape the CLI prints.
  { name: 'build.post', method: 'POST', path: '/api/build', response: (raw) => JSON.parse(raw) },
  // Slate
  { name: 'slate.post', method: 'POST', path: '/api/slate', response: (raw) => JSON.parse(raw) },
  // Symbols
  { name: 'symbols.get', method: 'GET', path: '/api/symbols', response: (raw) => JSON.parse(raw) },
  // mounted and implemented: a DB-level symbol (tex, display, lean, fixity), NOT a corpus definition
  { name: 'symbols.add', method: 'POST', path: '/api/symbols' },
  // Projects: a project is a directory carrying a charter, and the engine is one
  // executable that can serve several of them.
  { name: 'projects.list', method: 'GET', path: '/api/projects', response: (raw) => JSON.parse(raw) },
  { name: 'projects.add', method: 'POST', path: '/api/projects' },
  /* the other half of `projects.add`: a path the user wired in is remembered in the settings, so
     without this the interface can add a project it can never remove. Same method and the same
     body, because the two must agree about WHICH path they mean (`~` is expanded by the server
     for both — crates/api/src/handlers.rs::expand_home) */
  { name: 'projects.forget', method: 'POST', path: '/api/projects/forget' },
  { name: 'project.get', method: 'GET', path: '/api/project', response: (raw) => JSON.parse(raw) },
  { name: 'project.use', method: 'POST', path: '/api/project' },
  // Settings
  { name: 'settings.get', method: 'GET', path: '/api/settings' },
  { name: 'settings.save', method: 'POST', path: '/api/settings' },
  // Layout
  // Palette
  { name: 'palette.get', method: 'GET', path: '/api/palette', response: (raw) => JSON.parse(raw) },
  // Stats
  { name: 'stats.get', method: 'GET', path: '/api/stats', response: (raw) => JSON.parse(raw) },
  // Graph
  { name: 'graph.get', method: 'GET', path: '/api/graph', response: (raw) => JSON.parse(raw) },
  // Cite
  { name: 'cite.get', method: 'GET', path: '/api/cite', response: (raw) => JSON.parse(raw) },
  // Autocite
  { name: 'autocite.post', method: 'POST', path: '/api/autocite' },
  // The rewrites one cited equation reaches from an expression (the editor's suggestions)
  { name: 'steps.post', method: 'POST', path: '/api/steps', response: (raw) => JSON.parse(raw) },
  // Does each row of this buffer follow from the row above it? (the editor's live check)
  { name: 'rows.post', method: 'POST', path: '/api/rows', response: (raw) => JSON.parse(raw) },
  // Induce
  { name: 'induce.post', method: 'POST', path: '/api/induce' },
  // Expand
  { name: 'expand.post', method: 'POST', path: '/api/expand' },
  // Diagnostics
  { name: 'diagnostics.get', method: 'GET', path: '/api/diagnostics', response: (raw) => JSON.parse(raw) },
  // Citations
  { name: 'citations.get', method: 'GET', path: '/api/citations', response: (raw) => JSON.parse(raw) },
  // Parse
  { name: 'parse.post', method: 'POST', path: '/api/parse' },
  // Goal
  { name: 'goal.post', method: 'POST', path: '/api/goal' },
  // LaTeX
  { name: 'latex.post', method: 'POST', path: '/api/latex' },
  // AST
  { name: 'ast.post', method: 'POST', path: '/api/ast' },
  // IR
  { name: 'ir.get', method: 'GET', path: '/api/ir', response: (raw) => JSON.parse(raw) },
  { name: 'ir.post', method: 'POST', path: '/api/ir' },
  // HL
  { name: 'hl.post', method: 'POST', path: '/api/hl' },
  // PDF
  { name: 'pdf.get', method: 'GET', path: '/api/pdf', binary: true },
  // WebSockets
  { name: 'ws.check', method: 'WS', path: '/ws/check' },
  { name: 'ws.slate', method: 'WS', path: '/ws/slate' },
  { name: 'ws.build', method: 'WS', path: '/ws/build' },
  // Backend gap: handwriting
  // mounted; the OCR behind it is not: the handler answers "not implemented yet" (handlers_gap.rs)
  { name: 'handwrite.upload', method: 'POST', path: '/api/handwrite' },
];

/**
 * Register every endpoint in ENDPOINTS on the given registry.
 *
 * @param {object} registry — object with a `define(name, spec)` method.
 *   Provided by js/net/endpoint-node.js (createEndpointRegistry).
 */
/**
 * name -> path, for a client that builds its own requests.
 *
 * The single-file app (`web-next/index.html`) has no transport layer: it calls
 * `fetch` itself. It still must not WRITE a URL — that is how a route drifts from
 * the server — so it asks for this table once at boot and looks names up.
 *
 * @returns {Record<string, string>}
 */
export function routes() {
  const out = {};
  for (const spec of ENDPOINTS) out[spec.name] = spec.path;
  return out;
}

export function defineEndpoints(registry) {
  for (const spec of ENDPOINTS) {
    registry.define(spec.name, spec);
  }
}