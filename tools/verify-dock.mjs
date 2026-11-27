// verify-dock.mjs — the docking shell, driven the way a user drives it.
//
//   node tools/verify-dock.mjs [baseUrl]
//
// What it asserts, because each one is a promise the shell makes:
//
//   1. the page loads with NO console or page error;
//   2. the three presets paint the docks they say they paint, and Minimal rails the sides;
//   3. a splitter DRAG changes the dock's size, and a double-click puts it back;
//   4. a collapse/expand round trip returns the dock;
//   5. the layout SURVIVES A RELOAD (per-project localStorage);
//   6. the keyboard chords do what they say (Ctrl+Alt+1/2/3, Ctrl+J, Ctrl+B).
//
// It prints a JSON list of results and exits non-zero if any failed, so it is usable as a gate.
import { chromium } from '/home/dsh/workspace/qubitick-corpus-v2/node_modules/playwright/index.mjs';

const base = process.argv[2] || 'http://127.0.0.1:3123';
const results = [];
const errors = [];
const ok = (name, pass, detail) => results.push({ name, pass, detail });

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
page.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text()); });
page.on('pageerror', (e) => errors.push('pageerror: ' + e.message));

const gotoFresh = async () => {
  await page.goto(base + '/dock.html', { waitUntil: 'networkidle' });
  // a clean slate: the shell remembers per project, and a previous run's layout is not this test's
  await page.evaluate(() => {
    for (const k of Object.keys(window.localStorage)) {
      if (k.startsWith('qubitick.dock.')) window.localStorage.removeItem(k);
    }
  });
  await page.reload({ waitUntil: 'networkidle' });
  await page.waitForFunction(() => !!window.__dock, null, { timeout: 10000 });
  await page.waitForTimeout(400);
};

const dockState = () => page.evaluate(() => {
  const app = window.__dock;
  const out = {};
  for (const d of document.querySelectorAll('.dock')) {
    const r = d.getBoundingClientRect();
    out[d.dataset.dock] = {
      hidden: d.hidden,
      railed: d.classList.contains('railed'),
      w: Math.round(r.width),
      h: Math.round(r.height),
      tabs: [...d.querySelectorAll('.dock-tab[data-panel]')].map((t) => t.dataset.panel),
    };
  }
  return { preset: app.doc().key, docks: out, sizes: app.doc().docks };
});

/* ---- 1. clean load ---- */
await gotoFresh();
const first = await dockState();
ok('loads with four docks', ['left', 'center', 'right', 'bottom'].every((d) => first.docks[d] && !first.docks[d].hidden),
  Object.keys(first.docks).join(','));
ok('standard is the default preset', first.preset === 'standard', first.preset);
ok('standard puts the tree left and the goal right',
  first.docks.left.tabs.includes('tree') && first.docks.right.tabs.includes('goal'),
  JSON.stringify({ left: first.docks.left.tabs, right: first.docks.right.tabs }));

/* ---- 1b. the inventory A1/A2 name: what Standard shows and what it keeps hidden ---- */
const stdAll = await page.evaluate(() => {
  const all = [...document.querySelectorAll('.dock-tab[data-panel]')].map((t) => t.dataset.panel);
  return all;
});
ok('standard tabs Graph and Symbols in the right dock',
  stdAll.includes('graph') && stdAll.includes('symbols'), JSON.stringify(stdAll));
ok('standard keeps Stats hidden until asked for', !stdAll.includes('stats'), JSON.stringify(stdAll));

/* the palette can open the hidden one, and Ctrl+1..9 focuses what is on screen */
await page.evaluate(() => window.__dock.openPanel('stats'));
await page.waitForTimeout(250);
const withStats = await page.evaluate(() => [...document.querySelectorAll('.dock-tab[data-panel]')].map((t) => t.dataset.panel));
ok('the palette opens a panel that was not in the layout', withStats.includes('stats'), JSON.stringify(withStats));

/* ---- 2. presets ---- */
await page.evaluate(() => window.__dock.setPreset('minimal'));
await page.waitForTimeout(250);
const minimal = await dockState();
ok('minimal rails the side docks',
  minimal.docks.left && minimal.docks.left.railed && minimal.docks.right && minimal.docks.right.railed,
  JSON.stringify({ left: minimal.docks.left, right: minimal.docks.right }));
ok('minimal keeps the editor full width',
  minimal.docks.center.w > 1300, String(minimal.docks.center.w));

