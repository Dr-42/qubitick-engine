/*
 * console.js — a settings pane that is one column until you ask for two.
 *
 * Extracted from the inline script in web-next/index.html. These panes are a stack of
 * labelled rows in a grid of two columns (.ctl-host .row), so on a wide tile most of the
 * width is empty and every section sits above the next. The splitter here turns that stack
 * into two columns of sections, and — like the tile grips — the gesture has one meaning:
 * the boundary under the pointer moves.
 *
 * It is a real splitter, not a reflow: the sections keep their own layout, the two columns
 * scroll independently, and the position is saved with the workspace. The default is one
 * column (`null`), so the shipped arrangement is unchanged until the splitter is dragged —
 * except by the widened minimum in `paneMin()`, which is what stops a two-column pane from
 * being dragged narrower than the splitter can survive.
 *
 * THE ARITHMETIC is js/app/split.js and has node tests: the seam's permitted range, what a
 * stored fraction means when the tile is resized, and the three grid tracks. What is here is
 * the layout measurement (a section's natural width, a pane body's layout width) and the
 * gesture.
 *
 * A column has to hold its widest row on one line, and how wide that is belongs to the
 * section's content: the widest sections ask for 210-320 layout px, and a column under that
 * wraps its own label into three lines. So the floor is measured from the sections
 * themselves (measuredCol) and the seam's range follows the sections' own division
 * (planColumns) instead of a bare fraction.
 *
 * The host injects what it owns — the camera (layout px vs screen px), the grip geometry (a
 * seed must sit clear of the tile's own resize grips), how to re-render a tile and how to
 * save. Nothing in this file knows the app.
 */
import { SPLIT_BAR, splitAvail as availOf, splitRange as rangeOf, clampSplit as clampSplitTo,
         innerSplit as innerSplitOf, refitFraction, splitTracks, splitMinWidth } from '../app/split.js';

/**
 * @param {object} host
 * @param {{x:number,y:number,z:number}} host.cam     the camera, read for its zoom
 * @param {object} host.splits                        kind -> fraction (`null` = one column)
 * @param {(p) => void} host.render                   re-render a tile (a split changes its markup)
 * @param {() => void} host.save                      persist the arrangement
 * @param {(msg: string) => void} host.toast
 * @param {() => {band:number}} host.gripAt           the tile-grip band at this zoom
 * @param {(wr) => {band:number}} host.gripBand
 * @param {{w:number,h:number}} host.minSize          the smallest a tile may be
 * @param {Document} host.doc                         for the off-screen measuring ruler
 */
