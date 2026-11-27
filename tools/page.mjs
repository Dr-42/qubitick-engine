// page.mjs — a headless browser for the gates, with no dependencies.
//
//   import { launch } from './page.mjs';
//   const browser = await launch();
//   const page = await browser.newPage({ viewport: { width: 1500, height: 950 } });
//   page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
//   page.on('pageerror', (e) => errors.push(e.message));
//   await page.goto(url, { waitUntil: 'networkidle' });
//   const z = await page.evaluate(() => window.__spike.cam.z);
//   await page.screenshot({ path: '/tmp/canvas.png' });
//   await browser.close();
//
// WHY THIS EXISTS. The browser is the only thing that can settle "the page loads and paints what
// it says", and a test runner that needs an npm install, a config file and a 1 GB browser cache to
// ask that question is a dependency the engine does not need. Chromium speaks the DevTools protocol
// over a websocket, and Node 22 has `WebSocket` and `fetch` built in, so the whole runner is this
// file: spawn the system browser headless, attach one target, forward the events that matter.
//
// THE SURFACE IS DELIBERATELY PLAYWRIGHT-SHAPED — `goto`, `evaluate`, `screenshot`, `mouse`,
// `waitForTimeout`, `on('console'|'pageerror')` — so a gate written against it reads like every
// other gate and stays readable to someone who has never opened this file. It is not a compatibility
// layer and does not try to be: there is no locator API, no auto-waiting, no retries. A check that
// needs polling says so with `waitForFunction`.
//
// THE BROWSER IS THE MACHINE'S. `QUBITICK_CHROME` or `/usr/bin/chromium`; a browser that is not
// installed is a loud error at launch, not a silent skip, because a gate that quietly does nothing
// still prints "pass".

import { spawn } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';

const CHROME = process.env.QUBITICK_CHROME || '/usr/bin/chromium';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/* ---------------- the connection ---------------- */

/**
 * One websocket to the browser, and a router over the replies.
 *
 * Every command carries an `id` and every reply repeats it, so the router is a map from id to the
 * promise that is waiting. Events have no id: they are addressed by `sessionId` (one per page) plus
 * method name, and the page registers the handful it cares about.
 */
function connect(wsUrl) {
  const ws = new WebSocket(wsUrl);
  const pending = new Map();
  const handlers = new Map(); // `${sessionId||''} ${method}` -> Set<fn>
  let nextId = 0;
  const open = new Promise((resolve, reject) => {
    ws.addEventListener('open', resolve, { once: true });
    ws.addEventListener('error', () => reject(new Error('websocket to ' + wsUrl + ' failed')), { once: true });
  });
  ws.addEventListener('message', (ev) => {
    let m;
    try { m = JSON.parse(ev.data); } catch { return; }
    if (m.id && pending.has(m.id)) {
      const { resolve, reject, method } = pending.get(m.id);
      pending.delete(m.id);
      if (m.error) reject(new Error(method + ': ' + m.error.message));
      else resolve(m.result);
      return;
    }
    for (const fn of handlers.get((m.sessionId || '') + ' ' + m.method) || []) fn(m.params);
  });
  const send = (method, params = {}, sessionId) => {
    const id = ++nextId;
    return new Promise((resolve, reject) => {
      pending.set(id, { resolve, reject, method });
      ws.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
    });
  };
  /** Listen for one event on a session ('' = browser-level). */
  const on = (sessionId, method, fn) => {
    const key = sessionId + ' ' + method;
    if (!handlers.has(key)) handlers.set(key, new Set());
    handlers.get(key).add(fn);
    return () => handlers.get(key).delete(fn);
  };
  return { open, send, on, close: () => ws.close() };
}

/** The websocket URL of a browser, once it has one. Chromium writes the port it chose to
 *  `<user-data-dir>/DevToolsActivePort` (first line), which is how a launch can stay race-free. */
async function devtoolsPort(dir) {
  const file = path.join(dir, 'DevToolsActivePort');
  for (let i = 0; i < 120; i += 1) {
    try {
      const port = Number(readFileSync(file, 'utf8').split('\n')[0].trim());
      if (port > 0) return port;
    } catch { /* not written yet */ }
    await sleep(100);
  }
  throw new Error('chromium did not open a devtools port (is ' + CHROME + ' present?)');
}

/* ---------------- the browser ---------------- */

/**
 * Start a headless browser and return `{ newPage, close }`.
 *
 * Each launch gets its own user-data-dir under the temp dir, so two gates running at once cannot
 * share a singleton profile (the second chromium would attach to the first and report its tabs).
 * The directory is removed on close; a crashed run leaves it behind, which is why it is named.
 */
