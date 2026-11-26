# Qubitick Engine

A compiler and IDE server for a **declarative mathematics corpus**. You write `.qbit`
files; the engine reads them into a SQLite symbol table, emits Lean 4, and checks the
result with a real `lean`. A corpus is a **path** — the engine ships no corpus of its own,
and it knows no mathematical glyph of any corpus. Everything a symbol means arrives as a
declaration the corpus writes.

```
cargo test --workspace          44 targets · 301 tests · 2 ignored
qubitick build <corpus>         5 files, verified 5, error_count 0   (the starter ship)
qubitick axioms <corpus> <file> "axioms": []    (the ship's theorem stands on nothing)
```

---

## 1. What a corpus is

A corpus is a directory whose charter is `Canon.qbit`, with its nodes under era/strata
directories:

```
my-corpus/
  Canon.qbit                     the charter
  Era0/Strata0/Axioms/A1_Comm.qbit              an A block: vocabulary + composition
  Era0/Strata0/Definitions/D_A_00_Carrier.qbit  a D block: where a glyph lives
  Era0/Strata0/Definitions/D_A_01_Add.qbit
  Era0/Strata0/Definitions/D_A_02_Mul.qbit
  Era0/Strata0/Theorems/T_A_01_MulTwo.qbit      a T block: a proof
  prelude/Qubitick/HL.lean       the ground: the ONE file the project owns
```

Three kinds of node, and one file outside them:

| node | is | emits |
|---|---|---|
| **A block** (`A.` header) | the raw vocabulary and composition grammar of the display | nothing |
| **D block** (`D.` header) | a glyph's home: `Type:` (the ground has it) or `Sema:` (the corpus defines it) | `def` / `inductive`, or a reference |
| **T block** (`T.` header) | a theorem over composed glyphs, with a `Proof:` chain | `theorem` |
| **the ground** (`prelude/Qubitick/HL.lean`) | the interface and logic the corpus binds to | not emitted — imported |

`corpus-templates/algebra/` is the starter the binary carries, and the tutorial ship — the
smallest corpus that builds: a carrier, a primitive, a definition, an A block, a theorem,
and the ground they are written against.

### The ground

One file, and the project's own. `qubitick seed <corpus> --preset init|mathlib|bare|custom`
writes it; `qubitick scaffold <corpus>` suggests what the corpus's own blocks imply (the
root class carrying the symbols they name, and one class per A block); `qubitick
hash-ground` prints its hash, and `qubitick build --frozen <hash>` refuses a build whose
ground is not that file. The engine has no preset awareness at build time: it sees a file,
imports it, and hashes it.

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

`Type:` is where a glyph lives, and it takes the ground's name:

```
Type: ISA.Capacity                                   the ground declares it
Type: ISA.Capacity : \mathfrak{V} \to \mathfrak{E}      ...with a signature
```

A `Type:` that is a signature and no name (`Type: \mathfrak{V} \to \mathfrak{E}`) gives the
glyph no home, and the build says so. `Type:` and `Sema:` on one node is refused: a glyph
means one thing — the ground declares it, or the corpus does. A `Type:` *signature* under a
`Sema:` is a typed definition, which is a different question and stays legal.

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

**A home** comes from `Type:`: `Type: HMul.hMul` says the glyph IS that name, so the engine
declares nothing for it and every use renders it — a project that imports Mathlib calls
Mathlib's notation and Mathlib's lemmas. A home and a `Sema:` contradict each other (the
ground declares the glyph, or the corpus does) and one node cannot say both.

A name reached through the ground's instance is written as one: `Type: R.plus` says the
glyph is the `plus` of the structure a proof is parametric over, and the module writes
`R.plus` — the `R` coming from the `section variable [R : ...]` every module of a corpus
with an A block opens. The engine infers no instance from a class name and knows no model:
whatever follows `Type:` is exactly what is rendered.

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

A ground is also where a corpus's classes live. An A block states laws, and a law becomes a
field of a class in the ground — the structure every proof of that corpus is parametric
over (`section variable [R : Comm]`). The engine writes none of that: `qubitick scaffold`
suggests the chain, the project owns the file, and `--frozen` pins it. A corpus with A
blocks and no chain in its ground is a corpus whose proofs name a class nothing declares,
and Lean says so per file.

### The coverage invariant

`glyphs(A_era_stratum) ⊆ glyphs(D_era_stratum)` — every glyph an A block writes has a D
node, in the same era/stratum, that declares it. Checked before any Lean is produced:

```
`\Delta` appears in Era0/Strata0/Axioms/A1_Realm.qbit
  but no D node in Era0/Strata0 declares it.
Add a D node whose glyph line is `a \Delta b` with `Type:` or `Sema:`.
```

An A block is grammar, and a terminal with no meaning cannot be resolved. The engine used
to invent one — the `axiom` fallback — and this is the pass that refuses instead. The
declaration format's own alphabet (`=`, the iff, the arrows, the quantifiers, the citation
corners) is the engine's and is exempt: a corpus is never asked to declare the format it is
written in.

