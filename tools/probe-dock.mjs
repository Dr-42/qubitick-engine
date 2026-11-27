// probe-dock.mjs — boot the dock page and report what the shell actually built.
//
//   node tools/probe-dock.mjs [url] [preset]
//
// Prints the document, every dock element's size and visibility, and any console error. A
// layout bug is usually one of: a dock that is hidden, a dock with a zero size, or a pane
// whose mount threw — and this says which, without reading a screenshot.
import { chromium } from '/home/dsh/workspace/qubitick-corpus-v2/node_modules/playwright/index.mjs';

const url = process.argv[2] || 'http://127.0.0.1:3123/dock.html';
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
const errors = [];
page.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text()); });
page.on('pageerror', (e) => errors.push('pageerror: ' + e.message));

await page.goto(url, { waitUntil: 'networkidle' });
await page.waitForTimeout(1200);

const report = await page.evaluate(() => {
  const app = window.__dock;
  const docks = [...document.querySelectorAll('.dock')].map((d) => {
    const r = d.getBoundingClientRect();
    return {
      name: d.dataset.dock,
      hidden: d.hidden,
      railed: d.classList.contains('railed'),
      w: Math.round(r.width), h: Math.round(r.height),
      groups: [...d.querySelectorAll('.dock-group')].map((g) => ({
        tabs: [...g.querySelectorAll('[data-panel]')].map((t) => t.dataset.panel),
        h: Math.round(g.getBoundingClientRect().height),
        w: Math.round(g.getBoundingClientRect().width),
      })),
    };
  });
  return {
    hasApp: !!app,
    doc: app ? JSON.parse(JSON.stringify(app.doc().docks)) : null,
    preset: app ? app.doc().key : null,
    docks,
    paneBodies: [...document.querySelectorAll('.dock-pane')].map((p) => ({
      panel: p.dataset.panel, hidden: p.hidden,
      childCount: p.childElementCount,
      text: (p.textContent || '').trim().slice(0, 60),
    })),
    status: document.querySelector('.dock-status') ? document.querySelector('.dock-status').textContent.trim().slice(0, 120) : null,
  };
});

console.log(JSON.stringify({ report, errors }, null, 2));
await browser.close();