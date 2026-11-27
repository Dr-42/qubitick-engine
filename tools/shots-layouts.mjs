// shots-layouts.mjs — the A4 deliverable: every preset, in two themes.
//
//   node tools/shots-layouts.mjs [baseUrl] [outDir]
//
// Each shot opens a theorem so the panels have something to show (a screenshot of an empty
// editor says nothing about the layout), and each run CLEARS the stored layout first so a
// previous run cannot decide what this one captures. Prints the md5 of each file: two themes
// that produce the same bytes are two shots of the same theme, which is a bug in the script
// and not a coincidence worth shipping.
import { chromium } from '/home/dsh/workspace/qubitick-corpus-v2/node_modules/playwright/index.mjs';
import { createHash } from 'node:crypto';
import { readFileSync, mkdirSync } from 'node:fs';

const base = process.argv[2] || 'http://127.0.0.1:3123';
const outDir = process.argv[3] || 'docs/layouts';
const THEMES = ['midnight', 'paper'];
const PRESETS = ['minimal', 'standard', 'power'];
mkdirSync(outDir, { recursive: true });

const browser = await chromium.launch();
const out = [];
const errors = [];
for (const preset of PRESETS) {
  for (const theme of THEMES) {
    const page = await browser.newPage({ viewport: { width: 1500, height: 950 } });
    page.on('pageerror', (e) => errors.push(`${preset}/${theme} pageerror: ${e.message}`));
    page.on('console', (m) => { if (m.type() === 'error') errors.push(`${preset}/${theme} console: ${m.text()}`); });
    await page.goto(base + '/dock.html', { waitUntil: 'networkidle' });
    await page.waitForFunction(() => !!window.__dock, null, { timeout: 15000 });
    await page.evaluate(() => {
      for (const k of Object.keys(window.localStorage)) {
        if (k.startsWith('qubitick.dock.')) window.localStorage.removeItem(k);
      }
    });
    await page.evaluate(async (t) => { await window.__dock.setTheme(t); }, theme);
    /* OPEN THE FILE IN THE STANDARD PRESET FIRST. Minimal and Power both rail or shrink the
       tree, and a railed tree has no rows to click — so opening the theorem after choosing the
       preset captures an EMPTY editor, which is the one thing a layout screenshot should not
       show. The editor instance survives the preset switch (same panel id), so the file stays. */
    await page.evaluate(() => window.__dock.setPreset('standard'));
    await page.waitForTimeout(250);
    await page.evaluate(() => {
      const row = document.querySelector('.tree-pane [data-tfile="Era0/Strata0/Theorems/T_A_01_MulTwo.qbit"]');
      if (row) row.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    });
    await page.waitForTimeout(900);
    await page.evaluate((pr) => window.__dock.setPreset(pr), preset);
    /* A CHECK FIRST, so the panels that report on one have something to say: the Build panel's
       counts, the Diagnostics tab's verdict and the Goal panel's node status are all "never
       checked" until a report exists, and a screenshot of four "never checked" panels says
       nothing about the layout beyond where the boxes are. */
    await page.evaluate(() => window.__dock.buildService.run());
    await page.waitForTimeout(2200);
    await page.waitForTimeout(1200);
    const file = `${outDir}/${preset}-${theme}.png`;
    await page.screenshot({ path: file });
    const md5 = createHash('md5').update(readFileSync(file)).digest('hex').slice(0, 12);
    const mode = await page.evaluate(() => document.documentElement.dataset.mode);
    out.push({ file, preset, theme, mode, md5 });
    await page.close();
  }
}
await browser.close();
console.log(JSON.stringify({ shots: out, errors }, null, 2));
