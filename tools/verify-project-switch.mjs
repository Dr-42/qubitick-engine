// verify-project-switch.mjs — the layout belongs to the CORPUS, not to the browser.
//
//   node tools/verify-project-switch.mjs [baseUrl]
//
// The claim (A4): "Layout survives reload and project switch". Reload is one thing — the
// document is in localStorage — and a project switch is the harder one: two corpora must not
// share a layout, or opening a second corpus lands you in the first one's arrangement with
// panels sized for a tree that is not there.
//
// It needs a server with two projects wired in; the check names the pair it used and skips
// (exit 2) when the server has fewer.
import { chromium } from '/home/dsh/workspace/qubitick-corpus-v2/node_modules/playwright/index.mjs';

const base = process.argv[2] || 'http://127.0.0.1:3123';
const results = [];
const errors = [];
const ok = (name, pass, detail) => results.push({ name, pass, detail });

const projects = await (await fetch(base + '/api/projects')).json().catch(() => null);
const list = (projects && projects.projects) || [];
if (list.length < 2) {
  console.log(JSON.stringify({ skipped: 'this server has fewer than two projects', list }, null, 2));
  process.exit(2);
}
const [A, B] = list.map((p) => p.id);

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
page.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text()); });
page.on('pageerror', (e) => errors.push('pageerror: ' + e.message));

const boot = async () => {
  await page.goto(base + '/dock.html', { waitUntil: 'networkidle' });
  await page.waitForFunction(() => !!window.__dock, null, { timeout: 15000 });
  await page.waitForTimeout(700);
};
const state = () => page.evaluate(() => {
  const app = window.__dock;
  return {
    keys: Object.keys(window.localStorage).filter((k) => k.startsWith('qubitick.dock.')),
    left: app.doc().docks.left.size,
    leftRailed: app.doc().docks.left.collapsed,
    preset: app.doc().key,
  };
});

await boot();
// the layout for project A: rail the left dock, so the two projects are visibly different
await page.evaluate(() => window.__dock.toggleDock('left'));
await page.waitForTimeout(400);
const aState = await state();
ok('a rearrangement is stored under the active project', aState.keys.some((k) => k.includes(A)), aState.keys.join(', '));

// switch to project B, which reloads the page
await fetch(base + '/api/project', {
  method: 'POST',
  headers: { 'content-type': 'application/json' },
  body: JSON.stringify({ id: B }),
});
await boot();
const bState = await state();
ok('the other project does NOT inherit that layout', bState.leftRailed === false, JSON.stringify({ left: bState.leftRailed }));

// give B a different layout of its own, then go back to A
await page.evaluate(() => window.__dock.setPreset('minimal'));
await page.waitForTimeout(400);
await fetch(base + '/api/project', {
  method: 'POST',
  headers: { 'content-type': 'application/json' },
  body: JSON.stringify({ id: A }),
});
await boot();
const aAgain = await state();
ok('coming back to the first project restores ITS layout',
  aAgain.leftRailed === true && aAgain.preset === 'custom',
  JSON.stringify({ railed: aAgain.leftRailed, preset: aAgain.preset }));
ok('no console or page errors', errors.length === 0, errors.join(' | '));

await browser.close();
console.log(JSON.stringify({ projects: { A, B }, results, errors }, null, 2));
process.exit(results.every((r) => r.pass) ? 0 : 1);