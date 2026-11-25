//! Definition emission (CodeArch §5.11).

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use qubitick_ctx::Ctx;
use qubitick_model::corpus::{texes_with, Facet};
use qubitick_model::Definition;

/// Emit a definition per Document 5 §5.3/§5.4.
///
/// No `sorry` is ever emitted: a definition without a synthesizable shape fails
/// the build (returns `Err`) per Doc 5 §7.4 ("No `sorry`. If the compiler cannot
/// produce a term, it fails the build.").
pub fn emit_definition(ctx: &Ctx, def: &Definition) -> Result<String> {
    emit_definition_in(
        ctx,
        def,
        false,
        0,
        &HashMap::new(),
        &Default::default(),
        &HashMap::new(),
        &HashMap::new(),
        &Default::default(),
    )
}

/// As [`emit_definition`], but when `is_sort` the glyph is used as a binder sort
/// elsewhere in the module, so it is declared as a `Type`-valued space.
#[allow(clippy::too_many_arguments)]
pub fn emit_definition_in(
    ctx: &Ctx,
    def: &Definition,
    is_sort: bool,
    arity: usize,
    fn_names: &std::collections::HashMap<u32, String>,
    sorts: &std::collections::HashSet<u32>,
    formations: &HashMap<u32, (String, Vec<u32>)>,
    known: &HashMap<String, String>,
    extra_ok: &HashSet<u32>,
) -> Result<String> {
    // A dual-use glyph declares its APPLIED form under the `_fn` name: `Type:
    // \tau \to \mathfrak{E}` on `\mathfrak{L}` describes `\mathfrak{L}(t)`,
    // while the bare `\mathfrak{L}` stays the frame's `Prop`.
    let name = crate::module::definition_decl_name(ctx, def, fn_names);
    // Sort glyphs render as the carrier so a body's lambda binder type matches the
    // operator declarations (`bigcup : Prop → Prop`).
    let resolver = crate::resolver_with_sorts(ctx, fn_names, sorts);
    if is_sort {
        return Ok(format!("axiom {name} : Type\n"));
    }
    match &def.d_block.form {
        Some(qubitick_model::FormBody::MacroExpr(e)) => {
            // A compilable Sema becomes a real `def` (so proofs can unfold it
            // definitionally); otherwise the glyph is declared uninterpreted. A
            // recursive tick operator is emitted as a recursor `def` first; only if
            // that is not applicable do we fall back to a plain macro/axiom.
            // The frame's own answer about the RESULT sort is handed to the Z
            // recursor, exactly as the frame itself does when it emits the glyph:
            // without it the extended emitter cannot tell a `\mathbb{Z}`-valued
            // operator from a τ-valued one, gives up, and the glyph degrades to an
            // axiom — after which every `[\equiv]` row that reduced by kernel
            // conversion (`Eq.refl`) no longer closes.
            let res_z = known.get(&name).and_then(|t| t.ends_with("mathbb_Z").then_some(true));
            Ok(recursor_def_text(ctx, &name, e, &resolver, extra_ok)
                .or_else(|| {
                    recursor_def_text_extended(ctx, &name, e, &resolver, extra_ok, res_z)
                })
                .or_else(|| recursor_def_text_formation(ctx, &name, e, &resolver, formations))
                .or_else(|| macro_def_text(ctx, &name, e, &resolver, known))
                // Nothing about this Sema compiles to a term, so the glyph stays
                // uninterpreted — declared with the type the FRAME inferred for it,
                // not a flat `Prop → … → Prop`. The two must agree: the frame's
                // answer is what every OTHER module sees, and in a ground that
                // holds both, a flat declaration here made `+_{\mathbb{Z}}`'s own
                // node disagree with its own helper's body.
                .unwrap_or_else(|| match known.get(&name) {
                    Some(ty) if ty != "Type" => format!("axiom {name} : {ty}\n"),
                    _ => declaration(&name, arity),
                }))
        }
        Some(qubitick_model::FormBody::Formation(ctors)) => {
            // Document 5 §5.3: bare-prelude inductive with `genInjectivity` off.
            let ctors = ctors
                .iter()
                .map(|c| symbol_lean_name(ctx, c.constructor.primary))
                .collect::<Vec<_>>();
            let mut out = format!("set_option genInjectivity false in\ninductive {name} : Type\n");
            for c in ctors {
                out.push_str(&format!("  | {c}\n"));
            }
            Ok(out)
        }
        // A body we cannot compile (recursive/bracket forms) leaves the glyph as
        // an uninterpreted symbol rather than failing the whole file.
        Some(_) => Ok(declaration(&name, arity)),
        None => {
            // An authored `Type:` is read EXACTLY as the frame reads it
            // (`render_type_expr`), not rendered verbatim. The two must agree: the
            // frame declares the glyphs a signature mentions flat over the carrier
            // (`\mathfrak H : Prop`), so a definition that rendered its own
            // signature verbatim declared `\mathfrak T : \mathfrak H -> \mathcal R`
            // — and `\mathfrak H`, being a `Prop`, is then not a type at all
            // (`Unknown constant CoeSort`), which is precisely what an era ground
            // hit while the same definition passed alone.
            let sig = def
                .d_block
                .sig
                .as_ref()
                .map(|s| crate::module::render_type_expr(ctx, s));
            Ok(match sig {
                Some(ty) => format!("axiom {name} : {ty}\n"),
                None => declaration(&name, arity),
            })
        }
    }
}

