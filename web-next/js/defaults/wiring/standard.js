/**
 * standard.js — default wiring for the standard preset (M4.31+).
 *
 * Connects panes via ws.wire(). User-editable; stored via storage adapter.
 * Uses shared flows from _flows.js plus _common.js helpers.
 */

import { wireDesignPanes } from './_common.js';
import { flowsSelection, flowsCheckBuild, flowsChrome, flowsData } from './_flows.js';

export default function wiring(ws) {
  const { wire, pane, on, emit } = ws;

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
