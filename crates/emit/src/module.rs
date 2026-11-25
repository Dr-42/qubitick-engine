//! Module-level Lean emission (CodeArch §5.11, Document 5 §4-§6).

use crate::imports::assemble_imports;
use anyhow::Result;
use qubitick_ctx::Ctx;
use qubitick_lean::LeanTarget;
use qubitick_model::corpus::{is_class, texes_with, Facet};
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

/// Symbols that are aliases of the type arrow (declare no constant; Lean's `->`).
fn is_arrow_glyph(lean: &str) -> bool {
    matches!(lean, "to" | "rightarrow" | "longrightarrow" | "arrow")
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

/// The ℤ-constructor glyphs are ATOMS (constructors), never functions: their
/// occurrence as an argument (e.g. `\mathbb{Z}_{0}` in `a +_Z \mathbb{Z}_{0}`)
/// is not an "applied" use, so they must never be `_fn`-dual-split — splitting
/// them changes the citation ground (Strata1's commutative duplications of
/// Strata0 theorems hit the base-vs-_fn mismatch). Exclude them from dual-use.
fn exclude_ctor_dual(dual: &mut HashSet<u32>, ctx: &Ctx) {
    for tex in texes_with(Facet::ZCtorDual) {
        if let Some(r) = ctx.by_tex(tex).and_then(|rows| rows.first()) {
            dual.remove(&(r.id as u32));
        }
    }
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
/// A definition whose `Sema:` is an equation headed by a corpus OPERATOR is a
/// postulate about operands (`D.BT.43`: `e_1 \cap e_2 = \varnothing`), not a
/// definition of that operator, and it declares nothing. The DB loader skips these
/// nodes already; the emitter has to agree, or the node claims `cap` — which in a
/// shared ground evicts `cap` from the frame the class axioms are written in.
fn defines_its_glyph(ctx: &Ctx, d: &qubitick_model::Definition) -> bool {
    match &d.d_block.form {
        Some(FormBody::MacroExpr(e)) => !is_operand_postulate(ctx, e),
        _ => true,
    }
}

fn is_operand_postulate(ctx: &Ctx, e: &Expr) -> bool {
    qubitick_model::corpus::is_operand_postulate(e, |id| {
        ctx.by_id(id as i64)
            .map(|r| is_class(&r.tex, Facet::BinOp))
            .unwrap_or(false)
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
/// events — `\bigcup_{t : \tau} \mathfrak{L}(t)` is `bigcup (fun t => …)`. A binder
/// argument types as `Prop → Prop`, so declaring `bigcup : Prop → Prop` and then
/// passing it a `fun` is an `Application type mismatch`. `binder[i]` marks the
/// positions observed receiving a lambda.
#[derive(Default, Clone)]
struct GlyphSig {
    len: usize,
    binder: Vec<bool>,
    /// Argument positions observed over \mathbb{Z} (a ℤ-constructor application).
    z_genids: Vec<bool>,
    /// Positions observed receiving the GENERATED sort (`\tau`), which is its own
    /// inductive type rather than the `Prop` carrier. An operator over the tick
    /// sequence must be declared `tau → tau → tau`; typing it over `Prop` while the
    /// statement binds `a : tau` is an `Application type mismatch`.
    genids: Vec<bool>,
    /// The operator RETURNS the generated sort: `+_\tau`, `\times_\tau`, `\uparrow`
    /// are `tau → … → tau`, not `… → Prop`.
    res_tau: bool,
    /// The operator RETURNS the generated `\mathbb{Z}` sort (its result feeds a
    /// ℤ-constructor) — typed `… → mathbb_Z`, not `… → Prop`.
    res_z: bool,
}

impl GlyphSig {
    fn merge(&mut self, args: &[Expr], tau_flags: &[bool], z_flags: &[bool]) {
        if args.len() > self.len {
            self.len = args.len();
            self.binder.resize(args.len(), false);
            self.genids.resize(args.len(), false);
            self.z_genids.resize(args.len(), false);
        }
        for (i, a) in args.iter().enumerate() {
            // Only a LAMBDA types as `Prop → Prop`; `Exists (fun …)` is itself a Prop.
            if matches!(a, Expr::Binder(qubitick_model::BinderKind::Lambda, _, _)) {
                self.binder[i] = true;
            }
            if tau_flags.get(i).copied().unwrap_or(false) {
                self.genids[i] = true;
            }
            if z_flags.get(i).copied().unwrap_or(false) {
                self.z_genids[i] = true;
            }
        }
    }

    fn ty(&self) -> String {
        // An operator that returns the generated sort ends in `tau`, not `Prop`.
        let mut ty = if self.res_tau && self.len > 0 {
            "tau".to_string()
        } else if self.res_z && self.len > 0 {
            "mathbb_Z".to_string()
        } else {
            "Prop".to_string()
        };
        for i in (0..self.len).rev() {
            if self.z_genids.get(i).copied().unwrap_or(false) {
                ty = format!("mathbb_Z → {ty}");
                continue;
            }
            // A binder over the GENERATED sort is a `tau`-valued function
            // (`(tau → Prop) → …`), taking precedence over the plain-`Prop` form.
            if self.binder.get(i).copied().unwrap_or(false) {
                if self.genids.get(i).copied().unwrap_or(false) {
                    ty = format!("(tau → Prop) → {ty}");
                } else {
                    ty = format!("(Prop → Prop) → {ty}");
                }
            } else if self.genids.get(i).copied().unwrap_or(false) {
                ty = format!("tau → {ty}");
            } else {
                ty = format!("Prop → {ty}");
            }
        }
        ty
    }
}

/// A coarse name identifying the sort an argument expression belongs to.
///
/// Deliberately shallow: it recognises the corpus's named constructors of the
/// generated sort (`\tau_0`, `^+`). Anything deeper needs the binder scope, which the
/// frame does not carry — and this is enough for the tick/integer operators, whose
/// arguments are built from exactly those constructors.
fn render_head_name(e: &Expr) -> String {
    match e {
        Expr::Sym(ac) => format!("#{}", ac.primary),
        Expr::Apply(h, _) => format!("#{}", h.primary),
        _ => String::new(),
    }
}

/// Render an authored `Type:` expression as a Lean type.
///
/// The generated sort keeps its own type (`\tau` → `tau`); every other sort is the
/// carrier and becomes `Prop`. An arrow chain becomes a Lean function type.
pub fn render_type_expr(ctx: &Ctx, e: &Expr) -> String {
    let is_tau = |id: u32| {
        ctx.by_id(id as i64)
            .map(|r| is_class(&r.tex, Facet::SortTau))
            .unwrap_or(false)
    };
    match e {
        Expr::Sym(ac) => {
            if is_tau(ac.primary) {
                "tau".to_string()
            } else {
                "Prop".to_string()
            }
        }
        Expr::Apply(h, args) => {
            let is_arrow = ctx
                .by_id(h.primary as i64)
                .map(|r| is_class(&r.tex, Facet::Arrow))
                .unwrap_or(false);
            if is_arrow && args.len() == 2 {
                return format!(
                    "({}) → {}",
                    render_type_expr(ctx, &args[0]),
                    render_type_expr(ctx, &args[1])
                );
            }
            "Prop".to_string()
        }
        _ => "Prop".to_string(),
    }
}

/// Re-mark an operator's argument positions from the ARGUMENTS themselves.
///
/// The first walk reads an argument's sort off its head symbol (`\mathbb{Z}_0`,
/// `^+ t`), which is all it can know before the τ/Z result fixpoint has run. With
/// the fixpoint in hand it can also see that an argument is an APPLICATION whose
/// head returns τ/Z: `\frac{a \cdot_Z d +_Z c \cdot_Z b}{b \cdot_Z d}` gives
/// `\frac` two `\mathbb{Z}` argument positions. Without them the frame types
/// `\frac` over the carrier, and the definition that uses it cannot be typed at
/// all — its own body applies `\frac` to `\mathbb{Z}`-valued subterms.
fn remark_arg_sorts(
    nodes: &[Node],
    sigs: &mut HashMap<u32, GlyphSig>,
    tau_results: &HashSet<u32>,
    z_results: &HashSet<u32>,
    names: &HashMap<u32, String>,
    connectives: &HashSet<u32>,
) {
    let _ = names;
    fn walk(
        e: &Expr,
        sigs: &mut HashMap<u32, GlyphSig>,
        tau_results: &HashSet<u32>,
        z_results: &HashSet<u32>,
        connectives: &HashSet<u32>,
    ) {
        if let Expr::Apply(h, args) = e {
            let s = sigs.entry(h.primary).or_default();
            if args.len() > s.len {
                s.len = args.len();
                s.binder.resize(args.len(), false);
                s.genids.resize(args.len(), false);
                s.z_genids.resize(args.len(), false);
            }
            for (i, a) in args.iter().enumerate() {
                // A propositional CONNECTIVE's argument positions are propositions,
                // whatever an application of it happens to hold: its own `Sema:`
                // (`a \leftrightarrow b = 1 \Delta (a \Delta b)`) states that. The
                // one place a connectivity position here looks Z-valued is
                // `D.BT.76`'s `a \cdot_Z d \le_Z c \cdot_Z b`, whose mixed-operator
                // chain parses left-to-right as `((a \cdot_Z d) \le_Z c) \cdot_Z b`
                // — a precedence bug of its own, not evidence about `\leftrightarrow`.
                if connectives.contains(&h.primary) {
                    continue;
                }
                if let Expr::Apply(ah, _) = a {
                    if z_results.contains(&ah.primary) {
                        s.z_genids[i] = true;
                    }
                    if tau_results.contains(&ah.primary) {
                        s.genids[i] = true;
                    }
                }
            }
        }
        match e {
            Expr::Apply(_, args) | Expr::Bracket(_, args) | Expr::Vector(args)
            | Expr::Set(args) => {
                for a in args {
                    walk(a, sigs, tau_results, z_results, connectives);
                }
            }
            Expr::Binder(_, bs, body) => {
                for b in bs {
                    walk(&b.ty, sigs, tau_results, z_results, connectives);
                }
                walk(body, sigs, tau_results, z_results, connectives);
            }
            Expr::Recursor(sc, cases) => {
                walk(sc, sigs, tau_results, z_results, connectives);
                for (_, b) in cases {
                    walk(b, sigs, tau_results, z_results, connectives);
                }
            }
            Expr::Matrix(rows) => {
                for r in rows {
                    for c in r {
                        walk(c, sigs, tau_results, z_results, connectives);
                    }
                }
            }
            Expr::Sym(_) | Expr::Var(_) => {}
        }
    }
    for n in nodes {
        match n {
            Node::Axiom(a) => a.statements
                .iter()
                .for_each(|e| walk(e, sigs, tau_results, z_results, connectives)),
            Node::Theorem(t) => {
                walk(&t.statement, sigs, tau_results, z_results, connectives);
                for step in &t.proof {
                    if let qubitick_model::ProofStep::Chain { rows } = step {
                        for row in rows {
                            if let Some(l) = &row.lhs {
                                walk(l, sigs, tau_results, z_results, connectives);
                            }
                            walk(&row.rhs, sigs, tau_results, z_results, connectives);
                        }
                    }
                }
            }
            Node::Definition(d) => {
                if let Some(FormBody::MacroExpr(e)) = &d.d_block.form {
                    walk(e, sigs, tau_results, z_results, connectives);
                }
                if let Some(sig) = &d.d_block.sig {
                    walk(sig, sigs, tau_results, z_results, connectives);
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
/// `Prop`, so it may only be emitted when everything its equation names is Prop-SORTED — no `tau`,
/// no `mathbb_Z`, no `mathfrak_E`. This is a string test for exactly that question, not a type
/// checker: `Prop`, `(Prop → Prop)` and `(Prop) → (Prop) → Prop` all pass it, and a sort in any
/// position fails it.
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
    /// Every symbol the module mentions, for the recursor emitters' permission set.
    pub seen: HashSet<u32>,
}

/// The Lean type a frame gives a glyph, from what it observes of it.
///
/// Extracted so that a glyph a DEFINITION declares (and the frame therefore does
/// not) still gets the frame's answer: `+_{\mathbb{Z}^{+}}` is declared by its own
/// node, but `+_{\mathbb{Z}}`'s helper and every definition that applies it need
/// the same `\mathbb{Z}` type the frame would have inferred. Without this the
/// definition's module declared it flat (`Prop → Prop → Prop`) while every other
/// module saw the sharp one, and an era ground holding both could not type a
/// single body.
fn glyph_type(
    ctx: &Ctx,
    id: u32,
    lean: &str,
    sigs: &HashMap<u32, GlyphSig>,
    tau_results: &HashSet<u32>,
    z_results: &HashSet<u32>,
) -> String {
    let mut s = sigs.get(&id).cloned().unwrap_or_default();
    // A glyph whose own tex carries the generated sort's mod operates OVER that
    // sort: `+_{\tau}`, `\times_{\tau}`, `\uparrow_{\tau}`. This is the reliable
    // signal — the binder scope is not always available (in TickAdd's
    // `a +_{\tau} \tau_0 = a` the `a` is free), while the glyph's tex always
    // states which sequence it belongs to.
    if ctx
        .by_id(id as i64)
        .map(|r| r.tex.contains("_{\\tau}") || r.tex.contains("^{\\tau}"))
        .unwrap_or(false)
    {
        s.genids = vec![true; s.len.max(1)];
        // …and it RETURNS the tick: `+_{\tau}`, `\times_{\tau}` and `\uparrow`
        // are τ-arithmetic. Without this the result fell back to the carrier
        // wherever no tick consumer happened to be visible in the module, and the
        // recursor `def` for a tick operator (`tau.rec … (fun b rec => plus_tau a
        // rec)`) then applied a `Prop` where the recursor's motive is `tau`.
        // …but a RELATION over the tick (`\le_{\tau}`) states a proposition, so
        // its τ mod says what its ARGUMENTS are, not what it returns.
        let is_relation = ctx
            .by_id(id as i64)
            .is_some_and(|r| is_class(&r.tex, Facet::Relation));
        s.res_tau = !is_relation;
    }
    // Result-sort inference: operators whose result feeds a tau-consuming head
    // (`^+`, `\uparrow`, or another tau-returning operator) return tau.
    if tau_results.contains(&id) {
        s.res_tau = true;
    }
    if z_results.contains(&id) {
        s.res_z = true;
    }
    // Z-family rule: an operator whose lean name is a Z member has its non-τ
    // non-binder argument positions on \mathbb{Z}. Value-position arguments are Z
    // elements even as plain bound variables (e.g. T_BT_72's
    // `\ge_{0}^{\mathbb{Z}}` rows) where no Z-ctor application ever pins them. The
    // arithmetic ops (plus/minus/times/up/down) also return \mathbb{Z}; the ORDER
    // relations (le/ge) return Prop.
    if lean.contains("mathbb_Z") && s.len > 0 {
        for i in 0..s.len {
            if !s.genids[i] && !s.binder[i] {
                s.z_genids[i] = true;
            }
        }
        if !s.res_tau && !s.res_z && !lean.starts_with("le") && !lean.starts_with("ge") {
            s.res_z = true;
        }
    }
    s.ty()
}

fn symbol_declarations(
    ctx: &Ctx,
    nodes: &[Node],
    emitted: &[Node],
    fn_names: &std::collections::HashMap<u32, String>,
    opts: &Options,
) -> Frame {
    // Every name this frame DECLARES itself. The node loop needs it: the frame
    // declares the generated environments (the tick sequence, the integer
    // sequence, a formation's carrier and its constructors) unconditionally, so a
    // `Definition` node for one of those glyphs must not declare it a second time.
    // One file per module never showed this — an era ground, which holds every
    // definition at once, showed it immediately (`Qubitick.HL.tau has already been
    // declared`).
    let mut declared_names: HashSet<String> = HashSet::new();
    // The generated sort's constructors, as the tags `render_head_name` produces.
    // An argument built from one of these belongs to the generated sort.
    let mut genids: HashSet<String> = HashSet::new();
    for tex in texes_with(Facet::GenTag) {
        if let Some(r) = ctx.by_tex(tex).and_then(|rows| rows.first()) {
            genids.insert(format!("#{}", r.id));
        }
    }
    // The `\tau` SORT symbol id, recognised for a big-operator binder over the
    // generated sort (`\bigcup_{t : \tau}`) and a binder whose variables range
    // over `\tau`. Kept separate from `genids` (constructors): adding the sort
    // there would wrongly tau-type an operator mixing `Prop` and `tau` args.
    let tau_sort_ids: HashSet<String> = texes_with(Facet::SortTau)
        .filter_map(|tex| ctx.by_tex(tex).and_then(|rows| rows.first()))
        .map(|r| format!("#{}", r.id))
        .collect();
    // Authored `Type:` signatures: the corpus's own statement of a glyph's type.
    // `\mathfrak{L}(t)` with `Type: \tau \to \mathfrak{E}` IS a function
    // `tau → Prop`, and typing it by inferred arity gives `Prop → Prop`, which then
    // refuses a `tau`-typed argument (`unknown constant 'CoeFun'`).
    let mut authored: HashMap<u32, String> = HashMap::new();
    // An authored `Type:` describes the glyph's APPLIED form. A dual-use glyph
    // (`\mathfrak{L}` vs `\mathfrak{L}(t)`) has a `_fn` variant (id + FN_OFFSET)
    // whose name is recorded in `fn_names`; the family type belongs to THAT
    // variant, while the bare use is a `Prop` (the total-ledger set). Attaching the
    // family sig to the bare id typed bare `cap a \mathfrak{L}` against a
    // `tau→Prop`, which broke the whole `class` chain (R.ax_0 cascade).
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
                .or_insert_with(|| render_type_expr(ctx, sig));
        }
    }
    let mut sigs: HashMap<u32, GlyphSig> = HashMap::new();
    let mut sorts: HashSet<u32> = HashSet::new();
    let mut seen: HashSet<u32> = HashSet::new();
    // ℤ-constructor ids: argument positions holding one of these are \mathbb{Z}.
    let mut z_ctor_ids: HashSet<u32> = HashSet::new();
    for tex in texes_with(Facet::ZCtor) {
        if let Some(r) = ctx.by_tex(tex).and_then(|rows| rows.first()) {
            z_ctor_ids.insert(r.id as u32);
        }
    }
    let z_sort_ids: HashSet<String> = texes_with(Facet::SortZ)
        .filter_map(|tex| ctx.by_tex(tex))
        .flat_map(|rows| {
            rows.iter()
                .map(|r| format!("#{}", r.id))
                .collect::<HashSet<String>>()
        })
        .collect();
    for n in nodes {
        visit_node(n, &mut sigs, &mut sorts, &mut seen, &genids, &tau_sort_ids, &z_ctor_ids, &z_sort_ids);
    }
    // --- Infer operator RESULT sorts (the generated sort vs Prop) ---
    // `+_{\tau}`, `\times_{\tau}`, `\uparrow_{\tau}` … are total functions on the
    // tick: `a +_\tau b` is itself a `\tau`. The frame must type them `tau → tau →
    // tau`, NOT `tau → tau → Prop`, or `Eq (plus_tau a tau_0) a` (a proposition
    // `a=...` with `a : tau` on the right) is an application-type mismatch.
    // Signal: an operator application consumed in a tau position (as the ARGUMENT
    // of a tick constructor or of another operator already known to return tau)
    // returns tau. Propagate to fixpoint.
    let mut tau_results: HashSet<u32> = HashSet::new();
    // Head ids that demand tau-typed operands: the tick constructor(s) (`\tau_0` is
    // nullary; `^+`/`tick`/`succ` are unary tau → tau).
    let mut tau_consuming: HashSet<u32> = HashSet::new();
    for tex in texes_with(Facet::TauConsume) {
        if let Some(r) = ctx.by_tex(tex).and_then(|rows| rows.first()) {
            tau_consuming.insert(r.id as u32);
        }
    }
    // ℤ-constructors consume ℤ: an operator applied as an argument TO a
    // ℤ-constructor returns \mathbb{Z} (its result is a ℤ element).
    let mut z_consuming: HashSet<u32> = HashSet::new();
    for tex in texes_with(Facet::ZCtor) {
        if let Some(r) = ctx.by_tex(tex).and_then(|rows| rows.first()) {
            z_consuming.insert(r.id as u32);
        }
    }
    // τ-terminators: an `=`-counterpart to these forces the other side to be τ.
    let mut tau_term: HashSet<u32> = HashSet::new();
    for tex in texes_with(Facet::TickTerm) {
        if let Some(r) = ctx.by_tex(tex).and_then(|rows| rows.first()) {
            tau_term.insert(r.id as u32);
        }
    }
    // eater -> set of sub-application heads it is directly applied to
    let mut consumed_by: HashMap<u32, HashSet<u32>> = HashMap::new();
    let mut eq_tau: HashSet<u32> = HashSet::new();
    let mut eq_z: HashSet<u32> = HashSet::new();
    let mut z_term: HashSet<u32> = HashSet::new();
    for tex in texes_with(Facet::ZCtor) {
        if let Some(r) = ctx.by_tex(tex).and_then(|rows| rows.first()) {
            z_term.insert(r.id as u32);
        }
    }
    collect_consumed_by(nodes, &mut consumed_by, &mut eq_tau, &mut eq_z, &tau_term, &z_term);
    // An operator whose application is the `=`-side against a τ-terminator returns τ.
    tau_results.extend(eq_tau);
    // Seed: anything directly consumed by a tau-constructor returns tau; then
    // closure where a head consumed by a tau-returning head also returns tau.
    loop {
        let before = tau_results.len();
        // Any head whose application is directly consumed by a tau-constructor
        // returns tau; then closure over tau-returning consumers.
        let mut changed = false;
        for (eater, deps) in &consumed_by {
            if !tau_consuming.contains(eater) && !tau_results.contains(eater) {
                continue;
            }
            for d in deps {
                if tau_results.insert(*d) {
                    changed = true;
                }
            }
        }
        if !changed && tau_results.len() == before {
            break;
        }
    }
    // Same fixpoint for ℤ-returning operators (their result feeds a ℤ-constructor,
    // or an `=`-side against a ℤ-terminator).
    let mut z_results: HashSet<u32> = HashSet::new();
    z_results.extend(eq_z);
    loop {
        let before = z_results.len();
        let mut changed = false;
        for (eater, deps) in &consumed_by {
            if !z_consuming.contains(eater) && !z_results.contains(eater) {
                continue;
            }
            for d in deps {
                if z_results.insert(*d) {
                    changed = true;
                }
            }
        }
        if !changed && z_results.len() == before {
            break;
        }
    }
    // Now that the τ/Z result fixpoints have settled, re-read the argument
    // positions with that knowledge. The heads that COUNT as τ/Z-valued are the
    // same set the declaration loop uses: a result the fixpoint proved, or a glyph
    // whose own name/tex says which sequence it belongs to (`\cdot_{\mathbb{Z}}`
    // is Z-arithmetic and returns a Z, even where no Z-constructor ever pins it).
    // Only operators that RETURN τ/Z may say what an argument's sort is, and the
    // test for that is the frame's OWN answer for the head — not the raw `z_results`
    // fixpoint, whose rule reads "consumed by a Z-constructor" as "returns Z". That
    // rule is harmless where it only decides a result sort (`tick` returns τ, and
    // τ wins), but as an ARGUMENT test it said `tick` and `\times_{\tau}` were
    // `\mathbb{Z}`-valued, which marked `plus_{\tau}`'s second argument on Z and
    // broke `plus_tau m n` in every theorem that used it.
    //
    // A connective is excluded outright: its arguments are propositions by its own
    // `Sema:` (`a \leftrightarrow b = 1 \Delta (a \Delta b)`), whatever one
    // application happens to hold. The single place a connective position here looks
    // Z-valued is `D.BT.76`'s `a \cdot_Z d \le_Z c \cdot_Z b`, whose mixed-operator
    // chain parses left-to-right as `((a \cdot_Z d) \le_Z c) \cdot_Z b` — a
    // precedence bug of its own, not evidence about `\leftrightarrow`.
    let connectives: HashSet<u32> = texes_with(Facet::LeftIff)
        .chain(texes_with(Facet::RightIff))
        .filter_map(|t| ctx.by_tex(t).and_then(|rows| rows.first()))
        .map(|r| r.id as u32)
        .chain(seen.iter().copied().filter(|id| {
            matches!(crate::resolver(ctx, fn_names)(*id).as_str(), "land" | "lor")
        }))
        .collect();
    let name_of: HashMap<u32, String> = seen
        .iter()
        .map(|id| (*id, crate::resolver(ctx, fn_names)(*id)))
        .collect();
    let returns = |id: u32, sort: &str| -> bool {
        glyph_type(ctx, id, &name_of[&id], &sigs, &tau_results, &z_results).ends_with(sort)
    };
    let z_heads: HashSet<u32> = seen
        .iter()
        .copied()
        .filter(|id| !connectives.contains(id) && returns(*id, "mathbb_Z"))
        .collect();
    let tau_heads: HashSet<u32> = seen
        .iter()
        .copied()
        .filter(|id| !connectives.contains(id) && returns(*id, "tau"))
        .collect();
    remark_arg_sorts(nodes, &mut sigs, &tau_heads, &z_heads, &name_of, &connectives);

    // Recursor-definable operators (from CONTEXT definitions): their defining recursive
    // `\land`-Sema is compiled to a `noncomputable def … tau.rec …`, so every module
    // that USES the operator sees a real definition (`[\equiv]` can unfold it) instead
    // of an uninterpreted `axiom`. Keyed by the operator's glyph id (== definition
    // symbol id after `definition_symbol_id`).
    let recursor_resolver = crate::resolver(ctx, fn_names);
    // Cross-operator tick recursor closure, by fixpoint: a recursor op qualifies if
    // every glyph its base/succ RHS references is itself recursor-definable (the
    // tick closure `plus_tau < times_tau`, `downarrow < minus_tau`). Keying and
    // naming by the Sema's head id (not the glyph-line's first control word, which
    // can drop a `_τ` suffix) resolves times_tau/etc. correctly.
    let sema_head = |e: &Expr| -> Option<(u32, String)> {
        use qubitick_model::{BinderKind as BK, Expr as E, EQ_SYM as EQ};
        // Collect every conjunct of a (possibly nested) `land`.
        fn conj<'a>(x: &'a Expr, ctx: &Ctx, out: &mut Vec<&'a Expr>) {
            match x {
                E::Apply(h, args) if crate::lean_name_of(ctx, h.primary) == "land" => {
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
            // strip a `∀ n, …` wrapper
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
    // reference; context defs must not inject their recursors into frames that
    // never use (nor declare the sorts of) them. Gated by LEAN NAME (ids can
    // differ between the statement's glyph and the Sema's head for the same op).
    let mut own_names: HashSet<String> = HashSet::new();
    let mut own_ids: HashSet<u32> = HashSet::new();
    for n in emitted {
        match n {
            Node::Theorem(t) => {
                collect_plain_ids(&t.statement, &mut own_ids);
                // Proof rows may reference operators the statement never mentions
                // (e.g. T_BT_72's \ge_{0}^{\mathbb{Z}}, defined as a ℤ-recursor
                // in D_BT_78); without the demand they stay opaque axioms and
                // `[\equiv]` steps over them cannot reduce.
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
    // The op's result sort, keyed by LEAN NAME (ids can differ between the
    // statement's glyph, the def's symbol and the sigs key).
    let res_z_by_name: std::collections::HashMap<String, bool> = sigs
        .iter()
        .map(|(id, sg)| (recursor_resolver(*id), sg.res_z))
        .collect();

    // DEMAND CLOSURE: an admitted def's Sema glyphs are also demanded, so the
    // dependency chain (plus_ℤ -> plus_ℤ± -> uparrow_ℤ) becomes reducible defs.
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
                && (demand.contains(&head_id)
                    || demand_names.contains(&norm(&hname)))

                && let Some(lt) = crate::definition::recursor_def_text(
                    ctx, &hname, e, &recursor_resolver, &seen,
                )
                .or_else(|| {
                    crate::definition::recursor_def_text_extended(
                        ctx,
                        &hname,
                        e,
                        &recursor_resolver,
                        &seen,
                        res_z_by_name
                            .get(&hname)
                            .copied()
                            .or_else(|| res_z_by_name.get(&format!("{hname}_fn")).copied()),
                    )
                })
                .or_else(|| {
                    crate::definition::recursor_def_text_formation(
                        ctx,
                        &hname,
                        e,
                        &recursor_resolver,
                        &formations,
                    )
                })
            {
                recursor_defs.insert(head_id, lt);
                recursor_able.insert(head_id);
                // DEMAND CLOSURE: the def's Sema glyphs become demanded so the
                // dependency chain (plus_ℤ -> plus_ℤ± -> uparrow_ℤ) is reducible.
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
    let resolver = crate::resolver(ctx, fn_names);
    // Only names this module actually EMITS a `def` for may be suppressed from the
    // frame. `nodes` includes CONTEXT (a theorem's file holds one node, so the
    // corpus definitions arrive as context), and those definitions are never
    // emitted — suppressing their glyphs left them undeclared, so a statement that
    // APPLIES one (`plus_tau a tau_0`) applied an unknown constant and Lean
    // reported `unknown constant 'CoeFun'`.
    let defined_names = defined_symbols(ctx, emitted, fn_names);
    let mut out = String::new();
    let mut extra_defined: HashSet<String> = HashSet::new();
    // `nodes` and `emitted` are the same slice when a module IS its own context — a
    // frame assembled from a whole era, for instance. Chaining them would then emit
    // every formation twice, so each carrier is declared once and the second pass
    // over it is skipped.
    let mut formation_done: HashSet<u32> = HashSet::new();
    for n in nodes.iter().chain(emitted.iter()) {
        if let Node::Definition(d) = n
            && let Some(qubitick_model::FormBody::Formation(ctors)) = &d.d_block.form
            && formation_done.insert(d.symbol.primary)
        {
            let ctor_ids: Vec<u32> = ctors.iter().map(|c| c.constructor.primary).collect();
            let used = seen.contains(&d.symbol.primary)
                || ctor_ids.iter().any(|id| seen.contains(id));
            if !used {
                continue;
            }
            let carrier = crate::lean_name_of(ctx, d.symbol.primary);
            let mut f = format!("set_option genInjectivity false in\ninductive {carrier} : Type\n");
            for cid in &ctor_ids {
                let cnam = ctx
                    .by_id(*cid as i64)
                    .map(|r| r.lean_name.clone())
                    .unwrap_or_else(|| format!("s{cid}"));
                f.push_str(&format!("  | {cnam}\n"));
                extra_defined.insert(cnam);
            }
            extra_defined.insert(carrier.clone());
            declared_names.insert(carrier.clone());
            for cid in &ctor_ids {
                if let Some(r) = ctx.by_id(*cid as i64) {
                    declared_names.insert(r.lean_name.clone());
                }
            }
            out.push_str(&f); // the `inductive` FIRST
            for cid in &ctor_ids {
                let cnam = ctx
                    .by_id(*cid as i64)
                    .map(|r| r.lean_name.clone())
                    .unwrap_or_else(|| format!("s{cid}"));
                out.push_str(&format!(
                    "def {cnam} : {carrier} := {carrier}.{cnam}\n"
                ));
            }
        }
    }
    // The corpus identifies events with propositions, so the carrier sort is `Prop`
    // (Δ = XOR, ∩ = AND, 𝟙 = True, ∅ = False, `=` = propositional equality).

    // Emit the generated-sort inductive and its constructor aliases FIRST so that any
    // recursor `def` for tick operators (`plus_tau` etc.) can reference `tau`/`tick`/
    // `tau_0`. The `sorts` set marks `\tau` when it is used as a binder sort.
    let tau_id = texes_with(Facet::SortTau)
        .filter_map(|tex| ctx.by_tex(tex).and_then(|rows| rows.first()))
        .map(|r| r.id as u32)
        .next();
    // Emit the tick environment when `\tau` is a binder sort, OR the module uses
    // tick-valued operators (result-sort inference marked some glyph tau) — a tick-op
    // `def`/recursor needs `inductive tau`/`tick`/`tau_0` in scope.
    // Also trigger on the tick CONSTRUCTOR glyphs (`^+`/`tick`, `\tau_0`) being used
    // anywhere in the module (a tick-op definition references them even when `\tau`
    // the SORT symbol is absent).
    let ctor_used = texes_with(Facet::GenTag).any(|tex| {
        ctx.by_tex(tex)
            .and_then(|rows| rows.first())
            .map(|r| seen.contains(&(r.id as u32)))
            .unwrap_or(false)
    });
    // The generated sort `\mathbb{Z}` is also a real inductive in the corpus's own
    // semantics (`\mathbb{Z} = \mathbb{Z}_{0} \cup ⋃_{t:τ}(𝕫⁺(t) ∪ 𝕫⁻(t))`,
    // D.BT.47): three constructors over the tick. Emit it (and the constructor
    // ALIASES that all Z-glyph uses share) when any Z-constructor is referenced.
    let z_ctor_used = texes_with(Facet::ZCtor).any(|tex| {
        ctx.by_tex(tex)
            .and_then(|rows| rows.first())
            .map(|r| seen.contains(&(r.id as u32)))
            .unwrap_or(false)
    });
    let z_id = texes_with(Facet::SortZ)
        .filter_map(|tex| ctx.by_tex(tex).and_then(|rows| rows.first()))
        .map(|r| r.id as u32)
        .next();
    // A declared type that MENTIONS a generated sort forces that sort's inductive to
    // exist, exactly as using one of its constructors does. Without this the frame
    // wrote `axiom cdot___mathbb_Z : mathbb_Z → mathbb_Z → mathbb_Z` and never
    // declared `mathbb_Z` — and Lean, in a bare prelude with `autoImplicit` on,
    // silently bound every occurrence to a FRESH implicit variable, so the module
    // "checked" with each sort occurrence unrelated to the others.
    let z_in_types = seen.iter().any(|id| {
        sigs.get(id).is_some_and(|s| {
            (s.res_z && s.len > 0)
                || (!s.z_genids.is_empty() && s.z_genids.iter().any(|b| *b))
        })
    }) || authored.values().any(|t| t.contains("mathbb_Z"));
    let tau_in_types = seen.iter().any(|id| {
        sigs.get(id).is_some_and(|s| {
            (s.res_tau && s.len > 0)
                || (!s.genids.is_empty() && s.genids.iter().any(|b| *b))
        })
    }) || authored.values().any(|t| t.contains("tau") || t.contains("tick"));
    // The integer sequence's constructors take a tick (`\mathbb{Z}^{+} n`), so a
    // Z sort needs the tick inductive declared beside it.
    let tau_used = tau_in_types || z_in_types || ctor_used
        || z_ctor_used // ℤ-constructor aliases take `tau` args; the inductive needs it
        || z_id.is_some_and(|zid| seen.contains(&zid) || sorts.contains(&zid))
        || tau_id.is_some_and(|tid| {
            sorts.contains(&tid) || tau_results.contains(&tid) || seen.contains(&tid)
        });
    // lean name -> declared Lean type string, for every frame symbol.
    let mut declared_types: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    // The types the frame infers for glyphs it does NOT declare (a Definition owns
    // those). Kept apart from `declared_types`, which also means "this frame has
    // already written this name" and must not gain names it never wrote.
    let mut inferred_types: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    if tau_used {
        declared_names.extend(["tau", "tau_0", "tick"].map(String::from));
        // The generated environment is part of what a definition body may name, so
        // its TYPES go into `known_types` too: `\uparrow a = ^+ a` (D.BT.54) is
        // `tau → tau` because `tick` is, and the tick environment declares itself,
        // not through the ordinary symbol loop.
        declared_types.insert("tau".to_string(), "Type".to_string());
        declared_types.insert("tau_0".to_string(), "tau".to_string());
        declared_types.insert("tick".to_string(), "tau → tau".to_string());
        out.push_str(
            "inductive tau : Type\n  | zero : tau\n  | succ : tau → tau\n\
             def tau_0 : tau := tau.zero\n\
             def tick (x : tau) : tau := tau.succ x\n",
        );
    }
    let z_used = z_in_types
        || z_ctor_used
        || z_id.is_some_and(|zid| {
            sorts.contains(&zid) || seen.contains(&zid)
        });
    // lean names of the ℤ-constructor aliases (their actual sanitized spellings)
    let mut z_alias_names: HashSet<String> = HashSet::new();
    if z_used {
        declared_names.insert("mathbb_Z".to_string());
        declared_types.insert("mathbb_Z".to_string(), "Type".to_string());
        // Constructor aliases under the ACTUAL lean names of the glyphs in the
        // DB (the `\mathbb{Z}_{0}` symbol sanitizes to mathbb_Z__0 — the
        // underscore count comes from the symbol table, not from a hand-write).
        let mut zb = String::from(
            "inductive mathbb_Z : Type\n\
             \x20 | zero : mathbb_Z\n\
             \x20 | plus : tau → mathbb_Z\n\
             \x20 | minus : tau → mathbb_Z\n",
        );
        let ctor_aliases = qubitick_model::corpus::Z_CTOR_ALIASES;
        for (tex, ctor) in ctor_aliases {
            if let Some(r) = ctx.by_tex(tex).and_then(|rows| rows.first()) {
                let nm = &r.lean_name;
                z_alias_names.insert(nm.clone());
                declared_names.insert(nm.clone());
                declared_types.insert(
                    nm.clone(),
                    if *ctor == "mathbb_Z.plus" || *ctor == "mathbb_Z.minus" {
                        "tau → mathbb_Z".to_string()
                    } else {
                        "mathbb_Z".to_string()
                    },
                );
                let args = if *ctor == "mathbb_Z.plus" || *ctor == "mathbb_Z.minus" {
                    " (t0 : tau)"
                } else {
                    ""
                };
                zb.push_str(&format!(
                    "def {nm}{args} : mathbb_Z := {ctor}{}\n",
                    if args.is_empty() { "" } else { " t0" }
                ));
            }
        }
        out.push_str(&zb);
    }
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
        // may apply it. The name stays FREE for a later glyph of the same spelling,
        // exactly as before this branch existed.
        if defined_names.contains(&lean) {
            if !sorts.contains(&id) {
                inferred_types.insert(
                    lean.clone(),
                    glyph_type(ctx, id, &lean, &sigs, &tau_results, &z_results),
                );
            }
            continue;
        }
        if extra_defined.contains(&lean) || !emitted_names.insert(lean.clone()) {
            continue;
        }
        // Constructors of the generated sort are declared BY the inductive (emitted above).
        if matches!(lean.as_str(), "tau" | "tau_0" | "tick") {
            continue;
        }
        // The ℤ inductive declares `mathbb_Z` and its constructor aliases.
        if z_used
            && (lean == "mathbb_Z" || z_alias_names.contains(&lean))
        {
            continue;
        }
        if is_arrow_glyph(&lean) {
            continue;
        }
        if sorts.contains(&id) {
            // A GENERATED sort is a real inductive (emitted above). Any other "sort"
            // glyph stays `def : Type := Prop`.
            if lean == "tau" || (z_used && lean == "mathbb_Z") {
                continue;
            } else {
                out.push_str(&format!("def {lean} : Type := Prop\n"));
                // Record it: a frame assembled from a whole era can reach the same
                // glyph again as an operator, and one declaration is all Lean allows.
                declared_types.insert(lean.clone(), "Type".to_string());
            }
        } else if let Some(def_text) = recursor_defs.get(&id) {
            // A recursor-definable operator (τ/ℤ or custom-formation) is a real
            // `def` (so `[\equiv]` unfolds it by kernel conversion), not a frame
            // `axiom`. Take precedence over an authored `Type:` sig, which only
            // states the glyph's type (a recursor Sema means it actually computes).
            defer_recursor_defs.push((id, def_text.clone()));
        } else if let Some(ty) = authored.get(&id) {
            declared_types.insert(lean.clone(), ty.clone());
            out.push_str(&format!("axiom {lean} : {ty}\n"));
        } else {
            let ty = glyph_type(ctx, id, &lean, &sigs, &tau_results, &z_results);
            // `\top`/`\bot` are the order-theoretic truth constants: emit them
            // REDUCIBLE (`≡ True`/`≡ False`) so a `[\equiv]` row ends at a provable
            // object and a Prop-goal chain can close via `trivial` (e.g. T_BT_72's
            // `ge (a + -a) = ge Z0 = \top`).
            if declared_types.contains_key(&lean) {
                // Already declared by the sort loop above (a frame assembled from a
                // whole era can reach one glyph from both). One declaration, or the
                // module does not elaborate.
                continue;
            }
            if lean == "top" {
                out.push_str("def top : Prop := True\n");
            } else if lean == "bot" {
                out.push_str("def bot : Prop := False\n");
            } else {
                declared_types.insert(lean.clone(), ty.clone());
            out.push_str(&format!("axiom {lean} : {ty}\n"));
            }
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
                    && txt.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
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
    // Characterising axioms for `=`-defined macros (`\lnot a = \mathbb{1} Δ a`,
    // D.BT.02): `axiom {op}_def : ∀ params, Eq (op params) body`. Theorem proofs
    // cite these by name (chain.rs's `rule.definitional` branch) instead of
    // relying on `Eq.refl` over an opaque head, which cannot close
    // `Eq (lnot X) (Δ 𝟙 X)` when `lnot` is only an axiom in this frame.
    let resolver = crate::resolver(ctx, fn_names);
    // Characterising axioms for `=`-defined PROP macros (e.g. D.BT.02
    // `\lnot a = \mathbb{1} Δ a`): `axiom {op}_def : ∀ params, Eq (op params) body`.
    // Theorem proofs cite these by name (chain.rs's `rule.definitional`) instead
    // of `Eq.refl`, which cannot close `Eq (lnot X) (Δ 𝟙 X)` over an opaque axiom.
    // Restricted to Prop-valued macros: a `tick`/τ-sort equation would make the
    // Eq-frame axiom ill-typed (mixed sorts). `lnot` is the confirmed case.
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
            let own_def = emitted.iter().any(|nd| {
                matches!(nd, Node::Definition(dd)
                    if crate::lean_name_of(ctx, dd.symbol.primary) == name)
            });
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
            let skip = if opts.ground_only {
                // The ground declares the era's definitions, so it carries EVERY
                // `_def` line: the ones whose operator nothing defines (a frame
                // axiom) and the ones whose operator its own `def` declares — for
                // those the char-axiom is appended after the definitions, which is
                // the whole reason they cannot simply sit in the frame.
                false
            } else if opts.ground == Ground::Inline {
                // A per-file module declares the frame itself and no definitions:
                // it carries the `_def` lines of the operators it does not define.
                own_def
            } else {
                // A module that IMPORTS its ground carries none — the ground is the
                // only module that can see both halves.
                true
            };

            if skip {
                continue;
            }
            // Generalize beyond `lnot`: emit the characterising axiom for ANY
            // `=`-macro whose operands and result are the plain `Prop` carrier, so
            // an author's own defined operator is citable definitionally. The test
            // is the operator's observed GlyphSig (no τ/Z/binder positions, Prop
            // result), which is exactly the condition under which the `∀ (a : Prop)`
            // binder shape is well-typed. Sort-valued macros are skipped: their
            // equations close by `Eq.refl` and a Prop-binder axiom would be
            // ill-typed (the original mixed-sort failure).
            let prop_valued = sigs.get(&d.symbol.primary).is_none_or(|sg| {
                !sg.res_tau
                    && !sg.res_z
                    && sg.genids.iter().all(|b| !*b)
                    && sg.z_genids.iter().all(|b| !*b)
                    && sg.binder.iter().all(|b| !*b)
            });
            // Also skip anything whose equation mentions a SORT (`mathbb_Z`,
            // `mathfrak_E`, `tau`, …): those render `Type`-valued, so `Eq` over
            // them is ill-typed (and the equation closes by `Eq.refl` anyway).
            // Any symbol that is NOT a plain Prop carrier: sorts, and operators
            // whose observed result is τ or Z. An equation mentioning one of them
            // is not Prop-typed, so the `∀ (a : Prop)` binder shape would not
            // type-check — skip it (those equations close by `Eq.refl`).
            let mut sort_names: Vec<String> =
                sorts.iter().map(|id| crate::lean_name_of(ctx, *id)).collect();
            // A symbol is PROP-SAFE only when its declared type is built purely of
            // `Prop` (no τ/Z/binder positions and a Prop result). Any symbol that
            // is not prop-safe is added to the exclusion set, so an equation
            // mentioning it is skipped.
            for (id, sg) in sigs.iter() {
                let prop_safe = !sg.res_tau
                    && !sg.res_z
                    && sg.genids.iter().all(|b| !*b)
                    && sg.z_genids.iter().all(|b| !*b)
                    && sg.binder.iter().all(|b| !*b);
                if !prop_safe {
                    sort_names.push(crate::lean_name_of(ctx, *id));
                }
            }
            for extra in ["tau", "tick", "tau_0", "mathbb_Z"] {
                sort_names.push(extra.to_string());
            }
            let mentions_sort = |t: &str| {
                t.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .any(|w| !w.is_empty() && sort_names.iter().any(|sn| sn == w))
            };
            // Every identifier in either side must be a KNOWN Prop-valued symbol
            // (or the macro's own parameter). This is the precise well-typedness
            // condition: anything else (τ/Z-valued, or not in the frame at all)
            // would make `Eq (op …) …` ill-typed.
            let mut params_now = Vec::new();
            qubitick_pattern::expr_idents(&eqargs[0], &mut params_now);
            // Precisely: every identifier must be a frame symbol whose DECLARED
            // Lean type is built purely of `Prop` (so `Eq (op …) …` is well-typed).
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
    // leaves to the node). A definition body is typed from this, so the body and
    // the frame agree about which of τ/Z/Prop a glyph lives in.
    // Precedence: what the CORPUS says (`Type:`), then what this frame wrote, then
    // what it would have written for a glyph a definition owns.
    let mut known_types: HashMap<String, String> = HashMap::new();
    for (id, ty) in &authored {
        known_types.insert(resolver(*id), ty.clone());
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
        seen,
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

/// Walk all nodes, recording `Apply(outer, args)` where an arg is itself an
/// application `Apply(inner, _)`: record `outer` -> `inner`. Used to infer which
/// operators return the generated sort (their result feeds a tau-consuming head).
fn collect_consumed_by(
    nodes: &[Node],
    out: &mut HashMap<u32, HashSet<u32>>,
    eq_tau: &mut HashSet<u32>,
    eq_z: &mut HashSet<u32>,
    term: &HashSet<u32>,
    z_term: &HashSet<u32>,
) {
    let mut rec = |e: &Expr| rec_cb(e, out, eq_tau, eq_z, term, z_term);
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
}

/// Whether `e` is a τ-terminator: the `\tau_0` constant, or a tick/`^+`/`↑`/`↓`
/// constructor application. The OTHER side of an equality against one of these
/// must itself be a τ, so its head is a τ-returning operator.
fn is_tau_terminator(e: &Expr, term: &HashSet<u32>) -> bool {
    match e {
        Expr::Sym(ac) => term.contains(&ac.primary),
        Expr::Apply(h, _) => term.contains(&h.primary),
        _ => false,
    }
}

fn rec_cb(
    e: &Expr,
    out: &mut HashMap<u32, HashSet<u32>>,
    eq_tau: &mut HashSet<u32>,
    eq_z: &mut HashSet<u32>,
    term: &HashSet<u32>,
    z_term: &HashSet<u32>,
) {
    match e {
        Expr::Apply(head, args) => {
            // `Eq (op …) τ_0` / `Eq (op …) (tick …)`: an operator applied against a
            // τ-terminator returns τ (its result sort is the generated sort).
            if head.primary == EQ_SYM && args.len() == 2 {
                for (i, side) in args.iter().enumerate() {
                    let other = &args[1 - i];
                    if is_tau_terminator(other, term)
                        && let Expr::Apply(oh, _) = side
                    {
                        eq_tau.insert(oh.primary);
                    }
                    if is_tau_terminator(other, z_term)
                        && let Expr::Apply(oh, _) = side
                    {
                        eq_z.insert(oh.primary);
                    }
                }
            }
            for a in args {
                if let Expr::Apply(inner, _) = a {
                    out.entry(head.primary).or_default().insert(inner.primary);
                }
                rec_cb(a, out, eq_tau, eq_z, term, z_term);
            }
        }
        Expr::Bracket(_, args) | Expr::Vector(args) | Expr::Set(args) => {
            for a in args {
                rec_cb(a, out, eq_tau, eq_z, term, z_term);
            }
        }
        Expr::Binder(_, bs, body) => {
            for b in bs {
                rec_cb(&b.ty, out, eq_tau, eq_z, term, z_term);
            }
            rec_cb(body, out, eq_tau, eq_z, term, z_term);
        }
        Expr::Recursor(s, cases) => {
            rec_cb(s, out, eq_tau, eq_z, term, z_term);
            for (_, b) in cases {
                rec_cb(b, out, eq_tau, eq_z, term, z_term);
            }
        }
        Expr::Matrix(rows) => {
            for r in rows {
                for c in r {
                    rec_cb(c, out, eq_tau, eq_z, term, z_term);
                }
            }
        }
        Expr::Sym(_) | Expr::Var(_) => {}
    }
}

/// Walk one node's statements collecting glyph arities, sort-glyphs, and refs.
#[allow(clippy::too_many_arguments)]
fn visit_node(
    n: &Node,
    sigs: &mut HashMap<u32, GlyphSig>,
    sorts: &mut HashSet<u32>,
    seen: &mut HashSet<u32>,
    genids: &HashSet<String>,
    tau_sort_ids: &HashSet<String>,
    z_ctor_ids: &HashSet<u32>,
    z_sort_ids: &HashSet<String>,
) {
    let mut visit = |e: &Expr| visit_expr(e, sigs, sorts, seen, genids, tau_sort_ids, z_ctor_ids, z_sort_ids);
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

/// Collect arity/sort/reference info from one `Expr`.
#[allow(clippy::too_many_arguments)]
fn visit_expr(
    e: &Expr,
    sigs: &mut HashMap<u32, GlyphSig>,
    sorts: &mut HashSet<u32>,
    seen: &mut HashSet<u32>,
    genids: &HashSet<String>,
    tau_sort_ids: &HashSet<String>,
    z_ctor_ids: &HashSet<u32>,
    z_sort_ids: &HashSet<String>,
) {
    visit_expr_scoped(e, sigs, sorts, seen, genids, tau_sort_ids, &HashSet::new(), z_ctor_ids, z_sort_ids, &HashSet::new());
}

/// As [`visit_expr`], carrying which variables are bound to the GENERATED sort.
///
/// An operator's argument is frequently a plain metavariable — `plus_tau a tau_0`
/// has `a` bound by the enclosing `∀ a : \tau` — so its sort can only be read from
/// the binder scope, not from the argument expression alone.
#[allow(clippy::too_many_arguments)]
fn visit_expr_scoped(
    e: &Expr,
    sigs: &mut HashMap<u32, GlyphSig>,
    sorts: &mut HashSet<u32>,
    seen: &mut HashSet<u32>,
    genids: &HashSet<String>,
    tau_sort_ids: &HashSet<String>,
    tau_vars: &HashSet<String>,
    z_ctor_ids: &HashSet<u32>,
    z_sort_ids: &HashSet<String>,
    z_vars: &HashSet<String>,
) {
    match e {
        Expr::Sym(ac) => {
            seen.insert(ac.primary);
        }
        Expr::Var(_) => {}
        Expr::Apply(head, args) => {
            if head.primary != EQ_SYM {
                seen.insert(head.primary);
                let flags: Vec<bool> = args
                    .iter()
                    .map(|a| match a {
                        Expr::Var(v) => tau_vars.contains(v) || z_vars.contains(v),
                        // A LAMBDA (function) binder over the GENERATED sort, as in
                        // `\bigcup_{t : \tau} A(t)`, passes a `tau → Prop` body. An
                        // `Exists (fun t : \tau => …)` / `\forall` binder is itself a
                        // PROP, not a tau-valued function — it must NOT type the
                        // operand as `tau`.
                        Expr::Binder(qubitick_model::BinderKind::Lambda, bs, _) => bs
                            .iter()
                            .any(|b| tau_sort_ids.contains(&render_head_name(&b.ty))),
                        _ => genids.contains(&render_head_name(a)),
                    })
                    .collect();

                let z_flags: Vec<bool> = args
                    .iter()
                    .map(|a| match a {
                        Expr::Var(v) => z_vars.contains(v),
                        Expr::Sym(ac) => z_ctor_ids.contains(&ac.primary),
                        Expr::Apply(h, _) => z_ctor_ids.contains(&h.primary),
                        _ => false,
                    })
                    .collect();
                sigs.entry(head.primary).or_default().merge(args, &flags, &z_flags);
            }
            for a in args {
                visit_expr_scoped(a, sigs, sorts, seen, genids, tau_sort_ids, tau_vars, z_ctor_ids, z_sort_ids, z_vars);
            }
        }
        Expr::Bracket(id, args) => {
            seen.insert(*id);
            for a in args {
                visit_expr_scoped(a, sigs, sorts, seen, genids, tau_sort_ids, tau_vars, z_ctor_ids, z_sort_ids, z_vars);
            }
        }
        Expr::Binder(_, binders, body) => {
            // A binder whose type IS the generated sort introduces tau-typed
            // variables; a binder over \mathbb{Z} introduces ℤ-typed ones.
            let mut body_env = tau_vars.clone();
            let mut body_env_z = z_vars.clone();
            for b in binders {
                let tname = render_head_name(&b.ty);
                if z_sort_ids.contains(&tname) {
                    body_env_z.extend(b.vars.iter().cloned());
                }
                if genids.contains(&tname) || tau_sort_ids.contains(&tname)
                {
                    body_env.extend(b.vars.iter().cloned());
                }
            }
            for b in binders {
                // Binder types are sorts; record their head symbol as a sort.
                mark_sort_of(&b.ty, sorts, seen);
                visit_expr_scoped(&b.ty, sigs, sorts, seen, genids, tau_sort_ids, tau_vars, z_ctor_ids, z_sort_ids, z_vars);
            }
            visit_expr_scoped(body, sigs, sorts, seen, genids, tau_sort_ids, &body_env, z_ctor_ids, z_sort_ids, &body_env_z);
        }
        Expr::Recursor(s, cases) => {
            visit_expr_scoped(s, sigs, sorts, seen, genids, tau_sort_ids, tau_vars, z_ctor_ids, z_sort_ids, z_vars);
            for (_, b) in cases {
                visit_expr_scoped(b, sigs, sorts, seen, genids, tau_sort_ids, tau_vars, z_ctor_ids, z_sort_ids, z_vars);
            }
        }
        Expr::Matrix(rows) => {
            for r in rows {
                for c in r {
                    visit_expr_scoped(c, sigs, sorts, seen, genids, tau_sort_ids, tau_vars, z_ctor_ids, z_sort_ids, z_vars);
                }
            }
        }
        Expr::Vector(els) | Expr::Set(els) => {
            for a in els {
                visit_expr_scoped(a, sigs, sorts, seen, genids, tau_sort_ids, tau_vars, z_ctor_ids, z_sort_ids, z_vars);
            }
        }
    }
}

/// Record `e`'s head symbol as a sort (it is used as a binder type).
fn mark_sort_of(e: &Expr, sorts: &mut HashSet<u32>, seen: &mut HashSet<u32>) {
    let id = match e {
        Expr::Sym(ac) => ac.primary,
        Expr::Apply(h, _) => h.primary,
        _ => return,
    };
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
    let mut all: Vec<Node> = Vec::with_capacity(context.len() + own.len());
    all.extend_from_slice(context);
    all.extend_from_slice(own);
    emit_lean_text_impl(ctx, &all, own, namespace, def_index, thm_index, rules, &Options::inline())
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
            let id = h.primary;
            texes_with(Facet::LeftIff)
                .chain(texes_with(Facet::RightIff))
                .any(|t| {
                    ctx.by_tex(t)
                        .and_then(|rows| rows.first())
                        .is_some_and(|r| r.id as u32 == id)
                })
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
            let id = h.primary;
            texes_with(Facet::LeftIff)
                .chain(texes_with(Facet::RightIff))
                .any(|t| {
                    ctx.by_tex(t)
                        .and_then(|rows| rows.first())
                        .is_some_and(|r| r.id as u32 == id)
                })
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
            let sort_of = |v: &str| -> String {
                use qubitick_model::Expr as E;
                fn walk(e: &E, v: &str, resolver: &impl Fn(u32) -> String) -> Option<String> {
                    match e {
                        E::Apply(h, args) => {
                            if let Some(_i) = args.iter().position(|a| matches!(a, E::Var(x) if x == v))
                            {
                                let n = resolver(h.primary);
                                if n.contains("mathbb_Z") {
                                    return Some("mathbb_Z".into());
                                }
                                if n.contains("_tau") {
                                    return Some("tau".into());
                                }
                            }
                            args.iter().find_map(|a| walk(a, v, resolver))
                        }
                        E::Binder(_, bs, b) => {
                            if bs.iter().any(|bd| bd.vars.iter().any(|x| x == v)) {
                                None
                            } else {
                                walk(b, v, resolver)
                            }
                        }
                        E::Bracket(_, els) => els.iter().find_map(|a| walk(a, v, resolver)),
                        E::Recursor(sc, cases) => walk(sc, v, resolver)
                            .or_else(|| cases.iter().find_map(|(_, b)| walk(b, v, resolver))),
                        E::Matrix(rows) => rows
                            .iter()
                            .find_map(|r| r.iter().find_map(|x| walk(x, v, resolver))),
                        E::Vector(elems) | E::Set(elems) => {
                            elems.iter().find_map(|x| walk(x, v, resolver))
                        }
                        E::Sym(_) | E::Var(_) => None,
                    }
                }
                walk(&s, v, &|id| crate::resolver(ctx, &fn_names)(id))
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
        seen: frame_seen,
    } = symbol_declarations(ctx, &collect, emit_nodes, &fn_names, opts);
    let mut body = String::new();
    if inline {
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
        chain_body.push_str("class ISA where\n-- framework symbols (home = None)\nE : Type\n\n");
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
    if !opts.ground_only {
        // A node module needs the class before the theorems that live in it.
        body.push_str(&chain_body);

    }

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
                // The frame declares the generated environments itself (the tick
                // sequence, the integer sequence, a formation's carrier and
                // constructors). A definition node that names one of them would
                // declare it a second time.
                if frame_declared.contains(&definition_decl_name(ctx, d, &fn_names)) {
                    continue;
                }
                // The ℤ inductive + constructor aliases (emitted in the frame by
                // symbol_declarations) ARE the definitions of these glyphs; an own
                // `def mathbb_Z : Type := Prop` / macro would collide with it.
                let dname = crate::lean_name_of(ctx, d.symbol.primary);
                if matches!(
                    dname.as_str(),
                    "mathbb_Z" | "mathbb_Z___0" | "mathbb_Z__plus" | "mathbb_Z__minus"
                ) {
                    continue;
                }
                body.push_str(&crate::definition::emit_definition_in(
                    ctx,
                    d,
                    frame_sorts.contains(&d.symbol.primary),
                    frame_arity.get(&d.symbol.primary).copied().unwrap_or(0),
                    &fn_names,
                    &frame_sorts,
                    &formations,
                    &known_types,
                    // The same permission set the frame uses when IT emits a
                    // recursor def: every symbol this module mentions may appear in
                    // the recursive bodies. Without it the node path refused the
                    // recursor (`+_{\mathbb{Z}^{+}}` references `\uparrow`), the
                    // glyph degraded to an axiom, and every `[\equiv]` row that
                    // closed by kernel conversion stopped closing.
                    &frame_seen,
                )?);
                body.push('\n');
            }
            // Classes come from the chain above; an axiom contributes nothing here.
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
        // A ground's own definitions came first; the chain that MENTIONS them
        // follows them — and so do their characterising axioms, which name them.
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
    // identifier becomes a fresh implicit variable — so a module that never declares
    // `\mathbb{Z}` or `tau` still checks, with every mention a *different* variable
    // and the corpus's sorts hypothetical. Turning it off is what makes the per-file
    // check a check of the frame rather than of a guess.
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
) -> Result<String> {
    let mut all: Vec<Node> = Vec::with_capacity(context.len() + own.len());
    all.extend_from_slice(context);
    all.extend_from_slice(own);
    let opts = Options {
        ground: Ground::External,
        ground_module: ground_module.to_string(),
        declare_cited,
        extra_imports,
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
        let lean = qubitick_lean::render(
            &reduced,
            &crate::resolver_with_sorts(ctx, fn_names, sorts),
        );
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
            // Prop-SORTED, which is the question this predicate answers: no tau/Z/E sort appears
            "(Prop → Prop)",
            "  Prop  ",
        ] {
            assert!(prop_only_type(ty), "{ty} is the Prop carrier");
        }
        for ty in [
            "",
            "Type",
            "tau → Prop",
            "mathbb_Z → Prop",
            "Prop → tau",
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

    #[test]
    fn emits_class_chain_shell() {
        let ctx = Ctx::from_rows("Era0/Strata0", vec![]);
        let a1 = Node::Axiom(axiom(1, "Realm", 1));
        let a2 = Node::Axiom(axiom(2, "Capacity", 1));
        let out = emit_lean_text(&ctx, &[a1, a2], "Qubitick.HL", &mut RuleTable::new()).unwrap();
        assert!(out.contains("class ISA"));
        assert!(out.contains("class Realm extends ISA"));
        assert!(out.contains("class Capacity extends Realm"));
    }
}