/// An uninterpreted declaration of `name` at the observed arity.
fn declaration(name: &str, arity: usize) -> String {
    let mut ty = String::from("Prop");
    for _ in 0..arity {
        ty = format!("Prop → {ty}");
    }
    format!("axiom {name} : {ty}\n")
}

/// Whether `body` mentions `name` as a whole identifier.
fn mentions_word(body: &str, name: &str) -> bool {
    body.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '\''))
        .any(|t| t == name)
}

/// The `def` text for a `Sema:`, or `None` when the body cannot be compiled.
///
/// A body that still contains raw LaTeX means the Sema did not parse into a term
/// (unsupported surface constructs); emitting it would produce invalid Lean, so
/// the caller declares the glyph uninterpreted instead.
/// Emit a tick/integer operator DEFINED BY RECURSION on the bare `\tau` inductive, from
/// a `Sema:` `land (Eq (f …τ₀) rhs₀) (∀ t:τ, Eq (f …⁺t) rhs₁)`.
///
/// The bare `prelude` cannot elaborate `| ctor => …` equation lines (`PProd.mk`), so the
/// recursion is written as a kernel recursor call under `noncomputable` — which compiles
/// without the code generator and stays a real definition that `[\equiv]`/`Eq.refl` can
/// unfold (verified: `Eq (plus_tau a tau.zero) a` is definitional).
/// ```lean
/// noncomputable def plus_tau (a : tau) : tau → tau :=
///   fun b => tau.rec (motive := fun _ => tau) a (fun b rec => tau.succ rec) b
/// ```
pub fn recursor_def_text(
    ctx: &Ctx,
    name: &str,
    e: &qubitick_model::Expr,
    resolver: &impl Fn(u32) -> String,
    extra_ok: &std::collections::HashSet<u32>,
) -> Option<String> {
    // The two-case τ path hardcodes `tau.rec` / `tau → tau`. A ℤ-family op must go
    // through the N-case extended emitter (its recursor is over mathbb_Z).
    if name.contains("mathbb_Z") {
        return None;
    }
    use qubitick_model::Expr;
    // Top must be `land eq0 eq1` (base eq + a `\forall`-successor eq).
    let (eq0, eq1): (&Expr, &Expr) = match e {
        Expr::Apply(h, args) if args.len() == 2 && crate::lean_name_of(ctx, h.primary) == "land" => {
            (&args[0], &args[1])
        }
        _ => return None,
    };
    let (l0, r0) = eq_sides(eq0)?;
    let Expr::Apply(fhead, fargs) = l0 else { return None };
    if resolver(fhead.primary) != name || fargs.is_empty() {
        return None;
    }
    // params = all args but the last (the recursion arg).
    let (params, _rec_arg) = fargs.split_at(fargs.len() - 1);
    let param_list = params
        .iter()
        .map(|p| {
            let v = match p {
                Expr::Var(v) => v.clone(),
                _ => "p".to_string(),
            };
            format!("({v} : tau)")
        })
        .collect::<Vec<_>>()
        .join(" ");
    // eq1: `∀ t : τ, Eq (f p… (^+ t)) rhs1`
    let Expr::Binder(qubitick_model::BinderKind::Forall, bs, body) = eq1 else {
        return None;
    };
    let ctor_var = bs.first()?.vars.first()?.clone();
    let (l1, r1) = eq_sides(body)?;
    let Expr::Apply(f1, _args1) = l1 else { return None };
    if resolver(f1.primary) != name {
        return None;
    }
    let base_body = qubitick_lean::render(r0, resolver);
    // In the `τ.rec` successor lambda the recursive value is `rec`.
    let succ_body = render_succ_body(ctx, r1, &ctor_var, name, resolver)?;
    if base_body.contains('\\') || base_body.is_empty() {
        return None;
    }
    // Restrict to ATOMIC tick recursors: every glyph referenced by the base/succ RHS
    // must be one of the operator itself, its tick constructors (`\tau_0`, `^+`), or
    // `\tau`. An integer/derived operator (`+_{\mathbb{Z}^+}`, `\uparrow_{\mathbb{Z}^-}`,
    // …) references `\mathbb{Z}`, `\uparrow_{\mathbb{Z}}`, etc., which are not the bare
    // ``\tau` inductive constructors — recursor-def'ing it would emit forward refs to
    // undeclared glyphs. Only the atomic tick operators qualify.
    let mut extra: HashSet<u32> = HashSet::new();
    collect_sym_ids(r0, &mut extra);
    collect_sym_ids(r1, &mut extra);
    // Discard the operator head id and the params (Vars carry no symbol id).
    extra.remove(&fhead.primary);
    // The τ sort and its base/succ constructors are the recursor's own grammar.
    for tex in qubitick_model::corpus::texes_with(Facet::SortTau)
        .chain(qubitick_model::corpus::texes_with(Facet::GenTag))
    {
        if let Some(r) = ctx.by_tex(tex).and_then(|rows| rows.first()) {
            extra.remove(&(r.id as u32));
        }
    }
    // Cross-operator tick recursors are allowed when the referenced operator is
    // ITSELF recursor-definable (the tick-recursor closure: plus_tau < times_tau,
    // downarrow < minus_tau). This is the fixpoint, not a name-allowlist: an
    // integer/derived operator (`+_{\mathbb{Z}}`, …) is never in the closure.
    for id in extra_ok {
        extra.remove(id);
    }
    if !extra.is_empty() {
        return None;
    }
    Some(format!(
        "noncomputable def {name} {param_list} : tau → tau :=\n  fun b => tau.rec (motive := fun _ => tau) {base_body} (fun b rec => {succ_body}) b\n"
    ))
}

