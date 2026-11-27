// tree-ops.test.mjs — the layout document's edit operations, checked as tree laws.
//
//   node --test tests/unit
//
// These operations were written for a docking shell that no longer exists, but the layout document
// they edit is still what a saved workspace IS (`js/layout/schema.js`), and `/shell.html` still edits
// it. The laws that keep it a tree rather than a shape that merely nests:
//
//   · every operation returns a NEW tree and leaves the input alone (a store that mutates in place
//     cannot undo);
//   · a pane appears once — inserting a pane that is already there is not a second copy;
//   · removing the last child of a split collapses it, so no split is left holding one child;
//   · `sizes` always sum to 1.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  clone, findPane, getAt, insertBeside, insertTab, isDescendant, normalizeSizes, paneIds, removePane,
} from '../../web-next/js/layout/tree-ops.js';
import { isLeaf, isSplit, isTabs } from '../../web-next/js/layout/schema.js';

const leaf = (pane) => ({ pane });
const split = (a, b, sizes = [0.5, 0.5]) => ({ split: 'h', sizes, children: [a, b] });
const ids = (tree) => [...paneIds(tree)].sort();

const sumsToOne = (tree) => {
  let ok = true;
  (function walk(n) {
    if (!n) return;
    if (isSplit(n)) {
      const sum = n.sizes.reduce((x, y) => x + y, 0);
      if (Math.abs(sum - 1) > 1e-9) ok = false;
      n.children.forEach(walk);
    } else if (isTabs(n)) n.tabs.forEach(walk);
  })(tree);
  return ok;
};

test('insertBeside wraps the target in a new split and keeps both panes', () => {
  const before = split(leaf('a'), leaf('b'));
  const after = insertBeside(before, 'a', 'c', 'right');
  assert.deepEqual(ids(after), ['a', 'b', 'c']);
  assert.equal(isSplit(getAt(after, findPane(after, 'a').slice(0, -1))), true, 'a is now in a split');
  assert.deepEqual(ids(before), ['a', 'b'], 'the input tree is untouched');
  assert.equal(sumsToOne(after), true);
});

test('insertBeside on a shared orientation splices instead of nesting a split in a split', () => {
  const before = split(leaf('a'), leaf('b'));
  const after = insertBeside(before, 'b', 'c', 'right');
  const root = after;
  assert.equal(root.children.length, 3, 'same-orientation split grows to three children');
  assert.deepEqual(ids(after), ['a', 'b', 'c']);
  assert.equal(sumsToOne(after), true);
});

test('the side decides the order, not the calling code', () => {
  const left = insertBeside(leaf('a'), 'a', 'x', 'left');
  assert.deepEqual(ids(left.children[0]), ['x']);
  const top = insertBeside(leaf('a'), 'a', 'y', 'top');
  assert.equal(top.split, 'v', 'top/bottom split vertically');
  const bottom = insertBeside(leaf('a'), 'a', 'z', 'bottom');
  assert.deepEqual(ids(bottom.children[1]), ['z']);
});

test('a pane that is already there is not inserted twice', () => {
  const before = split(leaf('a'), leaf('b'));
  assert.deepEqual(ids(insertTool(before, 'a', 'a', 'right')), ['a', 'b']);
  function insertTool(t, target, pane, side) {
    return insertBeside(t, target, pane, side);
  }
});

test('insertTab makes a tab group, and inserts into one that exists', () => {
  const tabs = insertTab(leaf('a'), 'a', 'b');
  assert.equal(isTabs(tabs), true);
  assert.deepEqual(ids(tabs), ['a', 'b']);
  assert.equal(tabs.active, 1, 'the new tab comes forward');
  const three = insertTab(tabs, 'b', 'c');
  assert.equal(three.tabs.length, 3);
  assert.equal(three.active, 2);
});

test('removing the last child of a split collapses it', () => {
  const three = { split: 'h', sizes: [0.3, 0.3, 0.4], children: [leaf('a'), leaf('b'), leaf('c')] };
  const two = removePane(three, 'b');
  assert.deepEqual(ids(two), ['a', 'c']);
  assert.equal(two.children.length, 2, 'a split with two left stays a split');
  assert.equal(sumsToOne(two), true);
  const one = removePane(two, 'c');
  assert.equal(isLeaf(one), true, 'a split with one left becomes that child');
  assert.deepEqual(ids(one), ['a']);
});

test('removing a tab from a two-tab group leaves the other tab, not a group of one', () => {
  const tabs = { tabs: [leaf('a'), leaf('b')], active: 1 };
  const one = removePane(tabs, 'b');
  assert.equal(isLeaf(one), true);
  assert.deepEqual(ids(one), ['a']);
});

test('removing the only pane gives null, and removing an unknown pane changes nothing', () => {
  assert.equal(removePane(leaf('a'), 'a'), null);
  const tree = split(leaf('a'), leaf('b'));
  assert.equal(removePane(tree, 'nope'), null, 'a miss is reported as a miss');
  assert.deepEqual(ids(tree), ['a', 'b']);
});

test('clone is deep: editing the copy never reaches the original', () => {
  const tree = split(leaf('a'), leaf('b'));
  const copy = clone(tree);
  copy.children[0].pane = 'changed';
  copy.sizes[0] = 0.9;
  assert.deepEqual(ids(tree), ['a', 'b']);
  assert.equal(tree.sizes[0], 0.5);
});

test('isDescendant answers for real paths only', () => {
  const tree = split(leaf('a'), split(leaf('b'), leaf('c')));
  assert.equal(isDescendant(tree, [], [{ children: 1 }]), true);
  assert.equal(isDescendant(tree, [{ children: 1 }], [{ children: 1 }, { children: 0 }]), true);
  assert.equal(isDescendant(tree, [{ children: 1 }, { children: 0 }], [{ children: 1 }]), false);
  assert.equal(isDescendant(tree, [{ children: 7 }], [{ children: 7 }, { children: 0 }]), false,
    'an ancestor that does not exist is not an ancestor');
});

test('normalizeSizes makes any list sum to 1, and refuses to divide by garbage', () => {
  assert.deepEqual(normalizeSizes([1, 1]), [0.5, 0.5]);
  const third = normalizeSizes([2, 4, 6]);
  assert.ok(Math.abs(third.reduce((a, b) => a + b, 0) - 1) < 1e-12);
  assert.deepEqual(normalizeSizes([0, 0]), [0.5, 0.5], 'zeros are replaced, not divided by');
  assert.deepEqual(normalizeSizes([-1, 2]), [0.5, 0.5], 'a negative size is as invalid as a zero');
  assert.deepEqual(normalizeSizes([]), []);
});