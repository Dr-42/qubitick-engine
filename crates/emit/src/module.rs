//! Module-level Lean emission (CodeArch §5.11, Document 5 §4-§6).

use crate::imports::assemble_imports;
use anyhow::Result;
use qubitick_ctx::Ctx;
use qubitick_lean::LeanTarget;
use qubitick_model::corpus::Facet;
use qubitick_model::{Axiom, EQ_SYM, Expr, FormBody, Node, NodeBase};
use qubitick_rewrite::RuleTable;
use std::collections::{HashMap, HashSet};

/// A valid Lean identifier for an assumed theorem lemma, derived from its name.
pub fn lemma_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '\'' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut s = s.trim_matches('_').to_string();
    if s.is_empty() {
        s = "lemma".into();
    }
    if s.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        s.insert(0, '_');
    }
    s
}

/// Whether `id` is the declaration format's arrow.
///
/// A `Type:` signature is written `\varphi \to \psi` and nothing else, so an
/// application inside a signature IS the arrow of the format; the glyph the corpus
/// used to spell it names no constant of its own (Lean's `→` is built in).
fn is_format_arrow(ctx: &Ctx, id: u32) -> bool {
    match ctx.by_id(id as i64) {
        Some(row) => qubitick_model::format::is_arrow(&row.tex),
        // The lexer binds an arrow it has no row for to the reserved id 0, so an
        // unresolved head inside a signature is the format's arrow.
        None => id == 0,
    }
}

/// Symbols used both bare (`\mathfrak{L}`) and applied (`\mathfrak{L}(t)`).
fn dual_use_symbols(nodes: &[Node]) -> HashSet<u32> {
    let mut bare: HashSet<u32> = HashSet::new();
    let mut applied: HashSet<u32> = HashSet::new();
    let mut visit = |e: &Expr| collect_use(e, &mut bare, &mut applied);
    for n in nodes {
        match n {
            Node::Axiom(a) => a.statements.iter().for_each(&mut visit),
            Node::Theorem(t) => {
                visit(&t.statement);
                // Proof rows use glyphs the statement may not mention (e.g.
                // T_BT_72's \ge_{0}^{\mathbb{Z}}); without their sigs the frame
                // declares such an op `Prop → Prop` and rejects its ℤ argument.
                for step in &t.proof {
                    if let qubitick_model::ProofStep::Chain { rows } = step {
                        for row in rows {
                            if let Some(l) = &row.lhs {
                                visit(l);
                            }
                            visit(&row.rhs);
                        }
                    }
                }
            }
            Node::Definition(d) => {
                if let Some(FormBody::MacroExpr(e)) = &d.d_block.form {
                    visit(e);
                }
                if let Some(sig) = &d.d_block.sig {
                    visit(sig);
                }
            }
            Node::Calibration(_) | Node::Census(_) => {}
        }
    }
    bare.intersection(&applied).copied().collect()
}

/// Keep the constructors of the corpus's formations out of the bare/applied split.
///
/// A constructor is an ATOM, never a function: its occurrence as an argument
/// (`\mathbf{p}` in `\mathbf{p} \vee \mathbf{q}`) is not an "applied" use, so it must
/// never be `_fn`-dual-split. Splitting one changes the citation ground — a later
/// scope's commutative duplication of an earlier scope's theorem then hits the
/// base-vs-_fn mismatch. Which symbols those are is what the corpus declares (the
/// `CtorDecl`s of a formation); the emitter names no glyph.
fn exclude_ctor_dual(dual: &mut HashSet<u32>, ctx: &Ctx) {
    for row in ctx.declared_ctors() {
        dual.remove(&(row.id as u32));
    }
}

/// Whether a glyph heads a CONJUNCT of a `Sema:` — the connective a corpus writes a
/// case-by-case definition with (`Eq (f c0) r0 \land Eq (f c1) r1`). Which glyph that
/// is belongs to the declaration; the emitter names none.
pub(crate) fn is_conjunct_head(ctx: &Ctx, id: u32) -> bool {
    ctx.by_id(id as i64)
        .is_some_and(|row| ctx.has_facet(&row.tex, Facet::Connective))
}

/// Record bare vs applied head uses of every symbol in `e`.
fn collect_use(e: &Expr, bare: &mut HashSet<u32>, applied: &mut HashSet<u32>) {
    match e {
        Expr::Sym(ac) => {
            bare.insert(ac.primary);
        }
        Expr::Var(_) => {}
        Expr::Apply(h, args) => {
            applied.insert(h.primary);
            for a in args {
                collect_use(a, bare, applied);
            }
        }
        Expr::Bracket(_, args) => {
            for a in args {
                collect_use(a, bare, applied);
            }
        }
        Expr::Binder(_, bs, body) => {
            for b in bs {
                collect_use(&b.ty, bare, applied);
            }
            collect_use(body, bare, applied);
        }
        Expr::Recursor(s, cases) => {
            collect_use(s, bare, applied);
            for (_, b) in cases {
                collect_use(b, bare, applied);
            }
        }
        Expr::Matrix(rows) => {
            for r in rows {
                for c in r {
                    collect_use(c, bare, applied);
                }
            }
        }
        Expr::Vector(els) | Expr::Set(els) => {
            for a in els {
                collect_use(a, bare, applied);
            }
        }
    }
}

/// Erase the arguments of dual-use glyphs throughout a node.
fn split_node(
    n: &mut Node,
    dual: &HashSet<u32>,
    ctx: &Ctx,
    fn_names: &mut std::collections::HashMap<u32, String>,
    iff_map: Option<&std::collections::HashMap<u32, (Vec<String>, Expr)>>,
) {
    match n {
        Node::Axiom(a) => {
            for s in &mut a.statements {
                split_expr(s, dual, ctx, fn_names);
            }
        }
        Node::Theorem(t) => {
            split_expr(&mut t.statement, dual, ctx, fn_names);
            // A relational (iff-defined) statement unfolds into its defining
            // body, which mentions glyphs absent from the surface statement —
            // those must be split (and their _fn names registered) too, or the
            // unfolded statement renders the base name and reapplies it (CoeFun).
            if let Some(iff_map) = iff_map {
                let mut unfolded = crate::theorem::unfold_relational(&t.statement, ctx, iff_map);
                split_expr(&mut unfolded, dual, ctx, fn_names);
            }
            split_proof_steps(&mut t.proof, dual, ctx, fn_names);
        }
        Node::Definition(d) => {
            if let Some(FormBody::MacroExpr(e)) = &mut d.d_block.form {
                split_expr(e, dual, ctx, fn_names);
            }
            if let Some(sig) = &mut d.d_block.sig {
                split_expr(sig, dual, ctx, fn_names);
            }
        }
        Node::Calibration(_) | Node::Census(_) => {}
    }
}
const FN_OFFSET: u32 = 1_000_000_000;

/// Dual-use polysymbol split, recursed over the whole proof IR tree.
///
/// A theorem's proof chains are parsed from source with the BARE symbol id for an
/// applied glyph (`\mathfrak{L}(t)` kept as `mathfrak_L`), so rendering the proof
/// diverged from the axiom rules (which are split to `_fn`). Without this, a row
/// `cap (mathfrak_L t) (mathfrak_L (tick t))` matched the A3.3 rule
/// `Eq (mathfrak_L_fn (tick t)) …` — the whole `cannot-justify` family.
fn split_proof_steps(
    steps: &mut [qubitick_model::ProofStep],
    dual: &HashSet<u32>,
    ctx: &Ctx,
    fn_names: &mut std::collections::HashMap<u32, String>,
) {
    for step in steps {
        match step {
            qubitick_model::ProofStep::Chain { rows } => {
                for row in rows {
                    if let Some(l) = &mut row.lhs {
                        split_expr(l, dual, ctx, fn_names);
                    }
                    split_expr(&mut row.rhs, dual, ctx, fn_names);
                    split_chain_just(&mut row.just, dual, ctx, fn_names);
                }
            }
            qubitick_model::ProofStep::Conclude { term } => {
                split_expr(term, dual, ctx, fn_names)
            }
            qubitick_model::ProofStep::Case { scrutinee, arms } => {
                split_expr(scrutinee, dual, ctx, fn_names);
                for arm in arms {
                    split_proof_steps(&mut arm.body, dual, ctx, fn_names);
                }
            }
            qubitick_model::ProofStep::Induction { cases, .. } => {
                for c in cases {
                    split_proof_steps(&mut c.body, dual, ctx, fn_names);
                }
            }
            qubitick_model::ProofStep::Recursor { scrutinee, cases } => {
                split_expr(scrutinee, dual, ctx, fn_names);
                for c in cases {
                    split_proof_steps(&mut c.body, dual, ctx, fn_names);
                }
            }
            qubitick_model::ProofStep::Note { prop, body, .. } => {
                split_expr(prop, dual, ctx, fn_names);
                split_proof_steps(body, dual, ctx, fn_names);
            }
            qubitick_model::ProofStep::Rewrite { refs } => {
                split_refs(refs, dual, ctx, fn_names);
            }
            qubitick_model::ProofStep::Fix { .. } | qubitick_model::ProofStep::Discriminate { .. } => {}
        }
    }
}

fn split_chain_just(
    just: &mut qubitick_model::ChainJust,
    dual: &HashSet<u32>,
    ctx: &Ctx,
    fn_names: &mut std::collections::HashMap<u32, String>,
) {
    match just {
        qubitick_model::ChainJust::Rewrite { refs } => split_refs(refs, dual, ctx, fn_names),
        qubitick_model::ChainJust::Sym(r) => split_ref(r, dual, ctx, fn_names),
        qubitick_model::ChainJust::Exact { term } => split_expr(term, dual, ctx, fn_names),
        qubitick_model::ChainJust::Rfl => {}
    }
}

fn split_ref(
    r: &mut qubitick_model::JustRef,
    dual: &HashSet<u32>,
    ctx: &Ctx,
    fn_names: &mut std::collections::HashMap<u32, String>,
) {
    for a in &mut r.args {
        split_expr(a, dual, ctx, fn_names);
    }
}

fn split_refs(
    refs: &mut [qubitick_model::JustRef],
    dual: &HashSet<u32>,
    ctx: &Ctx,
    fn_names: &mut std::collections::HashMap<u32, String>,
) {
    for r in refs {
        split_ref(r, dual, ctx, fn_names);
    }
}

/// Recursively rewire APPLIED uses of a dual-use glyph `g` to the function
/// variant `g + FN_OFFSET` (Lean `X` → `X_fn`); bare uses stay `g` (`X`). Curry
/// keeps the arity faithful: `L_fn : Prop → Prop` for unary, `→ Prop → Prop →`
/// for a binary use, etc. (directive: deterministic lean-side polysymbol encoding).
fn split_expr(
    e: &mut Expr,
    dual: &HashSet<u32>,
    ctx: &Ctx,
    fn_names: &mut std::collections::HashMap<u32, String>,
) {
    match e {
        Expr::Apply(h, args) => {
            if dual.contains(&h.primary) {
                let base = crate::lean_name_of(ctx, h.primary);
                let fn_id = h.primary + FN_OFFSET;
                fn_names
                    .entry(fn_id)
                    .or_insert_with(|| format!("{base}_fn"));
                h.primary = fn_id;
            }
            for a in args.iter_mut() {
                split_expr(a, dual, ctx, fn_names);
            }
        }
        Expr::Bracket(_, args) | Expr::Vector(args) | Expr::Set(args) => {
            for a in args {
                split_expr(a, dual, ctx, fn_names);
            }
        }
        Expr::Binder(_, bs, body) => {
            for b in bs {
                split_expr(&mut b.ty, dual, ctx, fn_names);
            }
            split_expr(body, dual, ctx, fn_names);
        }
        Expr::Recursor(s, cases) => {
            split_expr(s, dual, ctx, fn_names);
            for (_, b) in cases {
                split_expr(b, dual, ctx, fn_names);
            }
        }
        Expr::Matrix(rows) => {
            for r in rows {
                for c in r {
                    split_expr(c, dual, ctx, fn_names);
                }
            }
        }
        Expr::Sym(_) | Expr::Var(_) => {}
    }
}

/// Emit an (untyped) algebra frame so a module's statements type-check.
///
/// The corpus identifies events with propositions, so the carrier is `Prop`
/// (Δ = XOR, ∩ = AND, 𝟙 = True, ∅ = False, `=` = `Eq`). Glyphs used as binder
/// sorts are declared `def : Type := Prop`; other glyphs are uninterpreted
/// `axiom`s over `Prop` at their observed arity. Dual-use glyphs have already
/// been erased to nullary by [`erase_node`]. Equality and the type arrow are
/// built into the framework and skipped.
/// The Lean name a `Definition` node DECLARES.
///
/// A dual-use glyph is written bare (`cap a \mathfrak{L}` — the total ledger) and
/// applied (`\mathfrak{L}(t)` — the ledger sequence). The emitter splits the two,
/// and a definition's `Type:` describes the APPLIED form, so its declaration is the
/// `_fn` variant while the bare name is the frame's. Everything that has to agree
/// about who declares what — the frame's suppression set, the node loop's
/// already-declared guard, and the declaration itself — asks here.
pub(crate) fn definition_decl_name(
    ctx: &Ctx,
    d: &qubitick_model::Definition,
    fn_names: &std::collections::HashMap<u32, String>,
) -> String {
    let id = if fn_names.contains_key(&(d.symbol.primary + FN_OFFSET)) {
        d.symbol.primary + FN_OFFSET
    } else {
        d.symbol.primary
    };
    crate::resolver(ctx, fn_names)(id)
}

/// Whether a `Definition` node actually declares its symbol.
///
/// A definition whose `Sema:` is an equation headed by an operator the node does
/// NOT itself declare is a postulate about operands (`D.BT.43`: `e_1 \cap e_2 =
/// \varnothing` — the node declares `e_1`), not a definition of that operator, and
/// it declares nothing. The DB loader skips these nodes already; the emitter has
/// to agree, or the node claims `cap` — which in a shared ground evicts `cap` from
/// the frame the class axioms are written in.
fn defines_its_glyph(ctx: &Ctx, d: &qubitick_model::Definition) -> bool {
    match &d.d_block.form {
        Some(FormBody::MacroExpr(e)) => !is_operand_postulate(ctx, e, d.symbol.primary),
        _ => true,
    }
}

/// Whether a `Sema:` postulates about two operands instead of defining the operator
/// that heads its equation.
///
/// The operator's own node defines it (`D.A.02` writes `a × b = a + a`); another
/// node's operator heads an equation ABOUT its operands (`D.BT.43` writes
/// `e_1 \cap e_2 = \varnothing` and declares `e_1`). So the question is the head's
/// role and whose glyph it is — never a list of builtin names.
fn is_operand_postulate(ctx: &Ctx, e: &Expr, own: u32) -> bool {
    // A glyph the corpus declares a surface role for is an operator; a glyph nothing
    // declares (a use-only mention) is not, and cannot head a postulate.
    qubitick_model::corpus::is_operand_postulate(e, |id| {
        id != own
            && ctx
                .by_id(id as i64)
                .is_some_and(|r| r.role.is_some())
    })
}

/// Lean names the module defines itself (its `Definition` nodes).
fn defined_symbols(
    ctx: &Ctx,
    nodes: &[Node],
    fn_names: &std::collections::HashMap<u32, String>,
) -> HashSet<String> {
    let mut out = HashSet::new();
    for n in nodes {
        if let Node::Definition(d) = n
            && defines_its_glyph(ctx, d)
        {
            out.insert(definition_decl_name(ctx, d, fn_names));
        }
    }
    out
}


/// The observed application signature of a glyph in one module.
///
/// A flat arity is not enough. The corpus applies operators to BINDERS as well as to
/// events — an indexed union `\bigcup_{x \in S} A(x)` is `bigcup (fun x => ...)`. A
/// binder argument types as `Prop → Prop`, so declaring `bigcup : Prop → Prop` and
/// then passing it a `fun` is an `Application type mismatch`. `binder[i]` marks the
/// positions observed receiving a lambda.
///
/// An argument or a result over a DECLARED sort is typed with that sort's carrier
/// (`C → C → C`), never with the `Prop` carrier every undeclared glyph lives on:
/// applying such an operator to a term of `C` and typing it over `Prop` is an
/// `Application type mismatch` at the first use, two files from the declaration.
#[derive(Default, Clone)]
struct GlyphSig {
    len: usize,
    binder: Vec<bool>,
    /// The declared sort each argument position was observed at, when one was.
    arg_carrier: Vec<Option<u32>>,
    /// The declared sort the operator RETURNS: what its own `Type:` states, or what
    /// the result-sort inference read off its uses.
    res_carrier: Option<u32>,
}

