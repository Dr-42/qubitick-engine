// shot.mjs — open a page, wait for it to settle, screenshot, and report console errors.
//
//   node tools/shot.mjs http://127.0.0.1:3123/shell.html /tmp/shell.png [width] [height]
//
// The console and page errors are the point: a layout change that throws on load still paints
// something, and "no console or page errors" is a claim only a browser can settle.
//
// The browser is `tools/page.mjs`: the machine's chromium, driven over the DevTools protocol. No npm
// package, no config file, nothing to install before a gate can run.
import { launch } from './page.mjs';

const [url, out, w = '1400', h = '900'] = process.argv.slice(2);
if (!url || !out) {
  console.error('usage: node tools/shot.mjs <url> <out.png> [width] [height]');
  process.exit(2);
}

const browser = await launch();
const page = await browser.newPage({ viewport: { width: Number(w), height: Number(h) } });
const errors = [];
page.on('console', (m) => {
  if (m.type() === 'error') errors.push('console: ' + m.text());
});
page.on('pageerror', (e) => errors.push('pageerror: ' + e.message));

await page.goto(url, { waitUntil: 'networkidle', timeout: 30000 }).catch((e) => errors.push('goto: ' + e.message));
// the canvas keeps its restore promise on `window.__boot`: await it, then let a couple of frames land
await page.evaluate(() => (window.__boot && window.__boot.catch(() => {})) || null).catch(() => {});
await page.waitForTimeout(1500);
await page.screenshot({ path: out, fullPage: false });

console.log(JSON.stringify({ url, out, errors }, null, 2));
await browser.close();
process.exit(errors.length ? 1 : 0);