export async function launch({ chrome = CHROME, args = [], timeout = 30000 } = {}) {
  const dir = mkdtempSync(path.join(tmpdir(), 'qubitick-chrome-'));
  const proc = spawn(chrome, [
    '--headless=new',
    '--no-sandbox',
    '--disable-gpu',
    '--disable-dev-shm-usage',
    '--disable-extensions',
    '--no-first-run',
    '--no-default-browser-check',
    '--hide-scrollbars',
    '--mute-audio',
    '--remote-debugging-port=0',
    '--user-data-dir=' + dir,
    ...args,
    'about:blank',
  ], { stdio: 'ignore' });
  let exited = null;
  proc.on('exit', (code, sig) => { exited = { code, sig }; });

  const port = await devtoolsPort(dir).catch((e) => { proc.kill('SIGKILL'); throw e; });
  let wsUrl;
  for (let i = 0; i < 100 && !wsUrl; i += 1) {
    try {
      const r = await fetch(`http://127.0.0.1:${port}/json/version`);
      wsUrl = (await r.json()).webSocketDebuggerUrl;
    } catch { await sleep(100); }
  }
  if (!wsUrl) {
    proc.kill('SIGKILL');
    throw new Error('chromium opened port ' + port + ' but never announced a browser target');
  }

  const conn = connect(wsUrl);
  await conn.open;

  let closed = false;
  const close = async () => {
    if (closed) return;
    closed = true;
    try { conn.close(); } catch { /* already gone */ }
    if (!exited) {
      proc.kill('SIGTERM');
      for (let i = 0; i < 20 && !exited; i += 1) await sleep(50);
      if (!exited) proc.kill('SIGKILL');
    }
    try { rmSync(dir, { recursive: true, force: true }); } catch { /* best effort */ }
  };

  const newPage = async ({ viewport = { width: 1400, height: 900 } } = {}) => {
    const { targetId } = await conn.send('Target.createTarget', { url: 'about:blank' });
    const { sessionId } = await conn.send('Target.attachToTarget', { targetId, flatten: true });
    const page = makePage({ conn, sessionId, targetId, viewport, timeout });
    await page.init();
    return page;
  };

  return { newPage, close, port, proc };
}

/* ---------------- the page ---------------- */

