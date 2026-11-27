# `web-next/js/` — the client

The client is this module tree plus two pages. **207 modules** (`find web-next/js -name '*.js' | wc
-l`), loaded from the two entries and from the manifests and presets they read at runtime.

| Page | Lines | App |
|---|---|---|
| `../index.html` | 33 | the **canvas**: a pannable world of tiles |
| `../shell.html` | 32 | the **engine host**: the workspace as a tree of roots |

Both load the same `/js/main.js`, which picks by what the page has — a **world** element means
`js/host/canvas/main.js`, a **`#root`** means `js/engine/index.js::createEngine`. There is no fork in
the panes: a pane module runs in either app, unchanged, because both hosts hand it the same `ctx`.

```bash
node tools/check.mjs                      # lint + unit tests + the browser gates, in one go
node tools/lint.mjs                       # the rules below, and the allowances that must stay true
node --test "tests/unit/*.test.mjs"       # camera, tiling, layout trees, wires, routes
node tools/verify-default-view.mjs        # the shipped arrangement, in a real browser
node tools/shot.mjs <url> <out.png>       # any page to a PNG, failing on a console error
```

There is no `package.json` and nothing to install: `node --test` is in node, and the browser gates
drive the machine's chromium through `tools/page.mjs` (the DevTools protocol over node's own
`WebSocket`).

## The two apps

| | canvas (`index.html`) | engine (`shell.html`) |
|---|---|---|
| what a screen is | tiles with world rects, gathered into clusters | roots, each with a split/tab tree |
| the plumbing | `js/host/canvas/*`, the canvas's own state | `js/core/{frame,bus,scope,wire}.js` + `js/net/endpoint-node.js` |
| mounting a pane | `js/app/pane-host.js`, by the table in `js/panes/app-mounts.js` | `js/panes/instances.js`, into a layout tree's leaves |
| the pointer | `js/world/{canvas-input,tile-gestures,grips}.js` | `js/input/*` |
| the transform | both write through `js/engine/world.js` — the canvas calls it synchronously, the shell through its frame | |
| the camera | one module, `js/app/camera.js`, for both apps; the frame loop is injected, so each app owns its own | |
| the ports | one loop, `js/app/pane-ports.js`, connects a pane's in/out/call ports to whichever host is running it | |
| the document | a canvas arrangement exports as an engine layout document (`mode: 'canvas'`); a layout document restores into the canvas through `js/app/layout-to-canvas.js` | |

One rule of the canvas's model survives the conversion: a cluster holds **one tile per kind**
(`js/world/clusters.js` keys its members by kind and refuses a second), so a layout document with
two panes of one kind in a cluster converts to the members the canvas can represent.

## The map