impl GlyphSig {
    /// Record `args` at this signature's positions, growing it to fit.
    fn merge(&mut self, args: &[Expr], arg_carrier: &[Option<u32>]) {
        if args.len() > self.len {
            self.len = args.len();
        }
        self.binder.resize(self.len, false);
        self.arg_carrier.resize(self.len, None);
        for (i, a) in args.iter().enumerate() {
            // Only a LAMBDA types as `Prop → Prop`; `Exists (fun ...)` is itself a Prop.
            if matches!(a, Expr::Binder(qubitick_model::BinderKind::Lambda, _, _)) {
                self.binder[i] = true;
            }
            if let Some(c) = arg_carrier.get(i).copied().flatten() {
                self.arg_carrier[i] = Some(c);
            }
        }
    }

    /// The Lean type, with every carrier rendered through `name_of`.
    fn ty(&self, name_of: &impl Fn(u32) -> String) -> String {
        let mut ty = match self.res_carrier {
            Some(c) if self.len > 0 => name_of(c),
            _ => "Prop".to_string(),
        };
        for i in (0..self.len).rev() {
            let carrier = self.arg_carrier.get(i).copied().flatten();
            let binder = self.binder.get(i).copied().unwrap_or(false);
            ty = match (carrier, binder) {
                // A binder over a declared sort is a function ON that sort
                // (`(C → Prop) → ...`), which takes precedence over the plain form.
                (Some(c), true) => format!("({} → Prop) → {ty}", name_of(c)),
                (None, true) => format!("(Prop → Prop) → {ty}"),
                (Some(c), false) => format!("{} → {ty}", name_of(c)),
                (None, false) => format!("Prop → {ty}"),
            };
        }
        ty
    }
}

/// An authored `Type:` signature, read as the declaration format states it: an arrow
/// chain over declared sorts, with the sort at each argument position and at the
/// result.
#[derive(Clone, Default)]
struct AuthoredType {
    args: Vec<Option<u32>>,
    res: Option<u32>,
    /// The whole signature, when it is a SORT WORD rather than a glyph: `Type: R.E : Type`
    /// says the carrier's type is `Type`, and `Type` is a literal of Lean's, not a symbol
    /// of the corpus's. Rendering it through a symbol table gives `Prop` by accident.
    verbatim: Option<String>,
}

/// The sorts a module's corpus DECLARES, and the constructors of each.
///
/// A sort is a symbol the corpus declares as one — a `Formation`'s carrier, or a
/// `Type:` carrier — and a constructor is a symbol a formation declares, which the
/// loader records with the formation as its home. The emitter keeps no list of its
/// own: a glyph is a sort here because a declaration says so, and the constructors of
/// that sort are the ones the declaration names.
#[derive(Default, Clone)]
struct SortEnv {
    /// Every declared sort, by symbol id.
    sorts: HashSet<u32>,
    /// A formation's carrier → the constructors it declares, in declaration order.
    ctors_of: HashMap<u32, Vec<u32>>,
    /// A constructor → the carrier of the formation that declares it.
    carrier_of: HashMap<u32, u32>,
}

impl SortEnv {
    /// Read the declared sorts out of the symbol table, then out of the module's own
    /// `Formation` bodies — a scope's `Ctx` is not required to carry a row for a
    /// formation the module itself declares.
    fn of(ctx: &Ctx, nodes: &[Node]) -> Self {
        let mut env = Self::default();
        for row in ctx.declared_sorts() {
            env.sorts.insert(row.id as u32);
        }
        for ctor in ctx.declared_ctors() {
            let Some(carrier) = ctx.carrier_of_ctor(ctor.id) else {
                continue;
            };
            let (cid, sid) = (ctor.id as u32, carrier.id as u32);
            env.sorts.insert(sid);
            env.carrier_of.insert(cid, sid);
            env.ctors_of.entry(sid).or_default().push(cid);
        }
        for n in nodes {
            if let Node::Definition(d) = n
                && let Some(FormBody::Formation(ctors)) = &d.d_block.form
            {
                let sid = d.symbol.primary;
                let ctor_ids: Vec<u32> = ctors.iter().map(|c| c.constructor.primary).collect();
                env.sorts.insert(sid);
                for cid in &ctor_ids {
                    env.carrier_of.entry(*cid).or_insert(sid);
                }
                env.ctors_of.entry(sid).or_insert(ctor_ids);
            }
        }
        env
    }

    /// The declared sort a term belongs to, read off its head symbol: the sort
    /// itself, or the carrier of the constructor that builds the term.
    fn carrier_of_head(&self, id: u32) -> Option<u32> {
        self.carrier_of
            .get(&id)
            .copied()
            .or_else(|| self.sorts.contains(&id).then_some(id))
    }

    /// Whether the sort is a formation — an inductive the emitter declares for it.
    fn is_formation(&self, id: u32) -> bool {
        self.ctors_of.contains_key(&id)
    }

    /// Every constructor of every declared sort.
    fn ctors(&self) -> impl Iterator<Item = u32> + '_ {
        self.carrier_of.keys().copied()
    }
}

/// The head symbol of a term.
fn head_id(e: &Expr) -> Option<u32> {
    match e {
        Expr::Sym(ac) => Some(ac.primary),
        Expr::Apply(h, _) => Some(h.primary),
        _ => None,
    }
}

/// Render an arrow chain over carriers with `name_of`; a position that names no
/// declared sort is the `Prop` carrier.
/// `Type`, `Prop`, `Sort u` — the sort words Lean owns (README §2.6).
fn is_sort_word(word: &str) -> bool {
    word == "Type" || word == "Prop" || word == "Sort" || word.starts_with("Sort ")
}

fn arrow_ty(args: &[Option<u32>], res: Option<u32>, name_of: &impl Fn(u32) -> String) -> String {
    let one = |c: Option<u32>| match c {
        Some(id) => name_of(id),
        // A missing component is a component the corpus did not write — `Type: R.E : Type`
        // and `Type: R.x : Prop` are literals, and they arrive as `verbatim` below. `Prop`
        // is the carrier the rest of the format writes over.
        None => "Prop".to_string(),
    };
    let mut ty = one(res);
    for a in args.iter().rev() {
        ty = format!("({}) → {ty}", one(*a));
    }
    ty
}

/// Read an authored `Type:` signature as its argument sorts and its result sort.
fn read_type_arrow(ctx: &Ctx, e: &Expr) -> AuthoredType {
    match e {
        // The declaration format writes a signature as an arrow chain and nothing
        // else, so an application inside a `Type:` is that arrow.
        Expr::Apply(h, args) if args.len() == 2 && is_format_arrow(ctx, h.primary) => {
            let lhs = read_type_arrow(ctx, &args[0]);
            let rhs = read_type_arrow(ctx, &args[1]);
            let mut sorted = lhs.args;
            sorted.push(lhs.res);
            sorted.extend(rhs.args);
            AuthoredType {
                args: sorted,
                res: rhs.res,
                verbatim: None,
            }
        }
        /* A COMPONENT OF A SIGNATURE IS A TYPE, whether the corpus declares it or the
         * GROUND does. A `Type: E` carrier is kind `bound` — the ground says what `E` is —
         * and reading it as "not a sort" collapsed every signature over it to `Prop`:
         * a scaffolded ground came out `plus : (Prop) → (Prop) → Prop` for a corpus whose
         * carrier is a `Type`. The glyph is named inside an arrow chain, which is where a
         * type belongs; rendering its name is the whole answer. */
        /* EVERY SYMBOL IN A SIGNATURE IS PART OF THE TYPE.
         *
         * This used to keep only a `Sym` the corpus DECLARED as a sort, and write
         * everything else over `Prop` — so `Type: R.plus : \mathfrak{R} \to \mathfrak{R}`
         * lost its carrier twice over: once for a ground-named carrier (kind `bound`), and
         * once for the literal `Type` in `Type: R.E : Type`. A signature is a type
         * expression; a glyph in it is a type. Whether that glyph HAS a home is the
         * coverage pass's question, and it answers it before any of this runs. */
        Expr::Sym(ac) => AuthoredType {
            args: Vec::new(),
            res: Some(ac.primary),
            verbatim: None,
        },
        // A SORT WORD IS LEAN'S, NOT THE CORPUS'S: `Type: R.E : Type` says the carrier's
        // type is `Type`. There is no symbol for it to resolve, and `Prop` would be a
        // different claim.
        Expr::Var(word) if is_sort_word(word) => AuthoredType {
            args: Vec::new(),
            res: None,
            verbatim: Some(word.clone()),
        },
        _ => AuthoredType::default(),
    }
}

/// Render an authored `Type:` expression as a Lean type.
///
/// A signature keeps the carrier each position declares (`C \to D` is `C → D`); a
/// position naming no declared sort is the `Prop` carrier. An arrow chain becomes a
/// Lean function type.
pub fn render_type_expr(ctx: &Ctx, e: &Expr) -> String {
    let auth = read_type_arrow(ctx, e);
    auth.verbatim
        .clone()
        .unwrap_or_else(|| arrow_ty(&auth.args, auth.res, &|id| crate::lean_name_of(ctx, id)))
}

/// Re-mark an operator's argument positions from the ARGUMENTS themselves.
///
/// The first walk reads an argument's sort off its head symbol (`^+ t`, a
/// constructor of a declared sort), which is all it can know before the result sorts
/// are inferred. With them in hand it can also see that an argument is an
/// APPLICATION whose head returns a declared sort, and type that position with the
/// carrier too: a `\frac`-style operator applied to two terms of a sort has two
/// positions on it. Without this it is typed over the carrier and a definition whose
/// own body applies it to sort-valued subterms cannot be typed at all.
fn remark_arg_sorts(
    nodes: &[Node],
    sigs: &mut HashMap<u32, GlyphSig>,
    connectives: &HashSet<u32>,
) {
    fn walk(e: &Expr, sigs: &mut HashMap<u32, GlyphSig>, connectives: &HashSet<u32>) {
        if let Expr::Apply(h, args) = e {
            // A propositional CONNECTIVE's argument positions are propositions,
            // whatever an application of it happens to hold: its own `Sema:` states
            // that (`a \leftrightarrow b = 1 \Delta (a \Delta b)`).
            let marks: Vec<(usize, u32)> = if connectives.contains(&h.primary) {
                Vec::new()
            } else {
                args.iter()
                    .enumerate()
                    .filter_map(|(i, a)| match a {
                        Expr::Apply(ah, _) => sigs
                            .get(&ah.primary)
                            .and_then(|s| s.res_carrier)
                            .map(|c| (i, c)),
                        _ => None,
                    })
                    .collect()
            };
            let s = sigs.entry(h.primary).or_default();
            if args.len() > s.len {
                s.len = args.len();
            }
            s.binder.resize(s.len, false);
            s.arg_carrier.resize(s.len, None);
            for (i, carrier) in marks {
                s.arg_carrier[i] = Some(carrier);
            }
        }
        match e {
            Expr::Apply(_, args)
            | Expr::Bracket(_, args)
            | Expr::Vector(args)
            | Expr::Set(args) => {
                for a in args {
                    walk(a, sigs, connectives);
                }
            }
            Expr::Binder(_, bs, body) => {
                for b in bs {
                    walk(&b.ty, sigs, connectives);
                }
                walk(body, sigs, connectives);
            }
            Expr::Recursor(sc, cases) => {
                walk(sc, sigs, connectives);
                for (_, b) in cases {
                    walk(b, sigs, connectives);
                }
            }
            Expr::Matrix(rows) => {
                for r in rows {
                    for c in r {
                        walk(c, sigs, connectives);
                    }
                }
            }
            Expr::Sym(_) | Expr::Var(_) => {}
        }
    }
    for n in nodes {
        match n {
            Node::Axiom(a) => a.statements.iter().for_each(|e| walk(e, sigs, connectives)),
            Node::Theorem(t) => {
                walk(&t.statement, sigs, connectives);
                for step in &t.proof {
                    if let qubitick_model::ProofStep::Chain { rows } = step {
                        for row in rows {
                            if let Some(l) = &row.lhs {
                                walk(l, sigs, connectives);
                            }
                            walk(&row.rhs, sigs, connectives);
                        }
                    }
                }
            }
            Node::Definition(d) => {
                if let Some(FormBody::MacroExpr(e)) = &d.d_block.form {
                    walk(e, sigs, connectives);
                }
                if let Some(sig) = &d.d_block.sig {
                    walk(sig, sigs, connectives);
                }
            }
            Node::Calibration(_) | Node::Census(_) => {}
        }
    }
}

/// Is this RENDERED Lean type the plain `Prop` carrier — `Prop`, or a function whose arguments and
/// result are all `Prop`?
///
/// A characterising axiom (`{op}_def : ∀ params, Eq (op params) body`) binds its parameters at
/// `Prop`, so it may only be emitted when everything its equation names is Prop-SORTED — no
/// declared carrier anywhere in the type. This is a string test for exactly that question, not a
/// type checker: `Prop`, `(Prop → Prop)` and `(Prop) → (Prop) → Prop` all pass it, and a carrier in
/// any position fails it.
/// The test is a string test because these types are strings, and it must not care HOW the renderer
/// parenthesised them: `Prop → Prop → Prop`, `(Prop) → (Prop) → Prop` and `(Prop → Prop)` are the
/// same type three ways. Reading only the first spelling silently dropped every `_def` line for a
/// module whose frame rendered the second — the theorem then cited `otimes_def` and nothing declared
/// it, and the tutorial ship failed with `Unknown identifier otimes_def`.
fn prop_only_type(ty: &str) -> bool {
    /// One balanced pair of parens around the WHOLE string comes off; a pair around part of it stays.
    fn unparen(s: &str) -> &str {
        let t = s.trim();
        let Some(inner) = t.strip_prefix('(').and_then(|r| r.strip_suffix(')')) else {
            return t;
        };
        let mut depth = 0i32;
        for (i, c) in t.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 && i + c.len_utf8() != t.len() {
                        return t; // the first pair closed before the end: not wrapping
                    }
                }
                _ => {}
            }
        }
        unparen(inner)
    }
    unparen(ty).split('→').all(|part| unparen(part) == "Prop")
}

/// What a symbol frame produced, for the module body that follows it.
pub(crate) struct Frame {
    /// The declarations themselves.
    pub text: String,
    /// Characterising axioms (`{op}_def`), which cite the operator and therefore
    /// belong after it: appended to the module BODY, not to the frame.
    pub char_axioms: String,
    /// Glyphs the frame treats as binder sorts.
    pub sorts: HashSet<u32>,
    /// Observed arity per glyph.
    pub arity: HashMap<u32, usize>,
    /// Every name the frame declared (the node loop must not declare it again).
    pub declared: HashSet<String>,
    /// The Lean type of everything this module can name.
    pub types: HashMap<String, String>,
    /// Glyphs this module NAMES and does not declare, as `(lean name, home node)`.
    ///
    /// The frame used to write `axiom <name> : <ty>` for these, which is the fallback
    /// this architecture removes: an axiom is a global fact nobody chose. Each one is
    /// a DEFINITION that lives in another module, so the answer is an `import` — the
    /// glyph arrives as the `def` its own module wrote, and a citation of it closes by
    /// conversion instead of by a characterising axiom.
    pub needed: Vec<(String, i64)>,
}

