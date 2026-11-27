# AGENT.md — working on the Qubitick engine

This repository is the **engine**: it compiles a declarative corpus to Lean and serves the
IDE. Read `README.md` first for what the thing is. This file is how to work on it without
breaking a promise.

## Roles

| role | owns |
|---|---|
| **Builder** | this repository — the compiler, the emitter, the server, the gates |
| **Generator** | formalisation and the corpus's own content (which glyphs a corpus declares, and the mathematics written with them) |
| **Imaginator** | exploration — the mess that happens before a design is decided |

An agent editing this checkout is the Builder. Corpus content arrives from the Generator
as `.qbit` files; the engine's job is to make what they say expressible, and to refuse
what it cannot read instead of guessing.

## The invariants

Each one is enforced by a test, not by this paragraph.

**1. The engine names no mathematical glyph.** No list says `\cap` is an operator or `\tau`
is a sort. A corpus declares what it writes; an undeclared operator fails, naming the
glyph. *Enforced by* `crates/build/tests/no_glyph_names.rs` (a source guard over
production code) and `crates/build/tests/declaration_authority.rs`.

**2. The declaration is the authority.** Fixity from the glyph line; classification from
`Role:`; a signature and a home from `Type:` (the glyph IS a name the ground carries, and
the engine declares nothing for it); an inductive from a `Sema:` whose
alternatives a declared connective separates; precedence from the charter. Nothing else
may decide. `Lean:` was the old spelling of the binding and is retired: the parser refuses
it and says the ground name is the argument of `Type:` now (`crates/parse/src/node.rs`).

**2a. Ingestion never refuses a corpus.** All of its passes read tolerantly
(`qubitick_parse::tolerate_prose(true)`), because a project the editor cannot open is a
project nobody can fix. The refusal is per file, by the build
(`pipeline::refused_row_diagnostics` asks the parser once, strictly). Do not "fix" the
tolerance by making ingestion strict: the sentence belongs in the report.

**3. The engine's whole vocabulary is the declaration FORMAT** —
`crates/model/src/corpus.rs` (`Facet`), `crates/model/src/symbol.rs` (`DeclaredRole`) and
`crates/model/src/format.rs`
(the `=`, the iff, the arrow, the binders, the citation corners). Adding to either is a
surface change: write the reason in the doc comment, make the parser read it, and make
ingestion persist it.

