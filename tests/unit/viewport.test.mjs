// viewport.test.mjs — the camera as arithmetic, and the rule that decides what a first view frames.
//
//   node --test tests/unit
//
// The claim these tests defend is the one line every other camera fact is built on:
//
//     screen = world * z + (cam.x, cam.y)
//
// A screen-space translation and a scale. The engine's other copy of this arithmetic read x/y as a
// world-space ORIGIN (`screen = (world - x) * z`), which agrees only while x = y = 0 — so a fit
// computed with it put the content off screen by exactly the pan it asked for. Anything that
// re-derives the convention has to keep this file green.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  ZMAX, ZMIN, Z_READABLE, bboxOf, camForRect, clampZoom, firstViewCam, onScreen, screenRect,
  viewportWorld, worldAt, zoomAbout, zoomAtCentre, zoomToFit,
} from '../../web-next/js/app/viewport.js';

/* The shipped arrangement's box: the writing root (1400x760) with the corpus pair 240px above it,
   which is what `js/world/home.js` frames. */
const OPEN_BOX = { x: 0, y: -240, w: 1400, h: 1000 };

test('a world rect lands at world*z + cam, and worldAt inverts it', () => {
  const cam = { x: 169, y: 259.2, z: 0.83 };
  const r = screenRect({ x: 100, y: 40, w: 300, h: 200 }, cam);
  assert.deepEqual(r, { x: 100 * 0.83 + 169, y: 40 * 0.83 + 259.2, w: 300 * 0.83, h: 200 * 0.83 });
  const back = worldAt(r.x + r.w / 2, r.y + r.h / 2, cam);
  assert.ok(Math.abs(back.x - 250) < 1e-9, `inverse x, got ${back.x}`);
  assert.ok(Math.abs(back.y - 140) < 1e-9, `inverse y, got ${back.y}`);
});

test('the camera is a SCREEN translation: panning by a screen delta moves the content by it', () => {
  /* The old convention would have needed x to be scaled by z. At z = 2 the two disagree by a
     factor of two, which is the whole reason this is asserted rather than assumed. */
  const cam = { x: 250, y: -40, z: 2 };
  const before = screenRect({ x: 0, y: 0, w: 10, h: 10 }, cam);
  const after = screenRect({ x: 0, y: 0, w: 10, h: 10 }, { ...cam, x: cam.x + 30, y: cam.y + 12 });
  assert.equal(after.x - before.x, 30);
  assert.equal(after.y - before.y, 12);
  assert.equal(after.w, before.w);
});

test('zoomAbout keeps the world point under the pointer exactly still', () => {
  const cam = { x: 120, y: 80, z: 0.6 };
  const point = worldAt(431, 512, cam);
  const zoomed = zoomAbout(cam, 431, 512, 1.9);
  const landed = worldAt(431, 512, zoomed);
  assert.ok(Math.abs(landed.x - point.x) < 1e-9 && Math.abs(landed.y - point.y) < 1e-9,
    `point drifted: ${JSON.stringify(point)} -> ${JSON.stringify(landed)}`);
  assert.ok(zoomed.z > cam.z);
});

test('zoomAtCentre keeps the viewport centre on the same world point', () => {
  const cam = { x: 0, y: 0, z: 1 };
  const centre = viewportWorld(cam, 1200, 800);
  const zoomed = zoomAtCentre(cam, 0.5, 1200, 800);
  const after = viewportWorld(zoomed, 1200, 800);
  assert.ok(Math.abs((after.x + after.w / 2) - (centre.x + centre.w / 2)) < 1e-9);
  assert.ok(Math.abs((after.y + after.h / 2) - (centre.y + centre.h / 2)) < 1e-9);
});

test('the zoom limits hold, and zoomToFit never blows a small rect up past maxZ', () => {
  assert.equal(clampZoom(0), ZMIN);
  assert.equal(clampZoom(99), ZMAX);
  assert.equal(ZMAX, 5);
  const z = zoomToFit({ x: 0, y: 0, w: 80, h: 60 }, 1400, 900, 40, 1);
  assert.equal(z, 1, 'a small rect fits at 100%, not at 400%');
});

test('a first view fits the rect when the fit is legible', () => {
  const cam = firstViewCam({ x: 0, y: 0, w: 600, h: 400 }, 1400, 900, { pad: 60 });
  assert.ok(cam.z >= Z_READABLE);
  const r = screenRect({ x: 0, y: 0, w: 600, h: 400 }, cam);
  assert.ok(r.x >= 60 - 0.5 && r.y >= 60 - 0.5, 'the pad is breathing room on screen, all round');
  assert.ok(r.x + r.w <= 1400 - 60 + 0.5 && r.y + r.h <= 900 - 60 + 0.5);
});

test('a first view below the readable threshold anchors the rect at 50%, it does not fit the world', () => {
  const cam = firstViewCam(OPEN_BOX, 1280, 800, { pad: 60 });
  /* 1400x1000 in 1280x800 fits at 68% — legible, so it fits. A box the size of the whole canvas is
     what falls through to the anchored branch, and the anchor is the TOP-LEFT of the rect at the pad. */
  assert.ok(Math.abs(cam.z - 0.68) < 0.005, `expected the 68% fit, got ${cam.z}`);
  const huge = firstViewCam({ x: 0, y: -240, w: 2436, h: 1992 }, 1500, 950, { pad: 60 });
  assert.equal(huge.z, Z_READABLE);
  assert.equal(huge.x, 60 - 0 * Z_READABLE);
  assert.equal(huge.y, 60 - (-240) * Z_READABLE);
});

test('the first view depends on the window — the regression that made every size look alike', () => {
  /* The home host asked a `deps.viewport()` that was never passed, so every framing silently used a
     1400x900 fallback and the zoom was identical at every window size. Three window sizes, three
     zooms, each equal to the fit it claims to be. */
  const sizes = [[1500, 950], [1440, 900], [1280, 800]];
  const zs = sizes.map(([w, h]) => firstViewCam(OPEN_BOX, w, h, { pad: 60 }).z);
  assert.equal(new Set(zs.map((z) => z.toFixed(3))).size, 3, `zooms must differ per window: ${zs}`);
  sizes.forEach(([w, h], i) => {
    const expected = Math.min((w - 120) / OPEN_BOX.w, (h - 120) / OPEN_BOX.h);
    assert.ok(Math.abs(zs[i] - expected) < 1e-9, `${w}x${h}: ${zs[i]} vs ${expected}`);
  });
});

test('a first view never zooms above 100%', () => {
  const cam = firstViewCam({ x: 0, y: 0, w: 100, h: 100 }, 1400, 900);
  assert.equal(cam.z, 1);
});

test('onScreen includes both edges, and bboxOf has no box for an empty list', () => {
  assert.equal(onScreen({ x: 1400, y: 0, w: 10, h: 10 }, 1400, 900), true);
  assert.equal(onScreen({ x: -10, y: 0, w: 10, h: 10 }, 1400, 900), true);
  assert.equal(onScreen({ x: 1401, y: 0, w: 10, h: 10 }, 1400, 900), false);
  assert.equal(onScreen({ x: 0, y: 0, w: 10, h: 10 }, 1400, 900, 20), true);
  assert.equal(bboxOf([]), null);
  assert.deepEqual(bboxOf([{ x: 5, y: 7, w: 10, h: 10 }, { x: -5, y: 20, w: 10, h: 5 }]),
    { x: -5, y: 7, w: 20, h: 18 });
});