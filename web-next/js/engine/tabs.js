/**
 * tabs.js — Pure tab navigation logic (M2.08).
 *
 * Handles tab cycling with 'fixed' or 'mru' (most-recently-used) modes.
 */

/**
 * Compute the next active tab index.
 * @param {object} opts
 * @param {number} opts.count - Total number of tabs
 * @param {number} opts.active - Current active index (0-based)
 * @param {number} opts.dir - Direction: +1 (next) or -1 (previous)
 * @param {'fixed'|'mru'} opts.cycle - Cycle mode: 'fixed' wraps around, 'mru' uses MRU order
 * @param {number[]} [opts.mru] - MRU stack (most recent first), required if cycle='mru'
 * @returns {number} New active index
 */
export function nextTab({ count, active, dir, cycle, mru }) {
  if (count <= 1) return 0;
  if (cycle === 'fixed') {
    // Fixed wrapping: simple modulo arithmetic
    return (active + dir + count) % count;
  }
  
  // MRU mode
  if (!mru || mru.length === 0) {
    // Fallback to fixed if no MRU stack
    return (active + dir + count) % count;
  }
  
  // Find the current active tab in MRU stack
  const currentIdx = mru.indexOf(active);
  if (currentIdx === -1) {
    // Current tab not in MRU (shouldn't happen), fallback
    return (active + dir + count) % count;
  }
  
  // MRU stack is most recent first: mru[0] = most recent, mru[last] = oldest
  // dir === +1: advance in MRU order (towards older tabs), wrap at bottom
  // dir === -1: retreat in MRU order (towards newer tabs)
  //   - If at oldest (bottom of MRU), wrap to most recent (top)
  let nextIdx;
  if (dir === 1) {
    nextIdx = (currentIdx + 1) % mru.length;
  } else {
    nextIdx = currentIdx === mru.length - 1 ? 0 : currentIdx - 1;
  }
  
  return mru[nextIdx];
}

/**
 * Update the MRU stack when a tab is activated.
 * @param {number[]} mru - MRU stack (most recent first), mutated in place
 * @param {number} idx - Index of newly activated tab
 * @returns {number[]} The updated MRU stack (same array reference)
 */
export function touchMru(mru, idx) {
  const existingIdx = mru.indexOf(idx);
  if (existingIdx !== -1) {
    mru.splice(existingIdx, 1);
  }
  mru.unshift(idx);
  return mru;
}