function makePage({ conn, sessionId, targetId, viewport, timeout }) {
  const send = (method, params) => conn.send(method, params, sessionId);
  const consoleHandlers = new Set();
  const pageErrorHandlers = new Set();
  const requestFailedHandlers = new Set();

  /* Network accounting for `waitUntil: 'networkidle'`: a request is in flight between
     `Network.requestWillBeSent` and the matching finish/fail. The quiet test is both "nothing in
     flight" and "nothing has moved for `quiet` ms", because this app fetches twice on boot (routes,
     then the corpus) and a naive "zero in flight right now" can pass in the gap between them. */
  const inflight = new Set();
  let lastActivity = Date.now();

  const page = {
    viewport,
    /** A page is closed by the browser at the end of every check; this makes that explicit. */
    async close() { await conn.send('Target.closeTarget', { targetId }).catch(() => {}); },

    on(event, fn) {
      if (event === 'console') consoleHandlers.add(fn);
      else if (event === 'pageerror') pageErrorHandlers.add(fn);
      else if (event === 'requestfailed') requestFailedHandlers.add(fn);
      else throw new Error('page.mjs does not forward the "' + event + '" event');
      return page;
    },

    async init() {
      await send('Page.enable');
      await send('Runtime.enable');
      await send('Network.enable');
      await send('Log.enable');
      await send('Emulation.setDeviceMetricsOverride', {
        width: viewport.width, height: viewport.height, deviceScaleFactor: 1, mobile: false,
      });
      conn.on(sessionId, 'Runtime.consoleAPICalled', (p) => {
        const text = (p.args || []).map((a) => (a.value !== undefined ? String(a.value) : a.description || a.type)).join(' ');
        /* `type` and `text` are FUNCTIONS, as Playwright's are: a gate reads `m.type() === 'error'`
           and would otherwise have to know which of the two browsers it is talking to. */
        for (const fn of consoleHandlers) fn({ type: () => p.type, text: () => text });
      });
      conn.on(sessionId, 'Runtime.exceptionThrown', (p) => {
        const d = p.exceptionDetails || {};
        const text = (d.exception && (d.exception.description || d.exception.value)) || d.text || 'exception';
        for (const fn of pageErrorHandlers) fn({ message: String(text) });
      });
      conn.on(sessionId, 'Network.requestWillBeSent', (p) => { inflight.add(p.requestId); lastActivity = Date.now(); });
      const done = (p) => { inflight.delete(p.requestId); lastActivity = Date.now(); };
      conn.on(sessionId, 'Network.loadingFinished', done);
      conn.on(sessionId, 'Network.loadingFailed', (p) => {
        done(p);
        for (const fn of requestFailedHandlers) fn({ url: p.requestId, error: p.errorText });
      });
      /* A 404 or a 500 is not a console error, and a gate that only watches the console reports a
         page with a broken stylesheet as clean. Surface HTTP failures as page errors — except the
         favicon, which the BROWSER asks for on its own: the document cannot request it, cannot
         answer it and never sees the failure, so counting it would put a permanent red mark on a
         page that is fine. */
      const isFavicon = (url) => /\/favicon\.ico(\?|$)/.test(url || '');
      conn.on(sessionId, 'Network.responseReceived', (p) => {
        const r = p.response || {};
        if (r.status >= 400 && !isFavicon(r.url)) {
          for (const fn of pageErrorHandlers) fn({ message: `http ${r.status} ${r.url}` });
        }
      });
      conn.on(sessionId, 'Log.entryAdded', (p) => {
        const e = p.entry || {};
        if (e.level === 'error' && !isFavicon(e.url)) {
          for (const fn of consoleHandlers) fn({ type: () => 'error', text: () => e.text + (e.url ? ' (' + e.url + ')' : '') });
        }
      });
    },

    async goto(url, { waitUntil = 'networkidle', timeout: ms = timeout } = {}) {
      const deadline = Date.now() + ms;
      const loaded = new Promise((resolve) => conn.on(sessionId, 'Page.loadEventFired', resolve));
      const nav = await send('Page.navigate', { url });
      if (nav.errorText) throw new Error('navigate to ' + url + ': ' + nav.errorText);
      await Promise.race([loaded, sleep(Math.max(1, deadline - Date.now()))]);
      if (waitUntil === 'load' || waitUntil === 'domcontentloaded') return;
      await page.waitForNetworkIdle({ timeout: Math.max(1, deadline - Date.now()) });
    },

    /** Resolve when nothing has been in flight for `quiet` ms. */
    async waitForNetworkIdle({ quiet = 500, timeout: ms = timeout } = {}) {
      const deadline = Date.now() + ms;
      for (;;) {
        if (inflight.size === 0 && Date.now() - lastActivity >= quiet) return;
        if (Date.now() > deadline) return; // a page that never goes quiet is the caller's problem to assert
        await sleep(50);
      }
    },

    waitForTimeout: (ms) => sleep(ms),

    /** Poll an in-page predicate until it returns truthy, or throw at the deadline. */
    async waitForFunction(fn, { timeout: ms = timeout, polling = 100 } = {}) {
      const deadline = Date.now() + ms;
      for (;;) {
        if (await page.evaluate(fn)) return true;
        if (Date.now() > deadline) throw new Error('waitForFunction timed out after ' + ms + ' ms: ' + fn.toString().slice(0, 120));
        await sleep(polling);
      }
    },

    /**
     * Run a function in the page and bring its return value back.
     *
     * The value crosses as JSON: a function that touches a DOM node and returns the node gives the
     * caller `undefined` rather than a lie, which is what `returnByValue` would do anyway.
     */
    async evaluate(fn, arg) {
      const expression = typeof fn === 'function'
        ? '(' + fn.toString() + ')(...' + JSON.stringify(arg === undefined ? [] : [arg]) + ')'
        : String(fn);
      const r = await send('Runtime.evaluate', {
        expression,
        returnByValue: true,
        awaitPromise: true,
        userGesture: true,
      });
      if (r.exceptionDetails) {
        const d = r.exceptionDetails;
        const text = (d.exception && (d.exception.description || d.exception.value)) || d.text || 'evaluate threw';
        throw new Error('page.evaluate: ' + text);
      }
      return r.result && r.result.value;
    },

    async title() { return page.evaluate(() => document.title); },

    /** Write a PNG, making the directory if it is not there yet (a gate takes an output dir and
     *  should not fail because nobody made it). `fullPage` is not supported: the viewport is enough. */
    async screenshot({ path: out }) {
      const shot = await send('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false });
      if (out) {
        mkdirSync(path.dirname(path.resolve(out)), { recursive: true });
        writeFileSync(out, Buffer.from(shot.data, 'base64'));
      }
      return Buffer.from(shot.data, 'base64');
    },

    /**
     * Real input events, because a drag only proves the wiring if it arrives as a drag.
     *
     * A point left out means "where the pointer already is" — a press at the last move position is
     * what a hand does, and a gate that says `move(x, y); down();` reads like the gesture it is.
     */
    mouse: {
      at: { x: 0, y: 0 },
      async move(x, y) {
        page.mouse.at = { x, y };
        await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y, button: 'none', buttons: 0 });
      },
      async down(x = page.mouse.at.x, y = page.mouse.at.y, { button = 'left' } = {}) {
        page.mouse.at = { x, y };
        await send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button, buttons: 1, clickCount: 1 });
      },
      async up(x = page.mouse.at.x, y = page.mouse.at.y, { button = 'left' } = {}) {
        page.mouse.at = { x, y };
        await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button, buttons: 0, clickCount: 1 });
      },
      async click(x, y) {
        await page.mouse.move(x, y);
        await page.mouse.down(x, y);
        await page.mouse.up(x, y);
      },
    },
  };
  return page;
}