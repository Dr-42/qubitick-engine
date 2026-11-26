# Qubitick Engine

A compiler and IDE server for a **declarative mathematics corpus**. You write `.qbit`
files; the engine reads them into a SQLite symbol table, emits Lean 4, and checks the
result with a real `lean`. A corpus is a **path** — the engine ships no corpus of its own,
and it knows no mathematical glyph of any corpus. Everything a symbol means arrives as a
declaration the corpus writes.

```
cargo test --workspace          38 targets · 278 tests · 2 ignored
qubitick build <corpus>         4 files, verified 4, error_count 0   (the starter ship)
```

---

## 1. What a corpus is

A corpus is a directory whose charter is `Canon.qbit`, with its nodes under era/strata
directories:

```
my-corpus/
  Canon.qbit                     the charter
  Era0/Strata0/Definitions/D_A_01_Add.qbit
  Era0/Strata0/Definitions/D_A_02_Mul.qbit
  Era0/Strata0/Axioms/A1_Comm.qbit
  Era0/Strata0/Theorems/T_A_01_MulTwo.qbit
  prelude/Qubitick/HL.lean       OPTIONAL: this project's own ground
```

`corpus-templates/algebra/` is the starter the binary carries, and the tutorial ship —
the smallest corpus that builds: one primitive, one definition, one axiom, one theorem.

### The charter

```
P. Algebra — the starter corpus
Id: algebra
Kind: project
Module: Qubitick.HL
Note: prose is fine here; the charter is read by a person and a header parser.
LeanDir: lean                 # optional: the project's own ground directory
Require: mathlib              # optional, repeatable: Lake requirements
Precedence: +                 # one line per level, LOOSEST FIRST
Precedence: ×
```

`Id:` is the identity a scope and a module name derive from. `LeanDir:` and `Module:`
carry the ground (§4). `Precedence:` is the master precedence table (§3). The compiled
artifact `canon.bit` (node census + theorem graph + acyclicity) is written by a green
build; it is never an input.

### A node

```
D.A.02. Mul
a × b                         the glyph line: what this node DECLARES, and its fixity
Sema:
a × b = a + a                 the definition's body
Math: Multiplication by repeated addition.
```

A proof is a chain and, optionally, the term that closes it:

```
Proof:
a + b
  = b + a        [A1.Comm]     one step of the chain
⊢ h                            the term that proves the goal (the derivation sign)
∎                              the end-of-proof mark; it carries no step
```

A row that is none of those — an English sentence, a stray expression — is **refused**,
and the build reports it per file naming the row.

The header is `<kind>.<division>.<n>. <Name>` (`D.` definition, `A.` axiom, `T.` theorem).
The first bare line is the **glyph line**. The fields a node may carry are a closed,
short list: `Type:`, `Sema:`, `Role:`, `Proof:`, `Falsify:`, `Math:`, `Phys:`, and — for a
calibration — `Symbol:`, `QUnits:`, `Dims:`, `Rate:`.

---

## 2. The engine owns no glyph

There is no glyph table in the engine: no list saying that `\cap` is an operator, `\tau`
is a sort or `\mathbb{Z}` is an inductive. A corpus declares the glyphs it writes, and a
corpus that uses one it never declared **fails, naming the glyph**:

```
`\Delta` is used as an operator, but this corpus never declares it; declare it with a
node whose glyph line is `a \Delta b`, and a `Sema:`, a `Type:` or a `Role:`
```

`crates/build/tests/no_glyph_names.rs` is a source guard: it fails if a corpus glyph
appears as a string literal anywhere in the engine's production code.

### What the engine does know: the declaration format

`crates/model/src/format.rs` holds the only alphabet the engine owns — the tokens a
declaration is *written with*, which no corpus declares:

| token | meaning |
|---|---|
| `=` | a `Sema:` and a statement relate two sides with it (the reserved `EQ_SYM`) |
| `\leftrightarrow`, `\iff` | the iff that links a relational definition to its body |
| `\to`, `\rightarrow`, `\longrightarrow`, `\Longrightarrow` | the arrow of a `Type:` signature, and implication |
| `\forall`, `\exists` | the quantifiers, with the `:` that introduces their sort |
| `\ulcorner … \urcorner` | the corners a citation's explicit arguments are written in |

---

## 3. The declaration surface

A declaration states three things, and nothing else decides them.

**Fixity** comes from the glyph line: `a + b` is infix, `^+ a` is prefix, `\Vert x \Vert`
is a collector.