/// The N-case extension of [`recursor_def_text`] for generated sorts with more
/// than one constructor family (`\mathbb{Z}`, D.BT.47): a `land` of 2 or 3 case
/// equations (base + `∀ n, case` per constructor) compiled to
/// `tau.rec`/`mathbb_Z.rec`. Kept SEPARATE from the two-case tick path so the
/// working τ emitter is untouched; callers try [`recursor_def_text`] first and
/// only fall back to this.
///
/// `res_z_hint` is the frame's own result-sort decision for the op (from
/// `GlyphSig.res_z`) — the motive MUST agree with the frame's declaration or the
/// consuming theorem type-mismatches. `None` infers from the rendered bodies.
pub fn recursor_def_text_extended(
    ctx: &Ctx,
    name: &str,
    e: &qubitick_model::Expr,
    resolver: &impl Fn(u32) -> String,
    extra_ok: &std::collections::HashSet<u32>,
    res_z_hint: Option<bool>,
) -> Option<String> {
    use qubitick_model::{BinderKind, Expr};
    let mut parts: Vec<Expr> = Vec::new();
    {
        fn fl(x: &Expr, ctx: &Ctx, out: &mut Vec<Expr>) {
            match x {
                Expr::Apply(h, args) if crate::lean_name_of(ctx, h.primary) == "land" => {
                    for a in args {
                        fl(a, ctx, out);
                    }
                }
                o => out.push(o.clone()),
            }
        }
        fl(e, ctx, &mut parts);
    }
    let mut cases: Vec<(Expr, Expr, Option<String>)> = Vec::new();
    for c in &parts {
        let (b0, bvar) = match c {
            Expr::Binder(BinderKind::Forall, bs, b) => {
                let v = bs.first()?.vars.first()?.clone();
                (b.as_ref().clone(), Some(v))
            }
            o => (o.clone(), None),
        };
        let (l, r) = eq_sides(&b0)?;
        cases.push((l.clone(), r.clone(), bvar));
    }
    if cases.is_empty() {
        return None;
    }
    let ctor_id = |tex: &str| -> Option<u32> {
        ctx.by_tex(tex).and_then(|rows| rows.first()).map(|r| r.id as u32)
    };
    // Branch constructors classified by config facet: the τ sort's own
    // base+succ (`\tau_0`, `^+`) vs the ℤ element constructors.
    let z_ctors: Vec<&str> = texes_with(Facet::ZCtor).collect();
    let (t0, tk) = (
        texes_with(Facet::TickNull).filter_map(&ctor_id).next()?,
        texes_with(Facet::Succ).filter_map(&ctor_id).next()?,
    );
    let (z0, zp, zm) = (
        ctor_id(z_ctors.first()?)?,
        ctor_id(z_ctors.get(1)?)?,
        ctor_id(z_ctors.get(2)?)?,
    );
    let mut branches: Vec<(u32, Expr, Option<String>)> = Vec::new();
    let mut fhead = 0u32;
    let mut fargs_len = 0usize;
    for (l, r, bvar) in &cases {
        let Expr::Apply(f, fargs) = l else { return None };
        if resolver(f.primary) != name {
            return None;
        }
        fhead = f.primary;
        fargs_len = fargs.len();
        let last = fargs.last()?;
        let cid = match last {
            Expr::Sym(ac) => ac.primary,
            Expr::Apply(h, _) => h.primary,
            _ => return None,
        };
        branches.push((cid, r.clone(), bvar.clone()));
    }
    let sort_rec = if branches.iter().any(|(c, _, _)| *c == t0 || *c == tk) {
        "tau"
    } else if branches.iter().any(|(c, _, _)| *c == z0 || *c == zp || *c == zm) {
        "mathbb_Z"
    } else {
        return None;
    };
    let order: Vec<u32> = if sort_rec == "tau" { vec![t0, tk] } else { vec![z0, zp, zm] };
    // MOTIVE: from the caller's frame inference when available; else from the
    // rendered bodies (any ℤ-ctor/op name ⇒ mathbb_Z; else default tau).
    let branch_res = || -> &'static str {
            let any_z = branches.iter().any(|(_, r, _)| {
                let s = qubitick_lean::render(r, resolver);
                s.contains("mathbb_Z")
            });
            if any_z {
                "mathbb_Z"
            } else {
                let any_prop = branches.iter().any(|(_, r, _)| {
                    let s = qubitick_lean::render(r, resolver);
                    s.contains("top") || s.contains("bot")
                });
                if any_prop { "Prop" } else { "tau" }
            }
        };
        let rsort = match res_z_hint {
            Some(z) if z => "mathbb_Z",
            Some(_) => branch_res(),
            None => branch_res(),
        };
    let params = {
        let n = fargs_len.saturating_sub(1);
        let (l0, _, _) = &cases[0];
        let Expr::Apply(_, fargs) = l0 else { return None };
        fargs[..n].to_vec()
    };
    let z_used = cases.iter().any(|(_, r, _)| {
        let mut ids: HashSet<u32> = HashSet::new();
        collect_sym_ids(r, &mut ids);
        ids.contains(&zp) || ids.contains(&zm) || ids.contains(&z0)
    });
    let z_named = cases.iter().any(|(_, r, _)| {
        let mut ids: HashSet<u32> = HashSet::new();
        collect_sym_ids(r, &mut ids);
        ids.iter().any(|id| resolver(*id).contains("mathbb_Z"))
    });
    // Param sort: a param whose usage sits alongside any ℤ-named op, or an op
    // whose frame result is \mathbb{Z}, has its params on ℤ (D62/63 accumulator,
    // D64 both args); z_used alone mis-reads D62/63 (RHS heads are +_Z±, not
    // ℤ-ctors).
    let pty = if z_used || z_named || res_z_hint == Some(true) {
        "mathbb_Z"
    } else {
        "tau"
    };
    let param_list = params
        .iter()
        .map(|p| {
            let v = match p { Expr::Var(v) => v.clone(), _ => "p".to_string() };
            format!("({v} : {pty})")
        })
        .collect::<Vec<_>>()
        .join(" ");
    let mut branch_terms: Vec<String> = Vec::new();
    let mut zero_body: Option<String> = None;
    for cid in &order {
        let (_, r, bv) = branches.iter().find(|(c, _, _)| c == cid)?;
        let rb = qubitick_lean::render(r, resolver);
        if *cid == t0 || *cid == z0 {
            zero_body = Some(rb);
        } else if sort_rec == "tau" {
            let vr: &str = bv.as_deref().unwrap_or("n");
            let sb = render_succ_body(ctx, r, vr, name, resolver).unwrap_or_else(|| rb.clone());
            branch_terms.push(format!("fun {vr} rec => {sb}"));
        } else {
            let vr: &str = bv.as_deref().unwrap_or("n");
            branch_terms.push(format!("fun {vr} => {rb}"));
        }
    }
    let base_body = zero_body?;
    if base_body.contains('\\') || base_body.is_empty() {
        return None;
    }
    if branch_terms.len() + 1 != order.len() {
        return None;
    }
    let mut extra: HashSet<u32> = HashSet::new();
    for (_, r, _) in &branches {
        collect_sym_ids(r, &mut extra);
    }
    extra.remove(&fhead);
    for id in [t0, tk, z0, zp, zm] {
        extra.remove(&id);
    }
    for (tex, out_name) in [("\\tau", "tau"), ("\\mathbb{Z}", "mathbb_Z")] {
        if let Some(t) = ctor_id(tex) { extra.remove(&t); }
        let _ = out_name;
    }
    for id in extra_ok {
        extra.remove(id);
    }
    // The Prop constants `\top`/`\bot` close recursor branches (D_BT_78's
    // `\ge_{0}^{\mathbb{Z}}` over ℤ); they are frame axioms, not recursable ops.
    for id in extra.clone() {
        if matches!(resolver(id).as_str(), "top" | "bot") {
            extra.remove(&id);
        }
    }
    if !extra.is_empty() {
        return None;
    }
    let rec_call = if sort_rec == "tau" {
        format!(
            "tau.rec (motive := fun _ => {rsort}) ({base_body}) ({b0})",
            b0 = branch_terms[0]
        )
    } else {
        format!(
            "mathbb_Z.rec (motive := fun _ => {rsort}) ({base_body}) ({b0}) ({b1})",
            b0 = branch_terms[0],
            b1 = branch_terms[1]
        )
    };
    Some(format!(
        "noncomputable def {name} {param_list} : {psort} → {rsort} :=\n  fun b => {rec_call} b\n",
        psort = if sort_rec == "tau" { "tau" } else { "mathbb_Z" },
    ))
}