---

## 5. What gets emitted

Four constructs, and nothing else: `def`, `inductive`, `theorem`, and a reference.

- **a definition** (`Sema:`) — `noncomputable def`, in the corpus's own scope;
- **a formation** — `set_option genInjectivity false in inductive C : Type | ...` with each
  constructor's declared argument sorts, plus one `def ctor : ... := C.ctor` alias per
  constructor, and a `C.rec` definition for an operator whose `Sema:` recurses on one of
  them;
- **a theorem** — a `theorem`, with the proof's chain compiled to a term; an equality
  through the nullity operator is emitted in the nullity form the corpus's `Role:` marks;
- **a reference** — a glyph whose home is the ground (`Type: ISA.Capacity`, `Type: R.plus`)
  is declared by nobody in the corpus, and every use site writes the name the corpus wrote.

The engine emits **no `axiom`**, ever — not for a primitive, not for a definition seen from
another module, not for a sort word. A module that names a glyph another module defines
**imports** that module, and the build compiles the corpus in dependency order so the import
resolves. A glyph the engine cannot reach has no home, and that is a build error naming it.

An A block emits **nothing at all**: it is vocabulary and composition, and its law becomes a
field of a class in the ground (§4). Its glyphs must still be homed — see the coverage
invariant in §4.

**Admitted theorems** are the exception the corpus asks for: a `Proof:` that proves nothing
is emitted as an axiom of its own module plus a warning, so the file still checks while the
report counts what is assumed.

---

## 6. Commands

```
qubitick files   <root>            # the .qbit files under a corpus
qubitick palette <root>            # per-symbol colours for the editor
qubitick parse   <file>            # one file's AST
qubitick emit    <file> --root R   # the Lean module a file emits
qubitick build   <root>            # ingest, emit, and check every file with lean
qubitick build   <root> --frozen H # ...and refuse unless the ground hashes to H
qubitick scaffold <corpus>          # the class chain the A blocks imply, for the ground
qubitick hash-ground <root>         # the ground file's hash, for --frozen
qubitick seed    <corpus> --preset  # init | mathlib | bare | custom
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

The engine reports the **axiom footprint** Lean computes for each theorem. Under this
architecture it is a **regression test on the ground**, not a budget meter: the engine emits
no axiom, so anything that shows up came from the ground the project carries.

```console
$ qubitick axioms corpus-templates/algebra corpus-templates/algebra/Era0/Strata0/Theorems/T_A_01_MulTwo.qbit
{ "footprints": [ { "name": "Qubitick.HL.MulTwo",
                    "axioms": [] } ] }
```

- On the ship, nothing. Its symbols are fields of the structure the proof is parametric
  over, `times` unfolds to `R.plus a a`, and a class field is not an axiom.
- On a project whose ground is `import Init`, a theorem that reaches `Classical` prints
  Init's three (`propext`, `Classical.choice`, `Quot.sound`).
- On Mathlib, whatever Mathlib costs — usually the same trio.

Given a frozen ground and a fixed corpus the answer is deterministic, so a change in it
means someone edited the ground or the corpus. Both are git-tracked; both are visible.

A theorem's footprint includes its **type**, not only its proof term.

## 8. Gates

```
./toolchain cargo test --workspace        # 44 targets, 301 tests, 2 ignored
./toolchain cargo clippy --workspace --all-targets
./toolchain cargo build -p qubitick-cli
./toolchain ./target/debug/qubitick build corpus-templates/algebra   # built 5, verified 5
./toolchain ./target/debug/qubitick axioms corpus-templates/algebra \
    corpus-templates/algebra/Era0/Strata0/Theorems/T_A_01_MulTwo.qbit  # "axioms": []
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
| `crates/build/tests/print_axioms.rs` | an Init ground prints Init's three, the bare prelude prints nothing, and the ship stands on nothing |
| `crates/build/tests/coverage.rs` | an A-block glyph with no D node fails naming it; the shipped ship homes every glyph its A blocks write; and `scaffold` writes the chain |
| `crates/build/tests/emission_surface.rs` | no module emits an `axiom`; a ground-named glyph declares nothing; a sort word is a literal; a cross-module definition is imported; a `Type:` with no name is refused |
| `crates/build/tests/freeze.rs` | the ground hash is stable and follows the file; `--frozen` accepts its own hash and refuses another; every preset is a ground |
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
- **A corpus with A blocks needs a ground that carries their chain.** A law an A block
  states becomes a field of a class in the ground, so a corpus that has blocks and no
  chain is a corpus whose proofs name a class nothing declares — reported per file, not
  invented. `qubitick scaffold` is how a project gets one; `--frozen` is how it stays
  the file the project chose.
- **A class in the ground cannot be written over a type a module declares.** A law about a
  corpus's own formation (`\tau = \tau_0 \vee (^+ \tau)`) is a T block, not an A block:
  the inductive lives in its node's module, and the ground is compiled before it.
- The freeze covers the ground file, not the charter: `Canon.qbit` stays editable while
  `--frozen` holds.