**Classification** comes from `Role:` when no shape can give it:

| `Role:` | the glyph is |
|---|---|
| `infix`, `prefix`, `collector` | that fixity (an override, when the shape is ambiguous) |
| `connective` | a propositional connective: binds looser than `=` |
| `relation` | a relation between two terms: binds looser than the arithmetic it compares |
| `null` | the carrier's empty element (the rewriter anchors an equality on it) |
| `nullity` | the nullity operator a proof rewrites through |

**A binding** comes from `Lean:`: `Lean: HMul.hMul` says the glyph IS that name, so the
engine declares nothing for it and every use renders it — a project that imports Mathlib
calls Mathlib's notation and Mathlib's lemmas. A binding and a `Sema:` contradict each
other (the ground declares the glyph, or the corpus does) and one node cannot say both.

**Type and form** come from `Type:` (a signature, so an operator is typed over its
carrier) and from `Sema:` (a body). A `Sema:` whose alternatives are separated by a
declared connective declares a **formation** — an inductive: `\tau = \tau_0 \vee (^+ \tau)`
is a carrier with a nullary and a recursive constructor, and the emitter generates the
Lean `inductive` and its constructor aliases.

**Precedence** is the charter's, not a glyph's: `Precedence:` lines run loosest first, and a
glyph the table does not name binds looser than every glyph it does. Association is not
metadata — `(a + b) + c = a + (b + c)` is a theorem a corpus states, and the parser nests
equal levels to the left.

---

## 4. The ground, and what a proof may call

Emission is **term mode** against a fixed contract of 16 Lean names in three tiers
(kernel `Eq`/`Eq.refl`/`Eq.symm`/`Eq.trans`; logic `False`…`DecidableEq`; codegen
`lcErased`/`lcAny`). `crates/build/src/prelude_contract.rs` is that list;
`qubitick prelude-check` proves a ground satisfies it by compiling a probe with real
`lean`.