**3a. Axioms are reported, not policed.** `qubitick axioms <root> <file>` prints what Lean
says a theorem stands on, and the answer belongs to the ground (Init's three, or the bare
prelude's none) plus whatever the corpus itself admits. Never gate a build on it, and never
put it in a diagnostic: `warning_count` counts every non-error diagnostic, so an info line
per theorem breaks every test that asserts a clean build.

**4. Documentation describes the current state.** Present tense. No "this used to", no
migration stories, no before/after. A document that contradicts the code is wrong, and the
fix is the document.

**5. The tests are scaffolding.** They exist to build faithfully now and will be
compressed to a maintenance set when construction ends. Keep them green; do not trim them
early; do not buy permanence for an assertion that only describes a stage of construction.
A test that pins a *promise* (the table in `README.md` §9) is not scaffolding.

**6. Publishing.** Push `dev` after every change. `master` advances only on an explicit
instruction. Each commit names its traps — the thing that was wrong, measured, not the
feature it shipped.

**7. Verify by measurement.** A claim ships with the command that shows it, and a claim
about a *refusal* ships with the negative control (the corpus that must fail). Do not
report a green run you did not run. If a binary is involved, rebuild it first: a stale
`target/debug/qubitick` has measured the wrong engine here before.

## Gates

```bash
cargo test --workspace                    # 48 targets
cargo clippy --workspace --all-targets    # pre-existing style notes
cargo test -p qubitick-parse --lib        # one crate while iterating
cargo test -p qubitick-build --test declared_formation   # one promise
LEAN_PATH=$PWD/prelude ./target/debug/qubitick build corpus-templates/algebra  # built 5
node tools/check.mjs                      # lint + unit + browser gates, no dependencies
```

The client's checks are `tools/` and `tests/unit/`, and none of them needs a package install:

- `node tools/check.mjs [baseUrl]` runs all three below in one go; it fails when a server does not
  answer rather than skipping the gates;
- `node tools/lint.mjs` — the client's rules, with an allowance list that must stay true;
- `node --test "tests/unit/*.test.mjs"` — camera arithmetic, tiling, layout trees, wires, and the
  client's route table against the engine's router;
- `tools/verify-default-view.mjs` and `tools/shot.mjs` drive the machine's chromium through
  `tools/page.mjs` (the DevTools protocol, no Playwright). They need a server with a project in it,
  and they fail on a console error — the only way to notice a pane that threw while the page still
  painted.

Two requirements: cargo 1.85+ (edition 2024) and Lean 4.35.0, the version
`lean-toolchain` names. `./toolchain` resolves both, refuses a `cargo` that cannot build
the workspace, exports `LEAN_PATH`, and stops with a message naming what is missing
(`$QUBITICK_LEAN`, then `lean` on `PATH`, then `elan`). Use it when a command's toolchain
matters; the bare commands above are the same thing. `QUBITICK_SANDBOX=1` selects the
original sandbox's store path instead — nothing here depends on it.

## Ingestion order, and the traps in it

Ingest is four passes over the same files, and the order is load-bearing:

1. **Phase A** reads every file with an **empty context** — enough to list the nodes and
   register the glyphs they mention, and *not* enough to classify anything (a formation is
   unrecognisable without the connective that separates its constructors).
2. **Phase B** writes nodes and symbols (every kind `notation`, every facet `0`).
3. **The metadata pass runs TWICE**, rebuilding the contexts between rounds: round one
   writes each node's role, facets and precedence; round two re-parses with those in the
   context, which is when a formation becomes a formation.
4. **The declaration-facts pass** writes the declared kind, `home_node` (**the declaring
   node owns its glyph** — not the first node that used it) and `symbol_hash`, and then
   marks a formation's constructors `kind='ctor'` **after** every declaration is recorded,
   because a constructor may have a declaration node of its own.
5. Only then are expressions written, against contexts carrying all of the above.

Consequences that have bitten:

- A **formation's constructor list** lives in `form_bodies.ctors` (JSON). Without it a
  formation loaded as *context* — every axiom and theorem module — emits no inductive.
- A **`Sema:` body** treats the corpus's declared connectives as terms: the format's `=`
  or iff links the body, and a separator must not swallow it.
- **`corpus_root`** anchors on the corpus's own era tree when it carries no `prelude/`.
  Reaching for the working directory scanned another corpus's definitions into the module.
- **Editing the schema** means a `SCHEMA_VERSION` bump: the database is a cache and a
  stamped file is rebuilt, not migrated.
- **Symbol hashes are derived from the kind**, so anything that changes a kind must rewrite
  `symbol_hash` in the same transaction, before any expression is stored.

## Where to add what

- **A classification a corpus may state** (`Role:`): add a `DeclaredRole` variant and its
  facet in `crates/model/src/symbol.rs` + `corpus.rs`, read it in
  `crates/parse/src/node.rs`, persist it in `crates/build/src/ingest.rs`, and ask for it
  through `Ctx::has_facet`. Give it a test in both directions: declared ⇒ the behaviour,
  undeclared ⇒ the refusal.
- **A token of the declaration format**: `crates/model/src/format.rs`, then every reader of
  that fact (`lexer`, `parser`, `emit`, `rewrite`), and extend the module's own test that
  lists what is *not* the format's.
- **An emitter feature**: a unit test in the crate, plus a built-corpus test
  (`crates/build/tests/…`) that declares the corpus, builds it, and asserts on the emitted
  Lean. The emitter's declaration-driven paths are the interesting ones; nothing that keys
  on a glyph name is acceptable in it.
- **A corpus-shaped fixture**: declare the glyphs it writes (see
  `crates/build/tests/declared_formation.rs`), use distinct division numbers (two nodes
  claiming one is an ingest error), and write real newlines — a fixture that writes `\n`
  literally produces one-line files and looks exactly like an engine bug.

## Known limits

- The rewriter's `C.rec` branch takes the induction hypothesis exactly when the compiled
  body uses it; the declaration-side recursion is not reachable from that crate.
- `POST /api/induce` answers `{"ok": false, "error": "theorem induction not implemented yet"}`.
- A corpus that writes an undeclared glyph still *opens* (ingestion is tolerant) and its
  build reports the refusal; `/api/parse` answers the refusal directly to the editor.
- The axiom footprint includes a theorem's TYPE as well as its proof term, so "what did my
  proof use" is not what `qubitick axioms` answers.
- `qubitick axioms` is the surface for that footprint: `lean_check::axioms_footprint` hands
  Lean the module text plus `#print axioms <name>` and reads its own lines back
  (`crates/build/tests/print_axioms.rs` holds it against an Init ground, a bare prelude and
  the ship). A build never gates on it.
- **Five HTTP routes answer "not implemented yet"**: `POST /api/rewrite` (a placeholder
  `by sorry`), `POST /api/ir` (echoes the buffer), `POST /api/latex`, `POST /api/expand`,
  `POST /api/handwrite`, and `POST /api/induce` (theorem induction). The routes are mounted
  and the client shows the server's words; the features behind them are not written.
- **The Slate pane does not evaluate Lean.** Its Run button posts to `/api/slate`, which
  parses `.qbit` (`handlers_gap::post_slate`) and refuses Lean input with `unknown node
  header`; the pane labels it "Lean scratch".