| Directory | Modules | What lives there |
|---|---|---|
| `js/core/` | 13 | the four singletons — `bus`, `frame` (the only home of `requestAnimationFrame`), `storage` (the only home of `localStorage`), `scope` — plus wire and config |
| `js/defaults/` | 8 | the **only** place route strings live (`endpoints.js`), plus the wiring and scroll presets |
| `js/input/` | 7 | keyboard map, pointer, touch, wheel, scroll hooks |
| `js/panes/` | 47 | one module per pane kind, the mounts table, `source-of`, user panes, the registry |
| `js/live/` | 19 | the panes that need a live object rather than a render — tree, graph, editor — plus the state machines behind the field and the theme, `proof-row.js` and `induce.js` (what the editor's two chords write), and the wire layer (`wire/`) |
| `js/app/` | 27 | pure pieces: `qbit-lines`, `math`, `viewport`, `camera`, `tiling`, `snap`, `split`, `docs`, `layout-doc`, `project-services`, `diagnostics`, `graph-run` |
| `js/world/` | 14 | the canvas's world: camera motion, tile grips, gestures, clusters, the console splitter, focus, restore |
| `js/host/canvas/` | 16 | the canvas host: boot order, the tile factory, the panes mounts table, wires, persistence, the runtime assembly |
| `js/engine/` | 13 | the engine host's skeleton: world, camera, instances, renderer, HUD, modals, LOD, tabs, sashes, root handles |
| `js/layout/` | 18 | the workspace as data: schema, store, split tree, serialize, presets, validation |
| `js/design/` | 9 | palette, design bar, dialogs, device selection, panic reset |
| `js/lib/` | 9 | shared helpers: corpus data, symbols, citations, KaTeX macros, LaTeX OCR |
| `js/net/` | 3 | the request layer and the route table |
| `js/main.js` | 1 | the entry both pages load |

A module belongs where it is *used*, not where it was first written: the canvas host's directory
holds what arranges tiles, and anything a pane can use from either host lives in `js/app/`, `js/lib/`
or `js/panes/`.

## A pane

```js
export default {
  kind: 'corpus',
  title: 'Corpus · source',
  ports: { in: { project: 'value' }, out: { files: 'value', stats: 'value' }, call: { reload: 'call' } },
  create(ctx) { return { mount(el), unmount() }; }
};
```

`ctx` is everything a pane may know (`js/host/canvas/services.js` hands it over; the engine host
supplies the same fields from `ws`/`event`):

| field | what it is |
|---|---|
| `data` | the corpus the app has loaded (files, graph, symbols, stats) |
| `editor` | the editor's ports: `buffer`, `file`, `cursor` |
| `url(name)` | route lookup through `js/defaults/endpoints.js` — never a literal path |
| `ui` | element builders and `esc` (`js/app/ctl.js`) |
| `wires` | the live wire list, for the panes that show it |
| `projects`, `params` | the server's project list; a node's own parameters |

**Ports are the only coupling between panes.** No pane calls another: a wire carries a typed value
from one pane's out-port to another's in-port (`js/live/wire/`), delivery happens on change, and
removing a wire delivers `undefined`, which every consumer reads as *gone* rather than *empty*.
A pane may declare a port or a handler; both hosts read both conventions (`js/app/pane-ports.js`).

The canvas's kinds and their ports are declared in one table (`js/host/canvas/panes.js`, **27
kinds**). `js/panes/source-of.js::MODULE_OF` answers "which file implements this kind" for **27
kinds across both hosts** — it is what "edit source" opens. A workspace can declare a pane
kind of its own as data, code body included, in `../panes/manifests.json`
(`js/panes/user-panes.js`); user code that throws fails into its tile, not into the page.

## Making a node

`js/panes/new-node.js` writes a node into the corpus, and every part of the name comes from the
corpus it is being written into:

| | where it comes from |
|---|---|
| the LISTS | the directories the corpus's files are in (`ctx.data.files`), plus the path the pane was opened on — the engine host gives a pane its `parent` and no `ctx.data` at all |
| the TOKEN | the one the files in that directory already use (`T_BT_01_X.qbit` → `BT`); a project whose files carry none is named after the project (`.../corpus-b` → `CORPUS_B`) |
| the NUMBER | the next free one in that directory, from the names already there |
| the BODY | a header and a `Phys: TODO`. The header grammar is real and not negotiable (`T.<token>.<n>`, `D.<token>.<n>`, `A<n>`; `D10. Name` is `unknown node header`), and the statement is the author's |

Two shapes stay apart: a **flat** corpus (files at its root, no era/strata level — its nodes belong
at the root, and the engine gives them `Era0/Strata0` from their path anyway) and a pane with **no
corpus behind it** (then the answer is the engine's default tree, not a guess about the user's
directories). The `parent` in-port is a real value port, so a host can tell the pane which directory
it was opened on.

## The pane header

