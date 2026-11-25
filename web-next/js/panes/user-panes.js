/*
 * user-panes.js — panes the workspace declares, rather than the app.
 *
 * A pane does not have to be a module in this repository. `web-next/panes/manifests.json`
 * declares kinds as DATA — a title, some ports, and a body that is either a snippet of HTML
 * or a snippet of code. This file turns one of those declarations into something the canvas
 * can mount, which is the whole point of a pane registry that a user can extend: a workspace
 * can add a tile kind without a build step.
 *
 * A manifest body comes in two shapes and both are useless without care:
 *
 *   html   static markup. Nothing to run, nothing to break.
 *   code   a function body, compiled with `new Function`. USER CODE — so it is compiled
 *          once, at registration, inside a try: a typo becomes a tile that says what is
 *          wrong instead of an exception during a re-render, and it cannot take the rest of
 *          the workspace down with it.
 *
 * A behaviour node is the other half: a program that takes named inputs and returns named
 * outputs. The inputs come from two places and the second wins — the statically declared
 * `from` (an editor port, say) and whatever arrived on a WIRE, which is what makes a drawn
 * wire actually drive the program instead of only being drawn. An error inside the program
 * is a RESULT (`__error`), not a throw: the tile shows it, the cascade carries on.
 */

/** Compile a manifest's `render` descriptor into a mount function, or null. */
export function mountOf(render, ctx, panes) {
  if (!render) return null;
  if (render.type === 'builtin') {
    const ref = panes && panes[render.ref];
    return (ref && ref.mount) || null;
  }
  if (render.type === 'html') {
    return (p) => { p.body.innerHTML = render.view || ''; };
  }
  if (render.type === 'code') {
    try {
      const fn = new Function('p', 'ctx', 'return (' + render.code + ')');
      return (p) => fn(p, ctx);
    } catch (e) {
      // compiled once, so the failure is reported once — and the tile says so for ever
      return (p) => {
        p.body.innerHTML = '<div class="empty">bad pane code: ' + String(e.message).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])) + '</div>';
      };
    }
  }
  return null;
}

/**
 * The mount for a behaviour node: run its program, show the result, and cascade.
 *
 * @param {object} rec   the pane's registry record (its `program`, `inputs`, `id`, `last`)
 * @param {object} deps  { ctx, resolveFrom, sources, schedule, results, cascade, now }
 */
export function behaviourMount(rec, deps) {
  const { ctx, resolveFrom, sources = {}, schedule, cascade } = deps;
  const debounce = schedule || ((fn) => fn);

  return function (p) {
    p.body.innerHTML = '';
    const wrap = document.createElement('div');
    wrap.className = 'bnode';
    const prog = document.createElement('textarea');
    prog.className = 'bprog';
    prog.spellcheck = false;
    prog.value = rec.program || 'function(inputs, ctx){\n  return { result: (inputs.content || "").length };\n}';
    const row = document.createElement('div');
    row.className = 'brow';
    const btn = document.createElement('button');
    btn.textContent = '▶ run';
    btn.className = 'brun';
    const src = document.createElement('span');
    src.className = 'bsrc';
    src.textContent = (rec.inputs || []).map((i) => i.name + ' ← ' + i.from).join(', ') + ' · ' + rec.category;
    row.append(src, btn);
    const out = document.createElement('pre');
    out.className = 'code bout';
    out.textContent = '{}';
    wrap.append(prog, row, out);
    p.body.appendChild(wrap);

    /* `extra` carries values that arrived on a WIRE, which win over the statically
       declared `from` — that is what makes a dragged wire drive the program. */
    function runProgram(extra) {
      const inputs = {};
      for (const inp of (rec.inputs || [])) inputs[inp.name] = resolveFrom ? resolveFrom(inp.from) : undefined;
      if (extra) for (const k in extra) if (extra[k] !== undefined) inputs[k] = extra[k];
      let res;
      try {
        const fn = new Function('return (' + prog.value + ')')();
        res = fn(inputs, ctx) || {};
      } catch (e) {
        // an error is a result the tile shows, not an exception that stops the cascade
        res = { __error: e.message };
      }
      res.__at = Date.now();
      rec.last = res;
      if (deps.onResult) deps.onResult(rec, res);
      out.textContent = JSON.stringify(res, null, 2);
      p.summary = () => rec.id + ' · ' + (Object.keys(res).filter((k) => k[0] !== '_').length) + ' out';
      if (p._summary && p.card && p.card.lastChild) p.card.lastChild.textContent = p.summary();
      if (deps.registerRun) deps.registerRun(rec.id, runProgram);
      return res;
    }

    // the scheduler drives wired ticks; the push cascade serves panes whose inputs
    // come from a port subscription
    p.runProgram = runProgram;
    p.tickRun = (ins) => runProgram(ins);
    const runAndCascade = () => { runProgram(); if (cascade) cascade(); };
    for (const inp of (rec.inputs || [])) {
      const s = sources[inp.from];
      if (s && typeof s.on === 'function') s.on(debounce(runAndCascade, 250));
    }
    if (deps.registerRun) deps.registerRun(rec.id, runProgram);
    btn.addEventListener('click', runAndCascade);
    prog.addEventListener('keydown', (e) => { if (e.key === 'Enter' && e.ctrlKey) runAndCascade(); });
    prog.addEventListener('focus', () => { prog.select(); });
    runProgram();
    setTimeout(() => { if (cascade) cascade(); }, 0);
  };
}

/**
 * Read the workspace's pane manifests. They are OPTIONAL: a workspace without the file is a
 * workspace with the app's own kinds, which is the normal case.
 */
export async function loadManifests(fetcher, url = '/panes/manifests.json') {
  try {
    const r = await fetcher(url);
    if (!r.ok) return [];
    const list = await r.json();
    return (list || []).filter((m) => m && m.id);
  } catch (e) {
    return [];
  }
}

/** The port definitions a manifest declares, in the registry's shape. */
export function portsOf(manifest) {
  const norm = (list) => (list || []).map((x) => (Array.isArray(x) ? { name: x[0], type: x[1] } : x));
  return { inputs: norm(manifest.inputs), outputs: norm(manifest.outputs) };
}