/// The Lean type a frame gives a glyph.
///
/// The corpus's own `Type:` is the first answer — it STATES the sorts the glyph works
/// over — and where there is none, the observed uses are, with an argument or a
/// result over a declared sort rendered as that sort's carrier. Extracted so that a
/// glyph a DEFINITION declares (and the frame therefore does not) still gets the
/// frame's answer: a glyph's type belongs to the glyph, and every module that names
/// it has to agree — otherwise an era ground holding both cannot type a single body.
fn glyph_type(
    id: u32,
    sigs: &HashMap<u32, GlyphSig>,
    authored: &HashMap<u32, AuthoredType>,
    name_of: &impl Fn(u32) -> String,
) -> String {
    if let Some(auth) = authored.get(&id) {
        if let Some(text) = &auth.verbatim {
            return text.clone();
        }
        return arrow_ty(&auth.args, auth.res, name_of);
    }
    sigs.get(&id).cloned().unwrap_or_default().ty(name_of)
}

/// The formation a glyph is a CONSTRUCTOR of, if it is one.
fn formulations_carrier(
    formations: &HashMap<u32, (String, Vec<u32>)>,
    id: u32,
) -> Option<u32> {
    formations
        .iter()
        .find(|(_, (_, ctors))| ctors.contains(&id))
        .map(|(carrier, _)| *carrier)
}

/// Whether the corpus BINDS this glyph to a Lean name that already exists.
///
/// A bound glyph is declared by the project's ground, so the frame declares nothing for
/// it — the name is used as it stands.
fn is_bound(ctx: &Ctx, id: u32) -> bool {
    ctx.by_id(id as i64).is_some_and(|r| r.kind == "bound")
}

fn symbol_declarations(
    ctx: &Ctx,
    nodes: &[Node],
    emitted: &[Node],
    fn_names: &std::collections::HashMap<u32, String>,
    opts: &Options,
) -> Frame {
    let base_resolver = crate::resolver(ctx, fn_names);
    /* IN A GROUND, A FIELD OF THE INSTANCE NAMES ITSELF.
     *
     * The corpus reaches a ground symbol through `R` (`Type: R.plus`) because that is how
     * a proof module says it; the ground is where the class DECLARING that field lives, and
     * inside it there is no instance yet. So a ground writes the field's own name — the
     * same reading the class body makes, applied to the whole file. */
    let resolver = move |id: u32| -> String {
        let name = base_resolver(id);
        if opts.ground_only {
            name.strip_prefix("R.").map(str::to_string).unwrap_or(name)
        } else {
            name
        }
    };
    let name_of = |id: u32| resolver(id);
    // The sorts the corpus DECLARES, and the constructors of each formation. Every
    // typing decision below reads this map and the glyphs' own authored signatures;
    // the emitter supplies no meaning for a glyph of its own.
    let env = SortEnv::of(ctx, nodes);

    // Every name this frame DECLARES itself. The node loop needs it: the frame
    // declares a formation's carrier and its constructors, so a `Definition` node for
    // one of those glyphs must not declare it a second time — an era ground, which
    // holds every definition at once, showed it as
    // `Qubitick.HL.W has already been declared`.
    let mut declared_names: HashSet<String> = HashSet::new();
    // Glyphs this module names and does not declare: each is another module's
    // definition, and each becomes an IMPORT (see `Frame::needed`).
    let mut needed: Vec<(String, i64)> = Vec::new();
    // lean name -> declared Lean type string, for every frame symbol.
    let mut declared_types: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    // The types the frame infers for glyphs it does NOT declare (a Definition owns
    // those). Kept apart from `declared_types`, which also means "this frame has
    // already written this name" and must not gain names it never wrote.
    let mut inferred_types: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    // Authored `Type:` signatures, read as the declaration format states them.
    //
    // An authored signature describes the glyph's APPLIED form: a dual-use glyph
    // (`\mathfrak{L}` vs `\mathfrak{L}(t)`) has a `_fn` variant whose id carries the
    // family type, while the bare use is a `Prop` (the total-ledger set). Attaching
    // the family sig to the bare id typed bare `cap a \mathfrak{L}` against a
    // function type, which broke the whole `class` chain.
    let mut authored: HashMap<u32, AuthoredType> = HashMap::new();
    for n in nodes.iter().chain(emitted.iter()) {
        if let Node::Definition(d) = n
            && let Some(sig) = &d.d_block.sig
        {
            let family = if fn_names.contains_key(&(d.symbol.primary + FN_OFFSET)) {
                d.symbol.primary + FN_OFFSET
            } else {
                d.symbol.primary
            };
            authored
                .entry(family)
                .or_insert_with(|| read_type_arrow(ctx, sig));
        }
    }
    let mut sigs: HashMap<u32, GlyphSig> = HashMap::new();
    let mut sorts: HashSet<u32> = HashSet::new();
    let mut seen: HashSet<u32> = HashSet::new();
    for n in nodes {
        visit_node(n, &mut sigs, &mut sorts, &mut seen, &env);
    }
    /* WHAT THIS MODULE ITSELF WRITES.
     *
     * `nodes` is the module's CONTEXT — the whole scope — and the frame is built over
     * it so a body can resolve what its neighbours declare. But an IMPORT is only owed
     * for a glyph the module's own text names: deriving one from the context made every
     * module in a corpus import every other and the compile order became a cycle. */
    let mut own: HashSet<u32> = HashSet::new();
    {
        let (mut s2, mut so2, mut own2) = (HashMap::new(), HashSet::new(), HashSet::new());
        for n in emitted {
            visit_node(n, &mut s2, &mut so2, &mut own2, &env);
        }
        own.extend(own2);
    }
    // --- Infer operator RESULT sorts (a declared carrier, or `Prop`) ---
    // An operator whose result a CONSTRUCTOR of a declared sort consumes, or that
    // stands on the other side of `=` from a term of one, returns that sort; the
    // closure carries the same answer to whatever feeds such an operator. This is what
    // types an operator over its sort (`C → C → C`) instead of over the carrier —
    // typed over `Prop`, `Eq (op a c) a` with `a : C` is an application-type
    // mismatch.
    let (consumed_by, eq_terms) = collect_consumed_by(nodes, &env);
    for (id, carrier) in infer_result_sorts(&env, &consumed_by, &eq_terms) {
        if let Some(sig) = sigs.get_mut(&id) {
            sig.res_carrier = Some(carrier);
        }
    }
    // With the result sorts settled, re-read the argument positions: an argument that
    // is an APPLICATION whose head returns a declared sort belongs to that sort.
    //
    // A connective is excluded outright: its arguments are propositions by its own
    // `Sema:`, whatever one application happens to hold.
    let connectives: HashSet<u32> = seen
        .iter()
        .copied()
        .filter(|id| {
            let Some(row) = ctx.by_id(*id as i64) else {
                return false;
            };
            qubitick_model::format::is_iff(&row.tex) || ctx.has_facet(&row.tex, Facet::Connective)
        })
        .collect();
    remark_arg_sorts(nodes, &mut sigs, &connectives);

    // Every declared sort an emitted type NAMES. A carrier a type mentions has to
    // exist in the text even when no statement uses the carrier itself, and a
    // formation's carrier additionally has to carry its inductive.
    let mut named_carriers: HashSet<u32> = HashSet::new();
    for auth in authored.values() {
        named_carriers.extend(auth.args.iter().copied().flatten());
        named_carriers.extend(auth.res);
    }
    for (id, sig) in &sigs {
        if seen.contains(id) {
            named_carriers.extend(sig.arg_carrier.iter().copied().flatten());
            named_carriers.extend(sig.res_carrier);
        }
    }

    // Recursor-definable operators (from CONTEXT definitions): their defining
    // recursive `\land`-Sema is compiled to a `noncomputable def ... .rec`, so every
    // module that USES the operator sees a real definition (`[\equiv]` can unfold it)
    // instead of an uninterpreted `axiom`. Keyed by the operator's glyph id (==
    // definition symbol id after `definition_symbol_id`).
    let recursor_resolver = crate::resolver(ctx, fn_names);
    // Cross-operator recursor closure, by fixpoint: a recursor op qualifies if every
    // glyph its case RHS references is itself recursor-definable. Keying and naming by
    // the Sema's head id (not the glyph-line's first control word, which can drop a
    // mod suffix) resolves the operator correctly.
    let sema_head = |e: &Expr| -> Option<(u32, String)> {
        use qubitick_model::{BinderKind as BK, Expr as E, EQ_SYM as EQ};
        // Collect every conjunct of a (possibly nested) conjunction.
        fn conj<'a>(x: &'a Expr, ctx: &Ctx, out: &mut Vec<&'a Expr>) {
            match x {
                E::Apply(h, args) if crate::module::is_conjunct_head(ctx, h.primary) => {
                    for a in args {
                        conj(a, ctx, out);
                    }
                }
                o => out.push(o),
            }
        }
        let mut cs: Vec<&Expr> = Vec::new();
        conj(e, ctx, &mut cs);
        for c in cs {
            // strip a `∀ n, ...` wrapper
            let body = match c {
                E::Binder(BK::Forall, _, b) => b.as_ref(),
                o => o,
            };
            if let E::Apply(eq0, a0) = body
                && eq0.primary == EQ
                && a0.len() == 2
                && let E::Apply(fh, _) = &a0[0]
            {
                return Some((fh.primary, recursor_resolver(fh.primary)));
            }
        }
        None
    };
    let mut recursor_able: HashSet<u32> = HashSet::new();
    let mut recursor_defs: HashMap<u32, String> = HashMap::new();
    // DEMAND-DRIVEN: only recursor-def operators the module's OWN emitted nodes
    // reference; context defs must not inject their recursors into frames that never
    // use (nor declare the sorts of) them. Gated by LEAN NAME (ids can differ between
    // the statement's glyph and the Sema's head for the same op).
    let mut own_names: HashSet<String> = HashSet::new();
    let mut own_ids: HashSet<u32> = HashSet::new();
    for n in emitted {
        match n {
            Node::Theorem(t) => {
                collect_plain_ids(&t.statement, &mut own_ids);
                // Proof rows may reference operators the statement never mentions;
                // without the demand they stay opaque axioms and `[\equiv]` steps
                // over them cannot reduce.
                for step in &t.proof {
                    if let qubitick_model::ProofStep::Chain { rows } = step {
                        for row in rows {
                            if let Some(l) = &row.lhs {
                                collect_plain_ids(l, &mut own_ids);
                            }
                            collect_plain_ids(&row.rhs, &mut own_ids);
                        }
                    }
                }
            }
            Node::Definition(d) => {
                own_ids.insert(d.symbol.primary);
                if let Some(FormBody::MacroExpr(s)) = &d.d_block.form {
                    collect_plain_ids(s, &mut own_ids);
                }
                if let Some(s) = &d.d_block.sig {
                    collect_plain_ids(s, &mut own_ids);
                }
            }
            Node::Axiom(a) => {
                for s in &a.statements {
                    collect_plain_ids(s, &mut own_ids);
                }
            }
            Node::Calibration(_) | Node::Census(_) => {}
        }
    }
    for id in &own_ids {
        let nm = recursor_resolver(*id);
        own_names.insert(nm);
    }

    // DEMAND CLOSURE: an admitted def's Sema glyphs are also demanded, so the
    // dependency chain of a definition over a formation becomes reducible defs.
    // Names are compared with a trailing `_fn` (applied-use split) normalized away.
    let norm = |n: &str| n.strip_suffix("_fn").unwrap_or(n).to_string();
    let mut demand: HashSet<u32> = own_ids.clone();
    let mut demand_names: HashSet<String> = own_names.iter().map(|n| norm(n)).collect();
    // ctor → (carrier lean name, ordered ctor ids), for formation recursors.
    let formations: std::collections::HashMap<u32, (String, Vec<u32>)> = {
        let mut m = std::collections::HashMap::new();
        for n in nodes {
            if let Node::Definition(d) = n
                && let Some(FormBody::Formation(ctors)) = &d.d_block.form
            {
                let name = crate::lean_name_of(ctx, d.symbol.primary);
                let ids: Vec<u32> = ctors.iter().map(|c| c.constructor.primary).collect();
                for &id in &ids {
                    m.entry(id).or_insert_with(|| (name.clone(), ids.clone()));
                }
            }
        }
        m
    };
    loop {
        let before = recursor_able.len();
        for n in nodes {
            if let Node::Definition(d) = n
                && let Some(FormBody::MacroExpr(e)) = &d.d_block.form
                && let Some((head_id, hname)) = sema_head(e)
                && !recursor_defs.contains_key(&head_id)
                && (demand.contains(&head_id) || demand_names.contains(&norm(&hname)))
                && let Some(lt) = crate::definition::recursor_def_text_formation(
                    ctx,
                    &hname,
                    e,
                    &recursor_resolver,
                    &formations,
                )
            {
                recursor_defs.insert(head_id, lt);
                recursor_able.insert(head_id);
                // DEMAND CLOSURE: the def's Sema glyphs become demanded so the
                // dependency chain is reducible.
                let mut ids = HashSet::new();
                collect_plain_ids(e, &mut ids);
                for x in ids {
                    demand.insert(x);
                    demand_names.insert(norm(&recursor_resolver(x)));
                }
            }
        }
        if recursor_able.len() == before {
            break;
        }
    }
    // Symbols the module DEFINES itself get a real `def` and must not also be
    // declared as frame axioms (name collision).
    // Only names this module actually EMITS a `def` for may be suppressed from the
    // frame. `nodes` includes CONTEXT (a theorem's file holds one node, so the
    // corpus definitions arrive as context), and those definitions are never
    // emitted — suppressing their glyphs left them undeclared, so a statement that
    // APPLIES one applied an unknown constant and Lean reported
    // `unknown constant 'CoeFun'`.
    let defined_names = defined_symbols(ctx, emitted, fn_names);
    let mut out = String::new();
    let mut extra_defined: HashSet<String> = HashSet::new();
    // `nodes` and `emitted` are the same slice when a module IS its own context — a
    // frame assembled from a whole era, for instance. Chaining them would then emit
    // every formation twice, so each carrier is declared once and the second pass
    // over it is skipped.
    let mut formation_done: HashSet<u32> = HashSet::new();
    // The one way an inductive appears: a formation the corpus declares, named by the
    // module (its carrier or one of its constructors, in a statement or in a type).
    for n in nodes.iter().chain(emitted.iter()) {
        if let Node::Definition(d) = n
            && let Some(qubitick_model::FormBody::Formation(ctors)) = &d.d_block.form
            && formation_done.insert(d.symbol.primary)
        {
            let ctor_ids: Vec<u32> = ctors.iter().map(|c| c.constructor.primary).collect();
            let used = seen.contains(&d.symbol.primary)
                || named_carriers.contains(&d.symbol.primary)
                || ctor_ids.iter().any(|id| seen.contains(id));
            if !used {
                continue;
            }
            let carrier = name_of(d.symbol.primary);
            // A constructor's Lean type: its carrier when nullary, its declared argument
            // sorts armed into the carrier when it takes arguments.
            let ctor_ty = |c: &qubitick_model::CtorDecl| -> String {
                if c.args.is_empty() {
                    return carrier.clone();
                }
                let mut ty = String::new();
                for arg in &c.args {
                    ty.push_str(&name_of(*arg));
                    ty.push_str(" \u{2192} ");
                }
                ty.push_str(&carrier);
                ty
            };
            let mut f = format!("set_option genInjectivity false in\ninductive {carrier} : Type\n");
            // A CONSTRUCTOR CARRIES ITS DECLARED ARGUMENT SORTS: a nullary one is
            // written bare (Lean reads it as its carrier), and one declared with
            // arguments is a function into the carrier — which is what makes a
            // RECURSIVE formation a real inductive rather than a pile of constants.
            for c in ctors {
                let cnam = name_of(c.constructor.primary);
                let ty = ctor_ty(c);
                if c.args.is_empty() {
                    f.push_str(&format!("  | {cnam}\n"));
                } else {
                    f.push_str(&format!("  | {cnam} : {ty}\n"));
                }
                declared_names.insert(cnam.clone());
                declared_types.insert(cnam.clone(), ty);
                extra_defined.insert(cnam);
            }
            extra_defined.insert(carrier.clone());
            declared_names.insert(carrier.clone());
            declared_types.insert(carrier.clone(), "Type".to_string());
            for c in ctors {
                let cnam = name_of(c.constructor.primary);
                declared_names.insert(cnam.clone());
                // A nullary constructor is a value of its carrier; one with arguments
                // was given its function type above. Either way `known_types` agrees
                // with the inductive that declares it.
                if c.args.is_empty() {
                    declared_types.insert(cnam, carrier.clone());
                }
            }
            out.push_str(&f); // the `inductive` FIRST
            for c in ctors {
                let cnam = name_of(c.constructor.primary);
                // The corpus writes the bare glyph, so the glyph gets a name of its own
                // bound to the constructor — typed with the CONSTRUCTOR's type, or a
                // recursive one would be closed over the wrong arity.
                out.push_str(&format!(
                    "def {cnam} : {} := {carrier}.{cnam}\n",
                    ctor_ty(c)
                ));
            }
        }
    }
    // A declared sort that is NOT a formation is the `Type` carrier the corpus's
    // signatures are written over. A carrier a type names has to exist even when no
    // statement uses the carrier glyph itself.
    for sid in named_carriers.clone() {
        let lean = name_of(sid);
        if env.is_formation(sid)
            || seen.contains(&sid)
            || defined_names.contains(&lean)
            || extra_defined.contains(&lean)
            || declared_types.contains_key(&lean)
        {
            continue;
        }
        out.push_str(&format!("def {lean} : Type := Prop\n"));
        declared_types.insert(lean, "Type".to_string());
    }
    // Recursor defs are deferred: they follow every ordinary axiom symbol, and are
    // ordered among themselves by dependency (below).
    let mut defer_recursor_defs: Vec<(u32, String)> = Vec::new();
    let mut ids: Vec<u32> = seen.iter().copied().collect();
    ids.sort_unstable();
    // Two distinct tex glyphs can sanitize to the SAME Lean name (`\widehat{J}_{+}`
    // and `\widehat{J}_{-}` → `widehat_J`), so each name is emitted once.
    let mut emitted_names: HashSet<String> = HashSet::new();
    for id in ids {
        // Skip anything a Definition in this module declares, matched by LEAN
        // NAME (ids can differ between the definition's symbol and a referenced
        // glyph of the same tex — matching by id caused duplicate declarations).
        let lean = resolver(id);
        if id == EQ_SYM {
            continue;
        }
        // A glyph a Definition in this module declares: the frame leaves it alone —
        // but its TYPE is still the frame's answer, and another definition's body
        // may apply it. The name stays FREE for a later glyph of the same spelling.
        if defined_names.contains(&lean) {
            if !sorts.contains(&id) {
                inferred_types.insert(lean.clone(), glyph_type(id, &sigs, &authored, &name_of));
            }
            continue;
        }
        if extra_defined.contains(&lean) || !emitted_names.insert(lean.clone()) {
            continue;
        }
        // A BOUND glyph (`Lean: HMul.hMul`) is declared BY THE GROUND. The frame
        // declares nothing for it — an `axiom HMul.hMul` beside Mathlib's own
        // declaration is a clash, and the point of the binding is to call what the
        // ground provides.
        if is_bound(ctx, id) {
            continue;
        }
        // The declaration format's arrow declares no constant: Lean's `→` is built in.
        if is_format_arrow(ctx, id) {
            continue;
        }
        if sorts.contains(&id) || env.sorts.contains(&id) {
            // A DECLARED sort is a `Type` carrier, and a glyph used as a binder type
            // is one too (the corpus's spaces are written over `Prop`). A formation's
            // carrier is its own inductive and was skipped above.
            out.push_str(&format!("def {lean} : Type := Prop\n"));
            // Record it: a frame assembled from a whole era can reach the same
            // glyph again as an operator, and one declaration is all Lean allows.
            declared_types.insert(lean.clone(), "Type".to_string());
        } else if let Some(def_text) = recursor_defs.get(&id) {
            // A recursor-definable operator over a declared formation is a real
            // `def` (so `[\equiv]` unfolds it by kernel conversion), not a frame
            // `axiom`.
            defer_recursor_defs.push((id, def_text.clone()));
        } else {
            if declared_types.contains_key(&lean) {
                // Already declared by the sort loop above (a frame assembled from a
                // whole era can reach one glyph from both). One declaration, or the
                // module does not elaborate.
                continue;
            }
            /* NO `axiom`. A glyph this module names and does not declare is a
               definition that lives in ANOTHER module, and the honest encoding of
               that is an `import`: the module gets the `def` its own module wrote, so
               a citation of it closes by kernel conversion and `#print axioms` sees
               nothing. Writing `axiom {lean} : {ty}` instead made every cross-module
               reference a global assumption nobody chose — the bug this removes. */
            if !own.contains(&id) && !own.contains(&(id.wrapping_sub(FN_OFFSET))) {
                // The context declares it for a neighbour; this module never writes it.
                continue;
            }
            /* A CONSTRUCTOR'S HOME IS THE FORMATION'S MODULE.
               One node per file means `\tau` and its constructors are separate nodes, and
               they refer to each other: the formation is typed by its constructors' sorts,
               a constructor by its carrier. Importing each other's modules is a cycle; the
               module that declares the inductive declares its constructors too (as the
               `def ctor : ... := C.ctor` aliases), so that is the one to import. */
            /* A FORMATION'S OWN MODULE DECLARES ITS CONSTRUCTORS, so it does not
               import them: `\tau = \tau_0 \vee (^+ \tau)` writes the constructors to
               declare them, and the module writes `inductive tau | tau_0 | caret`
               itself. Importing the constructor's node instead would be that node
               importing the carrier back — the one cycle a one-node-per-file corpus
               cannot break any other way. */
            if emitted.iter().any(|n| {
                matches!(n, Node::Definition(d)
                    if matches!(&d.d_block.form, Some(qubitick_model::FormBody::Formation(ctors))
                        if ctors.iter().any(|c| c.constructor.primary == id)))
            }) {
                continue;
            }
            let carrier = formulations_carrier(&formations, id);
            let home = carrier
                .and_then(|c| ctx.by_id(c as i64))
                .or_else(|| ctx.carrier_of_ctor(id as i64))
                .or_else(|| ctx.by_id(id as i64))
                .map(|r| r.home_node)
                .unwrap_or(-1);
            /* A GLYPH THIS MODULE DECLARES IS NOT IMPORTED. `Type: \mathcal{P} -> ...`
               with no ground name is a glyph the module itself declares (a frame sort), so
               its home node is this one — importing it made the module import ITSELF, and
               the compile order had no first module. */
            if emitted.iter().any(|n| n.base().id as i64 == home) {
                continue;
            }
            needed.push((lean.clone(), home));
        }
    }
    // Every ordinary frame declaration counts as declared too.
    declared_names.extend(declared_types.keys().cloned());
    declared_names.extend(extra_defined.iter().cloned());
    let arity: HashMap<u32, usize> = sigs.iter().map(|(k, v)| (*k, v.len)).collect();
    // Recursor defs, emitted after every ordinary axiom symbol, in DEPENDENCY
    // order: a def whose text references another pending def's NAME waits for it
    // (name-substring matching is robust to the `_fn` id splits). Self-references
    // (recursion) are excluded.
    let mut pending: Vec<(u32, String, String)> = defer_recursor_defs
        .into_iter()
        .map(|(id, txt)| {
            let nm = recursor_resolver(id);
            (id, txt, nm)
        })
        .collect();
    let mut guard = 0usize;
    while !pending.is_empty() && guard < 64 {
        guard += 1;
        let mut emitted_this = false;
        let mut still: Vec<(u32, String, String)> = Vec::new();
        let all_names: Vec<String> = pending.iter().map(|(_, _, n)| n.clone()).collect();
        for (id, txt, nm) in pending {
            let depends = all_names.iter().any(|onm| {
                onm != &nm
                    && txt
                        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                        .any(|t| t == onm.as_str())
            });
            if !depends {
                let t = if txt.ends_with('\n') { txt } else { format!("{txt}\n") };
                out.push_str(&t);
                emitted_this = true;
            } else {
                still.push((id, txt, nm));
            }
        }
        pending = still;
        if !emitted_this {
            // cycle: emit the rest in id order to keep the frame total
            pending.sort_by_key(|(id, _, _)| *id);
            break;
        }
    }
    for (_, txt, _) in &pending {
        let t = if txt.ends_with('\n') { txt.clone() } else { format!("{txt}\n") };
        out.push_str(&t);
    }
    // The characterising axioms are collected APART from the frame: a module that
    // does not declare the operator itself still has to carry them (a proof cites
    // `lnot_def` as a term), and the frame's text is dropped for a module that
    // imports a ground. They reference the operator, so they go into the body where
    // the operator is in scope — after the definition in a ground.
    let mut char_axioms = String::new();
    // Characterising axioms for `=`-defined PROP macros (`\lnot a = \mathbb{1} Δ a`,
    // D.BT.02): `axiom {op}_def : ∀ params, Eq (op params) body`. Theorem proofs cite
    // these by name (chain.rs's `rule.definitional` branch) instead of relying on
    // `Eq.refl` over an opaque head, which cannot close `Eq (lnot X) (Δ 1 X)` when
    // `lnot` is only an axiom in this frame.
    for n in nodes {
        if let Node::Definition(d) = n
            && let Some(FormBody::MacroExpr(e)) = &d.d_block.form
            && let Expr::Apply(h, eqargs) = e
            && h.primary == EQ_SYM
            && eqargs.len() == 2
        {
            let name = crate::lean_name_of(ctx, d.symbol.primary);
            let l = qubitick_lean::render(&eqargs[0], &resolver);
            let r = qubitick_lean::render(&eqargs[1], &resolver);
            // ONE carrier per characterising axiom, and it is whoever declares the
            // operator it names — an `axiom {op}_def` must FOLLOW the `def {op}`:
            //   * the GROUND declares the era's definitions, so it carries their
            //     `_def` lines (and only those: an operator nothing defines is a
            //     frame axiom, and a frame axiom needs no characterisation);
            //   * a per-file module declares the frame itself and no definitions,
            //     so it carries the `_def` lines of the operators it does not define;
            //   * a module that IMPORTS its ground carries neither — the ground is
            //     the only module that can see both halves. Emitting them here as
            //     well duplicated every one the moment two scope modules of the same
            //     era joined the same ground.
            /* A NODE MODULE NEVER CARRIES A CHARACTERISING AXIOM.
             *
             * The pair `axiom {op}` + `axiom {op}_def` existed because a definition
             * seen from a module that could not unfold it had to be restated as an
             * assumption. A module IMPORTS the module that defines the operator now, so
             * the operator IS a `def` there and a citation of it closes by kernel
             * conversion (`Eq.refl`) — the `_def` half has nothing left to say. Only a
             * GROUND, which declares the era's definitions itself, still writes one, and
             * only for an operator whose own declaration it carries. */
            let skip = !opts.ground_only;

            if skip {
                continue;
            }
            // Emit the characterising axiom for any `=`-macro whose operands and
            // result are the plain `Prop` carrier, so an author's own defined
            // operator is citable definitionally. The test is the operator's observed
            // GlyphSig (no carrier positions, Prop result), which is exactly the
            // condition under which the `∀ (a : Prop)` binder shape is well-typed.
            let prop_valued = sigs.get(&d.symbol.primary).is_none_or(|sg| {
                sg.res_carrier.is_none()
                    && sg.arg_carrier.iter().all(|c| c.is_none())
                    && sg.binder.iter().all(|b| !*b)
            });
            // A symbol is PROP-SAFE only when its declared type is built purely of
            // `Prop`. Any symbol that is not prop-safe is added to the exclusion set,
            // together with every declared sort and every constructor of one, so an
            // equation mentioning a `Type`-valued glyph is skipped: its `∀ (a : Prop)`
            // binder shape would not type-check (and it closes by `Eq.refl` anyway).
            let mut sort_names: Vec<String> =
                sorts.iter().map(|id| crate::lean_name_of(ctx, *id)).collect();
            for (id, sg) in sigs.iter() {
                let prop_safe = sg.res_carrier.is_none()
                    && sg.arg_carrier.iter().all(|c| c.is_none())
                    && sg.binder.iter().all(|b| !*b);
                if !prop_safe {
                    sort_names.push(crate::lean_name_of(ctx, *id));
                }
            }
            for sid in &env.sorts {
                sort_names.push(name_of(*sid));
            }
            for cid in env.ctors() {
                sort_names.push(name_of(cid));
            }
            let mentions_sort = |t: &str| {
                t.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .any(|w| !w.is_empty() && sort_names.iter().any(|sn| sn == w))
            };
            // Every identifier in either side must be a KNOWN Prop-valued symbol
            // (or the macro's own parameter). This is the precise well-typedness
            // condition: anything else (a carrier-valued glyph, or one not in the
            // frame at all) would make `Eq (op ...) ...` ill-typed.
            let mut params_now = Vec::new();
            qubitick_pattern::expr_idents(&eqargs[0], &mut params_now);
            let prop_ok: std::collections::HashSet<String> = declared_types
                .iter()
                .filter(|(_, ty)| prop_only_type(ty))
                .map(|(nm, _)| nm.clone())
                .collect();
            let all_idents_prop = |t: &str| {
                t.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .filter(|w| !w.is_empty())
                    .filter(|w| w.chars().next().is_some_and(|c| c.is_alphabetic()))
                    .all(|w| {
                        params_now.iter().any(|p| p == w)
                            || w == name
                            || prop_ok.contains(w)
                    })
            };
            if !all_idents_prop(&l) || !all_idents_prop(&r) {
                continue;
            }
            if !prop_valued
                || sorts.contains(&d.symbol.primary)
                || mentions_sort(&l)
                || mentions_sort(&r)
            {
                continue;
            }
            let mut params = Vec::new();
            qubitick_pattern::expr_idents(&eqargs[0], &mut params);
            let p = params
                .iter()
                .map(|v| format!("({v} : Prop)"))
                .collect::<Vec<_>>()
                .join(" ");
            let quant = if p.is_empty() { String::new() } else { format!("∀ {p}, ") };
            char_axioms.push_str(&format!("axiom {name}_def : {quant}Eq ({l}) ({r})\n"));
        }
    }
    // The types of everything this module can name: what the frame declared, plus
    // the authored `Type:` of every definition in it (whose declaration the frame
    // leaves to the node). A definition body is typed from this, so the body and the
    // frame agree about which carrier a glyph lives on.
    // Precedence: what the CORPUS says (`Type:`), then what this frame would have
    // written for a glyph a definition owns, then what it wrote.
    let mut known_types: HashMap<String, String> = HashMap::new();
    for (id, auth) in &authored {
        known_types.insert(
            resolver(*id),
            auth.verbatim
                .clone()
                .unwrap_or_else(|| arrow_ty(&auth.args, auth.res, &name_of)),
        );
    }
    known_types.extend(inferred_types.clone());
    known_types.extend(declared_types.clone());
    Frame {
        text: out,
        char_axioms,
        sorts,
        arity,
        declared: declared_names,
        types: known_types,
        needed,
    }
}