/// Collect every `SymbolId` (head + nested) appearing in `e` into `out`.
fn collect_sym_ids(e: &qubitick_model::Expr, out: &mut HashSet<u32>) {
    use qubitick_model::Expr;
    match e {
        Expr::Sym(ac) => {
            out.insert(ac.primary);
        }
        Expr::Apply(h, args) => {
            out.insert(h.primary);
            for a in args {
                collect_sym_ids(a, out);
            }
        }
        Expr::Bracket(id, args) => {
            out.insert(*id);
            for a in args {
                collect_sym_ids(a, out);
            }
        }
        Expr::Binder(_, bs, body) => {
            for b in bs {
                collect_sym_ids(&b.ty, out);
            }
            collect_sym_ids(body, out);
        }
        Expr::Recursor(s, cases) => {
            collect_sym_ids(s, out);
            for (_, b) in cases {
                collect_sym_ids(b, out);
            }
        }
        Expr::Matrix(rows) => {
            for r in rows {
                for c in r {
                    collect_sym_ids(c, out);
                }
            }
        }
        Expr::Vector(vs) | Expr::Set(vs) => {
            for v in vs {
                collect_sym_ids(v, out);
            }
        }
        Expr::Var(_) => {}
    }
}

/// Render the successor RHS. In the `τ.rec` successor lambda the recursive value is
/// `rec`; the Sema writes it as the operator applied to the constructor-variable
/// (`a +_\tau t`). So a term `f :: args… :: t` (head `f`=operator, last arg the ctor
/// var) renders as `rec`; everything else renders literally (the tau-typecheck
/// validates the rest, e.g. `tick (…rec…)`).
fn render_succ_body(
    _ctx: &Ctx,
    rhs: &qubitick_model::Expr,
    ctor_var: &str,
    name: &str,
    resolver: &impl Fn(u32) -> String,
) -> Option<String> {
    use qubitick_model::Expr;
    fn sub(
        e: &Expr,
        ctor_var: &str,
        name: &str,
        resolver: &impl Fn(u32) -> String,
        out: &mut String,
    ) -> bool {
        match e {
            Expr::Apply(h, args) => {
                if resolver(h.primary) == name
                    && args.last().is_some_and(|a| matches!(a, Expr::Var(v) if v == ctor_var))
                {
                    out.push_str("rec");
                    return true;
                }
                // recurse: head is a symbol, args recurse
                let head_name = resolver(h.primary);
                out.push_str(&head_name);
                for a in args {
                    out.push(' ');
                    let mut piece = String::new();
                    if !sub(a, ctor_var, name, resolver, &mut piece) {
                        // leave literal — parenthesize composite args so an
                        // applied argument (e.g. `minus_ℤ a` under `plus_ℤ`)
                        // stays ONE application in the emitted term.
                        let r = qubitick_lean::render(a, resolver);
                        match a {
                            Expr::Apply(_, _)
                            | Expr::Bracket(_, _)
                            | Expr::Binder(_, _, _)
                            | Expr::Recursor(_, _)
                            | Expr::Matrix(_) => {
                                out.push('(');
                                out.push_str(&r);
                                out.push(')');
                            }
                            _ => out.push_str(&r),
                        }
                    } else {
                        match a {
                            Expr::Apply(_, _)
                            | Expr::Bracket(_, _)
                            | Expr::Binder(_, _, _)
                            | Expr::Recursor(_, _)
                            | Expr::Matrix(_) => {
                                out.push('(');
                                out.push_str(&piece);
                                out.push(')');
                            }
                            _ => out.push_str(&piece),
                        }
                    }
                }
                true
            }
            Expr::Bracket(id, args) => {
                out.push_str(&resolver(*id));
                out.push('[');
                let rendered: Vec<String> = args
                    .iter()
                    .map(|a| {
                        let mut p = String::new();
                        if sub(a, ctor_var, name, resolver, &mut p) {
                            p
                        } else {
                            qubitick_lean::render(a, resolver)
                        }
                    })
                    .collect();
                out.push_str(&rendered.join(" "));
                out.push(']');
                true
            }
            _ => false
        }
    }
    let mut s = String::new();
    if !sub(rhs, ctor_var, name, resolver, &mut s) {
        return None;
    }
    if s.contains('\\') || s.is_empty() {
        return None;
    }
    Some(s)
}

