// verify-default-view.mjs — the arrangement the workspace ships with, and the button that restores it.
//
//   node tools/verify-default-view.mjs [baseUrl] [outDir]
//
// The claims, in the order the page has to keep them:
//
//   1. the canvas boots the shipped arrangement: root c0 (tree, editor, render, goal), root c1
//      (graph, symbols, stats), root c2 (the console), and the project/corpus pair;
//   2. it opens on the WRITING root, legibly — the root's tiles are on screen and the zoom is
//      above the readable threshold (js/app/viewport.js::Z_READABLE);
//   3. the corpus tile is on screen at that camera, because nothing off screen is built
//      (js/world/tile-mount.js) and the corpus TILE is where the interface gets its corpus: a
//      first view that frames the writing root alone opens an app with no nodes in it;
//   4. "default view" restores the same arrangement AND the same camera, after the workspace has
//      been panned away — the two paths (js/host/canvas/boot.js and js/world/home.js::resetToDefault)
//      frame through the same function, and both used to throw before framing at all
//      (`app.firstView is not a function`, `app.exitFullscreen is not a function`), which left the
//      tiles back and the camera where it was;
//   5. no console or page error, which is the only way to see a pane that threw while the page
//      still painted.
//
// Safe mode (`?safe=1`) is the boot it drives: nothing is restored, so what is measured is the
// shipped arrangement rather than whatever this machine last arranged. The browser is
// `tools/page.mjs` — the machine's chromium over the DevTools protocol, no package to install.
import { launch } from './page.mjs';

const base = (process.argv[2] || 'http://127.0.0.1:3123').replace(/\/$/, '');
const outDir = process.argv[3] || '/tmp';
const results = [];
const errors = [];
const ok = (name, pass, detail) => results.push({ name, pass: !!pass, detail });

/** "root c0 · tiled cluster (4 tiles)" -> { c0: 4 } */
const parseFrames = (labels) => {
  const m = {};
  for (const l of labels) {
    const g = /root (c\d+) .*\((\d+) tiles?\)/.exec(l);
    if (g) m[g[1]] = Number(g[2]);
  }
  return m;
};

const browser = await launch();
const page = await browser.newPage({ viewport: { width: 1500, height: 950 } });
page.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text()); });
page.on('pageerror', (e) => errors.push('pageerror: ' + e.message));

await page.goto(base + '/?safe=1', { waitUntil: 'networkidle', timeout: 30000 }).catch((e) => errors.push('goto: ' + e.message));

/* A FIXED SLEEP IS NOT A GATE. The interface is wired in stages (`web-next/js/host/canvas/boot.js`),
   the restore is a promise on `window.__boot`, and the render pane only loses its placeholder once
   `/api/lean` has answered — so a snapshot taken "2.5 s after the load event" measures the machine's
   mood as much as the arrangement. Wait for the promise, then wait for the four tiles of the writing
   root to be BUILT, and let the check below report a timeout as the failure it is. */
await page.evaluate(() => (window.__boot && window.__boot.catch(() => {})) || null).catch(() => {});
const rootBuilt = await page.waitForFunction(() => {
  const built = (id) => {
    const el = document.querySelector(`[data-pane="${id}"]`);
    return !!el && !el.querySelector('.empty');
  };
  return ['c0:tree', 'c0:editor', 'c0:render', 'c0:goal'].every(built);
}, { timeout: 15000 }).catch(() => false);
if (!rootBuilt) errors.push('the writing root never finished building');
await page.waitForTimeout(400);

/** Tiles as the user sees them: one entry per pane, measured against the viewport. */
const snap = () => page.evaluate(() => {
  const seen = new Set();
  const tiles = [];
  for (const el of document.querySelectorAll('[data-pane]')) {
    const r = el.getBoundingClientRect();
    if (r.width <= 40 || r.height <= 40 || seen.has(el.dataset.pane)) continue;
    seen.add(el.dataset.pane);
    tiles.push({
      id: el.dataset.pane,
      screen: { x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.width), h: Math.round(r.height) },
      onScreen: r.x + r.width > 0 && r.x < innerWidth && r.y + r.height > 0 && r.y < innerHeight,
      built: !el.querySelector('.empty'),
    });
  }
  const cam = window.__spike && window.__spike.cam;
  return {
    tiles,
    frames: [...document.querySelectorAll('.frame-label')].map((f) => f.textContent),
    cam: cam ? { x: Math.round(cam.x), y: Math.round(cam.y), z: +cam.z.toFixed(3) } : null,
    nodes: Number((document.querySelector('#r-nodes') || {}).textContent || 0),
    zoomText: (document.querySelector('#r-zoom') || {}).textContent,
    viewport: { w: innerWidth, h: innerHeight },
  };
});

