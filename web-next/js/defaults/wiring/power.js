/**
 * power.js — power-user wiring (M4.31+).
 *
 * Connects all M2 panes plus hooks for future panes via shared flows.
 * Missing panes are skipped gracefully so the script never crashes.
 */

import { wireDesignPanes } from './_common.js';
import { flowsSelection, flowsCheckBuild, flowsChrome, flowsData } from './_flows.js';

export default function wiring(ws) {
  const { wire, pane, on, endpoint } = ws;

  // Shared wiring infrastructure
  wireDesignPanes(ws);

  // Selection flow
  flowsSelection(ws);

  // Check/build flow
  flowsCheckBuild(ws);

  // Chrome/action flow
  flowsChrome(ws);

  // Data flow
  flowsData(ws);
}