/// Walk the scalar/apply/binder structure collecting every symbol id.
fn collect_plain_ids(e: &Expr, out: &mut HashSet<u32>) {
    use qubitick_model::Expr as E;
    match e {
        E::Sym(ac) => {
            out.insert(ac.primary);
        }
        E::Apply(h, args) => {
            out.insert(h.primary);
            for a in args {
                collect_plain_ids(a, out);
            }
        }
        E::Binder(_, bs, body) => {
            for b in bs {
                collect_plain_ids(&b.ty, out);
            }
            collect_plain_ids(body, out);
        }
        _ => {}
    }
}

/// Record, for every node, which operator applications feed which head, and which
/// operators stand on the other side of `=` from a term of a declared sort.
///
/// `Apply(outer, args)` whose argument is itself an application records
/// `outer → inner`: the inner operator's result is consumed by the outer head.
/// A head and the heads directly applied to its result, plus the `(lhs, rhs)` head pairs
/// of the module's equations — the shape the result-carrier inference reads.
type ConsumedBy = (HashMap<u32, HashSet<u32>>, Vec<(u32, u32)>);

fn collect_consumed_by(nodes: &[Node], env: &SortEnv) -> ConsumedBy {
    let mut out: HashMap<u32, HashSet<u32>> = HashMap::new();
    let mut eq_terms: Vec<(u32, u32)> = Vec::new();
    let mut rec = |e: &Expr| rec_cb(e, &mut out, &mut eq_terms, env);
    for n in nodes {
        match n {
            Node::Axiom(a) => a.statements.iter().for_each(&mut rec),
            Node::Theorem(t) => rec(&t.statement),
            Node::Definition(d) => {
                if let Some(FormBody::MacroExpr(e)) = &d.d_block.form {
                    rec(e);
                }
                if let Some(sig) = &d.d_block.sig {
                    rec(sig);
                }
            }
            Node::Calibration(_) | Node::Census(_) => {}
        }
    }
    (out, eq_terms)
}