The header's controls give up their width while they are invisible: `.pane-head .actions` is
`width: 0; min-width: 0; overflow: hidden` (`min-width` unpins it — a flex item's automatic minimum
size is its content's, and this content is eight buttons wide), and `.pane:hover` / `:focus-within`
hands the width back at `flex: 0 0 auto`. The title is the flexible one in that row, so under the
pointer it yields instead of the buttons being clipped. A coarse pointer pays one tap for the
controls and nothing else: a tap sets `:hover` on the pane.

The rule, which a rendering check states: at rest every shipped title is drawn whole
(`scrollWidth <= clientWidth + 1`) and the group is 0 px wide at `opacity: 0`; under the pointer the
group is back over 100 px and the row still does not overflow.

## Enter writes a proof row

In an editor tile, type the statement you want next and press Enter. The line becomes the next row
of the proof above it — the sign that comes, the sentence as typed, and the citation that justifies
the step, in the column the block already lines its tags up in:

```
(a \Delta b) \Delta (b \Delta a)
  = a \Delta (b \Delta (b \Delta a))                [A1.1]
```

Nothing is fetched until the decision is made (`js/live/proof-row.js::plan` reads the caret's line
and the nearest statement above it, skipping blank lines, comments, `Proof:`-style labels and the
node's own title), because Enter has to be claimed *before* the newline lands. Then the host asks
the engine — `app.autocite(lhs, rhs)`, which is `POST /api/autocite` through the route table — and
writes the row (`rowFor`).

What the two halves decide, and why:

| question | answer |
|---|---|
| which sign | the sign the proof already steps with; the first row under a statement takes the relation the statement opens with (`=`, `\leftrightarrow`, …) |
| where the tag | the mode of the block's own tag columns; a block with no tag yet gets two spaces past its longest line; a row that runs past the column gets one space |
| when nothing is written | the engine named no citation for the step (the sentence stays exactly as typed and the error says so), the engine needs several rows for it (the error names the citations, and the author writes the intermediate row — an intermediate state has no source text yet, and the engine renders one in its own curried form), or the line changed while the search ran |

The arithmetic (`js/live/proof-row.js`: signs, tags, refusals) is pure and a node-only check can
read it; the round trip — a row written, and a sentence left alone with the error said out loud — is
what the app does and what a person watching it sees.

## The induction you declared, with its cases written

`Mod+Enter` in an editor tile reads the induction block the caret is in and writes each case's goal
under its pattern:

```
\mu a
  \tau_0 \mapsto
    \tau_0 +_{\tau} \tau_0
  ^+ a \mapsto
    \tau_0 +_{\tau} ^+ a
```

The goal is the statement above the block with the induction variable replaced by that case's
pattern, and it is written by substituting in the author's own TEXT (`js/live/induce.js`) rather than
by instantiating the parsed expression: the engine has no renderer for the corpus's surface — an
`Expr` renders to Lean, curried (`\Delta a b`) where the corpus writes `a \Delta b` — so a
round-tripped row is not a row the corpus reads back. The two lines it writes for `T.BT.11` are the
two lines that theorem already has.
The case opens with the statement's own left-hand side (`splitRelation` in `proof-row.js`), which is
where a proof's rows start: the rows under it walk to the right-hand side.

A case that already has a body is left alone; a chord with nothing to do says which reason applies
(no `\mu` header on or above the caret's line, nothing above the header that states an expression,
every case already proved).

The two chords compose, which is the point of them: press Enter on a row under an induced goal and
the citation is looked up for **the goal above it** — not for the `\mapsto` pattern that announced
the case, and not for the `\mu` header (the walk to "the statement above" skips both, in
`proof-row.js`).

## A citation is a link

A `[A1.4]` in a proof row is a reference, and it reads and behaves as one: `js/live/editor.js` marks
every citation tag in an editor tile (`citeTag` decides which text — the same reading the row's own
tag column comes from, so the clickable text is exactly what the corpus treats as a citation) and
clicking it follows the reference through `GET /api/cite?name=`, which answers the node, the
statement and the corpus-relative FILE. `js/live/cite-link.js` opens it; a tag that names nothing,
and a local law (`\equiv`, `^i`, which have no node), say so rather than doing nothing.

Two things worth knowing, both measured rather than guessed:

- **The reference is followed on the PRESS, not on a click.** CodeMirror re-renders the line when
  the mousedown moves the caret (active-line highlight), so the element that was pressed is
  detached before `click` is dispatched and the click lands on the content element instead: a real
  mouse click did nothing while a synthetic `click` at the tag worked.
- **There is no hover tooltip.** The vendored CodeMirror exports `Decoration` and `ViewPlugin` but
  not `hoverTooltip` (`web-next/vendor/`), so the hover affordance is the element's `title` — which
  can only carry what is known synchronously, the citation itself. Resolving it is the click.

## The PDF is the corpus, with its citations as links

`GET /api/pdf` used to answer a list of node HEADERS — no statements, no proofs, so a citation had
nothing to point at. It answers a document now: every node's own statements and proof rows as they
are written, with `\hypertarget` on each citable statement (and on each node, since `T.1`/`D.2`
name a node while `A1.4` names a statement) and `\hyperlink` on each tag. `\equiv` and `^i` print as
plain text: a local law has no node, and a link to nowhere is a promise the document cannot keep.

The material is generated by `crates/build/src/pdf_doc.rs` (reading the corpus, resolving citations
through the same `cite_target` the editor's links use) and typeset by `qubitick_pdf` (the preamble —
`hyperref`, coloured links — and the assembly). `qubitick pdf <root> --latex` prints it; compiling
needs `tectonic`.

Two traps are pinned by tests rather than by care, because both were silent:

- **A statement's own colon is not a label.** Reading `\forall a : \mathfrak{E}, …` as `Label: value`
  left the axiom's first statement unnumbered, and every target after it was off by one — a document
  whose `[A1.3]` links pointed at `[A1.4]`. The test asserts what `cite:A1.3` is ON.
- **The mathematics is never text-escaped.** The first version escaped the body in the one path it
  did not think about (a line with no citation tag), turning `\Delta` into a paragraph about a
  backslash. Only names and prose are escaped.

## The check strip, and "try this"

Under an editor tile's bar there is a strip (`js/live/check-strip.js`, plus the pure half in
`js/live/row-check.js`) that answers, for the row the caret is on, what the corpus makes of it —
and offers the one thing an author most often wants filled in.

`POST /api/rows` answers per row, and **the check confirms; it cannot refute**: the search is
bounded and the rewriting engine is incomplete, so the verdicts are `confirmed` / `unconfirmed`
(with the states the citation *does* reach) / `local` (`\equiv`, `^i` — no node to cite) /
`unknown-cite` (which IS provable, and the build calls it an error). That wording is measured, not
cautious: `T.BT.01`'s row `= a \Delta (a \Delta (a \Delta a))  [\ulcornerA1.4 a\urcorner, A1.1]`
is a CORRECT step that expands `∅` and then regroups, and no rewriting search can derive it,
because expanding `∅` means instantiating a metavariable with a term only the author knows
(`crates/build/src/row_check.rs`). Six of that theorem's seven rows confirm; the seventh is that
one. A row that cites several equations is checked as their **composition**, because that is what
the compiler does with them.

**"Try this" writes a citation tag, never an expression.** The engine renders a state in its own
curried notation (`\Delta (\Delta a \varnothing) b`) and that is not a `.qbit` line — measured, it
does not parse back — so the tag is the only part of a row the engine can hand over verbatim. When
the caret's row carries no citation and `autocite` bridges it in one step, the strip offers
`try this: [A1.4]`; clicking it writes the tag into the row, aligned with the block's own column.

With the **`suggest rewrites`** preference on (Editor prefs), the strip also lists what the corpus
can reach from where the caret is (`POST /api/steps`), each with its citation. Those are
INFORMATION, not source: they are shown in the engine's notation and are deliberately not
insertable, for the reason above.

The arithmetic is row numbering, tag alignment and what the strip says; the two halves are driven
in the app against a server, whose `/api/rows` answer is the engine's own output for one buffer and
whose `/api/autocite` answer is one pair — anything else is answered "nothing to suggest" rather than
invented.

## Known gaps

- **The shell has no check runner.** The ribbon's Build button emits `ui:build`, and
  `js/defaults/wiring/_flows.js` answers over `/ws/build` and asks the diag pane to re-run — but
  `rerun` calls `ctx.runCheck`, which only the canvas host supplies, so in the shell it does nothing.
- **The strip's suggestions cannot be inserted, and the check cannot refute.** Both are the same
  root cause — the engine has no renderer for the corpus's surface (an `Expr` renders to Lean,
  curried), so a reached state cannot be written back as `.qbit`, and a rewriting search that
  cannot instantiate a fresh metavariable cannot prove a row unjustified. A source renderer would
  lift both limits; `POST /api/induce` is still a stub as well.