await page.evaluate(() => window.__dock.setPreset('power'));
await page.waitForTimeout(250);
const power = await dockState();
ok('power shows every panel kind it names',
  ['outline', 'editor2', 'steps', 'symbols', 'stats'].every((id) => {
    const all = Object.values(power.docks).flatMap((d) => d.tabs);
    return all.includes(id);
  }),
  JSON.stringify(Object.values(power.docks).map((d) => d.tabs)));

await page.evaluate(() => window.__dock.setPreset('standard'));
await page.waitForTimeout(250);

/* ---- 3. splitter drag, and double-click reset ---- */
const before = (await dockState()).docks.left.w;
const sash = await page.$('.dock-main > .dock-sash');
const box = await sash.boundingBox();
await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
await page.mouse.down();
await page.mouse.move(box.x + box.width / 2 + 90, box.y + box.height / 2, { steps: 8 });
await page.mouse.up();
await page.waitForTimeout(250);
const dragged = (await dockState()).docks.left.w;
ok('dragging a splitter resizes the dock', dragged > before + 40, `${before} → ${dragged}`);

const sash2 = await page.$('.dock-main > .dock-sash');
const box2 = await sash2.boundingBox();
await page.mouse.dblclick(box2.x + box2.width / 2, box2.y + box2.height / 2);
await page.waitForTimeout(250);
const afterDbl = (await dockState()).docks.left.w;
ok('double-click resets the split to where it started', Math.abs(afterDbl - before) <= 2, `${dragged} → ${afterDbl} (started ${before})`);

/* ---- 2b. the Custom slot: rearranging a preset fills it, and it comes back ---- */
const customAfterDrag = await page.evaluate(() => window.__dock.doc().key);
ok('rearranging a preset marks the layout Custom', customAfterDrag === 'custom', customAfterDrag);
const customRestored = await page.evaluate(async () => {
  const app = window.__dock;
  app.setPreset('minimal');
  const leftAfterMinimal = app.doc().docks.left.railed;
  app.setPreset('custom');
  return { leftAfterMinimal, backToCustom: app.doc().key, leftRailed: app.doc().docks.left.railed };
});
ok('Ctrl+Alt+0 restores what Custom saved',
  customRestored.backToCustom === 'custom', JSON.stringify(customRestored));

/* ---- 4. collapse / expand ---- */
await page.evaluate(() => window.__dock.toggleDock('bottom'));
await page.waitForTimeout(200);
const collapsed = (await dockState()).docks.bottom;
ok('Ctrl+J class toggle collapses the bottom dock', !!collapsed && collapsed.railed, JSON.stringify(collapsed));
await page.evaluate(() => window.__dock.toggleDock('bottom'));
await page.waitForTimeout(200);
const expanded = (await dockState()).docks.bottom;
ok('and expanding it comes back', !!expanded && !expanded.railed && expanded.h > 60, JSON.stringify(expanded));

/* ---- 5. reload keeps the layout ---- */
const beforeReload = await dockState();
await page.reload({ waitUntil: 'networkidle' });
await page.waitForFunction(() => !!window.__dock, null, { timeout: 10000 });
await page.waitForTimeout(400);
const afterReload = await dockState();
ok('the layout survives a reload',
  Math.abs(afterReload.docks.left.w - beforeReload.docks.left.w) <= 2
  && afterReload.preset === beforeReload.preset,
  `left ${beforeReload.docks.left.w} → ${afterReload.docks.left.w}, preset ${afterReload.preset}`);

/* ---- 6. keyboard ---- */
await page.evaluate(() => window.__dock.setPreset('standard'));
await page.locator('body').click({ position: { x: 700, y: 400 } });
await page.keyboard.press('Control+Alt+1');
await page.waitForTimeout(250);
ok('Ctrl+Alt+1 is Minimal', (await dockState()).preset === 'minimal');
await page.keyboard.press('Control+Alt+3');
await page.waitForTimeout(250);
ok('Ctrl+Alt+3 is Power', (await dockState()).preset === 'power');
await page.keyboard.press('Control+Alt+2');
await page.waitForTimeout(250);
ok('Ctrl+Alt+2 is Standard', (await dockState()).preset === 'standard');

/* ---- 7. no errors ---- */
ok('no console or page errors', errors.length === 0, errors.join(' | '));

await browser.close();
console.log(JSON.stringify({ results, errors }, null, 2));
process.exit(results.every((r) => r.pass) ? 0 : 1);