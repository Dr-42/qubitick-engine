/*
 * host/canvas/net-app.js — the server-facing app: routes, projects, and the corpus check.
 *
 * Extracted from the inline script in web-next/index.html. The rules are modules already
 * (`js/net/routes.js` for the table and its wait, `js/app/diagnostics.js` for the check); what
 * was in the page was the app's own side of them, and two of those are worth naming:
 *
 *   THE ROUTE TABLE IS WAITED FOR, NOT ASSUMED. `ROUTES_READY` is the promise the two loaders
 *   that talk to the server before the first paint await — the corpus and the workspace. Asking
 *   a route before it resolves is a request to a URL that does not exist yet.
 *
 *   A CHECK IS NEVER STARTED BY LOOKING AT A TILE. The report tile and the rail's chip both READ
 *   the stored report; the chip and the tile's button start a run. A panel that begins a
 *   minute-long build because you glanced at it is a trap, and this is where that promise lives.
 *
 * A project is a directory carrying a charter: switching one asks the server and reloads into it,
 * and a path the server refuses is refused with the server's own words — the client does not
 * re-derive the rule.
 */

import { createRoutes } from '../../net/routes.js';
/* THE ENDPOINT NODES A PANE MAY CALL, from the shell's own module and the shell's own route table
   (js/defaults/endpoints.js::defineEndpoints). A pane written for the engine host asks its workspace
   facade for one — `ctx.ws.endpoint('slate.post').call.invoke.invoke({content})` in
   js/panes/slate.js — and that facade is the shell's, so the canvas answers the one capability such a
   pane actually uses rather than re-implementing the call. Same table, same node shape: not a second
   way to reach a route. */
import { createEndpointRegistry } from '../../net/endpoint-node.js';
import { defineEndpoints } from '../../defaults/endpoints.js';
import { createProjectServices } from '../../app/project-services.js';
import { createNodeServices } from '../../app/node-services.js';
import { saveModeOf } from '../../app/prefs.js';
import { createDiagnostics, diagPath } from '../../app/diagnostics.js';

/**
 * @param {object} app { fetch(u,init), toast(m), paintChip(), paintDiag(), panes(), reload(), saveMode() }
 */
export function createNet(app) {
  const saveMode = app.saveMode;
  const ROUTES = createRoutes();
  const url = (name) => ROUTES.url(name);

  /* One registry for the page (not per pane): an endpoint node is stateless, and a pane that calls
     one twice must reach the same route object both times. */
  const ENDPOINTS = createEndpointRegistry({
    fetch: (u, init) => app.fetch(u, init),
    WebSocket: typeof WebSocket !== 'undefined' ? WebSocket : null,
    location: typeof window !== 'undefined' ? window.location : undefined,
  });
  defineEndpoints(ENDPOINTS);
  const endpoint = (name) => ENDPOINTS.endpoint(name);
  /* THE PROJECT LIST HAS ONE OWNER, and it is `js/app/project-services.js` — the canvas had it here
     and the engine host had no copy at all, which is why a projects tile opened in `shell.html` drew
     the empty state. What this host supplies is what only it can: the route names, its own fetch, a
     toast, and a reload after a switch (the engine re-ingests the corpus, so the page starts again on
     the new project rather than half-reloading it). */
  const PROJECT_SERVICES = createProjectServices({
    url: (name) => url(name),
    fetch: (u, init) => app.fetch(u, init),
    toast: (m) => app.toast(m),
    onSwitched: () => app.reload(),
  });

  /* MAKING A NODE IS THE APP'S TOO, for the same reason: the New Node pane writes a corpus file, and
     it did it through the engine host's socket, so the canvas could not mount it at all. */
  const NODE_SERVICES = createNodeServices({
    url: (name) => url(name),
    fetch: (u, init) => app.fetch(u, init),
    toast: (m) => app.toast(m),
    /* A CREATED NODE IS A WRITE, and it runs the same follow-up an editor's save does
       (js/app/save-mode.js): 'rebuild project on save' is the mode that makes a new node and its symbol
       land together, because a brand-new file has no node row for the injection to hang a symbol on. */
    // asked per write, through the dep the app hands this layer — `app` here is the deps object
    mode: () => (typeof saveMode === 'function' ? saveMode() : 'none'),
    runCheck: () => CHECKS.check(),
    rows: (content) => rows(content),
  });
  const createNode = (node) => NODE_SERVICES.create(node);

  const loadProjects = () => PROJECT_SERVICES.load();
  const useProject = (id) => PROJECT_SERVICES.use(id);
  const addProject = (path) => PROJECT_SERVICES.add(path);
  const forgetProject = (path) => PROJECT_SERVICES.forget(path);
  /** Follow the project document (a switch made anywhere, a path wired in, a path forgotten). */
  const subscribeProjects = (fn) => PROJECT_SERVICES.subscribe(fn);


  /**
   * `POST /api/autocite` — the chain of cited equations that bridges two expressions.
   *
   * The editor asks for this BY NAME (`app.autocite`): what a proof row needs is the citation,
   * and a pane that assembled the URL itself would be a second place a route string lives.
   */
  const autocite = async (lhs, rhs) => {
    const r = await app.fetch(url('autocite.post'), {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ lhs, rhs }),
    });
    if (!r.ok) throw new Error('HTTP ' + r.status);
    return r.json();
  };

  /**
   * `GET /api/cite?name=` — follow a citation (`A1.4`, `T.BT.01`) to the node it names.
   *
   * The editor's citation links ask by name: a reference in a proof is a link, and what a link
   * needs is where it points (`js/live/editor.js` decorates the tags, the tile opens the target).
   */
  const cite = async (name) => {
    const r = await app.fetch(url('cite.get') + '?name=' + encodeURIComponent(name));
    if (!r.ok) throw new Error('HTTP ' + r.status);
    return r.json();
  };

  /**
   * `POST /api/rows` — does each row of this buffer follow from the row above it?
   *
   * The editor's check strip asks by name (`app.rows(buffer)`), like every other question a pane
   * has: a pane that assembled the URL itself would be a second place a route string lives.
   */
  const rows = async (content) => {
    const r = await app.fetch(url('rows.post'), {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ content }),
    });
    if (!r.ok) throw new Error('HTTP ' + r.status);
    return r.json();
  };

  /** `POST /api/steps` — the rewrites one cited equation reaches from an expression. */
  const steps = async (expr, limit) => {
    const r = await app.fetch(url('steps.post'), {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ expr, limit }),
    });
    if (!r.ok) throw new Error('HTTP ' + r.status);
    return r.json();
  };

  /* the check: one module owns the run, the rows and the words; the app paints them */
  const CHECKS = createDiagnostics({
    url: (name) => url(name),
    fetch: (u, init) => app.fetch(u, init),
    paint: () => { app.paintChip(); app.paintDiag(); },
    toast: (m) => app.toast(m),
  });

  /** The two loaders that talk to the server before the first paint await this. */
  const ROUTES_READY = ROUTES.load();

  return {
    ROUTES, url, ROUTES_READY, endpoint, autocite, rows, steps, cite,
    projects: () => PROJECT_SERVICES.get(),
    projectKey: () => PROJECT_SERVICES.key(), loadProjects, useProject, addProject, forgetProject,
    createNode,
    subscribeProjects,
    CHECKS,
    DIAG: CHECKS.state,
    DIAG_PATH: diagPath,
    diagCounts: () => CHECKS.counts(),
    diagRows: () => CHECKS.rows(),
    runCheck: () => CHECKS.check(),
    chip: () => CHECKS.chip(),
  };}