- **`POST /api/induce` is still a stub, and the chord does not invent the cases.** A carrier
  declares its constructors only when its `Sema:` is a disjunction of bare atoms
  (`\mathcal{W} = \mathbf{p} \vee \mathbf{q}` is a `Formation`; `\tau` has no such declaration), so
  the `\mapsto` groups are the author's. Deriving them from the corpus — from a recursive
  definition's constructor equations (`a +_{\tau} \tau_0 = a` and
  `a +_{\tau} (^+ b) = {^+}(a +_{\tau} b)` name both of the tick's), or from how the corpus's own
  proofs induct on the same carrier — is an engine step that has not been taken.
- **The Enter conversion is the canvas editor's.** `js/host/canvas` hands `onEnter` to
  `js/live/editor.js` (and `app.autocite` to the pane); the shell's citation-aware editor
  (`js/panes/editor-citations.js`) has decorations, hover and trailing-cite alignment but no
  conversion, and neither has a key for a multi-step chain.
- **Panes only the shell mounts have no checks of their own.** They are drawn when the host boots
  (`tools/shot.mjs` on `/shell.html` fails on a console error) and their kinds are registered, but a
  pane like `handwrite` or `script-editor` has nothing that drives its own controls.

## Rules this directory keeps

`tools/lint.mjs` checks eight rules in one pass, each of which exists because allowing it caused a
real bug:

| Rule | Why |
|---|---|
| (a) no `/api/` or `/ws/` strings outside `js/defaults/endpoints.js` | a route written anywhere else is a route nobody can rename |
| (b) no `requestAnimationFrame` outside `js/core/frame.js` | one frame governor, or the LOD and HUD passes fight over the frame |
| (c) no framework imports (react/vue/svelte/alpine/lit/htmx) | the client ships as plain modules; a framework would arrive unbuilt |
| (d) no file over 450 lines | a page that grows past this has the app back inside it |
| (e) no `localStorage` outside `js/core/storage.js` | one storage adapter, with the server copy and the mirror in one place |
| (f) no `getElementById` outside `js/main.js` | panes reach their own elements through `ctx`, not the document |
| (g) a tool in `tools/` may only import from inside this repo | an absolute path into another checkout is a dependency nobody declared (it was one, for the browser) |
| (h) every file a page names exists | a 404 request paints nothing and errors nowhere the gate can see |

There is no baseline of known violations. What the linter carries instead is an allowance list —
one entry per `file|rule` with a REASON — and the check fails in both directions: an offence that is
not listed fails, and a listed entry whose offence is gone fails too. Nine allowances stand today:
rule (b) in `js/panes/graph.js` (the graph animates its own canvas — folding it into the governor is
a pane change) and in `js/main.js` (one frame yielded while the host swaps), and rule (d) in the
seven editor/graph files, each with the reason it is one file.

Four more rules are structural:

- **A route the client can name is a route the engine mounts, and the other way round**
  (`tests/unit/routes.test.mjs` compares `js/defaults/endpoints.js` with
  `crates/api/src/router.rs` in both directions, and requires a server route the client does not call
  to say why it is exempt).
- **One pane module is one pane**; a helper it needs is a module too.
- **A pane's behaviour never goes in a page.** If it does not fit the contract, the contract changes.
- **A pane's look does not change with its wiring**: the tile is the same tile whichever port feeds it.

## Working on it

`?debug=1` installs the surfaces the UI specs assert against: `window.__spike` (panes, clusters,
camera, wires, routes, restore state; and `layoutDocument()` for the engine's shape), `window.__ws`
(the engine host's app), `window.__wire`, `window.__bus`.

Three facts about the canvas that a check written against it keeps tripping over:

- **a tile at the fitted zoom is at the CARD level, where there is no pane body to hit-test.** Bring
  the tile into view at 100 % first.
- **a camera moved by hand is a frame ahead of the LOD pass.** Call `__spike.updateLOD()` before
  asking the DOM who is at a point; a culled tile is `visibility: hidden`, so `elementFromPoint`
  answers `#viewport` and a pointer aimed at it lands nowhere.
- **a touch context emits touch for mouse input**, so a hover written as a mouse move is an
  intermittent no-op there. A check that needs hover asks `(hover: none)` and asserts the tap.

## Where to look next

- `../../README.md` — the engine half: crates, the corpus format, the pipeline, the API.
- `theme.js` in `js/live/` and `js/core/storage.js` — the theme module format and the live model
  around it.
- `js/panes/app-mounts.js` and `js/live/wire/{host,kinds}.js` — many tiles, one document: shared
  document, private view.
