/*
 * latexocr.js — the OCR build behind the Handwrite pane.
 *
 * The model is a separate WASM bundle (`web-next/wasm/latexocr/`) and its loader is a
 * script that defines one global, `createLatexOCR`. That global is not part of this shell
 * (WASM is out of scope), and the pane is still useful without it: typed LaTeX and the
 * upload fallback do not touch the model. So the whole build is behind `load()` and the
 * answer "there is no model" is a normal return value, not a throw.
 *
 * The pane used to carry this itself — the loader, the init call, the pixel conversion
 * and the read — which is the kind of thing that grows inside a pane until the pane is
 * the only place the format is known. Here the model's contract is in one file:
 *
 *   const ocr = await load();          // null when the build is not on the page
 *   ocr.w, ocr.h                       // the input size the model wants
 *   ocr.read(strokes, drawPath)        // strokes -> latex string
 *
 * `read` takes a *drawing* callback rather than the pane's stroke model: the pane knows
 * how its strokes look, this file only knows the size and the greyscale conversion the
 * model expects. That keeps the pane free to change how it draws a pen stroke.
 */

/** Is the build on this page at all? */
export const available = () => typeof globalThis.createLatexOCR === 'function';

/**
 * Load and initialise the model. Resolves to null when the build is absent — the caller
 * shows its degraded mode — and rejects only when the build is present and broken, which
 * is a real failure worth reporting.
 */
export async function load({ base = '/wasm/latexocr/', fonts = 'fonts/math.ttf', weights = 'models/model.pt' } = {}) {
  if (!available()) return null;
  const create = globalThis.createLatexOCR;
  const Module = await create({ locateFile: (p) => base + p });
  const rc = Module.ccall('lx_init', 'number', ['string', 'string'], [fonts, weights]);
  if (rc !== 0) {
    const why = Module.ccall('lx_why', 'string', [], []);
    throw new Error(why || 'lx_init failed');
  }
  const w = Module.ccall('lx_w', 'number', [], []);
  const h = Module.ccall('lx_h', 'number', [], []);
  const buf = Module._malloc(w * h);
  return {
    w,
    h,
    /** Draw the strokes onto a canvas of the model's size, then read them back. */
    read(draw, strokes) {
      const off = document.createElement('canvas');
      off.width = w;
      off.height = h;
      const octx = off.getContext('2d');
      octx.fillStyle = '#ffffff';
      octx.fillRect(0, 0, w, h);
      draw(octx, w, h);
      const px = octx.getImageData(0, 0, w, h).data;
      const heap = Module.HEAPU8;
      for (let i = 0; i < w * h; ++i) {
        // the model reads luminance, one byte per pixel
        heap[buf + i] = (px[4 * i] * 299 + px[4 * i + 1] * 587 + px[4 * i + 2] * 114) / 1000 | 0;
      }
      return Module.ccall('lx_read', 'string', ['number'], [buf]) || '';
    },
  };
}