/// The declared sort each operator RETURNS, read off the way it is used.
///
/// The argument of a constructor of a declared sort is a term of that sort, so an
/// operator whose result that constructor consumes returns it; and an operator that
/// stands on the other side of `=` from a term of a sort returns it too. The answer
/// then propagates along `consumed_by`: whatever feeds an operator that returns `C`
/// also returns `C`.
///
/// Deterministic: heads and their consumers are visited in id order, and the first
/// sort a head is proved to return is the one it keeps.
fn infer_result_sorts(
    env: &SortEnv,
    consumed_by: &HashMap<u32, HashSet<u32>>,
    eq_terms: &[(u32, u32)],
) -> HashMap<u32, u32> {
    let mut edges: Vec<(u32, Vec<u32>)> = consumed_by
        .iter()
        .map(|(eater, deps)| {
            let mut deps: Vec<u32> = deps.iter().copied().collect();
            deps.sort_unstable();
            (*eater, deps)
        })
        .collect();
    edges.sort_unstable_by_key(|(eater, _)| *eater);
    let mut res: HashMap<u32, u32> = HashMap::new();
    // A constructor of a declared sort demands a term of that sort.
    for (eater, deps) in &edges {
        if let Some(carrier) = env.carrier_of.get(eater) {
            for dep in deps {
                res.entry(*dep).or_insert(*carrier);
            }
        }
    }
    let mut terms: Vec<(u32, u32)> = eq_terms.to_vec();
    terms.sort_unstable();
    for (head, carrier) in terms {
        res.entry(head).or_insert(carrier);
    }
    loop {
        let before = res.len();
        for (eater, deps) in &edges {
            let Some(carrier) = res.get(eater).copied() else {
                continue;
            };
            for dep in deps {
                res.entry(*dep).or_insert(carrier);
            }
        }
        if res.len() == before {
            break;
        }
    }
    res
}

fn rec_cb(
    e: &Expr,
    out: &mut HashMap<u32, HashSet<u32>>,
    eq_terms: &mut Vec<(u32, u32)>,
    env: &SortEnv,
) {
    match e {
        Expr::Apply(head, args) => {
            // `Eq (op ...) term`: when the other side is a term of a declared sort,
            // the operator applied on this side returns that sort.
            if head.primary == EQ_SYM && args.len() == 2 {
                for (i, side) in args.iter().enumerate() {
                    let other = &args[1 - i];
                    if let (Expr::Apply(oh, _), Some(carrier)) = (
                        side,
                        head_id(other).and_then(|h| env.carrier_of.get(&h).copied()),
                    ) {
                        eq_terms.push((oh.primary, carrier));
                    }
                }
            }
            for a in args {
                if let Expr::Apply(inner, _) = a {
                    out.entry(head.primary).or_default().insert(inner.primary);
                }
                rec_cb(a, out, eq_terms, env);
            }
        }
        Expr::Bracket(_, args) | Expr::Vector(args) | Expr::Set(args) => {
            for a in args {
                rec_cb(a, out, eq_terms, env);
            }
        }
        Expr::Binder(_, bs, body) => {
            for b in bs {
                rec_cb(&b.ty, out, eq_terms, env);
            }
            rec_cb(body, out, eq_terms, env);
        }
        Expr::Recursor(s, cases) => {
            rec_cb(s, out, eq_terms, env);
            for (_, b) in cases {
                rec_cb(b, out, eq_terms, env);
            }
        }
        Expr::Matrix(rows) => {
            for r in rows {
                for c in r {
                    rec_cb(c, out, eq_terms, env);
                }
            }
        }
        Expr::Sym(_) | Expr::Var(_) => {}
    }
}

/// Walk one node's statements collecting glyph arities, sort-glyphs, and refs.
fn visit_node(
    n: &Node,
    sigs: &mut HashMap<u32, GlyphSig>,
    sorts: &mut HashSet<u32>,
    seen: &mut HashSet<u32>,
    env: &SortEnv,
) {
    let mut visit = |e: &Expr| visit_expr(e, sigs, sorts, seen, env);
    match n {
        Node::Axiom(a) => a.statements.iter().for_each(&mut visit),
        Node::Theorem(t) => {
            visit(&t.statement);
            // Proof rows use glyphs the statement may not mention; without their
            // sigs the frame declares such an operator at the wrong arity and rejects
            // its argument.
            for step in &t.proof {
                if let qubitick_model::ProofStep::Chain { rows } = step {
                    for row in rows {
                        if let Some(l) = &row.lhs {
                            visit(l);
                        }
                        visit(&row.rhs);
                    }
                }
            }
        }
        Node::Definition(d) => {
            if let Some(FormBody::MacroExpr(e)) = &d.d_block.form {
                visit(e);
            }
            if let Some(sig) = &d.d_block.sig {
                visit(sig);
            }
        }
        Node::Calibration(_) | Node::Census(_) => {}
    }
}

/// Collect arity/sort/reference info from one `Expr`.
fn visit_expr(
    e: &Expr,
    sigs: &mut HashMap<u32, GlyphSig>,
    sorts: &mut HashSet<u32>,
    seen: &mut HashSet<u32>,
    env: &SortEnv,
) {
    visit_expr_scoped(e, sigs, sorts, seen, env, &HashMap::new());
}

/// As [`visit_expr`], carrying which variables are bound to which DECLARED sort.
///
/// An operator's argument is frequently a plain metavariable — the `a` of `a + c0`
/// is bound by the enclosing `∀ a : C` — so its sort can only be read from the binder
/// scope, not from the argument expression alone.
fn visit_expr_scoped(
    e: &Expr,
    sigs: &mut HashMap<u32, GlyphSig>,
    sorts: &mut HashSet<u32>,
    seen: &mut HashSet<u32>,
    env: &SortEnv,
    vars: &HashMap<String, u32>,
) {
    match e {
        Expr::Sym(ac) => {
            seen.insert(ac.primary);
        }
        Expr::Var(_) => {}
        Expr::Apply(head, args) => {
            if head.primary != EQ_SYM {
                seen.insert(head.primary);
                let arg_carrier: Vec<Option<u32>> = args
                    .iter()
                    .map(|a| match a {
                        Expr::Var(v) => vars.get(v).copied(),
                        // A LAMBDA (function) binder over a declared sort passes a
                        // `C → Prop` body. An `Exists (fun c : C => ...)` is itself a
                        // PROP, not a sort-valued function, so it must NOT type its
                        // operand.
                        Expr::Binder(qubitick_model::BinderKind::Lambda, bs, _) => bs
                            .iter()
                            .filter_map(|b| head_id(&b.ty))
                            .find_map(|id| env.carrier_of_head(id)),
                        other => head_id(other).and_then(|id| env.carrier_of_head(id)),
                    })
                    .collect();
                sigs.entry(head.primary)
                    .or_default()
                    .merge(args, &arg_carrier);
            }
            for a in args {
                visit_expr_scoped(a, sigs, sorts, seen, env, vars);
            }
        }
        Expr::Bracket(id, args) => {
            seen.insert(*id);
            for a in args {
                visit_expr_scoped(a, sigs, sorts, seen, env, vars);
            }
        }
        Expr::Binder(_, binders, body) => {
            // A binder whose type IS a declared sort introduces variables of that
            // sort for the body.
            let mut body_env = vars.clone();
            for b in binders {
                if let Some(carrier) = head_id(&b.ty).and_then(|id| env.carrier_of_head(id)) {
                    for v in &b.vars {
                        body_env.insert(v.clone(), carrier);
                    }
                }
            }
            for b in binders {
                // Binder types are sorts; record their head symbol as a sort.
                mark_sort_of(&b.ty, sorts, seen);
                visit_expr_scoped(&b.ty, sigs, sorts, seen, env, vars);
            }
            visit_expr_scoped(body, sigs, sorts, seen, env, &body_env);
        }
        Expr::Recursor(s, cases) => {
            visit_expr_scoped(s, sigs, sorts, seen, env, vars);
            for (_, b) in cases {
                visit_expr_scoped(b, sigs, sorts, seen, env, vars);
            }
        }
        Expr::Matrix(rows) => {
            for r in rows {
                for c in r {
                    visit_expr_scoped(c, sigs, sorts, seen, env, vars);
                }
            }
        }
        Expr::Vector(els) | Expr::Set(els) => {
            for a in els {
                visit_expr_scoped(a, sigs, sorts, seen, env, vars);
            }
        }
    }
}

/// Record `e`'s head symbol as a sort (it is used as a binder type).
fn mark_sort_of(e: &Expr, sorts: &mut HashSet<u32>, seen: &mut HashSet<u32>) {
    let Some(id) = head_id(e) else { return };
    seen.insert(id);
    sorts.insert(id);
}

/// One axiom's place in the linear class chain (Document 5 §6.2).
struct ChainEntry {
    /// The axiom node's id (used to resolve citations and section variables).
    id: u32,
    name: String,
    /// Cumulative number of statements across all earlier axioms (offset).
    offset: usize,
    /// The class this axiom's class extends; `None` for the root (→ `ISA`).
    parent: Option<String>,
}

/// Compute the linear axiom class chain by cumulative statement offsets.
fn build_chain(axioms: &[&Axiom]) -> Vec<ChainEntry> {
    let mut offset = 0usize;
    let mut prev: Option<String> = None;
    axioms
        .iter()
        .map(|a| {
            let entry = ChainEntry {
                id: a.base.id,
                name: a.base.name.clone(),
                offset,
                parent: prev.clone(),
            };
            offset += a.statements.len();
            prev = Some(a.base.name.clone());
            entry
        })
        .collect()
}

/// Emit a full `.lean` module text for a set of nodes.
pub fn emit_lean_text(
    ctx: &Ctx,
    nodes: &[Node],
    namespace: &str,
    rules: &mut RuleTable,
) -> Result<String> {
    emit_lean_text_indexed(
        ctx,
        nodes,
        namespace,
        &HashMap::new(),
        &HashMap::new(),
        rules,
    )
}

/// As [`emit_lean_text`], but for a module that must know about nodes it does NOT
/// declare.
///
/// A theorem's file holds one node, yet its module has to resolve the glyphs of
/// the corpus axioms and definitions it cites — those arrive as `context`. They
/// contribute symbol declarations, the `RuleTable` rewrite dictionary, and the
/// observed arities, but must never be *emitted*: the single frame already
/// declares everything, and re-emitting them collides with it
/// (`Qubitick.HL.Realm has already been declared`). Only `own` is emitted.
pub fn emit_lean_text_split(
    ctx: &Ctx,
    context: &[Node],
    own: &[Node],
    namespace: &str,
    def_index: &HashMap<u32, u32>,
    thm_index: &HashMap<u32, (String, qubitick_model::Expr)>,
    rules: &mut RuleTable,
) -> Result<String> {
    emit_lean_text_split_with(
        ctx,
        context,
        own,
        namespace,
        def_index,
        thm_index,
        rules,
        &Options::inline(),
    )
}

/// As [`emit_lean_text_split`], but with the caller's options.
///
/// The build path needs this one: `Options::home_modules` is how a module names the
/// modules it must import, and a caller that builds its own default options silently
/// loses that map — every cross-module glyph then reaches the emitter with no home.
#[allow(clippy::too_many_arguments)]
pub fn emit_lean_text_split_with(
    ctx: &Ctx,
    context: &[Node],
    own: &[Node],
    namespace: &str,
    def_index: &HashMap<u32, u32>,
    thm_index: &HashMap<u32, (String, qubitick_model::Expr)>,
    rules: &mut RuleTable,
    opts: &Options,
) -> Result<String> {
    let mut all: Vec<Node> = Vec::with_capacity(context.len() + own.len());
    all.extend_from_slice(context);
    all.extend_from_slice(own);
    emit_lean_text_impl(ctx, &all, own, namespace, def_index, thm_index, rules, opts)
}

/// As [`emit_lean_text`], but told each definition symbol's corpus division index
/// so a proof citation `[D.BT.<n>]` can resolve to that definition's unfolding.
pub fn emit_lean_text_indexed(
    ctx: &Ctx,
    nodes: &[Node],
    namespace: &str,
    def_index: &HashMap<u32, u32>,
    thm_index: &HashMap<u32, (String, qubitick_model::Expr)>,
    rules: &mut RuleTable,
) -> Result<String> {
    emit_lean_text_impl(ctx, nodes, nodes, namespace, def_index, thm_index, rules, &Options::inline())
}

/// A whole era as one self-contained module: it declares its own ground and its own
/// theorems, which is what makes the era the composable unit — its vocabulary and its
/// axiom class chain are its own, so nothing has to agree with another module.
pub fn emit_lean_text_era(
    ctx: &Ctx,
    nodes: &[Node],
    namespace: &str,
    def_index: &HashMap<u32, u32>,
    thm_index: &HashMap<u32, (String, qubitick_model::Expr)>,
    rules: &mut RuleTable,
    extra_imports: Vec<String>,
) -> Result<String> {
    let opts = Options {
        // The cited theorems are declared here (same scope, earlier in the file) or
        // imported from an earlier scope — never assumed as axioms.
        declare_cited: false,
        extra_imports,
        ..Options::inline()
    };
    emit_lean_text_impl(ctx, nodes, nodes, namespace, def_index, thm_index, rules, &opts)
}

impl Options {
    /// A self-contained module: it declares its own ground (the per-file shape).
    pub fn inline() -> Self {
        Options {
            ground: Ground::Inline,
            ground_module: String::new(),
            declare_cited: true,
            extra_imports: Vec::new(),
            home_modules: std::collections::HashMap::new(),
            ground_chain: false,
            ground_only: false,
        }
    }
}

/// Whether a module declares the shared ground itself or imports it.
///
/// In a corpus checked one file at a time, every module re-declares the symbol
/// frame, the axiom class chain and the theorems it cites (as assumed lemmas),
/// because nothing else is in its environment. Put those modules in ONE environment
/// — a Lake package — and those declarations collide ("environment already contains
/// 'Qubitick.HL.mathbb_R'"). `External` is the fix: the ground is one generated
/// module, and a node module declares only its own content, importing the ground
/// and the modules of the theorems it cites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Ground {
    /// Declare the frame, the class chain and the cited theorems here (per-file).
    #[default]
    Inline,
    /// Import them instead (meshed package).
    External,
}

/// Emission options for the meshed shape.
#[derive(Debug, Clone)]
pub struct Options {
    pub ground: Ground,
    /// The module that declares the shared ground this one imports. A corpus is
    /// built per ERA, because an era's vocabulary and its axiom class chain are its
    /// own — two eras both declaring `Realm` collide in one environment.
    pub ground_module: String,
    /// Modules this one must import beyond the ground (its cited theorems and the
    /// modules that define the glyphs it uses).
    pub extra_imports: Vec<String>,
    /// Node id -> the Lean module that node's file becomes, for the WHOLE corpus.
    ///
    /// A module that names a glyph another module defines imports that module; this is
    /// the map that turns a glyph's home node into a module name. Filled by the caller
    /// that has the corpus on disk (`crates/build`); empty for a caller that emits a
    /// single file with no corpus, which is why a glyph with no entry is an error rather
    /// than a silent `axiom`.
    pub home_modules: std::collections::HashMap<i64, String>,
    /// Declare the theorems this module cites as assumed lemmas.
    ///
    /// True for a per-file module, which cannot see the theorem it cites. False when
    /// the cited theorems are declared IN this module already — an era emitted as one
    /// module — where a second declaration would collide. The rewrite rules are
    /// registered either way.
    pub declare_cited: bool,
    /// The module imports a ground that declares an axiom class chain, so a
    /// theorem's `section variable [R : X]` names a class that exists. `false` for a
    /// module that declares its own (inline) or whose era has no axioms at all.
    pub ground_chain: bool,
    /// Emit a shared GROUND: the frame, its definitions, and the axiom chain —
    /// nothing that states a theorem. `emit_nodes` is the definitions (the frame
    /// must know every symbol they declare, and must not declare those itself);
    /// a node module instead gets the chain before its theorems and takes its
    /// definitions from the ground it imports.
    pub ground_only: bool,
}

