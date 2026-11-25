/**
 * live/syntax-button.js — the editor bar's Aa switch, painted.
 *
 * A pure function of the two flags a tile keeps (`syntax`, `syntaxWired`) and the element, so the rule
 * it encodes is readable in one place: while a boolean is wired to the tile's `syntax` port THE WIRE IS
 * AUTHORITATIVE — the control is disabled and its tooltip says why, rather than inviting a click that
 * the next delivery would undo.
 */
export function paintSyntaxButton(btn, { on, wired }) {
  if (!btn) return;
  btn.textContent = on ? 'Aa' : 'aa';
  btn.classList.toggle('on', !!on);
  btn.classList.toggle('wired', !!wired);
  btn.disabled = !!wired;
  btn.title = wired
    ? 'syntax highlighting: driven by the wire on this tile’s syntax port'
    : 'syntax highlighting: ' + (on ? 'on' : 'off') + ' — click to toggle';
}
