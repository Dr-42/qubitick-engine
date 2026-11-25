/*
 * split.js — where the seam between two columns may sit.
 *
 * Extracted from the inline script in web-next/index.html. This is the arithmetic of the
 * splitter in the console panes: given what each column needs and how much room there is,
 * which fractions are legal, and what happens to a stored fraction when the tile changes
 * size. It reads no DOM — the page measures (paneBodyW, measuredCol) and then asks here.
 *
 * The one idea that makes the rest fall out: the seam's range is the CONTENT's range, not
 * a percentage of the tile. A column that does not fit its own widest row wraps that row
 * into two lines, so a tile only offers a split when both columns can hold what they are
 * given — and says that it cannot, rather than folding a label into three lines.
 *
 * Fractions are of `avail` (the body's content box LESS the seam), never of the content
 * box, because the seam is a grid track of its own: a fraction of the whole box would
 * size the two columns as if the seam were free and push the right one under the body's
 * clipping.
 */

/** The seam's width. Must match `.ctl-host > .ctl-split` in the stylesheet, because the
 *  grid track and the element are the same 11 pixels. */
export const SPLIT_BAR = 11;

/** What the two columns share. */
export const splitAvail = (bodyW) => Math.max(1, bodyW - SPLIT_BAR);

/**
 * The seam's legal interval for this pane, as fractions of `avail`.
 * `colNeed` is {needL, needR} — what the left and right column need in layout px — or
 * null when nothing has been measured. `null` is the answer for a tile that cannot hold
 * two readable columns: the caller is expected to say so rather than wrap.
 */
export function splitRange(colNeed, avail){
  if(!colNeed || !colNeed.left || !colNeed.right) return null;
  if(!colNeed.left.length || !colNeed.right.length) return null;
  const a = avail > 0 ? avail : splitAvail(avail);
  const min = colNeed.needL, max = a - colNeed.needR;
  if(max < min) return null;
  return { min: min / a, max: max / a };
}

/** A fraction inside `range`, or the fraction unchanged when there is no range to obey. */
export function clampSplit(frac, range, clampFn){
  const clamp = clampFn || ((v, lo, hi) => Math.min(hi, Math.max(lo, v)));
  return range ? clamp(frac, range.min, range.max) : frac;
}

/** The fraction a pane should render at: null when it is unsplit or cannot be split. */
export function innerSplit(stored, range, clampFn){
  if(typeof stored !== 'number') return null;
  if(!range) return null;                    // too narrow: one column, for now
  return clampSplit(stored, range, clampFn);
}

/**
 * Re-clamp a stored fraction against a new width.
 *
 * Returns undefined when there is nothing to decide yet ("not measured" is not the same
 * as "too narrow"), null when the tile is measured and genuinely cannot hold two columns,
 * and the clamped fraction otherwise. The caller writes that answer back.
 *
 * The clamp happens on the fraction, which is the same thing as clamping in px and
 * dividing again — `clamp(w*f, min*w, max*w)/w` — because `min` and `max` were themselves
 * divided by `w`. Writing it this way removes a multiply/divide pair that looked like it
 * made a difference and never did.
 */
export function refitFraction(stored, colNeed, avail, clampFn){
  if(typeof stored !== 'number') return undefined;         // nothing stored: nothing to do
  const range = splitRange(colNeed, avail);
  if(!range){
    // unmeasured (no needs at all) and too narrow are different answers
    const measured = !!(colNeed && colNeed.left && colNeed.right && colNeed.left.length && colNeed.right.length);
    return measured ? null : undefined;
  }
  return clampSplit(stored, range, clampFn);
}

/**
 * The grid's three tracks for a split body, as CSS track lists.
 * `frac` is the seam's fraction of `avail`; the two column tracks are percentages of the
 * CONTENT box (which is `avail + SPLIT_BAR`) that leave the seam's own track unclaimed.
 */
export function splitTracks(needL, needR, frac, avail){
  const total = avail + SPLIT_BAR;
  const pctL = (frac * avail) / total * 100;
  const pctR = ((1 - frac) * avail) / total * 100;
  return `minmax(${needL}px, ${pctL.toFixed(3)}%) ${SPLIT_BAR}px minmax(${needR}px, ${pctR.toFixed(3)}%)`;
}

/** The width a split pane may not be dragged below: both columns, the seam, and whatever
 *  border the pane draws around its body (`chrome` = pane width - body width). */
export const splitMinWidth = (colNeed, chrome = 0) =>
  (colNeed ? colNeed.needL + colNeed.needR : 0) + SPLIT_BAR + chrome;