/// The two sides of an `Eq` application, or `None`.
fn eq_sides(x: &qubitick_model::Expr) -> Option<(&qubitick_model::Expr, &qubitick_model::Expr)> {
    use qubitick_model::{Expr, EQ_SYM};
    if let Expr::Apply(h, a) = x
        && h.primary == EQ_SYM
        && a.len() == 2
    {
        Some((&a[0], &a[1]))
    } else {
        None
    }
}

/// Emit an operator DEFINED BY RECURSION over a DECLARATIVE FORMATION carrier, from a
/// `Sema:` `land` of constructor equations `Eq (f ctor₁) rhs₁ ∧ … ∧ Eq (f ctorₙ) rhsₙ`,
/// every ctor a nullary constructor of one `FormBody::Formation`.
pub(crate) fn recursor_def_text_formation(
    ctx: &Ctx,
    name: &str,
    e: &qubitick_model::Expr,
    resolver: &impl Fn(u32) -> String,
    formations: &HashMap<u32, (String, Vec<u32>)>,
) -> Option<String> {
    use qubitick_model::Expr;
    if formations.is_empty() {
        return None;
    }
    let mut parts: Vec<&Expr> = Vec::new();
    fn fl<'a>(x: &'a Expr, ctx: &Ctx, out: &mut Vec<&'a Expr>) {
        match x {
            Expr::Apply(h, args)
                if crate::lean_name_of(ctx, h.primary) == "land"
                    && args.len() == 2 =>
            {
                for a in args {
                    fl(a, ctx, out);
                }
            }
            o => out.push(o),
        }
    }
    fl(e, ctx, &mut parts);
    let mut by_ctor: std::collections::HashMap<u32, String> = Default::default();
    for p in &parts {
        let (l, r) = eq_sides(p)?;
        let Expr::Apply(f, fargs) = l else { return None };
        if resolver(f.primary) != name {
            return None;
        }
        if fargs.len() != 1 {
            return None;
        }
        let last = fargs.last()?;
        let cid = match last {
            Expr::Sym(ac) => ac.primary,
            Expr::Apply(h2, _) => h2.primary,
            _ => return None,
        };
        if !formations.contains_key(&cid) {
            return None;
        }
        let rhs = qubitick_lean::render(r, resolver);
        if rhs.contains('\\') || rhs.is_empty() {
            return None;
        }
        by_ctor.insert(cid, rhs);
    }
    let (carrier, order) = formations.get(by_ctor.keys().next()?)?.clone();
    if !order.iter().all(|c| by_ctor.contains_key(c)) {
        return None;
    }
    let branches: Vec<String> = order.iter().filter_map(|c| by_ctor.get(c).cloned()).collect();
    if branches.len() != order.len() {
        return None;
    }
    Some(format!(
        "noncomputable def {name} : {carrier} → {carrier} :=\n  fun x => {carrier}.rec (motive := fun _ => {carrier}) {} x",
        branches.join(" "),
    ))
}
pub(crate) fn macro_def_text(
    ctx: &Ctx,
    name: &str,
    e: &qubitick_model::Expr,
    resolver: &impl Fn(u32) -> String,
    known: &HashMap<String, String>,
) -> Option<String> {
    use qubitick_model::{AtomicComposite, Expr, EQ_SYM};
    // `=`- and `\leftrightarrow`/`\iff`-headed defining equations are both accepted
    // as a definitional top. Relational glyphs are defined iff-wise in the corpus
    // (`a \subseteq b \leftrightarrow a \cap b = a`), and if they came out as plain
    // `axiom`s the theorem goals over them (opaque `subseteq X Y`) could never
    // kernel-reduce to the `Eq`-shaped proof terms — every such theorem is a type
    // mismatch. args[0] is the definiendum (`a ⊆ b`), its free idents are the
    // params, and args[1] (`a ∩ b = a`) renders as the body.
    let iff_ids: std::collections::HashSet<u32> = texes_with(Facet::LeftIff)
        .chain(texes_with(Facet::RightIff))
        .filter_map(|t| {
            ctx.by_tex(t)
                .and_then(|rows| rows.first())
                .map(|r| r.id as u32)
        })
        .collect();
    let is_def_top =
        |h: &AtomicComposite| h.primary == EQ_SYM || iff_ids.contains(&h.primary);
    let (definiens, params) = match e {
        Expr::Apply(h, args) if is_def_top(h) && args.len() == 2 => {
            let mut vars = Vec::new();
            qubitick_pattern::expr_idents(&args[0], &mut vars);
            (args[1].clone(), vars)
        }
        _ => (e.clone(), Vec::new()),
    };
    // Every free variable of the body must be a definition parameter; otherwise
    // the emitted `def` would reference an undefined `t`/`e`. Declare instead.
    {
        let mut body_vars = Vec::new();
        qubitick_pattern::expr_idents(&definiens, &mut body_vars);
        if body_vars.iter().any(|v| !params.contains(v)) {
            return None;
        }
    }
    let body = qubitick_lean::render(&definiens, resolver);
    // A body that mentions its own name would be a self-referential `def`, which
    // Lean rejects ("fail to show termination"); leave it uninterpreted instead.
    if mentions_word(&body, name) {
        return None;
    }
    if body.is_empty() || body.contains('\\') {
        return None;
    }
    // A definition whose body REFERS to the generated-sort constructors (`^+`/`tick`,
    // `\tau_0`) is tick-valued: `\uparrow a = ^+ a` is `tau → tau`, not `Prop → Prop`.
    // Type its params and result over `tau`.
    let ctor_texes: Vec<&str> = texes_with(Facet::TauConsume).collect();
    let is_tick_body = ctor_texes.iter().any(|t| {
        ctx.by_tex(t)
            .and_then(|rows| rows.first())
            .map(|r| mentions_word(&body, &r.lean_name))
            .unwrap_or(false)
    });
    // The frame is the AUTHORITY on a definition's type: it is what every other
    // module sees, so the definition's own declaration has to agree with it.
    //
    // A definition is a `def` only when it defines the glyph at the glyph's OWN
    // arity. `\ulcorner x \urcorner = \Vert x \Vert` is one argument and one
    // parameter, so it is a definition. `D.BT.73`'s `\frac{a}{b} +_{\mathbb{Q}}
    // \frac{c}{d} = …` states a RULE about the shape of the arguments of a
    // two-argument operator: emitting it as a four-parameter `def` gave one module
    // a `plus_{\mathbb{Q}}` of four arguments while every other module applied it
    // to two.
    let frame_ty = known.get(name).map(|ty| split_arrow_type(ty));
    if let Some((args, _)) = &frame_ty
        && !args.is_empty()
        && args.len() != params.len()
    {
        return Some(format!("axiom {name} : {}\n", known[name]));
    }
    // Otherwise the types come from what the frame says, and where it says nothing,
    // from the glyphs the body APPLIES: `\ulcorner x \urcorner = \Vert x \Vert`
    // is `Prop → τ` because `\Vert` is. The flat `Prop` fallback is not merely
    // loose — a module that never declares the generated sorts let `autoImplicit`
    // bind each sort occurrence separately, so the definition "type-checked"
    // against a fresh variable per mention.
    let derived = body_sorts(&definiens, &params, known, resolver);
    let fallback = if is_tick_body { "tau" } else { "Prop" };
    // The body's own result wins, for the same reason its parameters do: the
    // definition IS that expression, and `\ulcorner x \urcorner = \Vert x \Vert`
    // is τ-valued because `\Vert` is — even though the frame, which only knows how
    // `\ulcorner` is USED elsewhere, would have called it a proposition.
    let rty = derived
        .result
        .or_else(|| frame_ty.as_ref().map(|(_, r)| r.clone()))
        .unwrap_or_else(|| fallback.to_string());
    // A `def` whose body applies an uninterpreted symbol has no code to generate;
    // Lean's compiler says so rather than the type checker, which is why only a
    // `lake build` surfaced it.
    Some(if params.is_empty() {
        format!("noncomputable def {name} : {rty} := {body}\n")
    } else {
        let p = params
            .iter()
            .enumerate()
            .map(|(i, v)| {
                // The BODY's constraint wins for a parameter: the emitted `def`
                // has to compile, and the body is what applies the glyph. The
                // frame's own answer is the fallback, and its RESULT type is
                // preferred above because a result is not constrained by the body's
                // outermost application (`\ulcorner x \urcorner = \Vert x \Vert`
                // is τ-valued even though nothing in the body says so).
                let t = derived
                    .params
                    .get(v)
                    .map(String::as_str)
                    .or_else(|| frame_ty.as_ref().and_then(|(a, _)| a.get(i)).map(String::as_str))
                    .unwrap_or(fallback);
                format!("({v} : {t})")
            })
            .collect::<Vec<_>>()
            .join(" ");
        format!("noncomputable def {name} {p} : {rty} := {body}\n")
    })
}

