/**
 * registry.js — Pane registry (M2.04 + M2.05).
 *
 * Stores and retrieves pane definitions.
 * Registers all 24 pane kinds from D-028 with ports from Appendix B.
 */

import { definePane } from './define.js';
import logoPane from './logo.js';
import controlsPane from './controls.js';
import statsPane from './stats.js';
import treePane from './tree.js';
import nodesPane from './nodes.js';
import editorPane from './editor.js';
import editorVimPane from './editor-vim.js';
import editorSymbolsPane from './editor-symbols.js';
import editorCitationsPane from './editor-citations.js';
import editorCtxMenuPane from './editor-ctxmenu.js';
import newNodePane from './new-node.js';
import goalPane from './goal.js';
import irPane from './ir.js';
import leanPane from './lean.js';
import renderPane from './render.js';
import outputPane from './output.js';
import diagPane from './diag.js';
import buildPane from './build.js';
import slatePane from './slate.js';
import symbolAddPane from './symbol-add.js';
import settingsThemePane from './settings-theme.js';
import settingsTypographyPane from './settings-typography.js';
import settingsLayoutPane from './settings-layout.js';
import graphPane from './graph.js';
import handwritePane from './handwrite.js';
import scriptEditorPane from './script-editor.js';
import layoutOutlinePane from './layout-outline.js';
import layoutJsonPane from './layout-json.js';
import projectPane from './project.js';
import corpusPane from './corpus.js';
import projectsPane from './projects.js';

/**
 * Create a pane registry.
 * @returns {object} Registry API
 */
export function createPaneRegistry() {
  const kinds = new Map();

  /**
   * Internal register function.
   * @param {object} def - Pane definition (validated by definePane)
   * @returns {object} The registered definition
   * @throws {TypeError} If duplicate kind
   */
  function registerPane(def) {
    const validated = definePane(def);
    const kind = validated.kind;
    if (kinds.has(kind)) {
      console.log("Overwriting pane kind:", kind);
    }
    kinds.set(kind, validated);
    return validated;
  }

  /**
   * Register all 24 pane kinds with their port definitions from Appendix B.
   */
  function registerAll() {
    // logo: in { file, connection }, out {}, call {} (M4.05)
    registerPane(logoPane);

    // controls: in {}, out { ui:build, ui:slate, ui:pdf, ui:handwrite, ui:symbol, ui:settings, ui:graph, ui:fullscreen }, call {}
    registerPane(controlsPane);

    // nodes: in { highlight, corpus }, out { selection }, call { focus } (M4.09)
    registerPane(nodesPane);

    // tree: in { files, current }, out { selection }, call { reveal } (M4.08)
    registerPane(treePane);

    // editor: in { file, goto }, out { buffer, cursor, dirty, saved }, call { save, goto } (M4.10)
    registerPane(editorPane);

    // editor-vim: vim mode variant of editor (M4.11)
    registerPane(editorVimPane);

    // editor-symbols: symbol-aware editor (M4.12)
    registerPane(editorSymbolsPane);

    /* editor-citations (M4.13) and editor-ctxmenu (M4.15) are THE ENGINE HOST'S EDITORS. The canvas
       mounts one editor as a live pane (js/live/editor.js, with js/live/cite-link.js for following a
       citation tag and js/live/check-strip.js for the check chip), so these two are variants of a pane
       the canvas already has rather than kinds it is missing: they are registered here, for this host,
       and deliberately not in js/host/canvas/panes.js. The one part with no canvas counterpart is the
       right-click menu — a canvas tile leaves the browser's own menu in place. */
    registerPane(editorCitationsPane);
    registerPane(editorCtxMenuPane);

    // goal: in { cursor, buffer }, out { goalAt }, call { refresh } (M4.17)
    registerPane(goalPane);

    // ir: in { buffer }, out { applied }, call { apply } (M4.18)
    registerPane(irPane);

    // lean: in { buffer, file }, out {}, call { copy } (M4.19)
    registerPane(leanPane);

    // render: in { buffer }, out {}, call { print, pdf } (M4.20)
    registerPane(renderPane);

    // output: in { log, clear }, out {}, call { clear } (M4.21)
    registerPane(outputPane);

    // diag: in { diagnostics }, out { jump }, call { rerun } (M4.22)
    registerPane(diagPane);

    /* build: in { diagnostics }, out { jump }, call { rerun } — the corpus build as a panel:
       how many files verified and what the corpus still admits. `diag` says what is wrong and
       where; this says whether it built and what it takes on faith, which are different
       questions with different answers (a corpus with one admitted theorem builds clean). */
    registerPane(buildPane);

    // graph: in { highlight, graphData }, out { selection }, call { resetCamera } (M4.26)
    registerPane(graphPane);

    /* slate: in { ink }, out { latex, html }, call {} (M4.23). A canvas kind too
       (js/host/canvas/panes.js), so it is a tile in both hosts. */
    registerPane(slatePane);

    /* symbol-add: in {}, out { symbol }, call { open } (M4.24). A MODAL (`position: fixed`, `display:
       none`, opened by its `open` call) that posts to `symbols.add`, which IS implemented
       (crates/api/src/router.rs:21 — it inserts a DB-level symbol: tex, display, lean name, fixity).
       It is not a canvas kind because a canvas tile is a tile and this pane is a modal with no tile
       shape; the canvas's surface for adding a corpus node — a definition included — is
       js/panes/new-node.js, which writes the file the engine ingests. And a symbol AUTHORED in the
       corpus is a definition node whose `Type:` (primitive) or `Sema:` (composite) declares it
       (js/live/new-node-plan.js::stubFor writes the definition stub), which is a different thing from
       the row this modal creates. */
    registerPane(symbolAddPane);

    // handwrite: in {}, out { latex }, call { insertAtCursor } (M4.29)
    registerPane(handwritePane);

    // settings-theme: in {}, out { prefs }, call {} (M4.25)
    registerPane(settingsThemePane);

    // settings-typography: in {}, out { prefs }, call {} (M4.26)
    registerPane(settingsTypographyPane);

    // settings-layout: in {}, out { prefs }, call {} (M4.27)
    registerPane(settingsLayoutPane);

    // layout-json: in { layout }, out { json }, call { save } — the module
    registerPane(layoutJsonPane);

    /* the swap: a project source, and the corpus of whatever project is wired into it */
    registerPane(projectPane);
    registerPane(corpusPane);
    /* the LIST of projects, and the calls that change it. Its controls come from the HOST
       (`ctx.projects`, `ctx.addProject`, … supplied through js/app/project-services.js), which is
       what makes it a pane rather than a piece of the canvas. */
    registerPane(projectsPane);

    // layout-outline: in { layout }, out { outline, picked }, call { focus } — the module
    registerPane(layoutOutlinePane);

    // script-editor: in { layout }, out {}, call {} (M4.30)
    registerPane(scriptEditorPane);

    // stats: in { stats }, out {}, call {} (M4.07)
    registerPane(statsPane);

    // new-node: in { parent }, out { node }, call { open } (M4.16)
    registerPane(newNodePane);
  }

  // Auto-register all standard panes
  registerAll();

  // Public API
  return {
    registerPane,
    getPane: (kind) => kinds.get(kind),
    getPaneDef: (kind) => kinds.get(kind),
    hasPane: (kind) => kinds.has(kind),
    kinds: () => [...kinds.keys()],
    size: () => kinds.size,
  };
}