/// The emission core: `nodes` is everything the module may refer to, `emit_nodes`
/// the subset it declares.
#[allow(clippy::too_many_arguments)]
fn emit_lean_text_impl(
    ctx: &Ctx,
    nodes: &[Node],
    emit_nodes: &[Node],
    namespace: &str,
    def_index: &HashMap<u32, u32>,
    thm_index: &HashMap<u32, (String, qubitick_model::Expr)>,
    rules: &mut RuleTable,
    opts: &Options,
) -> Result<String> {
    let target = LeanTarget::new(namespace);
    // A glyph used BOTH bare and applied (e.g. `\mathfrak{L}` vs `\mathfrak{L}(t)`)
    // gets a deterministic lean-side split: the APPLIED use is encoded as
    // `{base}_fn : Prop → … → Prop` (the Ledger function), the bare use stays
    // `{base} : Prop` (the bare ledger). This keeps statement/axiom structure
    // (no argument erasure) while giving Lean unambiguous names (directive).
    let mut dual = dual_use_symbols(nodes);
    exclude_ctor_dual(&mut dual, ctx);
    // iff-defined relational glyphs (`a ⊆ b ⟷ body`) whose applications in a
    // theorem statement we unfold to the body, so the equality-chain proof terms
    // they generate typecheck against an otherwise-opaque `axiom` head.
    let mut fn_names: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
    let relational_split_map: std::collections::HashMap<u32, (Vec<String>, Expr)> = {
        use qubitick_model::Expr as E2;
        let is_iff = |h: &qubitick_model::AtomicComposite| {
            ctx.by_id(h.primary as i64)
                .is_some_and(|r| qubitick_model::format::is_iff(&r.tex))
        };
        let mut m = std::collections::HashMap::new();
        for n in nodes {
            if let Node::Definition(d) = n
                && let Some(qubitick_model::FormBody::MacroExpr(e)) = &d.d_block.form
                && let E2::Apply(h, args) = e
                && is_iff(h)
                && args.len() == 2
            {
                let mut params = Vec::new();
                qubitick_pattern::expr_idents(&args[0], &mut params);
                m.insert(d.symbol.primary, (params, args[1].clone()));
            }
        }
        m
    };
    let owned: Vec<Node> = nodes
        .iter()
        .cloned()
        .map(|mut n| {
            split_node(&mut n, &dual, ctx, &mut fn_names, Some(&relational_split_map));
            n
        })
        .collect();
    let nodes: &[Node] = &owned;
    // Iff-map built AFTER the split, over the SPLIT nodes: the relational
    // statement-unfolding must produce fn-ids, or the unfolded statement
    // re-applies the base glyph (CoeFun in a bare prelude).
    let relational_iffs: std::collections::HashMap<u32, (Vec<String>, Expr)> = {
        use qubitick_model::Expr as E;
        let is_iff = |h: &qubitick_model::AtomicComposite| {
            ctx.by_id(h.primary as i64)
                .is_some_and(|r| qubitick_model::format::is_iff(&r.tex))
        };
        let mut m = std::collections::HashMap::new();
        for n in nodes {
            if let Node::Definition(d) = n
                && let Some(qubitick_model::FormBody::MacroExpr(e)) = &d.d_block.form
                && let E::Apply(h, args) = e
                && is_iff(h)
                && args.len() == 2
            {
                let mut params = Vec::new();
                qubitick_pattern::expr_idents(&args[0], &mut params);
                m.insert(d.symbol.primary, (params, args[1].clone()));
            }
        }
        m
    };
    // Only the nodes this module DECLARES are emitted, and they must come from
    // split copies too: `emit_nodes` still holds the caller's un-split forms.
    // Emitting those left the frame declaring `X_fn : Prop → Prop` while the
    // statements still said `X`, which Lean reports on the first applied
    // occurrence as `error(lean.unknownIdentifier): Unknown constant 'CoeFun'`
    // (`CoeFun` is what application elaboration reaches for when a `Prop` head is
    // applied to an argument).
    let emit_owned: Vec<Node> = emit_nodes
        .iter()
        .cloned()
        .map(|mut n| {
            split_node(&mut n, &dual, ctx, &mut fn_names, Some(&relational_split_map));
            n
        })
        .collect();
    let emit_nodes: &[Node] = &emit_owned;

    // Map each formation's constructor symbol → (carrier lean name, ordered ctor
    // ids) so operators can be emitted as recursors over a custom formation.
    let formations: std::collections::HashMap<u32, (String, Vec<u32>)> = {
        let mut m = std::collections::HashMap::new();
        for n in nodes {
            if let Node::Definition(d) = n
                && let Some(qubitick_model::FormBody::Formation(ctors)) = &d.d_block.form
            {
                let name = crate::lean_name_of(ctx, d.symbol.primary);
                let ids: Vec<u32> = ctors.iter().map(|c| c.constructor.primary).collect();
                for &id in &ids {
                    m.entry(id).or_insert_with(|| (name.clone(), ids.clone()));
                }
            }
        }
        m
    };

    // Collect axioms (in corpus order) to build the class chain.
    let axioms: Vec<&Axiom> = nodes
        .iter()
        .filter_map(|n| match n {
            Node::Axiom(a) => Some(a),
            _ => None,
        })
        .collect();
    let chain = build_chain(&axioms);
    // The class's fields in order, as the emitted `class` carries them: a theorem's
    // proof may cite `R.ax_<k>` by number (the nullity bridge does), and that number
    // is the position here.
    let class_fields: Vec<Expr> = chain
        .iter()
        .filter_map(|entry| chain_statements(&axioms, entry))
        .flatten()
        .map(|s| reduce_field_statement(&s))
        .collect();

    /* CAN THIS MODULE NAME A CLASS? The chain is the GROUND's now: a module imports the
       ground, so a corpus with any A block has one, and the module hypothesizes the
       instance without declaring the class. The old question — does this module declare
       the class itself? — is gone with the per-module chain. */
    let chain_declared = if opts.ground == Ground::Inline {
        opts.ground_only || chain.iter().any(|c| c.parent.is_none())
    } else {
        opts.ground_chain
    };

    for entry in &chain {
        let Some(Node::Axiom(a)) = nodes
            .iter()
            .find(|n| matches!(n, Node::Axiom(x) if x.base.id == entry.id))
        else {
            continue;
        };
        for (k, stmt) in a.statements.iter().enumerate() {
            rules.insert(entry.id, (k + 1) as u32, entry.offset + k, stmt.clone());
        }
    }
    // Definitions are citable too: a step that IS the unfolding is closed by
    // kernel conversion (`Eq.refl`), so register each definition's Sema under its
    // corpus division index.
    for node in nodes {
        if let Node::Definition(d) = node
            && let Some(division) = def_index.get(&d.symbol.primary)
            && let Some(FormBody::MacroExpr(sema)) = &d.d_block.form
        {
            rules.insert_definition(*division, sema.clone());
        }
    }
    // Cited theorems are declared as assumed lemmas (they are proven in their
    // own files) and registered so a `[T.BT.<n>]` citation applies them.
    let mut thm_decls = String::new();
    let mut collect_extra: Vec<Node> = Vec::new();
    for (division, (name, stmt)) in thm_index {
        let mut s = stmt.clone();
        split_expr(&mut s, &dual, ctx, &mut fn_names);
        // A cited theorem whose file leaves its parameters FREE (no \forall in
        // the corpus text, e.g. T_BT_61) must still be declared with explicit
        // binders — proofs apply the lemma as a function, and a bare-equation
        // axiom with generalized free vars cannot be instantiated. Infer each
        // free ident's sort from the operator that applies it (name-based family
        // rule, like the recursor typing).
        let mut free_vars: Vec<String> = Vec::new();
        {
            let mut ids = Vec::new();
            qubitick_pattern::expr_idents(&s, &mut ids);
            for v in ids {
                if !free_vars.contains(&v) {
                    free_vars.push(v);
                }
            }
        }
let mut binder_prefix = String::new();
        if !free_vars.is_empty() {
            // The sort of a free variable is the declared sort whose constructor
            // takes it, and the `Prop` carrier otherwise. The frame types the same
            // glyphs the same way, so the assumed lemma agrees with it.
            let sort_env = SortEnv::of(ctx, nodes);
            let sort_of = |v: &str| -> String {
                use qubitick_model::Expr as E;
                fn walk(e: &E, v: &str) -> Option<u32> {
                    match e {
                        E::Apply(h, args) => {
                            if args.iter().any(|a| matches!(a, E::Var(x) if x == v)) {
                                return Some(h.primary);
                            }
                            args.iter().find_map(|a| walk(a, v))
                        }
                        E::Binder(_, bs, b) => {
                            if bs.iter().any(|bd| bd.vars.iter().any(|x| x == v)) {
                                None
                            } else {
                                walk(b, v)
                            }
                        }
                        E::Bracket(_, els) => els.iter().find_map(|a| walk(a, v)),
                        E::Recursor(sc, cases) => walk(sc, v)
                            .or_else(|| cases.iter().find_map(|(_, b)| walk(b, v))),
                        E::Matrix(rows) => {
                            rows.iter().find_map(|r| r.iter().find_map(|x| walk(x, v)))
                        }
                        E::Vector(elems) | E::Set(elems) => elems.iter().find_map(|x| walk(x, v)),
                        E::Sym(_) | E::Var(_) => None,
                    }
                }
                walk(&s, v)
                    .and_then(|id| sort_env.carrier_of_head(id))
                    .map(|c| crate::resolver(ctx, &fn_names)(c))
                    .unwrap_or_else(|| "Prop".into())
            };
            for v in &free_vars {
                let sort = sort_of(v);
                binder_prefix.push_str(&format!("∀ ({v} : {sort}), "));
            }
        }
        let lean_stmt = qubitick_lean::render(&s, &crate::resolver(ctx, &fn_names));
        let lemma = lemma_name(name);
        thm_decls.push_str(&format!(
            "axiom {lemma} : {binder_prefix}{lean_stmt}\n"
        ));
        rules.insert_theorem(*division, lemma, s.clone());
        // Collect the lemma's symbols into the frame so any fn-variant it uses
        // is declared.
        collect_extra.push(Node::Axiom(Axiom {
            base: NodeBase {
                id: 0,
                kind: qubitick_model::NodeKind::Axiom,
                era: qubitick_model::Era::Era0,
                strata: qubitick_model::Strata(0),
                name: "__thm_sym_collect".into(),
                req: vec![],
                derived: vec![],
                note: None,
            },
            statements: vec![s],
            tag: qubitick_model::PhysOrMath::Math,
            falsify: None,
        }));
    }
    let mut collect: Vec<Node> = nodes.to_vec();
    collect.extend(collect_extra);
    let inline = opts.ground == Ground::Inline;
    let Frame {
        text: declarations,
        char_axioms,
        sorts: frame_sorts,
        arity: frame_arity,
        declared: frame_declared,
        types: known_types,
        needed: frame_needed,
    } = symbol_declarations(ctx, &collect, emit_nodes, &fn_names, opts);
    /* THE MODULES THIS ONE MUST IMPORT, from the glyphs it names and does not own.
     *
     * Each is a definition that lives in another module. `opts.home_modules` is the
     * corpus's own map (node id -> Lean module name), assembled by the caller that has
     * the corpus on disk; a glyph whose home is not in the map reached the emitter
     * without a home at all, which is a build error, not an axiom. */
    /* AN A BLOCK EMITS NOTHING.
     *
     * Its module is vocabulary and composition — the grammar of the display — and a
     * grammar is not a set of claims. So the frame is not written here either: the
     * only declarations a node module owns are its own (`def` from a `Sema:`,
     * `theorem` from a `T` block), and an A block owns neither. This is what makes
     * "the emitted Lean contains no declaration from an A block" checkable. */
    let a_block_only = emit_nodes.iter().all(|n| {
        matches!(
            n,
            Node::Axiom(_) | Node::Calibration(_) | Node::Census(_)
        )
    });
    let mut needed_imports: Vec<String> = Vec::new();
    for (lean, home) in &frame_needed {
        if a_block_only {
            // An A block emits nothing at all, so it imports nothing either: the frame
            // it would have needed is exactly what it does not write.
            break;
        }
        match opts.home_modules.get(home) {
            Some(m) => {
                if !needed_imports.contains(m) {
                    needed_imports.push(m.clone());
                }
            }
            None => {
                return Err(anyhow::anyhow!(
                    "`Type:` requires a ground name (e.g. `Type: ISA.Tick : \\mathsf{{Type}}`)\n  \
                     or a `Sema:` body. A glyph with no home cannot be emitted.\n  \
                     (`{lean}` is named in this module and nothing declares it.)"
                ));
            }
        }
    }
    let mut body = String::new();
    if inline && !a_block_only {
        body.push_str(&declarations);
        // Assumed lemmas exist only because a per-file module cannot see the theorem
        // it cites. Where the theorems are declared in this module already — an era
        // emitted whole — a second declaration would collide, so they are skipped
        // while their rewrite rules stay registered.
        if opts.declare_cited {
            body.push_str(&thm_decls);
        }
        if !opts.ground_only {
            body.push_str(&char_axioms);
        }
    }
    body.push('\n');
    // The chain is a string, not a push: a GROUND declares its definitions before
    // it, because the axiom statements are written in the corpus's own vocabulary
    // (`ax_9` mentions the defined `\leftrightarrow`, `ax_17` the defined
    // `\subseteq`) and Lean wants a declaration before its use.
    let mut chain_body = String::new();
    if inline && chain.iter().any(|c| c.parent.is_none()) {
        /* THE ROOT CLASS CARRIES THE SYMBOLS THE CORPUS ONLY NAMES.
         *
         * A D block that says `Type: R.plus` names a field of the instance the proofs are
         * parametric over; the class that DECLARES that field is this one. The engine does
         * not decide the field's meaning — it writes the type the corpus's own `Type:`
         * signature gave it, and the user's ground may say anything else. */
        chain_body.push_str("class ISA where\n-- the symbols the corpus names but does not define (Type: R.<field>)\n");
        let mut fields: Vec<String> = Vec::new();
        let mut bound: Vec<&qubitick_db::queries::SymbolRow> = ctx
            .symbols()
            .filter(|r| r.kind == "bound")
            .filter(|r| r.lean_name.starts_with("R."))
            .collect();
        bound.sort_by_key(|r| {
            let ty = known_types
                .get(&r.lean_name)
                .or_else(|| known_types.get(r.lean_name.trim_start_matches("R.")))
                .cloned()
                .unwrap_or_default();
            (usize::from(ty != "Type"), r.id)
        });
        for row in bound {
            let field = row.lean_name.trim_start_matches("R.").to_string();
            if fields.contains(&field) {
                continue;
            }
            let ty = known_types
                .get(&row.lean_name)
                .or_else(|| known_types.get(&field))
                .cloned()
                .unwrap_or_else(|| "Prop".to_string());
            fields.push(field.clone());
            chain_body.push_str(&format!("  {field} : {ty}\n"));
        }
        chain_body.push('\n');
        // The class chain is emitted from `chain`, NOT from the node loop.
        //
        // A theorem's file holds ONE node: its axioms arrive as CONTEXT (so the frame
        // can resolve their glyphs) and are not in `emit_nodes`, so the node loop
        // emitted `class ISA` and nothing else. The theorem then sat inside
        // `section variable {R : Realm}` with `Realm` declared NOWHERE, and every
        // `R.ax_<k>` proof reference was `Unknown identifier \`R.ax_0\``.
        for entry in &chain {
            let Some(stmts) = chain_statements(&axioms, entry) else {
                continue;
            };
            chain_body.push_str(&emit_axiom_class(
                ctx,
                &entry.name,
                &stmts,
                entry.parent.as_deref(),
                entry.offset,
                &fn_names,
                &frame_sorts,
            ));
        }
    }
    /* THE CLASS CHAIN IS THE GROUND'S, NOT A NODE MODULE'S.
     *
     * `class ISA where ...` and one class per A block declare the ASSUMPTIONS the
     * corpus is written against, and they belong in the one file the user owns — not
     * re-emitted into every module that happens to name them. A node module imports
     * the ground and hypothesizes the instance (`section variable [R : Comm]`); the
     * classes are the ground's. Only a ground writes them, below, AFTER its own
     * definitions (a class field is written in the corpus's vocabulary, so a field
     * that mentions a defined glyph needs that glyph declared first). */

    for node in emit_nodes.iter() {
        match node {
            Node::Theorem(t) => {
                // NB: the theorem's RENDERED statement must not be fed to the
                // import assembler — it is an expression, not a module name, and
                // doing so emitted `import Qubitick.∀ ...` (a hard parse error).
                let class = section_class(&chain, t.base.req.as_slice());
                let (text, emitted) = crate::theorem::emit_theorem(
                    ctx,
                    t,
                    namespace,
                    &class,
                    rules,
                    &fn_names,
                    &frame_sorts,
                    &relational_iffs,
                    &class_fields,
                    // A class the module can actually see: its OWN chain when it
                    // declares one (an era whose axioms live in an earlier era has
                    // no chain and declares no class), or the ground it imports.
                    if inline {
                        chain.iter().any(|c| c.parent.is_none())
                    } else {
                        opts.ground_chain
                    },
                )?;
                // Re-register this theorem's rule with the statement it was EMITTED
                // with, so a citation applied inside this module has the def's type.
                let lemma = crate::lemma_name(&t.base.name);
                if let Some((division, _)) = thm_index.iter().find(|(_, (n, _))| n == &lemma) {
                    rules.insert_theorem(*division, lemma, emitted);
                }
                body.push_str(&text);
                body.push('\n');
            }
            Node::Definition(d) => {
                // A postulate about operands declares nothing (see
                // [`defines_its_glyph`]): emitting `def cap : Prop := varnothing` for
                // `D.BT.43 FrameLeg1` claimed `cap` for a definition that is not one,
                // and in a shared ground that evicted `cap` from the frame every
                // class axiom is written in.
                if !defines_its_glyph(ctx, d) {
                    continue;
                }
                // The frame declares a formation's carrier and its constructors
                // itself. A definition node that names one of those glyphs would
                // declare it a second time.
                if frame_declared.contains(&definition_decl_name(ctx, d, &fn_names)) {
                    continue;
                }
                /* A DEFINITION IS WRITTEN IN THE CORPUS'S VOCABULARY, so a body that
                   names a ground symbol reached through the instance (`R.plus`) needs
                   the instance hypothesized — the same `section variable [R : class]` a
                   theorem gets. An unused hypothesis binds nothing, so wrapping a
                   definition that does not need it is free. */
                let def_text = crate::definition::emit_definition_in(
                    ctx,
                    d,
                    frame_sorts.contains(&d.symbol.primary),
                    frame_arity.get(&d.symbol.primary).copied().unwrap_or(0),
                    &fn_names,
                    &frame_sorts,
                    &formations,
                    &known_types,
                    opts.ground_only,
                )?;
                /* A GROUND does not hypothesize its own chain: it DECLARES it, below,
                   after its definitions (a field is written in the corpus's vocabulary,
                   so a field that mentions a defined glyph needs that glyph first). */
                if chain_declared && !opts.ground_only {
                    let class = section_class(&chain, &[]);
                    body.push_str(&format!(
                        "section\nvariable [R : {class}]\n\n{def_text}end\n"
                    ));
                } else {
                    body.push_str(&def_text);
                }
                body.push('\n');
            }
            // Classes are the ground's; an A block contributes nothing here.
            Node::Axiom(_) => {}
            Node::Calibration(_) | Node::Census(_) => {
                body.push_str(&format!(
                    "-- {} (Calibration/Census)\n\n",
                    node_header_name(node)
                ));
            }
        }
    }

    if opts.ground_only {
        // A ground's definitions came first; the chain that MENTIONS them follows them,
        // and so do their characterising axioms, which name them.
        body.push_str(&chain_body);
        body.push_str(&char_axioms);
    }

    let mut imports = assemble_imports(&target);
    // One import per module: the ground is named here AND may arrive in the
    // caller's list (the scope module's own imports include it).
    let mut seen_imports: std::collections::HashSet<String> = imports
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    let mut more: Vec<String> = Vec::new();
    if !inline {
        more.push(opts.ground_module.clone());
    }
    more.extend(opts.extra_imports.iter().cloned());
    more.extend(needed_imports.iter().cloned());
    for m in more {
        if m.is_empty() || !seen_imports.insert(m.clone()) {
            continue;
        }
        imports.push_str(&format!("\nimport {m}"));
    }
    // Every emitted module declares the two options its honesty depends on. This is
    // the end of the one funnel all four shapes pass through (per-file, split, era,
    // ground, meshed), so a module cannot be emitted without them.
    //
    // `autoImplicit false`: with Lean's default ON, an unresolved lowercase
    // identifier becomes a fresh implicit variable — so a module that never declares a
    // carrier still checks, with every mention of it a *different* variable and the
    // corpus's sorts hypothetical. Turning it off is what makes the per-file check a
    // check of the frame rather than of a guess.
    //
    // `linter.defProp false`: a corpus theorem is emitted as `def N : Prop := ...`
    // (the nullity encoding is a proof, not a type). That is deliberate, and the
    // linter would otherwise flag all 172 of them.
    /* THE NODE'S PROSE, AS A MODULE DOC COMMENT. One `.qbit` emits one `.lean`, so the node's own
       `Math:`/`Phys:` reading belongs at the top of the file it is about — where a reader (and
       doc-gen) looks for it — rather than beside a declaration somewhere below. It is prose: it is
       never emitted as anything Lean parses. */
    let docs = module_docs(emit_nodes);
    Ok(format!(
        "prelude\nimport {}\n\nset_option autoImplicit false\nset_option linter.defProp false\n\n{}namespace {}\n\n{}\nend {}\n",
        imports, docs, namespace, body, namespace
    ))
}

