//! Theorem emission (CodeArch §5.11, Document 5 §5.7/§6.3).

use anyhow::Result;
use qubitick_ctx::Ctx;
use qubitick_model::corpus::{texes_with, Facet};
use qubitick_model::{AtomicComposite, EQ_SYM, Expr, Theorem};
/// The idents an expression uses WITHOUT binding them.
///
/// A corpus theorem may state its parameters free (`T.BT.61`'s statement is just an
/// equation in `a b c d`), so nothing in the statement says they exist. Everything
/// a binder introduces — at any depth, including the `∀` a statement does carry —
/// is excluded.
pub(crate) fn free_idents(e: &Expr) -> Vec<String> {
    fn go(e: &Expr, bound: &mut Vec<String>, out: &mut Vec<String>) {
        match e {
            Expr::Sym(_) => {}
            Expr::Var(v) => {
                if !bound.contains(v) && !out.contains(v) {
                    out.push(v.clone());
                }
            }
            Expr::Apply(_, args) | Expr::Bracket(_, args) | Expr::Vector(args)
            | Expr::Set(args) => {
                for a in args {
                    go(a, bound, out);
                }
            }
            Expr::Binder(_, bs, body) => {
                for b in bs {
                    go(&b.ty, bound, out);
                }
                let mut pushed = 0usize;
                for b in bs {
                    for v in &b.vars {
                        if !bound.contains(v) {
                            bound.push(v.clone());
                            pushed += 1;
                        }
                    }
                }
                go(body, bound, out);
                for _ in 0..pushed {
                    bound.pop();
                }
            }
            Expr::Recursor(sc, cases) => {
                go(sc, bound, out);
                for (_, b) in cases {
                    go(b, bound, out);
                }
            }
            Expr::Matrix(rows) => {
                for r in rows {
                    for c in r {
                        go(c, bound, out);
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    go(e, &mut Vec::new(), &mut out);
    out
}

/// The sort a free ident is read at, taken from the operator that applies it.
///
/// `a` in `a +_{\mathbb{Z}} b` is a `\mathbb{Z}` element because `+_{\mathbb{Z}}`
/// is; an ident no Z/τ operator applies stays the carrier. This is the same rule
/// that types the assumed lemmas a citing module declares, so a theorem and the
/// lemmas quoting it agree.
pub(crate) fn free_var_sort(
    ctx: &Ctx,
    e: &Expr,
    v: &str,
    fn_names: &std::collections::HashMap<u32, String>,
) -> String {
    fn walk(e: &Expr, v: &str, resolver: &impl Fn(u32) -> String) -> Option<String> {
        match e {
            Expr::Apply(h, args) => {
                if args.iter().any(|a| matches!(a, Expr::Var(x) if x == v)) {
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
            Expr::Binder(_, bs, b) => {
                if bs.iter().any(|bd| bd.vars.iter().any(|x| x == v)) {
                    None
                } else {
                    walk(b, v, resolver)
                }
            }
            Expr::Bracket(_, els) => els.iter().find_map(|a| walk(a, v, resolver)),
            Expr::Recursor(sc, cases) => walk(sc, v, resolver)
                .or_else(|| cases.iter().find_map(|(_, b)| walk(b, v, resolver))),
            Expr::Matrix(rows) => rows
                .iter()
                .find_map(|r| r.iter().find_map(|x| walk(x, v, resolver))),
            Expr::Vector(elems) | Expr::Set(elems) => {
                elems.iter().find_map(|x| walk(x, v, resolver))
            }
            Expr::Sym(_) | Expr::Var(_) => None,
        }
    }
    walk(e, v, &|id| crate::resolver(ctx, fn_names)(id)).unwrap_or_else(|| "Prop".into())
}

/// The three A1 rows the nullity bridge needs, found by SHAPE.
///
/// By shape and not by index: the field numbers are cumulative across the era's
/// whole axiom chain (`A1 → ax_0.., A2 → ax_12..`), so a hardcoded `ax_2` would
/// be right for one era and wrong for the next.
struct NullityParts {
    /// The statement's two sides, rendered.
    lhs: String,
    rhs: String,
    /// The rendered names of the Δ operator and of `\varnothing`.
    delta: String,
    null: String,
    /// `Δ (Δ a b) c = Δ a (Δ b c)` — field index.
    assoc: usize,
    /// `Δ a ∅ = a` — field index.
    vanish: usize,
    /// `Δ a a = ∅` — field index.
    self_delta: usize,
}

impl NullityParts {
    /// The bridge term: `l = r` from `h : Δ l r = ∅`.
    ///
    /// `l = Δ l ∅ = Δ l (Δ l r) = Δ (Δ l l) r = Δ ∅ r = Δ (Δ r r) r = Δ r (Δ r r)
    /// = Δ r ∅ = r`, each step one of A1's rows (associativity, the `∅` identity,
    /// the self-inverse) under `congrArg`. It is a DERIVATION, not an axiom: the
    /// corpus states `l = r` and proves `Δ l r = ∅`, and these rows are what makes
    /// the two the same proposition.
    ///
    /// Built as a fold rather than one literal, because the nesting is seven
    /// `Eq.trans` deep and a hand-counted parenthesis is a syntax error waiting to
    /// happen.
    fn term(&self, chain: &str) -> String {
        let (l, r, d, z) = (&self.lhs, &self.rhs, &self.delta, &self.null);
        let (a, v, s) = (self.assoc, self.vanish, self.self_delta);
        // Every spliced term is wrapped: `Delta a b` bare in an argument position
        // would parse as THREE arguments (`@Eq.trans _ Delta a b …`).
        let lp = format!("({l})");
        let rp = format!("({r})");
        let dlz = format!("({d} {lp} {z})");
        let dlr = format!("({d} {lp} {rp})");
        let dldlr = format!("({d} {lp} {dlr})");
        let dllr = format!("({d} ({d} {lp} {lp}) {rp})");
        let dzr = format!("({d} {z} {rp})");
        let drr = format!("({d} {rp} {rp})");
        let ddrrr = format!("({d} {drr} {rp})");
        let drdrr = format!("({d} {rp} {drr})");
        let drz = format!("({d} {rp} {z})");
        let steps: Vec<(String, String, String)> = vec![
            (lp.clone(), dlz.clone(), format!("Eq.symm (R.ax_{v} {lp})")),
            (
                dlz.clone(),
                dldlr.clone(),
                format!("@congrArg _ _ _ _ (fun x => {d} {lp} x) (Eq.symm ({chain}))"),
            ),
            (
                dldlr.clone(),
                dllr.clone(),
                format!("Eq.symm (R.ax_{a} {lp} {lp} {rp})"),
            ),
            (
                dllr.clone(),
                dzr.clone(),
                format!("@congrArg _ _ _ _ (fun x => {d} x {rp}) (R.ax_{s} {lp})"),
            ),
            (
                dzr.clone(),
                ddrrr.clone(),
                format!("@congrArg _ _ _ _ (fun x => {d} x {rp}) (Eq.symm (R.ax_{s} {rp}))"),
            ),
            (
                ddrrr.clone(),
                drdrr.clone(),
                format!("R.ax_{a} {rp} {rp} {rp}"),
            ),
            (
                drdrr.clone(),
                drz.clone(),
                format!("@congrArg _ _ _ _ (fun x => {d} {rp} x) (R.ax_{s} {rp})"),
            ),
            (drz.clone(), rp.clone(), format!("R.ax_{v} {rp}")),
        ];
        let last = steps.len() - 1;
        // Every `Eq.trans` in the fold runs from its own step to the SAME end term:
        // it composes `a → b` with the chain already built, whose type is `b → end`.
        let end = steps[last].1.clone();
        let mut acc = format!("({})", steps[last].2);
        for i in (0..last).rev() {
            acc = format!(
                "(@Eq.trans _ {} {} {} ({}) {acc})",
                steps[i].0, steps[i].1, end, steps[i].2
            );
        }
        acc
    }
}

/// Everything the bridge needs, or `None` when it does not apply.
///
/// It applies only to a stated `l = r` whose sides the nullity mapping leaves
/// alone (a nested `=` inside a side would move the goalposts) and whose class
/// chain actually carries A1's three rows.
fn nullity_parts(
    ctx: &Ctx,
    statement: &Expr,
    fields: &[Expr],
    fn_names: &std::collections::HashMap<u32, String>,
    sorts: &std::collections::HashSet<u32>,
) -> Option<NullityParts> {
    // The corpus's own nullity operator and empty element: whatever glyphs its
    // declarations marked, which is the only thing the engine may go on.
    let delta = ctx.declared_with_facet(Facet::Nullity)?.id as u32;
    let null = ctx.declared_with_facet(Facet::Null)?.id as u32;
    // Look THROUGH the statement's own quantifiers: `T.BT.01` states
    // `∀ a b, a Δ b = b Δ a`, and the equation the bridge connects is inside.
    let mut body = statement;
    while let Expr::Binder(_, _, inner) = body {
        body = inner;
    }
    let Expr::Apply(h, args) = body else {
        return None;
    };
    if h.primary != EQ_SYM || args.len() != 2 {
        return None;
    }
    let (l, r) = (&args[0], &args[1]);
    // The bridge connects the two sides THEMSELVES, so the nullity mapping must
    // leave them where they are — a nested `=` inside a side would move the goal.
    if map_nullity(l, delta, null) != *l || map_nullity(r, delta, null) != *r {
        return None;
    }
    let is_delta = |e: &Expr| matches!(e, Expr::Apply(d, a) if d.primary == delta && a.len() == 2);
    let is_null = |e: &Expr| matches!(e, Expr::Sym(a) if a.primary == null);
    let var_of = |e: &Expr| match e {
        Expr::Var(v) => Some(v.clone()),
        _ => None,
    };
    let mut assoc = None;
    let mut vanish = None;
    let mut self_delta = None;
    for (i, f) in fields.iter().enumerate() {
        // A class field is quantified (`∀ a, Eq (Delta a varnothing) a`); the
        // equation is what says which row it is.
        let mut f = f;
        while let Expr::Binder(_, _, inner) = f {
            f = inner;
        }
        let Expr::Apply(eq, fa) = f else { continue };
        if eq.primary != EQ_SYM || fa.len() != 2 {
            continue;
        }
        let (a, b) = (&fa[0], &fa[1]);
        // `Δ (Δ x y) z = Δ x (Δ y z)` — read structurally, with x, y, z pinned:
        // LHS `Δ (Δ x y) z` is `Apply(Δ, [Apply(Δ, [x, y]), z])` and RHS `Δ x (Δ y z)`
        // is `Apply(Δ, [x, Apply(Δ, [y, z])])`.
        if let (Expr::Apply(d1, lin), Expr::Apply(d2, rin)) = (a, b)
            && d1.primary == delta
            && d2.primary == delta
            && let Expr::Apply(d3, li) = &lin[0]
            && d3.primary == delta
            && let Expr::Apply(d4, ri) = &rin[1]
            && d4.primary == delta
        {
            let x = var_of(&rin[0]);
            let y = var_of(&li[1]);
            let z = var_of(&lin[1]);
            if x.is_some()
                && z.is_some()
                && var_of(&li[0]) == x
                && var_of(&ri[0]) == y
                && var_of(&ri[1]) == z
                && x != y
            {
                assoc.get_or_insert(i);
                continue;
            }
        }
        // `Δ x ∅ = x`
        if is_delta(a)
            && let Expr::Apply(_, da) = a
            && is_null(&da[1])
            && var_of(&da[0]).is_some()
            && var_of(&da[0]) == var_of(b)
        {
            vanish.get_or_insert(i);
            continue;
        }
        // `Δ x x = ∅`
        if is_delta(a)
            && let Expr::Apply(_, da) = a
            && var_of(&da[0]).is_some()
            && var_of(&da[0]) == var_of(&da[1])
            && is_null(b)
        {
            self_delta.get_or_insert(i);
        }
    }
    // The sides are rendered exactly as the type is: the same resolver, so a sort
    // glyph collapsed to the carrier in the statement is collapsed here too.
    let resolver = crate::resolver_with_sorts(ctx, fn_names, sorts);
    Some(NullityParts {
        lhs: qubitick_lean::render(l, &resolver),
        rhs: qubitick_lean::render(r, &resolver),
        delta: resolver(delta),
        null: resolver(null),
        assoc: assoc?,
        vanish: vanish?,
        self_delta: self_delta?,
    })
}

/// Emit a theorem's statement + proof as Lean text via the rewrite engine.
///
/// `class` is the section-class variable (from the module's class chain).
#[allow(clippy::too_many_arguments)]
pub fn emit_theorem(
    ctx: &Ctx,
    theorem: &Theorem,
    namespace: &str,
    class: &str,
    rules: &qubitick_rewrite::RuleTable,
    fn_names: &std::collections::HashMap<u32, String>,
    sorts: &std::collections::HashSet<u32>,
    relational_iffs: &std::collections::HashMap<u32, (Vec<String>, Expr)>,
    class_fields: &[Expr],
    // Whether `class` is actually declared in the environment this module sees
    // (`false` when the module has no axiom chain of its own and imports none).
    class_declared: bool,
) -> Result<(String, Expr)> {
    // Only apply nullity form if the proof chain explicitly ends at \varnothing
    let is_nullity = match &theorem.proof.last() {
        Some(qubitick_model::ProofStep::Chain { rows }) => rows.last().is_some_and(|r| {
            if let Expr::Sym(ac) = &r.rhs {
                ctx.by_id(ac.primary as i64)
                    .is_some_and(|row| ctx.has_facet(&row.tex, Facet::Null))
            } else {
                false
            }
        }),
        _ => false,
    };
    // The nullity-form (x = ∅ ↦ Δ x ∅ = ∅) applies when the PROOF runs through
    // the Δ-encoding: its first row's lhs is itself a Δ-application (T_BT_01/04's
    // `(a∩b) Δ (b∩a) = …`). A pure ∩-chain ending at ∅ (T_BT_07's ledger
    // disjointness) keeps the plain goal — Δ-anchoring it would mismatch the
    // chain's raw-∩ lhs rows.
    let is_nullity = is_nullity
        && match &theorem.proof.first() {
            Some(qubitick_model::ProofStep::Chain { rows }) => rows
                .first()
                .and_then(|r| r.lhs.as_ref())
                .is_some_and(|lhs| {
                    ctx.declared_with_facet(Facet::Nullity).is_some_and(|r| {
                        matches!(lhs, Expr::Apply(h, _) if h.primary == r.id as u32)
                    })
                }),
            _ => false,
        };

    // The nullity encoding is a device of the PROOF, not of the stated statement.
    // The theorem states `l = r`; its proof establishes `Δ l r = ∅`, which the
    // corpus's nullity-anchored logic reads as the same proposition. Declaring the
    // theorem IN the nullity form made that visible to Lean but unusable to anything
    // citing it: a citing proof writes plain rows (`Δ ∅ a → Δ a ∅`) and the
    // nullity-form def cannot be applied to them. So the def keeps the statement the
    // corpus states, and the proof carries the encoding — the chain (compiled
    // against the nullity proposition) followed by the bridge A1's own rows provide.
    let (statement, bridge) =
        match nullity_parts(ctx, &theorem.statement, class_fields, fn_names, sorts) {
        Some(parts) if is_nullity => {
            let chain_stmt =
                unfold_relational(&nullity_form(ctx, &theorem.statement), ctx, relational_iffs);
            let chain =
                qubitick_rewrite::compile_steps(ctx, &chain_stmt, &theorem.proof, rules, fn_names)?;
            (
                unfold_relational(&theorem.statement, ctx, relational_iffs),
                Some(parts.term(&chain)),
            )
        }
        _ => {
            let statement = if is_nullity {
                nullity_form(ctx, &theorem.statement)
            } else {
                theorem.statement.clone()
            };
            (unfold_relational(&statement, ctx, relational_iffs), None)
        }
    };
    // Sort glyphs render as the carrier `Prop`, matching the operator declarations.
    let resolver = crate::resolver_with_sorts(ctx, fn_names, sorts);
    let stmt = qubitick_lean::render(&statement, &resolver);

    // The proof: the bridge when the nullity encoding was used, the chain otherwise.
    //
    // AN ADMITTED THEOREM HAS NO PROOF TO COMPILE, and it is not a mistake to emit one: the
    // corpus says "stated, not yet proved" by carrying no steps (`Proof:` absent, empty, or
    // `\sorry` — all three parse to an empty vector). It is emitted as an AXIOM below, which
    // is why nothing is compiled here. The alternative this replaced — an empty chain
    // compiled to `Eq.refl`, written out as `fun a => Eq.refl _` — proved every statement
    // that happened to be a definitional identity and reported it `verified`.
    let admitted = theorem.proof.is_empty();
    let proof = if admitted {
        String::new()
    } else {
        match &bridge {
            Some(bridge) => bridge.clone(),
            None => qubitick_rewrite::compile_steps(ctx, &statement, &theorem.proof, rules, fn_names)?,
        }
    };

    let quantifiers = outer_binder_vars(ctx, &statement);
    // A statement that leaves its parameters FREE binds them here. Without this the
    // emitted `def N : <stmt> := <proof>` let Lean's `autoImplicit` capture `a b c d`
    // as IMPLICIT variables, so the definition was a nullary proposition — and a
    // proof that APPLIES it (`IntAddInterchange (…)`) failed with `Unknown constant
    // CoeFun`, which then made every later citation of that theorem an `Unknown
    // identifier`. A citing module never saw this: it declares the cited theorem as
    // an assumed lemma WITH explicit `∀` binders. A scope module, which applies the
    // real `def`, needs the theorem to bind its own parameters.
    let free: Vec<(String, String)> = free_idents(&statement)
        .into_iter()
        .filter(|v| !quantifiers.contains(v))
        .map(|v| {
            let sort = free_var_sort(ctx, &statement, &v, fn_names);
            (v, sort)
        })
        .collect();
    let stmt = if free.is_empty() {
        stmt
    } else {
        let binders = free
            .iter()
            .map(|(v, t)| format!("∀ ({v} : {t}), "))
            .collect::<String>();
        format!("{binders}{stmt}")
    };
    let mut params: Vec<String> = free.iter().map(|(v, _)| v.clone()).collect();
    params.extend(quantifiers.iter().cloned());
    let intro = if params.is_empty() {
        String::new()
    } else {
        format!("fun {} => ", params.join(" "))
    };

    let body = if admitted {
        /* AN ADMITTED THEOREM IS AN AXIOM OF ITS OWN MODULE, not a hidden `sorry`.
           Two reasons, and they are the answer to "how do we express sorry in a ground with
           no axioms": declaring Lean's `sorryAx` in `Core.lean`/`HL.lean` would put a
           prove-ANYTHING axiom in the shipped ground (a ground that is deliberately
           axiom-free: no propext, no funext, no Classical.choice, no Quot.sound), whereas
           `axiom <Name> : <statement>` assumes exactly the one statement the author has not
           proved — local, named, and countable by `#print axioms`, and the build reports it
           (`proof.admitted`, and the report's `admitted` count). */
        format!("axiom {} : {stmt}\n", theorem.base.name)
    } else if proof.contains('\n') {
        let mut b = format!("def {} : {stmt} :=\n", theorem.base.name);
        for (i, line) in proof.lines().enumerate() {
            b.push_str("  ");
            if i == 0 {
                b.push_str(&intro);
            }
            b.push_str(line);
            b.push('\n');
        }
        b
    } else {
        format!(
            "def {} : {stmt} :=\n  {intro}{proof}\n",
            theorem.base.name
        )
    };
    let _ = namespace;
    // The statement is returned WITH the text: a citation of this theorem has to be
    // stated the way the theorem was emitted (the nullity form, the unfolded
    // relation), or the proof that applies it builds a term of the wrong type —
    // `DeltaComm ∅ a : Eq (Delta (Delta ∅ a) (Delta a ∅)) ∅` where the rule said
    // `Eq (Delta ∅ a) (Delta a ∅)`.
    // A module with no axiom chain at all (an era whose own axioms live in an
    // earlier era, and which imports none) has no class to hypothesize: emitting the
    // section anyway named a class that was never declared, and `{R : ISA}` let
    // Lean's `autoImplicit` bind it to a fresh variable instead.
    if !class_declared {
        return Ok((body, statement));
    }
    // `[R : Realm]` and not `{R : Realm}`: the class hypothesis is an INSTANCE, so
    // Lean's instance synthesis finds it at every use site. An ordinary implicit is
    // only ever resolved by unification against the expected type, and a theorem
    // whose proof cites an axiom field carries `R` in its own type while its
    // STATEMENT does not — so a citation of it (`DeltaComm l varnothing` in another
    // proof) could not supply `R` and failed with `don't know how to synthesize
    // implicit argument R`.
    Ok((
        format!("section\nvariable [R : {class}]\n\n{}end\n", body),
        statement,
    ))
}

/// Replace applications of iff-defined relational glyphs (`R X Y` where
/// `R a b \leftrightarrow body`) by `body` with `a↦X, b↦Y`. Relational heads in
/// a theorem STATEMENT become the body the proof-chain actually reasons over.
pub fn unfold_relational(
    e: &Expr,
    ctx: &Ctx,
    iff_map: &std::collections::HashMap<u32, (Vec<String>, Expr)>,
) -> Expr {
    use qubitick_model::Binder;
    let is_sort_atom = |a: &Expr| -> bool {
        use qubitick_model::Expr as E;
        let head = match a {
            E::Sym(ac) => ac.primary,
            E::Apply(ac, _) => ac.primary,
            _ => return false,
        };
        texes_with(Facet::Sort).any(|t| {
            ctx.by_tex(t)
                .and_then(|rows| rows.first())
                .is_some_and(|r| r.id as u32 == head)
        })
    };
    match e {
        Expr::Apply(h, args) => {
            if let Some((params, body)) = iff_map.get(&h.primary) {
                // A SORT-typed operand (`\mathbb{Z} ⊆ \mathbb{Q}`) must NOT unfold
                // to the Prop-\cap form — that builds `\mathbb{Z} ∩ \mathbb{Q}`
                // over a Type. Keep set-level inclusions opaque.
                if params.len() == args.len() && !args.iter().any(is_sort_atom) {
                    let subs: Vec<(String, Expr)> =
                        params.iter().cloned().zip(args.iter().cloned()).collect();
                    let unfolded = qubitick_pattern::expr_subst(body, &subs);
                    // Avoid infinite idempotence: don't re-unfold the SAME head that
                    // just produced this body (the body may re-reference the glyph).
                    if unfolded != *e {
                        return unfolded;
                    }
                }
            }
            Expr::Apply(
                h.clone(),
                args.iter().map(|a| unfold_relational(a, ctx, iff_map)).collect(),
            )
        }
        Expr::Binder(k, binders, body) => Expr::Binder(
            *k,
            binders
                .iter()
                .map(|b| Binder {
                    vars: b.vars.clone(),
                    ty: unfold_relational(&b.ty, ctx, iff_map),
                })
                .collect(),
            Box::new(unfold_relational(body, ctx, iff_map)),
        ),
        Expr::Bracket(bb, els) => Expr::Bracket(
            *bb,
            els.iter().map(|x| unfold_relational(x, ctx, iff_map)).collect(),
        ),
        other => other.clone(),
    }
}

/// The variables bound by a statement's OUTER `∀`/`∃` binders, in order.
///
/// Only the leading quantifiers are collected: those are the ones a proof term
/// must abstract over to see the corpus's metavariables (`\forall a b : E, …`).
/// A binder nested inside the proposition (e.g. the `\exists t` in A1.9) is part
/// of the goal and is introduced by the relevant proof step instead.
fn outer_binder_vars(ctx: &Ctx, e: &Expr) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = e;
    // Leading `∀` binders first (they are the corpus metavariables a proof term
    // abstracts over).
    while let Expr::Binder(_, binders, body) = cur {
        for b in binders {
            out.extend(b.vars.iter().cloned());
        }
        cur = body;
    }
    // Then each leading implication premise is introduced as a hypothesis proof in
    // the SAME names the proof compiler notes (`h`, `h1`, … — see
    // rewrite/compile.rs), so a proof citing `[h]` resolves. `a = b \to c` is
    // bound `fun a b h => …` with `h : Eq a b`.
    let is_arrow = |id: u32, resolved: &str| -> bool {
        id == 0
            || matches!(
                resolved,
                "to" | "rightarrow" | "Longrightarrow" | "implies" | "impl"
            )
            || texes_with(Facet::RewriteArrow).any(|t| {
                ctx.by_tex(t)
                    .and_then(|rows| rows.first())
                    .is_some_and(|r| r.id as u32 == id)
            })
    };
    let mut k = 0usize;
    while let Expr::Apply(h, args) = cur {
        let resolved = ctx
            .by_id(h.primary as i64)
            .map(|r| r.lean_name.clone())
            .unwrap_or_default();
        if args.len() == 2 && is_arrow(h.primary, &resolved) {
            out.push(if k == 0 { "h".to_string() } else { format!("h{k}") });
            k += 1;
            cur = &args[1];
        } else {
            break;
        }
    }
    out
}

/// Emit a corpus `=` in its nullity-anchored form.
///
/// The corpus's equality is nullity-anchored — `x = y` ⟺ `x Δ y = ∅`, as
/// `prelude/Qubitick/Core.lean` states ("Canonically equality is nullity-anchored
/// ... a Δ a = ∅, a Δ ∅ = a are A1 rows"). So a theorem's stated equality is
/// emitted as `Eq (Delta l r) varnothing`, which is exactly what the corpus
/// proof chains establish (they run to `∅`).
fn nullity_form(ctx: &Ctx, e: &Expr) -> Expr {
    // Nullity-anchored equality is `x Δ y = ∅`: the corpus's nullity op and its
    // empty element, each found by the declaration that marked it.
    let delta = ctx.declared_with_facet(Facet::Nullity).map(|r| r.id as u32);
    let null = ctx.declared_with_facet(Facet::Null).map(|r| r.id as u32);
    match (delta, null) {
        (Some(delta), Some(null)) => map_nullity(e, delta, null),
        _ => e.clone(),
    }
}

fn map_nullity(e: &Expr, delta: u32, null: u32) -> Expr {
    match e {
        Expr::Apply(h, args) if h.primary == EQ_SYM && args.len() == 2 => {
            let l = map_nullity(&args[0], delta, null);
            let r = map_nullity(&args[1], delta, null);
            let diff = Expr::Apply(AtomicComposite::new(delta), vec![l, r]);
            Expr::Apply(
                AtomicComposite::new(EQ_SYM),
                vec![diff, Expr::Sym(AtomicComposite::new(null))],
            )
        }
        Expr::Binder(k, binders, body) => Expr::Binder(
            *k,
            binders.clone(),
            Box::new(map_nullity(body, delta, null)),
        ),
        _ => e.clone(),
    }
}

/// Mark a test fixture row as DECLARED with a facet — what a corpus node's
/// `Role:` does. A use-only row classifies nothing, so a fixture that wants the
/// rewriter to find the corpus's nullity operator must declare it.
#[cfg(test)]
fn declaring(
    mut row: qubitick_db::queries::SymbolRow,
    facet: qubitick_model::corpus::Facet,
) -> qubitick_db::queries::SymbolRow {
    row.facets |= facet.bits();
    row
}

#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_model::{AtomicComposite, Expr, NodeBase};

    /// A one-row reflexive chain — a proof that exists, for the tests that are about the `def`
    /// form. A theorem with NO rows is an admitted one and is emitted as an axiom (see
    /// `an_admitted_theorem_is_an_axiom_and_not_a_placeholder`), so a test that wants a proof
    /// term has to write one.
    fn reflexive(e: Expr) -> Vec<qubitick_model::ProofStep> {
        vec![qubitick_model::ProofStep::Chain {
            rows: vec![qubitick_model::ChainRow {
                line: 1,
                lhs: None,
                rhs: e,
                rel: qubitick_model::StepRel::Eq,
                just: qubitick_model::ChainJust::Rfl,
            }],
        }]
    }

    fn base() -> NodeBase {
        NodeBase {
            id: 1,
            kind: qubitick_model::NodeKind::Theorem,
            era: qubitick_model::Era::Era0,
            strata: qubitick_model::Strata(0),
            name: "Thm1".into(),
            req: vec![],
            derived: vec![],
            note: None,
        }
    }

    #[test]
    fn emits_theorem() {
        let ctx = Ctx::from_rows("Era0/Strata0", vec![]);
        let thm = Theorem {
            base: base(),
            statement: Expr::Sym(AtomicComposite::new(1)),
            proof: reflexive(Expr::Sym(AtomicComposite::new(1))),
            tag: qubitick_model::PhysOrMath::Math,
        };
        let (out, _) = emit_theorem(
            &ctx,
            &thm,
            "Qubitick.HL",
            "ISA",
            &qubitick_rewrite::RuleTable::new(),
            &Default::default(),
            &Default::default(),
            &std::collections::HashMap::new(),
            // No class chain and no class in scope: the test theorems are pure
            // `Eq` statements with no axiom field to cite.
            &[],
            true,
        )
        .unwrap();
        assert!(out.contains("def Thm1"));
        // `[R : ISA]`, not `{R : ISA}`: the class hypothesis is an INSTANCE, so a
        // citation of a theorem whose proof cites an axiom field can synthesize it.
        assert!(out.contains("variable [R : ISA]"), "{out}");
    }

    /// A proof term must BIND the theorem's `∀`-bound variables.
    ///
    /// Regression: the compiled chain was emitted as a bare term, so Lean reported
    /// `Unknown identifier 'a'` on the first occurrence of any metavariable —
    /// the theorem's own binder never entered the proof's scope.
    #[test]
    fn proof_term_binds_the_outer_quantified_variables() {
        use qubitick_db::queries::SymbolRow;
        let row = |id: i64, tex: &str| SymbolRow { facets: 0,
            id,
            scope: "Era0/Strata0".into(),
            tex: tex.into(),
            display: tex.into(),
            lean_name: tex.trim_start_matches('\\').to_string(),
            trigger: None,
            kind: "notation".into(),
            role: None,
            precedence: None,
            assoc: None,
            home_node: 1,
            elab: None,
            binder_kind: None,
        };
        let ctx = Ctx::from_rows(
            "Era0/Strata0",
            vec![declaring(row(5, "\\Delta"), Facet::Nullity)],
        );
        let b = qubitick_model::Binder {
            vars: vec!["a".into(), "b".into()],
            ty: Expr::var("E"),
        };
        let body = Expr::Apply(
            AtomicComposite::new(5),
            vec![Expr::var("a"), Expr::var("b")],
        );
        let thm = Theorem {
            base: base(),
            statement: Expr::Binder(qubitick_model::BinderKind::Forall, vec![b], Box::new(body.clone())),
            proof: reflexive(body),
            tag: qubitick_model::PhysOrMath::Math,
        };
        let (out, _) = emit_theorem(
            &ctx,
            &thm,
            "Qubitick.HL",
            "ISA",
            &qubitick_rewrite::RuleTable::new(),
            &Default::default(),
            &Default::default(),
            &std::collections::HashMap::new(),
            // No class chain and no class in scope: the test theorems are pure
            // `Eq` statements with no axiom field to cite.
            &[],
            false,
        )
        .unwrap();
        // `E` is free in this statement (it appears only as the binder `a`/`b`'s
        // sort), so it is bound here too, ahead of the statement's own `∀ a b`. The
        // proof term must abstract it: an unbound name would be captured by Lean's
        // `autoImplicit` as an implicit variable, and the definition would become a
        // nullary proposition that no proof could apply.
        assert!(
            out.contains("fun E a b =>"),
            "proof term must abstract the quantified and free variables:\n{out}"
        );
        assert!(
            !out.contains("by rfl"),
            "a bare-prelude module cannot use tactic syntax:\n{out}"
        );
    }

    /// AN ADMITTED THEOREM IS AN AXIOM, NOT A PLACEHOLDER PROOF.
    ///
    /// The corpus says "stated, not yet proved" by carrying no steps — `Proof:` absent, empty,
    /// or `\sorry`. What the emitter used to write for that was `fun a => Eq.refl _`, which
    /// proved every statement that happened to be a definitional identity (so `a Δ ∅ = a`
    /// reported `verified` with no proof at all) and failed on everything else with a Lean
    /// `Type mismatch` that never mentioned the missing proof. Now the statement is assumed:
    /// one axiom, named, local to the module — so the ground stays axiom-free (`Core.lean`/
    /// `HL.lean` deliberately declare no `sorryAx`) and the build reports it as admitted.
    #[test]
    fn an_admitted_theorem_is_an_axiom_and_not_a_placeholder() {
        let ctx = Ctx::from_rows("Era0/Strata0", vec![]);
        let thm = Theorem {
            base: base(),
            statement: Expr::Sym(AtomicComposite::new(1)),
            proof: vec![],
            tag: qubitick_model::PhysOrMath::Math,
        };
        let (out, _) = emit_theorem(
            &ctx,
            &thm,
            "Qubitick.HL",
            "ISA",
            &qubitick_rewrite::RuleTable::new(),
            &Default::default(),
            &Default::default(),
            &std::collections::HashMap::new(),
            &[],
            true,
        )
        .unwrap();
        assert!(out.contains("axiom Thm1 :"), "an admitted theorem is assumed, not proved:\n{out}");
        assert!(!out.contains("Eq.refl"), "no placeholder proof may stand in for one:\n{out}");
        assert!(!out.contains("sorryAx"), "and no prove-anything axiom enters the ground:\n{out}");
    }

    #[test]
    fn emits_stated_equality_in_nullity_form() {
        // The corpus's '=' is nullity-anchored, so `a D b = b D a` is emitted as
        // `Eq (Delta (Delta a b) (Delta b a)) varnothing` — the proposition the
        // corpus proof chains actually establish.
        use qubitick_db::queries::SymbolRow;
        let row = |id: i64, tex: &str| SymbolRow { facets: 0,
            id,
            scope: "Era0/Strata0".into(),
            tex: tex.into(),
            display: tex.into(),
            lean_name: tex.trim_start_matches('\\').to_string(),
            trigger: None,
            kind: "notation".into(),
            role: None,
            precedence: None,
            assoc: None,
            home_node: 1,
            elab: None,
            binder_kind: None,
        };
        let ctx = Ctx::from_rows(
            "Era0/Strata0",
            vec![
                declaring(row(5, "\\Delta"), Facet::Nullity),
                declaring(row(6, "\\varnothing"), Facet::Null),
            ],
        );
        let eq = |l: Expr, r: Expr| {
            Expr::Apply(AtomicComposite::new(qubitick_model::EQ_SYM), vec![l, r])
        };
        let delta = |l: Expr, r: Expr| Expr::Apply(AtomicComposite::new(5), vec![l, r]);
        let a = Expr::var("a");
        let b = Expr::var("b");
        let statement = eq(delta(a.clone(), b.clone()), delta(b.clone(), a.clone()));
        let thm = Theorem {
            base: base(),
            statement,
            // `nullity_form` applies only when the chain actually finishes at `∅`
            // (that is what makes it a nullity proof), so give it such a chain:
            // first row's lhs is a Δ-application (the T_BT_01-style Δ-encoded
            // opening), last row's rhs is \varnothing — together they make both
            // `is_nullity` clauses true.
            proof: vec![qubitick_model::ProofStep::Chain {
                rows: vec![
                    qubitick_model::ChainRow {
                        line: 1,
                        lhs: Some(delta(a.clone(), b.clone())),
                        rhs: delta(a.clone(), b.clone()),
                        rel: qubitick_model::StepRel::Eq,
                        just: qubitick_model::ChainJust::Rfl,
                    },
                    qubitick_model::ChainRow {
                        line: 1,
                        lhs: Some(Expr::Sym(AtomicComposite::new(6))),
                        rhs: Expr::Sym(AtomicComposite::new(6)),
                        rel: qubitick_model::StepRel::Eq,
                        just: qubitick_model::ChainJust::Rfl,
                    },
                ],
            }],
            tag: qubitick_model::PhysOrMath::Math,
        };
        let (out, _) = emit_theorem(
            &ctx,
            &thm,
            "Qubitick.HL",
            "ISA",
            &qubitick_rewrite::RuleTable::new(),
            &Default::default(),
            &Default::default(),
            &std::collections::HashMap::new(),
            // No class chain and no class in scope: the test theorems are pure
            // `Eq` statements with no axiom field to cite.
            &[],
            false,
        )
        .unwrap();
        assert!(
            out.contains("Eq (Delta (Delta a b) (Delta b a)) varnothing"),
            "expected nullity form, got:\n{out}"
        );
    }
}
