// tiling.test.mjs — how a cluster lays its tiles out, as properties rather than as numbers.
//
//   node --test tests/unit
//
// `js/app/tiling.js` turns the kinds in a cluster into a split tree and the tree into rectangles.
// The numbers are a matter of taste and will keep changing; what must not change is the shape of the
// answer:
//
//   · every kind gets exactly one rectangle, and no two rectangles overlap;
//   · the tiles stay inside the box, with a gap where a sash sits;
//   · the reading order is PIPELINE, not the order kinds were added in;
//   · a ratio near the end of its range is clamped, so no pane is ever given a negative box — a box
//     the browser collapses to nothing while the layout carries on as if the pane were there.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  GAP, MIN_SIDE, PIPELINE, autoTree, kindOrder, kindWeight, solveRects,
} from '../../web-next/js/app/tiling.js';

const overlap = (a, b) =>
  a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;

test('every kind gets exactly one rectangle, and none of them overlap', () => {
  const kinds = ['tree', 'editor', 'render', 'goal'];
  const { rects } = solveRects(autoTree(kinds, 1400, 760), { x: 0, y: 0, w: 1400, h: 760 });
  assert.deepEqual(Object.keys(rects).sort(), kinds.slice().sort());
  const names = Object.keys(rects);
  for (let i = 0; i < names.length; i += 1) {
    for (let j = i + 1; j < names.length; j += 1) {
      assert.equal(overlap(rects[names[i]], rects[names[j]]), false,
        `${names[i]} and ${names[j]} overlap: ${JSON.stringify(rects[names[i]])} / ${JSON.stringify(rects[names[j]])}`);
    }
  }
});

test('the tiles stay inside the box and stay usable', () => {
  const box = { x: 100, y: 50, w: 900, h: 1200 };
  const { rects } = solveRects(autoTree(['tree', 'editor', 'render', 'goal', 'stats'], box.w, box.h), box);
  for (const [kind, r] of Object.entries(rects)) {
    assert.ok(r.x >= box.x && r.y >= box.y, `${kind} starts outside the box`);
    assert.ok(r.x + r.w <= box.x + box.w && r.y + r.h <= box.y + box.h,
      `${kind} runs past the box: ${JSON.stringify(r)}`);
    assert.ok(r.w >= MIN_SIDE && r.h >= MIN_SIDE, `${kind} is below the minimum side: ${JSON.stringify(r)}`);
  }
});

test('the reading order is PIPELINE, whatever order the kinds arrived in', () => {
  const a = solveRects(autoTree(['goal', 'tree', 'render', 'editor'], 1400, 760), { x: 0, y: 0, w: 1400, h: 760 });
  const b = solveRects(autoTree(['editor', 'render', 'tree', 'goal'], 1400, 760), { x: 0, y: 0, w: 1400, h: 760 });
  assert.deepEqual(a.rects, b.rects);
  assert.ok(kindOrder('tree') < kindOrder('editor'), 'the tree reads before the editor');
  assert.ok(kindOrder('editor') < kindOrder('goal'), 'the editor reads before the goal');
  assert.equal(kindOrder('a-kind-nobody-listed'), PIPELINE.length, 'unknown kinds sort last');
});

test('the same kinds in the same box give the same rectangles, twice', () => {
  const box = { x: 0, y: 0, w: 1024, h: 768 };
  const once = solveRects(autoTree(['tree', 'graph', 'editor', 'render'], box.w, box.h), box);
  const twice = solveRects(autoTree(['tree', 'graph', 'editor', 'render'], box.w, box.h), box);
  assert.deepEqual(once.rects, twice.rects);
  assert.deepEqual(once.sashes.map((s) => [s.dir, s.x, s.y, s.w, s.h]), twice.sashes.map((s) => [s.dir, s.x, s.y, s.w, s.h]));
});

test('a cut never squashes a pane flat, however extreme the saved ratio', () => {
  const tree = { k: 'h', r: 0.999, a: { k: 'leaf', pane: 'a' }, b: { k: 'leaf', pane: 'b' } };
  const { rects } = solveRects(tree, { x: 0, y: 0, w: 400, h: 300 });
  assert.ok(rects.a.w >= MIN_SIDE, `a is ${rects.a.w}px wide`);
  assert.ok(rects.b.w >= MIN_SIDE, `b is ${rects.b.w}px wide`);
  const tall = solveRects({ k: 'v', r: 0.001, a: { k: 'leaf', pane: 'a' }, b: { k: 'leaf', pane: 'b' } },
    { x: 0, y: 0, w: 400, h: 300 });
  assert.ok(tall.rects.a.h >= MIN_SIDE && tall.rects.b.h >= MIN_SIDE, 'the same clamp holds on the other axis');
});

test('every cut has a sash that names the node it moves', () => {
  /* A sash drag changes ONE ratio. That is only true if the sash carries the tree node it belongs to,
     so this asserts the identity, not a copy. */
  const tree = autoTree(['tree', 'editor', 'render'], 1200, 800);
  const { sashes } = solveRects(tree, { x: 0, y: 0, w: 1200, h: 800 });
  assert.equal(sashes.length, 2, 'three tiles are two cuts');
  /* The walk emits a cut after its children, so the sash list is inner-first and the ROOT cut is the
     last one. What matters is identity: the sash carries the very node whose `r` a drag rewrites. */
  assert.equal(sashes.at(-1).node, tree, 'the outer sash moves the root cut');
  assert.ok(sashes.every((s) => s.node && typeof s.node.r === 'number'), 'every sash names a cut with a ratio');
  for (const s of sashes) {
    assert.ok(s.pw > 0 && s.ph > 0, 'a sash carries the box it divides, so a drag needs no second walk');
    assert.ok(s.dir === 'h' || s.dir === 'v');
  }
});

test('GAP is kept on each side of the cut, so two tiles never touch', () => {
  /* The cut sits ON the line and both children back away from it by GAP, which is why the empty
     space between two tiles is 2*GAP and not GAP. Asserting the code's own constant keeps the test
     honest when the spacing is retuned. */
  const { rects } = solveRects({ k: 'h', r: 0.5, a: { k: 'leaf', pane: 'a' }, b: { k: 'leaf', pane: 'b' } },
    { x: 0, y: 0, w: 500, h: 200 });
  const gap = rects.b.x - (rects.a.x + rects.a.w);
  assert.equal(gap, 2 * GAP, `gap is ${gap}, two lots of GAP is ${2 * GAP}`);
});

test('an unlisted kind weighs the default, so a user pane tiles without being registered', () => {
  assert.equal(kindWeight('a-kind-nobody-listed'), 0.9, 'the default weight, not zero and not an error');
  assert.deepEqual(autoTree(['solo'], 800, 600), { k: 'leaf', pane: 'solo' });
});