/// The sorts a definition's body imposes on its parameters, and on its result.
pub(crate) struct BodySorts {
    pub params: HashMap<String, String>,
    /// `None` when the body's outermost head has no known declaration.
    pub result: Option<String>,
}

/// Split a declared Lean type into its argument types and its result.
///
/// `(tau → Prop) → Prop` is ONE argument (`tau → Prop`) and result `Prop`; only
/// `→` at bracket depth zero separates.
fn split_arrow_type(ty: &str) -> (Vec<String>, String) {
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut cur = String::new();
    for c in ty.chars() {
        match c {
            '(' | '{' => {
                depth += 1;
            }
            ')' | '}' => depth = depth.saturating_sub(1),
            '→' if depth == 0 => {
                args.push(cur.trim().to_string());
                cur = String::new();
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if depth == 0 && ty.contains('→') {
        (args, cur.trim().to_string())
    } else if args.is_empty() {
        (vec![], ty.trim().to_string())
    } else {
        (args, cur.trim().to_string())
    }
}

/// Read the sorts a body imposes on its parameters.
///
/// Every application in the body whose head has a declared type constrains the
/// parameters sitting in its argument positions. A `\mathbb{Z}`/`τ` constraint
/// wins over the carrier default; anything left unconstrained stays the carrier.
fn body_sorts(
    body: &qubitick_model::Expr,
    params: &[String],
    known: &HashMap<String, String>,
    resolver: &impl Fn(u32) -> String,
) -> BodySorts {
    fn walk(
        e: &qubitick_model::Expr,
        params: &[String],
        known: &HashMap<String, String>,
        resolver: &impl Fn(u32) -> String,
        out: &mut HashMap<String, String>,
    ) {
        use qubitick_model::Expr;
        if let Expr::Apply(h, args) = e
            && let Some(ty) = known.get(&resolver(h.primary))
        {
            {
                let (argtys, _) = split_arrow_type(ty);
                for (i, a) in args.iter().enumerate() {
                    if let (Expr::Var(v), Some(t)) = (a, argtys.get(i))
                        && params.contains(v)
                    {
                        let t = t.trim_matches(|c| c == '(' || c == ')');
                        // A carrier default never displaces a real sort.
                        if out.get(v).is_none_or(|old| old == "Prop") {
                            out.insert(v.clone(), t.to_string());
                        }
                    }
                }
            }
        }
        match e {
            Expr::Apply(_, args) | Expr::Bracket(_, args) | Expr::Vector(args)
            | Expr::Set(args) => {
                for a in args {
                    walk(a, params, known, resolver, out);
                }
            }
            Expr::Binder(_, bs, b) => {
                for bd in bs {
                    walk(&bd.ty, params, known, resolver, out);
                }
                walk(b, params, known, resolver, out);
            }
            Expr::Recursor(s, cases) => {
                walk(s, params, known, resolver, out);
                for (_, b) in cases {
                    walk(b, params, known, resolver, out);
                }
            }
            Expr::Matrix(rows) => {
                for r in rows {
                    for c in r {
                        walk(c, params, known, resolver, out);
                    }
                }
            }
            Expr::Sym(_) | Expr::Var(_) => {}
        }
    }
    let result = match body {
        qubitick_model::Expr::Apply(h, _) => {
            known.get(&resolver(h.primary)).map(|ty| split_arrow_type(ty).1)
        }
        _ => None,
    };
    let mut p = HashMap::new();
    walk(body, params, known, resolver, &mut p);
    BodySorts { params: p, result }
}

fn symbol_lean_name(ctx: &Ctx, id: u32) -> String {
    ctx.by_id(id as i64)
        .map(|r| r.lean_name.clone())
        .unwrap_or_else(|| format!("s{id}"))
}


#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_db::queries::SymbolRow;
    use qubitick_model::NodeBase;

    fn ctx() -> Ctx {
        Ctx::from_rows(
            "Era0/Strata0",
            vec![SymbolRow { facets: 0,
                id: 1,
                scope: "".into(),
                tex: "\\tau".into(),
                display: "τ".into(),
                lean_name: "Tau".into(),
                trigger: None,
                kind: "macro".into(),
                role: Some("primary".into()),
                precedence: None,
                home_node: 1,
            elab: None,
            binder_kind: None,
            }],
        )
    }

    #[test]
    fn emits_definition() {
        let ctx = ctx();
        let def = Definition {
            base: NodeBase {
                id: 2,
                kind: qubitick_model::NodeKind::Definition,
                era: qubitick_model::Era::Era0,
                strata: qubitick_model::Strata(0),
                name: "Def1".into(),
                req: vec![],
                derived: vec![],
                note: None,
            },
            symbol: qubitick_model::AtomicComposite::new(1),
            extra_symbols: vec![],
            d_block: qubitick_model::DBlockIR {
                kind: qubitick_model::SymbolKind::Macro,
                form: Some(qubitick_model::FormBody::MacroExpr(
                    // `Tau x = x`: the body's free var is a definition parameter, so
                    // it compiles to a real `def Tau (x) := x`.
                    qubitick_model::Expr::Apply(
                        qubitick_model::AtomicComposite::new(qubitick_model::EQ_SYM),
                        vec![
                            qubitick_model::Expr::Apply(
                                qubitick_model::AtomicComposite::new(1),
                                vec![qubitick_model::Expr::Var("x".into())],
                            ),
                            qubitick_model::Expr::Var("x".into()),
                        ],
                    ),
                )),
                ..Default::default()
            },
            tag: qubitick_model::PhysOrMath::Math,
        };
        let out = emit_definition(&ctx, &def).unwrap();
        // `noncomputable`: a generated definition may apply an uninterpreted symbol,
        // and Lean's code generator refuses those outright (`Vert is not supported
        // by code generator`) where the type checker does not care.
        assert!(out.starts_with("noncomputable def Tau"), "{out}");
    }

    #[test]
    fn emits_sema_definition_with_parameters() {
        // Sema `a /\ b = <body>` defines the glyph's head applied to its
        // metavariables: those metavariables become the def's parameters.
        use qubitick_model::{AtomicComposite, Expr, FormBody};
        let ctx = ctx();
        let lhs = Expr::Apply(
            AtomicComposite::new(1),
            vec![Expr::Var("a".into()), Expr::Var("b".into())],
        );
        // `Tau a b = b`: the body must not mention `Tau` (that would be
        // self-referential and is declared uninterpreted instead).
        let rhs = Expr::Var("b".into());
        let sema = Expr::Apply(
            AtomicComposite::new(qubitick_model::EQ_SYM),
            vec![lhs, rhs],
        );
        let def = Definition {
            base: NodeBase {
                id: 2,
                kind: qubitick_model::NodeKind::Definition,
                era: qubitick_model::Era::Era0,
                strata: qubitick_model::Strata(0),
                name: "Def2".into(),
                req: vec![],
                derived: vec![],
                note: None,
            },
            symbol: AtomicComposite::new(1),
            extra_symbols: vec![],
            d_block: qubitick_model::DBlockIR {
                kind: qubitick_model::SymbolKind::Macro,
                form: Some(FormBody::MacroExpr(sema)),
                ..Default::default()
            },
            tag: qubitick_model::PhysOrMath::Math,
        };
        let out = emit_definition(&ctx, &def).unwrap();
        assert_eq!(
            out,
            "noncomputable def Tau (a : Prop) (b : Prop) : Prop := b\n"
        );
    }
}