const onBoot = await snap();
await page.screenshot({ path: outDir + '/default-view-boot.png' });

ok('no console or page error on boot', errors.length === 0, errors);
ok('the canvas is the page', await page.evaluate(() => /canvas world/.test(document.title)), await page.title());

const bootFrames = parseFrames(onBoot.frames);
ok('three roots: the writing root, the reference root, the console', bootFrames.c0 === 4 && bootFrames.c1 === 3 && bootFrames.c2 === 3, bootFrames);

const ids = new Set(onBoot.tiles.map((t) => t.id));
const want = ['c0:tree', 'c0:editor', 'c0:render', 'c0:goal', 'c1:graph', 'c1:symbols', 'c1:stats', 'p:project', 'p:corpus', 'c2:core', 'c2:view', 'c2:prefs'];
ok('every shipped tile exists', want.every((w) => ids.has(w)), want.filter((w) => !ids.has(w)));

const focus = onBoot.tiles.filter((t) => t.id.startsWith('c0:'));
ok('the writing root is what the first view frames', focus.length === 4 && focus.every((t) => t.onScreen && t.built), focus.map((t) => t.id + (t.onScreen ? '' : ' OFF-SCREEN')));

ok('it opens legibly, not as a map', onBoot.cam && onBoot.cam.z >= 0.5, onBoot.cam && onBoot.cam.z);

const corpusTile = onBoot.tiles.find((t) => t.id === 'p:corpus');
ok('the corpus tile is on screen, so the interface has a corpus', corpusTile && corpusTile.onScreen && onBoot.nodes > 0, { onScreen: !!(corpusTile && corpusTile.onScreen), nodes: onBoot.nodes });

/* Pan the world up until the console comes into view: "default view" lives in the core tile, and a
   check that teleports the camera would not prove the button is reachable at all. The drag starts
   on empty canvas — found by asking the DOM, because the gap between the two roots moves with the
   window — since a drag that starts on a tile moves the TILE. */
const gapPoint = await page.evaluate(() => {
  const background = (x, y) => {
    const e = document.elementFromPoint(x, y);
    return !!e && (e.id === 'viewport' || e.id === 'world' || e.id === 'grid');
  };
  const y = Math.round(innerHeight / 2);
  for (let x = innerWidth - 40; x > innerWidth / 2; x -= 15) if (background(x, y)) return { x, y };
  return null;
});
ok('there is empty canvas to drag from', gapPoint, gapPoint);
if (gapPoint) {
  await page.mouse.move(gapPoint.x, gapPoint.y);
  await page.mouse.down();
  for (let i = 1; i <= 10; i += 1) { await page.mouse.move(gapPoint.x, gapPoint.y - i * 55); await page.waitForTimeout(30); }
  await page.mouse.up();
  await page.waitForTimeout(1000);
}
const panned = await snap();
ok('the console is one pan away, and reachable', await page.evaluate(() => !!document.querySelector('#b-default-view')), panned.cam);

const clicked = await page.evaluate(() => { const b = document.querySelector('#b-default-view'); if (!b) return false; b.click(); return true; });
await page.waitForTimeout(2500);
const after = await snap();
await page.screenshot({ path: outDir + '/default-view-restored.png' });

ok('"default view" ran', clicked, clicked);
ok('it restored the arrangement', JSON.stringify(parseFrames(after.frames)) === JSON.stringify(bootFrames), after.frames);
ok('it restored the camera', after.cam && bootFrames && Math.abs(after.cam.z - onBoot.cam.z) < 0.005 && Math.abs(after.cam.x - onBoot.cam.x) <= 2 && Math.abs(after.cam.y - onBoot.cam.y) <= 2, { boot: onBoot.cam, after: after.cam });
ok('and the corpus survived the rebuild', after.nodes > 0 && after.nodes === onBoot.nodes, { boot: onBoot.nodes, after: after.nodes });
ok('no console or page error overall', errors.length === 0, errors);

const failed = results.filter((r) => !r.pass);
console.log(JSON.stringify({ base, results, failed: failed.length, errors, shots: [outDir + '/default-view-boot.png', outDir + '/default-view-restored.png'] }, null, 2));
await browser.close();
process.exit(failed.length ? 1 : 0);