export function createConsole(host) {
  const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

  /* A column has to hold its widest row on one line, and how wide that is belongs to the
     section's content, not to a constant: the widest sections here ask for 210–320 layout
     px, and a column under that wraps its own label into three lines. So the floor is
     measured from the sections themselves (measuredCol) and the seam's permitted range
     follows the sections' own division (planColumns) instead of a bare fraction. */
  /** A conservative floor for a split pane whose columns have not been measured yet — the
   *  only case being a workspace that arrives already split, before its first render. */
  const SPLIT_MIN_W = 420;
  /** The pane body's width in LAYOUT px: the space the section content is measured in, and
   *  therefore the only space a layout-space `needL`/`needR` may be compared against. The
   *  pane is scaled by a transform, but `clientWidth` is the layout width in CSS px before
   *  that transform, which is exactly the space the rows were laid out in. Reading this in
   *  screen px instead (width × zoom) makes the two sides of every comparison differ by the
   *  zoom — and the padding comes off because the columns are placed inside it, so a need
   *  that ignores the padding is a need that disagrees with the fold. */
  function paneBodyW(p){
    const b = p.body, cs = getComputedStyle(b);
    return Math.round(b.clientWidth - (parseFloat(cs.paddingLeft) || 0) - (parseFloat(cs.paddingRight) || 0));
  }
  /** What the two columns share: the body's content box less the seam. The seam is a GRID
   *  GAP, which sits between the tracks and is not part of either, so a fraction of the
   *  content box alone would size the columns as if the gap were free and push the right
   *  column out under the body's clipping. Every fraction here — the range, the pointer,
   *  the drag — is a fraction of THIS number. */
  const splitAvail = (p) => availOf(paneBodyW(p));
  /** What a section needs to keep its rows on one line, in layout px. The section is taken
   *  out to an off-screen ruler at its natural width and then put straight back where it
   *  was: a render that fails to hand a section on to a column would otherwise finish with
   *  an empty column and a row of controls that never got wired up. */
  let colRuler = null;
  function measuredCol(sec, home){
    if(!sec) return 0;
    if(!colRuler){
      colRuler = document.createElement('div');
      colRuler.className = 'ctl-host';
      colRuler.style.cssText = 'position:absolute;left:-10000px;top:0;visibility:hidden;width:max-content';
      document.body.append(colRuler);
    }
    colRuler.append(sec);
    const w = Math.ceil(sec.getBoundingClientRect().width);
    home.append(sec);
    return w;
  }
  /** Divide the sections into two columns the way a reader would: in order, each one going
   *  to whichever column is so far the shorter one. That keeps the reading order down each
   *  column and puts both columns to use, which a fraction of the section count cannot do
   *  (`prefs` has two sections, and `round(2 * 0.5)` left its second one alone in a column
   *  of its own — a whole column for a one-line note). Each column also reports the width
   *  its own widest section needs, and that is what bounds the seam: two columns may never
   *  be narrower than what is in them, so the range follows the content, not a percentage
   *  of the count. Measuring also hands every section back to the body, so nothing is left
   *  behind in the ruler when the split is refused for want of width. */
  function planColumns(sections, p){
    const left = [], right = []; let lh = 0, rh = 0;
    const heights = sections.map(s => s.getBoundingClientRect().height);
    /* A pane that is off screen is not laid out (`.pane.off .pane-body` is
       `content-visibility: hidden`), so every section measures 0 and "the shorter column"
       is always the left one — which would put the whole stack in one column and leave the
       other empty until the next render. When there is no height to go by, divide by count
       instead: the sections keep their order, both columns get work, and the widths below
       are still measured, because those are taken off screen in the ruler. */
    const knowHeights = heights.some(h => h > 0);
    if(!knowHeights){
      const half = Math.ceil(sections.length / 2);
      for(let i = 0; i < sections.length; i++) (i < half ? left : right).push(sections[i]);
    } else {
      sections.forEach((s, i) => {
        if(lh <= rh){ left.push(s); lh += heights[i]; } else { right.push(s); rh += heights[i]; }
      });
    }
    const need = (col) => col.reduce((m, s) => Math.max(m, measuredCol(s, p.body)), 0);
    return { left, right, needL: need(left), needR: need(right) };
  }
  /** Where the seam may sit for this pane: between the two columns' own needs. null = this
   *  tile cannot hold two readable columns, and is then told so rather than wrapped. */
  function splitRange(p){
    if(!p.colNeed) return null;
    return rangeOf(p.colNeed, splitAvail(p));
  }
  function innerSplit(p){
    const r = splitRange(p);
    // the tile is too narrow (or unmeasured): one column, for now
    return innerSplitOf(host.splits[p.kind], r, clamp);
  }
  const clampSplit = (p, f) => clampSplitTo(f, splitRange(p), clamp);
  /** Re-clamp a pane's stored split against a new width. A pane that has not been measured
   *  yet is left alone: "no range" means both "this tile is too narrow" and "nothing has
   *  laid these sections out yet", and only the first is a reason to drop the split. That
   *  distinction is what lets a split restored from a save survive until its first render,
   *  which is the only moment it can be measured. */
  function refitSplit(p){
    if(!p) return;
    const next = refitFraction(host.splits[p.kind], p.colNeed, splitAvail(p), clamp);
    if(next !== undefined) host.splits[p.kind] = next;
  }
  function renderControlColumns(p, sections){
    /* The division is decided from the sections this pane actually built, in layout px, once
       per render — for EVERY control pane, split or not, because the answer is what the
       edge seed has to offer: "there is room for two columns" and "this tile is too narrow"
       are both questions about the sections' own widths, and a pane that has never been
       split still has to answer them. The measurement is an off-screen ruler per section,
       and the sections come straight back. `colNeed` therefore survives even when the tile
       is too narrow to use it: the seed's tooltip, the resize minimum and refitSplit all
       read it. */
    p.colNeed = null;
    if(sections.length >= 2){
      for(const s of sections) p.body.append(s);      // height is only measurable in the DOM
      p.colNeed = planColumns(sections, p);            // ...and measurement hands them back
      // measuring walks the sections through the ruler, so the stack comes back in the
      // columns' order rather than its own: put it back before anything reads the markup,
      // or a one-column pane would draw its sections in the order the SPLIT would have had
      for(const s of sections) p.body.append(s);
    }
    const frac = innerSplit(p);
    if(frac == null){
      p.body.style.flexDirection = '';
      p.body.style.overflow = '';
      p.body.style.display = '';           // clear whatever the last (split) render set
      p.body.style.gridTemplateColumns = '';
      p.body.style.columnGap = '';
      if(sections) for(const s of sections) p.body.append(s);
      // A one-column pane offers the split at its own right edge: slip the pointer in
      // there and the seam appears under it, then drag. A control that only exists
      // after it has been used is a control nobody finds — EXCEPT when the tile is too
      // narrow to hold two columns, where the honest answer is that there is nothing
      // to offer (the seed is then a hint about that, not a control).
      p.body.append(splitSeed(p));
      // the seed has to sit clear of the tile's own resize grips, which live just
      // inside the pane's right edge and belong to the gesture that resizes the TILE —
      // two draggable things at the same coordinate is one thing nobody can grab
      seedInset(p);
      return;
    }
    const left = document.createElement('div');  left.className = 'ctl-col';
    const right = document.createElement('div'); right.className = 'ctl-col';
    const plan = p.colNeed;
    for(const s of plan.left) left.append(s);
    for(const s of plan.right){
      /* the first section of the right column keeps the rule that separates it from the
         column to its left; the ones below it keep the rules between rows */
      if(s !== plan.right[0]) s.classList.add('col-below');
      right.append(s);
    }
    /* Two columns are a GRID, not a flex row: the seam is a gap of a known size, so a
       PERCENT of the space can place the fold where the pointer was — a fraction of `fr`
       would not, because `fr` distributes what is left after the minimums instead of the
       space itself — and each column can carry its content width as a minimum, so neither
       is starved when the tile is at the narrow end of its range. The percentages are of
       the body width, which is exactly what `frac` is measured against, and the body's own
       padding is dropped for a split: the gutter a single column wanted around itself is
       the space the two columns need for themselves. Set inline, where it cannot depend on
       which of two stylesheets wins the cascade. */
    const avail = splitAvail(p);
    p.body.style.display = 'grid';
    /* THREE tracks: the two columns and the seam between them. The seam is a real element
       (it is what the pointer grabs), so it has to have a track of its own — with two
       tracks and a `column-gap` the seam takes a cell and the right column wraps to the
       next row, landing back under the left one. The two column tracks are percentages of
       the content box that leave the seam's width unclaimed, so the fold is exact. */
    p.body.style.gridTemplateColumns = splitTracks(plan.needL, plan.needR, frac, avail);
    p.body.style.columnGap = '0px';
    p.body.style.overflow = 'hidden';
    const bar = document.createElement('div');
    bar.className = 'ctl-split'; bar.dataset.pane = p.id;
    bar.title = 'drag to re-proportion the two columns · double-click for one column';
    p.body.append(left, bar, right);
    bar.addEventListener('dblclick', () => { host.splits[p.kind] = null; p.colNeed = null; host.render(p); host.save(); });
    bar.addEventListener('pointerdown', (e) => { e.preventDefault(); e.stopPropagation(); dragSeam(p, e); });
  }

  /** Where the seam sits for this pane: the pointer's fraction across the pane body. */
  const splitFromX = (p, clientX) => {
    const r = p.body.getBoundingClientRect();
    const cs = getComputedStyle(p.body);
    const pad = (parseFloat(cs.paddingLeft) || 0) * host.cam.z;      // the rect is in screen px
    return clampSplit(p, ((clientX - r.left - pad) / host.cam.z) / splitAvail(p));
  };

  /** Drag the seam of a two-column control pane.
   *
   *  The move handler cannot live on the thing it moves. Re-proportioning the columns
   *  means redrawing the pane, and redrawing replaces every element in it — including
   *  the seam under the pointer. So the drag listens on the document, identifies itself
   *  by pane, and the seam is only ever re-rendered, never held. */
  function dragSeam(p, e){
    const sx = e.clientX, start = host.splits[p.kind];
    const mv = (ev) => {
      host.splits[p.kind] = clampSplit(p, start + ((ev.clientX - sx) / host.cam.z) / splitAvail(p));
      host.render(p);
    };
    const up = () => {
      document.removeEventListener('pointermove', mv); document.removeEventListener('pointerup', up);
      document.body.style.cursor = '';
      dirty = true; host.save();
    };
    document.body.style.cursor = 'ew-resize';
    document.addEventListener('pointermove', mv); document.addEventListener('pointerup', up);
  }

  /** The smallest a tile may be dragged. A tile that has asked for two columns needs room
   *  for both of them plus the seam, measured in the pane's own outer width: the columns
   *  live in the body, which is the pane minus its border, so the border comes off the
   *  body's width and has to be added back or the minimum is two pixels short of the
   *  range it promises. */
  function paneMin(p){
    if(p && typeof host.splits[p.kind] === 'number'){
      const c = p.colNeed;
      if(c){
        const chrome = p.wr.w - paneBodyW(p);      // border (+ padding, if any) around the body
        return { w: splitMinWidth(c, Math.max(0, chrome)), h: host.minSize.h };
      }
      return { w: SPLIT_MIN_W, h: host.minSize.h };  // not measured yet (restored from a save)
    }
    return host.minSize;
  }

  /** The "make this pane two columns" affordance, in the empty right edge of a
   *  one-column control pane. It is a hint until it is grabbed: pointerdown splits the
   *  pane at the pointer and hands the gesture straight to the seam, so one drag from
   *  the edge both creates the split and places it. */
  /** Put the split seed just inside the pane's right-edge resize grips, in screen
   *  pixels: the grips are sized per zoom, so a fixed inset is wrong at one zoom or the
   *  other (they are 16 screen px wide, and at low zoom that is a large layout distance). */
  function seedInset(p){
    const { band } = host.gripAt();
    p.body.style.setProperty('--seed-inset', (band / 2 + 9) + 'px');
    p.body.style.setProperty('--seed-w', host.gripBand(p.wr).band + 'px');
  }

  function splitSeed(p){
    const el = document.createElement('div');
    el.className = 'ctl-seed'; el.dataset.pane = p.id;
    const room = !!splitRange(p);
    el.dataset.room = room ? '1' : '0';
    el.title = room ? 'drag into two columns'
                    : `two columns need about ${SPLIT_MIN_W}px of tile (this one is ${paneBodyW(p)}px)`;
    el.addEventListener('pointerdown', (e) => {
      e.preventDefault(); e.stopPropagation();
      if(!splitRange(p)){ host.toast('this tile is too narrow for two columns — resize it wider first'); return; }
      host.splits[p.kind] = splitFromX(p, e.clientX);
      host.render(p);                    // the seam appears under the pointer
      dragSeam(p, e);                   // ...and the same gesture keeps moving it
    });
    return el;
  }

  return {
    BAR: SPLIT_BAR,
    bodyW: paneBodyW,
    avail: splitAvail,
    range: splitRange,
    paneMin,
    refit: refitSplit,
    renderColumns: renderControlColumns,
  };
}
