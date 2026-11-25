/**
 * lock.js — design-mode lock toggle (M3.01).
 *
 * The lock state is a value port: `true` = locked (design chrome hidden,
 * pane drag disabled), `false` = unlocked (design mode active).
 * Defaults to locked (`true`). Not persisted — always boots locked.
 *
 * Exposed as `ws.lock`.
 */

import { valuePort } from '../core/ports.js';

/**
 * Create the design-mode lock.
 * @returns {{get: ()=>boolean, set:(v:boolean)=>void, subscribe:(fn)=>function, toggle:()=>void, destroy:()=>void}}
 */
export function createLock() {
  const locked = valuePort(true); // default: locked

  return {
    /** Current lock state (true = locked). */
    get: locked.get,

    /** Set lock state. Notifies subscribers. */
    set: locked.set,

    /** Subscribe to lock-state changes. Receives current value immediately. */
    subscribe: locked.subscribe,

    /** Toggle locked <-> unlocked. */
    toggle() {
      locked.set(!locked.get());
    },

    destroy() {},
  };
}