/// The module's doc comment: what each node of this file SAYS about itself.
///
/// A node's prose is authored text (`Math:` is its mathematical reading, `Phys:` its physical one),
/// kept on the node (crates/model/src/node.rs::NodeBase::note) since the parser stopped throwing it
/// away. Empty when no node here carries prose — a module with nothing to say gets no block.
fn module_docs(nodes: &[Node]) -> String {
    let mut out = String::new();
    for n in nodes {
        let Some(note) = n.base().note.as_deref().map(str::trim).filter(|t| !t.is_empty()) else {
            continue;
        };
        if out.is_empty() {
            out.push_str("/-!\n");
        } else {
            out.push('\n');
        }
        /* `# <label> <name>` then the reading, labelled the way the author labelled it. `-/` inside
           prose would end the comment early, so it is broken apart — the text is the author's and
           must survive verbatim otherwise. */
        let label = match prose_kind(n) {
            qubitick_model::PhysOrMath::Phys => "Phys",
            qubitick_model::PhysOrMath::Math => "Math",
        };
        /* THE NODE'S OWN NAME, not its corpus label: `T.BT.01` is formed from the file's header
           line, which this crate never sees (crates/build/src/cite_target.rs does, for the PDF). */
        out.push_str(&format!(
            "# {}\n\n{}: {}\n",
            n.base().name,
            label,
            note.replace("-/", "- /")
        ));
    }
    if out.is_empty() {
        return out;
    }
    out.push_str("-/\n\n");
    out
}

/// Which reading a node's prose is. `Calibration` has no prose tag of its own (its fields are its
/// reading), so a note there is labelled with the default.
fn prose_kind(n: &Node) -> qubitick_model::PhysOrMath {
    match n {
        Node::Axiom(a) => a.tag,
        Node::Theorem(t) => t.tag,
        Node::Definition(d) => d.tag,
        Node::Census(c) => c.tag,
        _ => qubitick_model::PhysOrMath::Math,
    }
}

/// Order definitions so that one whose body MENTIONS another comes after it.
///
/// A corpus file holds one node, so nothing ever had to order two definitions
/// against each other. A ground holds every definition of an era at once, and
/// Lean wants the declaration before its use: without this, `def Gamma : Prop :=
/// mathbb_E` came before `mathbb_E` and the ground failed with `Unknown
/// identifier`.
///
/// A cycle here is a build error elsewhere (`definition_cycles`), so a node whose
/// dependencies cannot be emitted first is emitted where it stands, not dropped.
fn dependency_order(definitions: Vec<Node>) -> Vec<Node> {
    fn body_of(n: &Node) -> Option<&Expr> {
        match n {
            Node::Definition(d) => match d.d_block.form.as_ref() {
                Some(FormBody::MacroExpr(e)) => Some(e),
                _ => None,
            },
            _ => None,
        }
    }
    let defined: std::collections::HashMap<u32, usize> = definitions
        .iter()
        .enumerate()
        .filter_map(|(i, n)| match n {
            Node::Definition(d) => Some((d.symbol.primary, i)),
            _ => None,
        })
        .collect();
    let mut out: Vec<Node> = Vec::with_capacity(definitions.len());
    let mut state: Vec<u8> = vec![0; definitions.len()]; // 0 new, 1 open, 2 done
    for i in 0..definitions.len() {
        if state[i] != 0 {
            continue;
        }
        // Iterative DFS: `stack` holds (node, dependencies emitted?) pairs, so a
        // deep definition graph cannot overflow the stack.
        let mut stack: Vec<(usize, bool)> = vec![(i, false)];
        while let Some((k, deps_done)) = stack.pop() {
            if deps_done {
                if state[k] == 1 {
                    state[k] = 2;
                    out.push(definitions[k].clone());
                }
                continue;
            }
            if state[k] != 0 {
                continue;
            }
            state[k] = 1;
            stack.push((k, true));
            if let Some(e) = body_of(&definitions[k]) {
                let mut ids: std::collections::HashSet<u32> = std::collections::HashSet::new();
                collect_plain_ids(e, &mut ids);
                let mut ids: Vec<u32> = ids.into_iter().collect();
                ids.sort_unstable();
                for id in ids {
                    if let Some(&j) = defined.get(&id)
                        && j != k
                        && state[j] == 0
                    {
                        stack.push((j, false));
                    }
                }
            }
        }
    }
    out
}

/// The shared ground of a corpus: the symbol frame and the axiom class chain,
/// declared once for every node module to import.
pub fn emit_ground(
    ctx: &Ctx,
    all_nodes: &[Node],
    namespace: &str,
) -> Result<String> {
    // A ground declares the frame, its DEFINITIONS, and the axiom chain, in that
    // order. The definitions belong here and not in a node module because the chain
    // is written in the corpus's own vocabulary — `ax_9` mentions the defined
    // `\leftrightarrow`, `ax_17` the defined `\subseteq` — and a module that
    // imports the ground could not also declare those glyphs. The theorems stay in
    // the node modules, which is what makes a citation an `import` of the module
    // that proved it.
    // ONE declaration per glyph. Two scopes can resolve the same symbol — a later
    // scope's context sees the earlier scope's rows — so an era's definitions can
    // hold two nodes claiming one name, and the ground would declare it twice
    // (`Qubitick.HL.mathcal_R has already been declared`). The DB loader drops the
    // later claimant the same way; the first node stands.
    let mut seen_names: HashSet<String> = HashSet::new();
    let definitions: Vec<Node> = dependency_order(
        all_nodes
            .iter()
            .filter(|n| matches!(n, Node::Definition(_)))
            .filter(|n| match n {
                Node::Definition(d) => {
                    seen_names.insert(crate::lean_name_of(ctx, d.symbol.primary))
                }
                _ => false,
            })
            .cloned()
            .collect(),
    );
    let mut rules = RuleTable::new();
    let opts = Options {
        ground: Ground::Inline,
        ground_module: String::new(),
        declare_cited: false,
        extra_imports: Vec::new(),
        home_modules: std::collections::HashMap::new(),
        ground_chain: false,
        ground_only: true,
    };
    emit_lean_text_impl(
        ctx,
        all_nodes,
        &definitions,
        namespace,
        &HashMap::new(),
        &HashMap::new(),
        &mut rules,
        &opts,
    )
}

/// THE CHAIN ALONE — what `qubitick scaffold` writes into a user's ground.
///
/// A ground supplies what the corpus only NAMES: the root class carrying those symbols,
/// and one class per A block carrying that block's statements as fields. It supplies no
/// definition — a `Sema:` is the corpus's, and its module writes it — so there is no
/// ordering problem between a definition and the class its body is written over.
pub fn emit_ground_chain(
    ctx: &Ctx,
    nodes: &[Node],
    namespace: &str,
) -> Result<String> {
    let opts = Options {
        ground: Ground::Inline,
        ground_module: String::new(),
        declare_cited: false,
        extra_imports: Vec::new(),
        home_modules: std::collections::HashMap::new(),
        ground_chain: false,
        ground_only: true,
    };
    let fn_names = HashMap::new();
    // The frame is not emitted — only consulted, for the type of every symbol the corpus
    // names. A ground carries no frame: the corpus's own modules do.
    let Frame { sorts, types, .. } = symbol_declarations(ctx, nodes, &[], &fn_names, &opts);
    let axioms: Vec<&Axiom> = nodes
        .iter()
        .filter_map(|n| match n {
            Node::Axiom(a) => Some(a),
            _ => None,
        })
        .collect();
    let chain = build_chain(&axioms);
    let mut out = String::new();
    if !namespace.trim().is_empty() {
        out.push_str(&format!("namespace {namespace}\n\n"));
    }
    /* THE CARRIERS THE CORPUS NAMES ARE THE GROUND'S TO DECLARE.
     *
     * `Type: E` says the glyph IS the ground's `E`. A scaffold that writes the classes and
     * not the carrier produces a ground that does not compile — and one where a bare name
     * in a class field would be `Prop` by accident rather than by anyone's decision. The
     * stub is the smallest model that typechecks; the corpus's carrier is whatever the
     * author replaces it with. */
    let mut carriers: Vec<String> = Vec::new();
    for row in ctx.symbols().filter(|r| r.kind == "bound") {
        // A `R.<field>` name is a field of the instance, not a carrier.
        if row.lean_name.contains('.') || row.lean_name.trim().is_empty() {
            continue;
        }
        if !carriers.contains(&row.lean_name) {
            carriers.push(row.lean_name.clone());
        }
    }
    carriers.sort();
    for name in &carriers {
        out.push_str(&format!(
            "/-- The carrier the corpus names (`Type: {name}`). A `Prop` model is the \
             smallest that typechecks; yours may be anything. -/\ndef {name} : Type := Prop\n\n"
        ));
    }
    out.push_str("class ISA where\n-- the symbols the corpus names but does not define (Type: R.<field>)\n");
    let mut fields: Vec<String> = Vec::new();
    let mut bound: Vec<&qubitick_db::queries::SymbolRow> = ctx
        .symbols()
        .filter(|r| r.kind == "bound")
        .filter(|r| r.lean_name.starts_with("R."))
        .collect();
    // A CARRIER COMES FIRST: a field typed `E → E → E` needs `E` declared above it, so
    // the symbols whose type is a sort lead. Then corpus order.
    bound.sort_by_key(|r| {
        let ty = types
            .get(&r.lean_name)
            .or_else(|| types.get(r.lean_name.trim_start_matches("R.")))
            .cloned()
            .unwrap_or_default();
        (usize::from(ty != "Type"), r.id)
    });
    for row in bound {
        let field = row.lean_name.trim_start_matches("R.").to_string();
        if fields.contains(&field) {
            continue;
        }
        let ty = types
            .get(&row.lean_name)
            .or_else(|| types.get(&field))
            .cloned()
            .unwrap_or_else(|| "Prop".to_string());
        fields.push(field.clone());
        out.push_str(&format!("  {field} : {ty}\n"));
    }
    out.push('\n');
    for entry in &chain {
        let Some(stmts) = chain_statements(&axioms, entry) else {
            continue;
        };
        out.push_str(&emit_axiom_class(
            ctx,
            &entry.name,
            &stmts,
            entry.parent.as_deref(),
            entry.offset,
            &fn_names,
            &sorts,
        ));
    }
    if !namespace.trim().is_empty() {
        out.push_str(&format!("\nend {namespace}\n"));
    }
    Ok(out)
}

