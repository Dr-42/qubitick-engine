// probe-goal.mjs — open a file from the tree and watch the editor and the Goal panel follow.
//
//   node tools/probe-goal.mjs [baseUrl]
//
// A1 and A3 are claims about a CHAIN: the tree's selection reaches the editor, the editor's
// cursor reaches the Goal panel, and the panel shows the row at the cursor with its citation
// list. Any link can be silently missing — the wrk is wired by port, and a port nobody filled
// is not an error, just an empty panel — so this walks the chain and prints each step.
import { chromium } from '/home/dsh/workspace/qubitick-corpus-v2/node_modules/playwright/index.mjs';

const base = process.argv[2] || 'http://127.0.0.1:3123';
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
const errors = [];
page.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text()); });
page.on('pageerror', (e) => errors.push('pageerror: ' + e.message));

await page.goto(base + '/dock.html', { waitUntil: 'networkidle' });
await page.waitForFunction(() => !!window.__dock, null, { timeout: 10000 });
await page.waitForTimeout(800);

const step = {};

// 1. the tree lists the corpus
step.treeFiles = await page.evaluate(() => [...document.querySelectorAll('.tree-pane [data-tfile]')].map((n) => n.dataset.tfile));

// 2. click a theorem file, so there is a proof to follow
const target = step.treeFiles.find((f) => f.includes('T_A_01')) || step.treeFiles[0];
await page.evaluate((f) => {
  const row = document.querySelector(`.tree-pane [data-tfile="${f}"]`);
  if (row) row.dispatchEvent(new MouseEvent('click', { bubbles: true }));
}, target);
await page.waitForTimeout(1500);

step.clicked = target;
step.editorText = await page.evaluate(() => {
  const body = document.querySelector('.dock-pane[data-panel="editor1"]');
  if (!body) return null;
  const cm = body.querySelector('.cm-content');
  return (cm ? cm.textContent : body.textContent || '').trim().slice(0, 400);
});
step.editorHasFile = /T\.A\.01|MulTwo/.test(step.editorText || '');

// 3. move the cursor into the proof, and see whether the goal panel reacts
step.goalBefore = await page.evaluate(() => {
  const b = document.querySelector('.dock-pane[data-panel="goal"]');
  return b ? (b.textContent || '').replace(/\s+/g, ' ').trim().slice(0, 300) : null;
});

await page.evaluate(() => {
  const body = document.querySelector('.dock-pane[data-panel="editor1"]');
  const cm = body && body.querySelector('.cm-content');
  if (cm) {
    cm.focus();
    const sel = window.getSelection();
    const range = document.createRange();
    const last = cm.lastElementChild || cm;
    range.selectNodeContents(last);
    range.collapse(true);
    sel.removeAllRanges();
    sel.addRange(range);
  }
});
await page.waitForTimeout(1200);

step.goalAfter = await page.evaluate(() => {
  const b = document.querySelector('.dock-pane[data-panel="goal"]');
  return b ? (b.textContent || '').replace(/\s+/g, ' ').trim().slice(0, 400) : null;
});

// 4. what the goal pane actually declares
step.goalPaneKind = await page.evaluate(() => {
  const b = document.querySelector('.dock-pane[data-panel="goal"]');
  return b ? [...b.querySelectorAll('*')].slice(0, 12).map((n) => n.tagName + '.' + n.className).join(' | ') : null;
});

console.log(JSON.stringify({ step, errors }, null, 2));
await browser.close();