The engine embeds a base ground (`prelude/Qubitick/HL.lean`). A project replaces it by
carrying its own — `LeanDir:` names the directory, `Module:` the module inside it — which
is how a project builds on Init, on Mathlib, or on a previous development. A ground
decides what the emitted *proofs* may call; it never decides what the corpus may *say*
(the surface is the corpus's own).

---

## 5. What gets emitted

Per file, in the file's own scope:

- **the symbol frame** — every glyph the module mentions gets a Lean name, an arity from
  observed application, and a type from the authored `Type:` signature, the observed
  arity, or `Prop`;
- **a declared formation** — `set_option genInjectivity false in inductive C : Type | …`
  with each constructor's declared argument sorts, plus one `def ctor : … := C.ctor` alias
  per constructor, and a `C.rec` definition for an operator whose `Sema:` recurses on one
  of them;
- **axioms as a class chain**, and **definitions** as `noncomputable def` (or an `axiom`
  when the body cannot be a term);
- **theorems** as a `theorem`, with the proof's chain compiled to a term; an equality
  through the nullity operator is emitted in the nullity form the corpus's `Role:` marks;
- **admitted theorems** as an axiom of the module plus a warning — stated, not proved.

---

## 6. Commands

```
qubitick files   <root>            # the .qbit files under a corpus
qubitick palette <root>            # per-symbol colours for the editor
qubitick parse   <file>            # one file's AST
qubitick emit    <file> --root R   # the Lean module a file emits
qubitick build   <root>            # ingest, emit, and check every file with lean
qubitick lake    <root>            # emit the corpus as one Lake package
qubitick rows    <root> <file>     # the file's proof rows and their verdicts
qubitick steps   <root> <expr>     # the rewrites the corpus can cite from an expression
qubitick autocite <root> <lhs> <rhs>  # the citations that bridge two expressions
qubitick pdf     <root> [--latex]  # the corpus as a document
qubitick prelude-check --module M  # prove a ground against the emitter's contract
qubitick axioms  <root> <file>     # what Lean says the file's theorems stand on
```

`qubitick-server` serves the same engine over HTTP (the routes the web client in
`web-next/` uses: `files`, `parse`, `emit`, `build`, `rows`, `steps`, `autocite`,
`pdf`, `symbols`, `palette`, `projects`, `settings`, `induce`, …).

---

## 7. What a proof stands on

The engine reports the **axiom footprint** Lean computes for each theorem — reported, never
policed, and the ground decides the answer:

```console
$ qubitick axioms corpus-templates/algebra corpus-templates/algebra/Era0/Strata0/Theorems/T_A_01_MulTwo.qbit
{ "footprints": [ { "name": "Qubitick.HL.MulTwo",
                    "axioms": ["plus", "times", "times_def"] } ] }
```

- On a project whose ground is `import Init`, a theorem that reaches `Classical` prints
  Init's three (`propext`, `Classical.choice`, `Quot.sound`).
- On the engine's bare prelude, a theorem of kernel reflexivity prints **nothing** — an
  empty list, not an error.
- The names are the corpus's own admissions when the corpus makes them: `plus` is a
  `Type:` primitive, `times`/`times_def` are a definition seen from a module that cannot
  unfold it. Binding a glyph to what the ground already provides (`Lean: True`,
  `Lean: HMul.hMul`) is how a corpus lowers that number.

A theorem's footprint includes its **type**, not only its proof term.

## 8. Gates

```
./toolchain cargo test --workspace        # 38 targets, 278 tests, 2 ignored
./toolchain cargo clippy --workspace --all-targets
./toolchain cargo build -p qubitick-cli
./toolchain ./target/debug/qubitick build corpus-templates/algebra   # built 4, verified 4
```

The tests that state a promise rather than a detail:

| test | the promise |
|---|---|
| `crates/build/tests/no_glyph_names.rs` | no corpus glyph appears as a string literal in the engine |
| `crates/build/tests/declaration_authority.rs` | an undeclared operator fails naming the glyph; declaring it makes the same corpus verify |
| `crates/build/tests/declared_formation.rs` | a corpus's formation becomes an inductive with typed constructors, and its operators are typed over the carrier |
| `crates/build/tests/precedence.rs` | the charter's table reaches the rows, and `a + b × a` binds as `a + (b × a)` when built |
| `crates/build/tests/tutorial_ship.rs` | the ship builds with school notation and never rots |
| `crates/build/tests/lean_binding.rs` | a bound glyph gets no declaration of its own, and the corpus verifies against the ground that carries the name |
| `crates/build/tests/no_prose_in_a_proof.rs` | a sentence in a proof still lets the corpus OPEN, and the build reports the row |
| `crates/build/tests/print_axioms.rs` | an Init ground prints Init's three, the bare prelude prints nothing, and the ship prints its own admissions |
| `crates/parse/tests/proof_surface.rs` | `⊢ <term>` closes a proof, `∎` ends it, and prose is refused by name |

`./toolchain` is the build wrapper: it provisions the pinned Rust and borrows the corpus's
Lean 4.35.0 (see its header for why `nix develop` cannot be used in this sandbox).

---

## 9. Layout

| crate | what it is |
|---|---|
| `model` | the data model: `Expr`, `Node`, `DBlockIR`, `SymbolKind`, `DeclaredRole`, and the declaration format's alphabet |
| `parse` | lexer, Pratt parser, node parsing, induction patterns |
| `db` | the SQLite schema and queries (a cache: rebuilt from the `.qbit` files on every ingest) |
| `build` | ingest, the ground, the pipeline, the charter, citation checks, Lean invocation |
| `ctx` | `Ctx`: the scope-visible symbol snapshot every engine reads |
| `emit` | Rust to Lean: the frame, axiom classes, definitions, theorems, recursor defs |
| `lean` | the Lean-side renderer |
| `rewrite` | the rewrite engine: rules, chains, induction, goals |
| `autocite` | the citations that bridge two expressions |
| `pattern` | term patterns (matching, substitution) |
| `palette` | per-symbol colours |
| `pdf` | the corpus as a LaTeX document |
| `api` | the HTTP routes |
| `cli` | the mirrored command line |
| `server` | the binary that ties `api` and the client together |

`web-next/` is the client the server serves. `corpus-templates/algebra/` is the starter
corpus. `prelude/Qubitick/HL.lean` is the embedded base ground.

---

## 10. Known limits

- **Induction** in the proof rewriter takes the induction hypothesis when the compiled
  body uses it; the declaration-side recursion is not reachable from that crate.
- `POST /api/induce` answers that theorem induction is not implemented yet.
- Ingestion never refuses a corpus: it builds the ledger from what it can read, and the
  refusal reaches the report per file. A corpus with a bad row opens; its build fails.
- A corpus that writes an undeclared glyph still *opens*; its build reports the refusal.