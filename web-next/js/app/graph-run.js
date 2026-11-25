/*
 * app/graph-run.js — the node graph: what runs, when, and where its values live.
 *
 * Extracted from the inline script in web-next/index.html. A "program node" is a pane whose
 * body is user code; it declares input ports, runs its program when its inputs change, and
 * publishes outputs that any other node can be wired to. Three rules make that work, and
 * each one is a bug this file exists to prevent:
 *
 *   ONE CASCADE AT A TIME. Running a program can change a value, which runs the next
 *   program, which changes a value again — so the cascade re-enters itself. The guard is not
 *   an optimisation: without it a node whose output feeds its own input recurses until the
 *   tab dies, and the symptom (a frozen canvas) points at the camera rather than at the node.
 *   A second pass started while one is running is DROPPED, because the pass already running
 *   will read the newer value anyway.
 *
 *   THE HEARTBEAT IS ONE CLOCK. Every behaviour graph shares one tick: the clock advances,
 *   its value is published as a result, and then either the wired scheduler takes the tick
 *   (when one is armed, so things run upstream-first) or the plain cascade does. The old
 *   cascade stays the default: a workspace with no wired kinds pays nothing for the
 *   scheduler.
 *
 *   VALUES ARE NAMED, NOT GRABBED. A wire names its source (`editor_buffer`, `clock`,
 *   `node:…`), and this module resolves the name through a table the app supplies. That is
 *   what lets the same wire vocabulary work for the editor's ports, the clock, the theme,
 *   and the output of any node — and it is why `PORT_SRC` (a pane's own port registered
 *   under its instance id) is part of the table rather than a special case.
 */

import { resolveValue } from './wires.js';

/**
 * @param {object} host
 * @param {(name:string) => any} host.valueOf     a named value: editor_buffer, clock, theme…
 * @param {(id:string, field?:string) => any} [host.nodeValue]  a node's published output
 * @param {(fn:Function) => Function} [host.setInterval]  the heartbeat's scheduler (injected)
 * @param {(fn:Function) => void} [host.clearInterval]
 * @param {number} [host.tickMs]
 */
export function createGraphRun(host) {
  const setIntervalFn = host.setInterval || ((fn, ms) => setInterval(fn, ms));
  const clearIntervalFn = host.clearInterval || ((h) => clearInterval(h));
  const nodeValue = host.nodeValue || ((id, field) => {
    const r = results[id];
    return field ? (r == null ? undefined : r[field]) : r;
  });

  /** The graph's own switch, tick, and manual step. */
  const run = { running: true, tick: 0, step: () => advance(true) };
  const runs = {};                  // id -> the node's run function, for the cascade
  const results = {};               // id -> the node's published outputs
  let scheduler = null;             // armed by the wire host once a wired kind exists
  let inCascade = false;
  let handle = null;

  /** Publish a node's outputs: the one place a result is written. */
  function out(rec, outs) {
    if (!rec || !rec.id) return undefined;
    results[rec.id] = outs;
    return outs;
  }

  /** What a wire from `from` currently carries.
   *
   * The app's own table is asked FIRST, and that order is the point: a pane's port is
   * registered under its instance id (`t1:on`, `t1:stats_theme`), and an instance id is not
   * the shape `resolveValue` parses — a colon is not a character it allows — so two switches
   * in one workspace would both read `undefined` and look broken rather than unconnected. */
  function valueOf(from) {
    if (!from) return undefined;
    const own = host.valueOf(String(from));
    if (own !== undefined) return own;
    return resolveValue(from, {
      value: (name) => host.valueOf(name),
      nodeValue: (id, field) => nodeValue(id, field),
    });
  }

  /** One pass over every node that has a run function. */
  function cascade() {
    if (inCascade) return false;      // a second pass would fight the first for the same values
    inCascade = true;
    try {
      for (const id in runs) if (runs[id]) runs[id]();
    } finally {
      inCascade = false;
    }
    return true;
  }

  /** The heartbeat: advance the clock, then tick. `force` runs even when paused. */
  function advance(force) {
    if (!run.running && !force) return 0;
    const hasClock = Object.prototype.hasOwnProperty.call(results, 'clock');
    if (hasClock) {
      run.tick++;
      out({ id: 'clock' }, { tick: run.tick });
    }
    if (scheduler) return scheduler.step(1);
    if (hasClock) cascade();
    return 0;
  }

  /** Arm (or disarm) the wired scheduler: with one, the cascade is not used at all. */
  function setScheduler(next) {
    scheduler = next || null;
    return scheduler;
  }

  const schedulerOf = () => scheduler;

  /** Start the heartbeat. Idempotent, so a re-boot does not leave two clocks running. */
  function start(ms = host.tickMs || 1000) {
    if (handle) return handle;
    handle = setIntervalFn(() => advance(false), ms);
    return handle;
  }

  function stop() {
    if (handle) clearIntervalFn(handle);
    handle = null;
  }

  return {
    run, runs, results,
    out, valueOf, cascade, advance, start, stop,
    setScheduler, scheduler: schedulerOf,
    running: () => run.running,
    setRunning: (v) => { run.running = !!v; return run.running; },
    /** Register a node's run function; returns the remover. */
    register: (id, fn) => {
      runs[id] = fn;
      return () => { if (runs[id] === fn) delete runs[id]; };
    },
  };
}