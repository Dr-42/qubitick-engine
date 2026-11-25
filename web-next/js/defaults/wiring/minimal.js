/**
 * minimal.js — minimal wiring (M4.31+).
 *
 * Only the essentials: nothing auto-connected; user wires manually.
 */

export default function wiring(ws) {
  // No automatic connections in minimal mode.
  // The user wires panes via ws.wire() in design mode or scripts.
}
