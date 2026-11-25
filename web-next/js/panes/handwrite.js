import { ENDPOINTS } from '../defaults/endpoints.js';

/**
 * panes/handwrite.js — Handwrite pane (M4.29).
 *
 * Pen-first handwriting canvas for LaTeX input. Draws strokes on a canvas,
 * recognizes them via WASM OCR (latexocr) with upload fallback, and supports
 * typed LaTeX with KaTeX preview.
 *
 * Ported from app.js 3194–3414 (hwTheme, hwPaint, recognizeCanvas,
 * initHandwrite, updatePreview, toggleHandwritePanel, insertLatexAtCursor).
 *
 * In:  {}
 * Out: `latex` (event string) when recognised/typed-and-inserted
 * Call: `insertAtCursor` → emits `latex`
 */

import { valuePort, eventPort, callPort } from '../core/ports.js';
import { mathHtml } from '../lib/katex.js';
import * as OCR from '../lib/latexocr.js';

const HW_W = 400;
const HW_H = 200;

export default {
  kind: 'handwrite',
  title: 'Handwrite',
  ports: {
    in: {},
    out: { latex: 'event' },
    call: { insertAtCursor: 'call' },
  },
  create(ctx) {
    const { ws, event } = ctx;
    let el = null;
    let container = null;
    let canvas = null;
    let cctx = null;
    let statusEl = null;
    let previewEl = null;
    let typedInput = null;

    let strokes = [];
    let curStroke = null;
    let drawing = false;
    let ready = false;
    let model = null;
    let autoTimer = null;

    // Ports
    const portLatex = eventPort();
    const callInsertAtCursor = callPort(doInsertAtCursor);

    function esc(s) {
      return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
    }

    function hwTheme() {
      const cs = typeof getComputedStyle !== 'undefined' ? getComputedStyle(document.documentElement) : null;
      const bg = cs ? (cs.getPropertyValue('--bg-inset') || '').trim() || '#0c1220' : '#0c1220';
      const ink = cs ? (cs.getPropertyValue('--ink') || '').trim() || '#dbe7f4' : '#dbe7f4';
      return { bg, ink };
    }

    function hwPaint() {
      if (!cctx || !canvas) return;
      const t = hwTheme();
      cctx.fillStyle = t.bg;
      cctx.fillRect(0, 0, canvas.width, canvas.height);
      cctx.strokeStyle = t.ink;
      cctx.lineWidth = 3;
      cctx.lineCap = 'round';
      cctx.lineJoin = 'round';
    }

    function toCanvas(e) {
      const r = canvas.getBoundingClientRect();
      return {
        x: (e.clientX - r.left) * HW_W / (r.width || HW_W),
        y: (e.clientY - r.top) * HW_H / (r.height || HW_H),
      };
    }

    function startStroke(e) {
      e.preventDefault();
      drawing = true;
      const p = toCanvas(e);
      curStroke = [{ x: p.x, y: p.y }];
      strokes.push(curStroke);
      cctx.beginPath();
      cctx.moveTo(p.x, p.y);
    }

    function moveStroke(e) {
      if (!drawing || !curStroke) return;
      const p = toCanvas(e);
      curStroke.push({ x: p.x, y: p.y });
      cctx.lineTo(p.x, p.y);
      cctx.stroke();
    }

    function endStroke() {
      if (!drawing) return;
      drawing = false;
      curStroke = null;
      cctx.closePath();
      if (autoTimer) clearTimeout(autoTimer);
      autoTimer = setTimeout(async () => {
        if (ready) {
          try { const latex = recognizeCanvas(); if (latex && latex.trim()) updatePreview(latex); } catch {}
        }
      }, 800);
    }

    function recognizeCanvas() {
      if (!ready || !model || !strokes.length) return '';
      /* The model's size is not this canvas's size, so the strokes are redrawn at the
         model's scale on a scratch canvas. What a stroke looks like is this pane's
         business — that is the callback; the greyscale conversion is the model's, and it
         lives in js/lib/latexocr.js. */
      const sx = model.w / HW_W;
      const sy = model.h / HW_H;
      const lw = Math.max(1.5, 3 * sx);
      return model.read((octx) => {
        octx.strokeStyle = '#000000';
        octx.fillStyle = '#000000';
        octx.lineCap = 'round';
        octx.lineJoin = 'round';
        octx.lineWidth = lw;
        const dot = (x, y) => {
          octx.beginPath();
          octx.arc(x, y, lw / 2, 0, Math.PI * 2);
          octx.fill();
        };
        for (const st of strokes) {
          if (!st.length) continue;
          if (st.length === 1) { dot(st[0].x * sx, st[0].y * sy); continue; }
          octx.beginPath();
          for (let i = 0; i < st.length; ++i) {
            const p = st[i];
            if (i) octx.lineTo(p.x * sx, p.y * sy); else octx.moveTo(p.x * sx, p.y * sy);
          }
          octx.stroke();
        }
      }, strokes);
    }

    async function loadWasm() {
      /* The OCR build is optional here (WASM is out of scope), and js/lib/latexocr.js
         answers null when it is not on the page: typed LaTeX and the upload fallback are
         the supported modes, so this is a state, not an error. A model that IS present
         and fails to initialise is a real failure and says so. */
      try {
        model = await OCR.load();
        if (!model) {
          if (statusEl) {
            statusEl.textContent = 'typed LaTeX — OCR model not loaded';
            statusEl.style.color = 'var(--muted)';
          }
          return;
        }
        ready = true;
        if (statusEl) statusEl.textContent = 'Ready (' + model.w + '×' + model.h + ' model)';
      } catch (e) {
        console.error('handwriting engine:', e);
        if (statusEl) {
          statusEl.textContent = 'WASM unavailable — use Upload';
          statusEl.style.color = 'var(--amber)';
        }
      }
    }

    function clearCanvas() {
      strokes = [];
      curStroke = null;
      hwPaint();
      updatePreview('');
    }

    function updatePreview(latex) {
      if (!previewEl) return;
      if (!latex) {
        previewEl.innerHTML = '<span style="color:var(--muted);font-size:12px;">Preview appears here…</span>';
        return;
      }
      try {
        const html = mathHtml(latex);
        previewEl.innerHTML = html;
      } catch {
        previewEl.innerHTML = '<span style="color:var(--red);font-size:12px;">Parse error</span>';
      }
    }

    function doInsertLatex(latex) {
      if (!latex || !latex.trim()) return;
      portLatex.emit(latex.trim());
      ws.emit('latex', latex.trim());
      updatePreview('');
      strokes = [];
      curStroke = null;
      hwPaint();
    }

    async function doInsertAtCursor() {
      const latex = typedInput ? typedInput.value.trim() : '';
      if (!latex) return { ok: false, error: 'empty' };
      portLatex.emit(latex);
      ws.emit('latex', latex);
      typedInput.value = '';
      updatePreview('');
      return { ok: true, latex };
    }

    async function doUpload(file) {
      if (!file) return;
      if (file.size > 512 * 1024) {
        if (statusEl) statusEl.textContent = 'Too large (>512 KB)';
        return;
      }
      const form = new FormData();
      form.append('image', file);
      if (statusEl) statusEl.textContent = 'Uploading to server (pix2tex)…';
      try {
        const ep = ENDPOINTS.find((e) => e.name === 'handwrite.upload');
        const url = ep && ep.path;
        const res = await fetch(url, { method: 'POST', body: form });
        if (res.status === 404) {
          if (statusEl) statusEl.textContent = 'Server fallback not available';
        } else if (res.status === 503) {
          if (statusEl) statusEl.textContent = 'Server fallback unavailable';
        } else if (!res.ok) {
          if (statusEl) statusEl.textContent = 'Server error ' + res.status;
        } else {
          const data = await res.json();
          if (data.latex) {
            doInsertLatex(data.latex);
          } else {
            if (statusEl) statusEl.textContent = 'Server recognised nothing';
          }
        }
      } catch (err) {
        if (statusEl) statusEl.textContent = 'Upload error: ' + err.message;
      }
      if (statusEl) statusEl.textContent = 'Ready';
    }

    function mount(containerEl) {
      container = containerEl;
      el = document.createElement('div');
      el.className = 'handwrite-pane';
      el.style.cssText = `display: flex; flex-direction: column; height: 100%; font-size: 11px; font-family: ui-sans-serif, system-ui, sans-serif;`;

      // Header
      const header = document.createElement('div');
      header.className = 'handwrite-header';
      header.style.cssText = `
        display: flex;
        align-items: center;
        gap: 6px;
        padding: 4px 8px;
        border-bottom: 1px solid var(--line);
      `;

      const title = document.createElement('div');
      title.textContent = 'Handwrite';
      title.style.cssText = 'font-weight: 600; font-size: 12px; color: var(--ink);';

      header.append(title);
      el.appendChild(header);

      // Body
      const body = document.createElement('div');
      body.className = 'handwrite-body';
      body.style.cssText = `flex: 1; overflow-y: auto; padding: 12px; display: flex; flex-direction: column; gap: 10px; background: var(--bg-inset);`;

      // Canvas + controls row
      const row1 = document.createElement('div');
      row1.style.cssText = `display: flex; gap: 10px; align-items: flex-start;`;

      // Canvas
      const canvasWrap = document.createElement('div');
      canvasWrap.style.cssText = `border: 1px solid var(--line); background: var(--bg-editor); cursor: crosshair; touch-action: none;`;

      canvas = document.createElement('canvas');
      canvas.className = 'handwrite-canvas';
      canvas.width = HW_W;
      canvas.height = HW_H;
      canvas.style.cssText = `display: block; width: ${HW_W}px; height: ${HW_H}px;`;
      cctx = canvas.getContext('2d');
      hwPaint();

      canvasWrap.appendChild(canvas);
      row1.appendChild(canvasWrap);

      // Controls
      const controls = document.createElement('div');
      controls.style.cssText = `display: flex; flex-direction: column; gap: 6px;`;

      const clearBtn = document.createElement('button');
      clearBtn.textContent = '🗑 Clear';
      clearBtn.title = 'Clear canvas';
      clearBtn.style.cssText = `padding: 6px 12px; font-size: 11px; background: var(--bg-inset); border: 1px solid var(--line); border-radius: 4px; color: var(--ink); cursor: pointer;`;
      clearBtn.addEventListener('click', clearCanvas);

      const recognizeBtn = document.createElement('button');
      recognizeBtn.textContent = 'Recognize';
      recognizeBtn.title = 'Recognize handwriting via WASM OCR';
      recognizeBtn.style.cssText = `padding: 6px 12px; font-size: 11px; background: var(--cyan); border: none; border-radius: 4px; color: var(--ink); cursor: pointer; font-weight: 600;`;
      recognizeBtn.addEventListener('click', async () => {
        if (!ready) {
          if (statusEl) statusEl.textContent = 'Model not ready';
          return;
        }
        if (statusEl) statusEl.textContent = 'Recognizing…';
        try {
          const latex = recognizeCanvas() || '';
          if (latex.trim()) { doInsertLatex(latex); }
          else { if (statusEl) statusEl.textContent = 'No formula recognized — write larger, darker strokes'; }
        } catch (e) {
          if (statusEl) statusEl.textContent = 'Error: ' + e.message;
        }
        if (statusEl) statusEl.textContent = 'Ready';
      });

      const uploadLabel = document.createElement('label');
      uploadLabel.style.cssText = `padding: 6px 12px; font-size: 11px; background: var(--bg-inset); border: 1px solid var(--line); border-radius: 4px; color: var(--ink); cursor: pointer;`;
      uploadLabel.textContent = 'Upload image';
      const uploadInput = document.createElement('input');
      uploadInput.type = 'file';
      uploadInput.accept = 'image/*';
      uploadInput.style.display = 'none';
      uploadInput.addEventListener('change', (e) => doUpload(e.target.files[0]));
      uploadLabel.addEventListener('click', () => uploadInput.click());

      controls.appendChild(clearBtn);
      controls.appendChild(recognizeBtn);
      controls.appendChild(uploadLabel);
      controls.appendChild(uploadInput);

      row1.appendChild(controls);
      body.appendChild(row1);

      // Status
      statusEl = document.createElement('div');
      statusEl.className = 'handwrite-status';
      statusEl.style.cssText = `padding: 4px 8px; font-size: 10px; color: var(--muted); background: var(--bg-panel); border-radius: 4px;`;
      statusEl.textContent = 'Loading WASM…';
      body.appendChild(statusEl);

      // Preview
      const previewLabel = document.createElement('div');
      previewLabel.textContent = 'Preview:';
      previewLabel.style.cssText = 'font-size: 10px; color: var(--muted);';
      body.appendChild(previewLabel);

      previewEl = document.createElement('div');
      previewEl.className = 'handwrite-preview';
      previewEl.style.cssText = `padding: 8px; border: 1px solid var(--line); border-radius: 4px; background: var(--bg-editor); min-height: 40px; color: var(--ink);`;
      previewEl.innerHTML = '<span style="color:var(--muted);font-size:12px;">Preview appears here…</span>';
      body.appendChild(previewEl);

      // Typed input
      const typedLabel = document.createElement('div');
      typedLabel.textContent = 'Or type LaTeX:';
      typedLabel.style.cssText = 'font-size: 10px; color: var(--muted);';
      body.appendChild(typedLabel);

      const typedRow = document.createElement('div');
      typedRow.style.cssText = 'display: flex; gap: 6px;';

      typedInput = document.createElement('input');
      typedInput.type = 'text';
      typedInput.placeholder = 'e.g. x^2 + \\alpha';
      typedInput.style.cssText = `flex: 1; padding: 4px 8px; font-size: 11px; background: var(--bg-editor); border: 1px solid var(--line); border-radius: 4px; color: var(--ink);`;
      typedInput.addEventListener('input', (e) => { updatePreview(e.target.value); });

      const insertTypedBtn = document.createElement('button');
      insertTypedBtn.textContent = 'Insert';
      insertTypedBtn.style.cssText = `padding: 4px 12px; font-size: 11px; background: var(--cyan); border: none; border-radius: 4px; color: var(--ink); cursor: pointer; font-weight: 600;`;
      insertTypedBtn.addEventListener('click', doInsertAtCursor);

      typedRow.appendChild(typedInput);
      typedRow.appendChild(insertTypedBtn);
      body.appendChild(typedRow);

      el.appendChild(body);

      // Pointer events
      canvas.addEventListener('pointerdown', startStroke);
      canvas.addEventListener('pointermove', moveStroke);
      window.addEventListener('pointerup', endStroke);
      window.addEventListener('pointerleave', endStroke);
      window.addEventListener('pointercancel', endStroke);

      container.appendChild(el);

      // Load WASM
      loadWasm();
    }

    return {
      mount,
      unmount() {
        if (autoTimer) clearTimeout(autoTimer);
        canvas.removeEventListener('pointerdown', startStroke);
        canvas.removeEventListener('pointermove', moveStroke);
        window.removeEventListener('pointerup', endStroke);
        window.removeEventListener('pointerleave', endStroke);
        window.removeEventListener('pointercancel', endStroke);
        if (el && el.parentNode) el.parentNode.removeChild(el);
        el = null;
        container = null;
        canvas = null;
        cctx = null;
        statusEl = null;
        previewEl = null;
        typedInput = null;
        strokes = [];
        curStroke = null;
        ready = false;
        model = null;
      },
      in: {},
      out: {
        latex: portLatex,
      },
      call: {
        insertAtCursor: callInsertAtCursor,
      },
    };
  },
};