/// A node module in the meshed shape: own content only, importing the ground and
/// the modules of the theorems it cites.
///
/// `declare_cited` says whether a theorem this module CITES but does not contain
/// has to be declared here as an assumed lemma. A module that cannot see the
/// cited theorem's own module needs that (the per-file shape); a scope module,
/// which imports the scope that proves it, must not — an axiom and the real `def`
/// of the same name cannot both exist.
#[allow(clippy::too_many_arguments)]
pub fn emit_lean_text_meshed(
    ctx: &Ctx,
    context: &[Node],
    own: &[Node],
    namespace: &str,
    def_index: &HashMap<u32, u32>,
    thm_index: &HashMap<u32, (String, qubitick_model::Expr)>,
    rules: &mut RuleTable,
    ground_module: &str,
    extra_imports: Vec<String>,
    declare_cited: bool,
    ground_chain: bool,
    home_modules: &HashMap<i64, String>,
) -> Result<String> {
    let mut all: Vec<Node> = Vec::with_capacity(context.len() + own.len());
    all.extend_from_slice(context);
    all.extend_from_slice(own);
    // The corpus's node-id -> Lean-module map. `opts.home_modules` is how the emitter
    // resolves a glyph the module names but does not declare (`Frame::needed`).
    //
    // This map CANNOT be rebuilt from the parsed nodes the meshed path holds: a
    // symbol's `home_node` is a DATABASE node id (`times` is 4 on the algebra
    // corpus), while `parse_content` numbers nodes per file (0, 1, 2). So the caller
    // assembles it once from `qubitick_db::read::get_node_list` (node id -> filename)
    // and passes it in. Without an entry, a glyph with no home is an error, not a
    // silent axiom — which is exactly the "emit scope Era0/Strata0" refusal this
    // parameter removes for the meshed shape.
    let opts = Options {
        ground: Ground::External,
        ground_module: ground_module.to_string(),
        declare_cited,
        extra_imports,
        home_modules: home_modules.clone(),
        ground_chain,
        ground_only: false,
    };
    emit_lean_text_impl(ctx, &all, own, namespace, def_index, thm_index, rules, &opts)
}

/// Convenience: emit a module and return the text (alias of `emit_lean_text`).
pub fn emit_module(
    ctx: &Ctx,
    nodes: &[Node],
    namespace: &str,
    rules: &mut RuleTable,
) -> Result<String> {
    emit_lean_text(ctx, nodes, namespace, rules)
}

/// The statements a chain entry's class should declare, if it has any.
///
/// Matched by node id; a context axiom carries its statements, the synthetic node
/// built for symbol collection carries none and is skipped.
fn chain_statements(axioms: &[&Axiom], entry: &ChainEntry) -> Option<Vec<Expr>> {
    axioms
        .iter()
        .find(|a| a.base.id == entry.id && !a.statements.is_empty())
        .map(|a| a.statements.clone())
}

fn node_header_name(node: &Node) -> &str {
    node.base().name.as_str()
}

/// Select the section class variable for a theorem (Document 5 §6.3).
///
/// Picks the axiom in `req` with the largest offset; falls back to `ISA`.
fn section_class(chain: &[ChainEntry], req: &[u32]) -> String {
    // Prefer the deepest class the node actually requires; otherwise the deepest
    // class in the module.
    //
    // The class chain is LINEAR (`ISA <- Realm <- Capacity <- ... extends`), so the
    // deepest class has every earlier class's fields. Falling back to the ROOT
    // (`ISA`) instead left the proofs citing `R.ax_3` on a variable known only as
    // `ISA`, which Lean rejects as `invalid field notation` — `ax_3` is not a field
    // of `ISA`. Falling back to the deepest class is strictly more permissive and
    // resolves every `R.ax_<k>` in the module.
    chain
        .iter()
        .filter(|entry| req.contains(&entry.id))
        .max_by_key(|entry| entry.offset)
        .or_else(|| chain.iter().max_by_key(|entry| entry.offset))
        .map(|entry| entry.name.clone())
        .unwrap_or_else(|| "ISA".to_string())
}

/// Emit an axiom as a `class` declaration (Document 5 §5.1/§5.2).
///
/// The class extends `parent` (or `ISA` for the root) and declares one `ax_<k>`
/// The class extends `parent` (or `ISA` for the root) and declares one
/// `ax_<offset + k>` field per statement — indices are **cumulative across the
/// whole axiom chain** (A1 → ax_0..ax_11, A2 → ax_12..), per Document 5 §5.1/§5.2
/// and §6.1. Statements render as `Prop`-typed fields. No `axiom`, no `sorry`.
///
/// Each field binds ONLY the variables its body actually uses (a `∀ a b c` whose
/// `b`,`c` are absent from the body becomes `∀ a`). This is load-bearing: proofs
/// cite a field with the rewrite engine's `law_application`, which renders the
/// variables occurring in the law body (`expr_idents`). If the field declared the
/// full `∀ a b c`, every such citation would under-apply (`R.ax_3 a` of a 3-arg
/// field → `Application type mismatch`).
fn reduce_field_statement(stmt: &Expr) -> Expr {
    // Collect the leading `∀` binders (var, ty) in order, leaving the body.
    let mut vars: Vec<(String, qubitick_model::Binder)> = Vec::new();
    let mut body = stmt.clone();
    while let Expr::Binder(qubitick_model::BinderKind::Forall, binders, inner) = body {
        for b in &binders {
            for v in &b.vars {
                vars.push((v.clone(), b.clone()));
            }
        }
        body = (*inner).clone();
    }
    if vars.is_empty() {
        return stmt.clone();
    }
    let ty_of: std::collections::HashMap<&str, qubitick_model::Binder> = vars
        .iter()
        .map(|(v, b)| (v.as_str(), b.clone()))
        .collect();
    // Free variables actually used in the body, in first-encountered order —
    // the SAME order both citation paths (`law_application`, `instantiate_axiom`)
    // render `R.ax_<k> …`, so the field's arity/order matches every proof cite.
    let mut used: Vec<String> = Vec::new();
    qubitick_pattern::expr_idents(&body, &mut used);
    let mut out = body;
    for v in used.into_iter().rev() {
        let Some(b) = ty_of.get(v.as_str()) else {
            continue;
        };
        out = Expr::Binder(
            qubitick_model::BinderKind::Forall,
            vec![qubitick_model::Binder {
                vars: vec![v],
                ty: b.ty.clone(),
            }],
            Box::new(out),
        );
    }
    out
}

fn emit_axiom_class(
    ctx: &Ctx,
    name: &str,
    statements: &[qubitick_model::Expr],
    parent: Option<&str>,
    offset: usize,
    fn_names: &std::collections::HashMap<u32, String>,
    sorts: &HashSet<u32>,
) -> String {
    let extends = parent.unwrap_or("ISA");
    if statements.is_empty() {
        return format!("class {name} extends {extends} where\n\n");
    }
    let mut out = format!("class {name} extends {extends} where\n");
    for (k, stmt) in statements.iter().enumerate() {
        // Sort glyphs render as the carrier so a field's binder type matches the
        // operator declarations (`Δ : Prop → Prop → Prop`).
        let reduced = reduce_field_statement(stmt);
        let base = crate::resolver_with_sorts(ctx, fn_names, sorts);
        /* A FIELD OF THE STRUCTURE NAMES ITSELF BARE HERE.
         *
         * The corpus reaches a field through the instance it quantifies over — a D block
         * says `Type: R.plus`, and a proof module writes `R.plus`. Inside the class that
         * DECLARES the field there is no instance yet, so the field's own name is what
         * the body says. This is the scaffold's only reading of the corpus's names, and it
         * reads nothing into them: whatever follows the `R.` is used as written. */
        let bare = |id: u32| -> String {
            let name = base(id);
            name.strip_prefix("R.").map(str::to_string).unwrap_or(name)
        };
        let lean = qubitick_lean::render(&reduced, &bare);
        out.push_str(&format!("  ax_{} : {lean}\n", offset + k));
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_model::{NodeBase, NodeKind};

    fn base(id: u32, name: &str) -> NodeBase {
        NodeBase {
            id,
            kind: NodeKind::Axiom,
            era: qubitick_model::Era::Era0,
            strata: qubitick_model::Strata(0),
            name: name.into(),
            req: vec![],
            derived: vec![],
            note: None,
        }
    }

    fn axiom(id: u32, name: &str, n: usize) -> Axiom {
        Axiom {
            base: base(id, name),
            statements: vec![qubitick_model::Expr::Var("a".into()); n],
            tag: qubitick_model::PhysOrMath::Math,
            falsify: None,
        }
    }

    /* THE RENDERER'S PARENTHESES ARE NOT PART OF THE TYPE.
     *
     * A characterising axiom binds its parameters at `Prop`, so it may only be emitted when
     * everything its equation names lives on that one carrier — and the test for that used to be a
     * string equal against two spellings, `Prop` and `(Prop → Prop)`. A frame that rendered the same
     * type as `(Prop) → (Prop) → Prop` therefore disqualified ITS OWN operators, every `_def` line
     * was dropped, and a theorem still cited `{op}_def`: the tutorial ship failed with `Unknown
     * identifier otimes_def` while a differently-parenthesised frame of the same corpus verified.
     * Three spellings of one type, and two that are genuinely not `Prop`. */
    #[test]
    fn prop_only_type_reads_the_type_not_its_parens() {
        for ty in [
            "Prop",
            "Prop → Prop",
            "Prop → Prop → Prop",
            "(Prop)",
            "(Prop) → (Prop) → Prop",
            // Prop-SORTED, which is the question this predicate answers: no carrier appears
            "(Prop → Prop)",
            "  Prop  ",
        ] {
            assert!(prop_only_type(ty), "{ty} is the Prop carrier");
        }
        for ty in [
            "",
            "Type",
            "C → Prop",
            "D → Prop",
            "Prop → C",
            // a function INTO Prop from a function is not flat `Prop`: the Eq would be over a
            // higher-order value, and the `∀ (v : Prop)` binder shape would not fit it
            "(Prop → Prop) → Prop",
        ] {
            assert!(!prop_only_type(ty), "{ty} is not the Prop carrier");
        }
    }

    #[test]
    fn chain_parents_follow_offsets() {
        // A1 has 2 statements, A2 has 1, A3 has 3.
        let a1 = axiom(1, "Realm", 2);
        let a2 = axiom(2, "Capacity", 1);
        let a3 = axiom(3, "Registration", 3);
        let refs: Vec<&Axiom> = vec![&a1, &a2, &a3];
        let chain = build_chain(&refs);
        assert_eq!(chain[0].parent, None);
        assert_eq!(chain[0].offset, 0);
        assert_eq!(chain[1].offset, 2);
        assert_eq!(chain[1].parent.as_deref(), Some("Realm"));
        assert_eq!(chain[2].offset, 3);
        assert_eq!(chain[2].parent.as_deref(), Some("Capacity"));
    }

    #[test]
    fn section_class_falls_back_to_the_deepest_class() {
        // A linear `extends` chain means the deepest class carries every field, so
        // it is the safe variable type when nothing is specifically required.
        let chain = build_chain(&[&axiom(1, "Realm", 1), &axiom(2, "Capacity", 1)]);
        assert_eq!(section_class(&chain, &[]), "Capacity");
        // An empty chain has no class at all.
        assert_eq!(section_class(&[], &[]), "ISA");
    }

    /* A NODE MODULE DECLARES NO CLASS — the ground owns the chain.
     *
     * The chain was emitted into every module that named it, which is what made a
     * corpus's assumptions live in a file nobody wrote. A module now imports the ground
     * and hypothesizes the instance (`section variable [R : ...]`); the classes are the
     * ground's, and `emit_ground_chain` is the only thing that writes them. */
    #[test]
    fn a_node_module_declares_no_class() {
        let ctx = Ctx::from_rows("Era0/Strata0", vec![]);
        let a1 = Node::Axiom(axiom(1, "Realm", 1));
        let a2 = Node::Axiom(axiom(2, "Capacity", 1));
        let out = emit_lean_text(&ctx, &[a1, a2], "Qubitick.HL", &mut RuleTable::new()).unwrap();
        assert!(!out.contains("class "), "{out}");
        assert!(!out.contains("axiom"), "{out}");
    }

    /* THE ONE WAY AN INDUCTIVE APPEARS IS A FORMATION THE CORPUS DECLARES.
     *
     * The fixture declares its own carrier (`W = p ∨ q ∨ r ∨ s`) and its own
     * operator (`Type: W \to W`). Nothing here is a glyph the emitter knows: the
     * inductive, the constructor aliases and the operator's type all come from those
     * two declarations, and a corpus that declares no formation gets no inductive. */
    #[test]
    fn a_declared_formation_is_the_only_inductive_and_types_its_operators() {
        use qubitick_db::queries::SymbolRow;
        use qubitick_model::{
            AtomicComposite, CtorDecl, DBlockIR, Definition, FormBody, SymbolKind,
        };
        let row = |id: i64, tex: &str, lean: &str, kind: &str, home: i64| SymbolRow {
            id,
            scope: "Era0/Strata0".into(),
            tex: tex.into(),
            display: tex.into(),
            lean_name: lean.into(),
            trigger: None,
            kind: kind.into(),
            role: None,
            precedence: None,
            home_node: home,
            elab: None,
            binder_kind: None,
            facets: 0,
        };
        let ctx = Ctx::from_rows(
            "Era0/Strata0",
            vec![
                row(1, "\\mathcal{W}", "W", "formation", 10),
                row(2, "\\mathbf{p}", "p", "ctor", 10),
                row(3, "\\mathbf{q}", "q", "ctor", 10),
                row(4, "\\mathbf{r}", "r", "ctor", 10),
                row(5, "\\mathbf{s}", "s", "ctor", 10),
                row(6, "\\oplus", "oplus", "primitive", 11),
            ],
        );
        let ctors = [2u32, 3, 4, 5]
            .iter()
            .map(|id| CtorDecl {
                constructor: AtomicComposite::new(*id),
                args: vec![],
            })
            .collect();
        let formation = Node::Definition(Definition {
            base: NodeBase {
                kind: NodeKind::Definition,
                ..base(10, "D.W")
            },
            symbol: AtomicComposite::new(1),
            extra_symbols: vec![],
            d_block: DBlockIR {
                kind: SymbolKind::Formation,
                form: Some(FormBody::Formation(ctors)),
                ..Default::default()
            },
            tag: qubitick_model::PhysOrMath::Math,
        });
        // `Type: \mathcal{W} \to \mathcal{W}`: the arrow carries the reserved id a
        // signature's arrow gets when no glyph declares it.
        let carrier = || Expr::Sym(AtomicComposite::new(1));
        let op = Node::Definition(Definition {
            base: NodeBase {
                kind: NodeKind::Definition,
                ..base(11, "D.Op")
            },
            symbol: AtomicComposite::new(6),
            extra_symbols: vec![],
            d_block: DBlockIR {
                kind: SymbolKind::Notation,
                sig: Some(Expr::Apply(
                    AtomicComposite::new(0),
                    vec![carrier(), carrier()],
                )),
                ..Default::default()
            },
            tag: qubitick_model::PhysOrMath::Math,
        });
        let out = emit_lean_text(
            &ctx,
            &[formation, op],
            "Qubitick.HL",
            &mut RuleTable::new(),
        )
        .unwrap();
        assert!(
            out.contains("set_option genInjectivity false in\ninductive W : Type"),
            "{out}"
        );
        for c in ["p", "q", "r", "s"] {
            assert!(out.contains(&format!("def {c} : W := W.{c}\n")), "{out}");
        }
        /* ...and the operator the corpus declares with a signature and NO home is
           declared by nobody: the engine writes no `axiom` for a glyph that lives
           nowhere. `\oplus` is one — the corpus owes it a `Type: <ground name>` or a
           `Sema:`, and the coverage pass says which (crates/build/src/coverage.rs). */
        assert!(!out.contains("axiom"), "{out}");
    }
}
